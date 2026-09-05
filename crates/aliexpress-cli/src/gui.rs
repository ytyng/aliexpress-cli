//! Desktop window.
//!
//! The window is a search form and a grid of product cards. Everything about a
//! search -- the URL, the parsing, the filters, the ranking -- is done by
//! `aliexpress_core`, exactly as for the command line; the window only collects
//! the form and draws the result.
//!
//! - **A search runs on a blocking worker.** Fetching several pages takes seconds
//!   (there is a pause between pages), and doing it on a command thread would
//!   freeze the window.
//! - **Only one search is in flight at a time.** A second one while the first
//!   runs is refused rather than queued: the user changed their mind, and the
//!   first result would land on top of the new form.
//! - **Product pages open in the default browser through a command of this
//!   app**, which accepts product URLs of the site being searched and nothing
//!   else. The window never gets a general "open any URL" permission.

use std::io;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;

use aliexpress_core::{Client, Filter, Kind, Product, SearchOptions, SearchResult, Sort, search};

/// The form as the window sends it.
#[derive(Debug, Clone, Deserialize)]
pub struct Query {
    pub keyword: String,
    /// `any`, `hundred_yen_shop` or `normal`.
    pub kind: String,
    pub choice: bool,
    pub free_shipping: bool,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub min_rating: Option<f64>,
    pub exclude_ads: bool,
    pub dedupe: bool,
    /// `best`, `value`, `price_asc`, `price_desc` or `orders`.
    pub sort: String,
    pub pages: u32,
    /// Show at most this many products. Absent, or 0, for all of them; the
    /// command line refuses 0, and the form's field has a minimum of 1.
    pub limit: Option<usize>,
}

impl Query {
    fn options(&self) -> Result<SearchOptions, String> {
        let keyword = self.keyword.trim();
        if keyword.is_empty() {
            return Err("Type something to search for.".to_string());
        }
        let kind = match self.kind.as_str() {
            "any" => Kind::Any,
            "hundred_yen_shop" => Kind::HundredYenShop,
            "normal" => Kind::Normal,
            other => return Err(format!("Unknown kind: {other}")),
        };
        let sort = match self.sort.as_str() {
            "best" => Sort::Best,
            "value" => Sort::Value,
            "price_asc" => Sort::PriceAsc,
            "price_desc" => Sort::PriceDesc,
            "orders" => Sort::Orders,
            other => return Err(format!("Unknown sort: {other}")),
        };
        Ok(SearchOptions {
            keyword: keyword.to_string(),
            pages: self.pages.clamp(1, 10),
            sort,
            filter: Filter {
                kind,
                min_price: self.min_price,
                max_price: self.max_price,
                min_rating: self.min_rating,
                exclude_ads: self.exclude_ads,
            },
            choice: self.choice,
            free_shipping: self.free_shipping,
            dedupe: self.dedupe,
            limit: self.limit.filter(|&limit| limit > 0),
        })
    }

    /// The form for a search given on the command line, so `--gui keyword`
    /// opens with the keyword filled in and the flags set.
    fn from_options(options: &SearchOptions) -> Self {
        Query {
            keyword: options.keyword.clone(),
            kind: match options.filter.kind {
                Kind::Any => "any",
                Kind::HundredYenShop => "hundred_yen_shop",
                Kind::Normal => "normal",
            }
            .to_string(),
            choice: options.choice,
            free_shipping: options.free_shipping,
            min_price: options.filter.min_price,
            max_price: options.filter.max_price,
            min_rating: options.filter.min_rating,
            exclude_ads: options.filter.exclude_ads,
            dedupe: options.dedupe,
            limit: options.limit,
            sort: match options.sort {
                Sort::Best => "best",
                Sort::Value => "value",
                Sort::PriceAsc => "price_asc",
                Sort::PriceDesc => "price_desc",
                Sort::Orders => "orders",
            }
            .to_string(),
            pages: options.pages,
        }
    }
}

/// What the window shows for one search.
#[derive(Debug, Clone, Serialize)]
pub struct Results {
    pub total_results: u64,
    pub fetched: usize,
    pub products: Vec<Product>,
    /// The `value_score` of each product, in the same order.
    pub scores: Vec<f64>,
}

impl From<SearchResult> for Results {
    fn from(result: SearchResult) -> Self {
        let scores = result
            .products
            .iter()
            .map(aliexpress_core::value_score)
            .collect();
        Results {
            total_results: result.total_results,
            fetched: result.fetched,
            products: result.products,
            scores,
        }
    }
}

/// What the window opens with: which site, and the form filled in from the
/// command line flags.
#[derive(Debug, Clone, Serialize)]
pub struct Initial {
    pub site_host: String,
    pub keyword: String,
    pub kind: String,
    pub choice: bool,
    pub free_shipping: bool,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub min_rating: Option<f64>,
    pub exclude_ads: bool,
    pub dedupe: bool,
    pub sort: String,
    pub pages: u32,
    pub limit: Option<usize>,
    /// A result the command line already fetched, shown as soon as the window
    /// opens instead of searching again.
    pub results: Option<Results>,
}

struct GuiState {
    client: Client,
    initial: SearchOptions,
    /// Taken by the first `initial_form` call, so a reload of the window
    /// searches afresh rather than showing a stale result.
    preset: Mutex<Option<Results>>,
    busy: Mutex<bool>,
}

/// Marks a search as running, or reports that one already is. Clears the mark
/// on drop, so an error path cannot leave the window refusing every search.
struct BusyGuard<'a>(&'a Mutex<bool>);

impl<'a> BusyGuard<'a> {
    fn take(busy: &'a Mutex<bool>) -> Result<Self, String> {
        let mut flag = busy.lock().unwrap_or_else(|poison| poison.into_inner());
        if *flag {
            return Err("A search is already running.".to_string());
        }
        *flag = true;
        Ok(BusyGuard(busy))
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        *self.0.lock().unwrap_or_else(|poison| poison.into_inner()) = false;
    }
}

/// The form the window opens with.
#[tauri::command]
fn initial_form(state: State<'_, GuiState>) -> Initial {
    let query = Query::from_options(&state.initial);
    Initial {
        site_host: state.client.site().host.clone(),
        keyword: query.keyword,
        kind: query.kind,
        choice: query.choice,
        free_shipping: query.free_shipping,
        min_price: query.min_price,
        max_price: query.max_price,
        min_rating: query.min_rating,
        exclude_ads: query.exclude_ads,
        dedupe: query.dedupe,
        sort: query.sort,
        pages: query.pages,
        limit: query.limit,
        results: state
            .preset
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take(),
    }
}

/// Runs a search. Refused while another one runs.
#[tauri::command]
async fn run_search(state: State<'_, GuiState>, query: Query) -> Result<Results, String> {
    let _busy = BusyGuard::take(&state.busy)?;
    let options = query.options()?;
    // The worker gets its own handle: `State` cannot cross into the thread, and
    // cloning the client is cheap (the HTTP agent is shared behind it).
    let client = state.client.clone();
    let result = tauri::async_runtime::spawn_blocking(move || search(&client, &options))
        .await
        .map_err(|error| format!("The worker thread died unexpectedly: {error}"))?
        .map_err(|error| error.to_string())?;
    let scores = result
        .products
        .iter()
        .map(aliexpress_core::value_score)
        .collect();
    Ok(Results {
        total_results: result.total_results,
        fetched: result.fetched,
        products: result.products,
        scores,
    })
}

/// Opens a product page in the default browser.
///
/// Only URLs of product pages on the site being searched are accepted. The URL
/// comes from the window, and the window renders text from AliExpress; a
/// general "open this" command would let that text open anything.
#[tauri::command]
fn open_product(state: State<'_, GuiState>, url: String) -> Result<(), String> {
    if !is_product_url(&url, &state.client.site().host) {
        return Err(format!("Refusing to open {url}: not a product page."));
    }
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|error| error.to_string())
}

fn is_product_url(url: &str, host: &str) -> bool {
    let prefix = format!("https://{host}/item/");
    url.strip_prefix(&prefix)
        .and_then(|rest| rest.strip_suffix(".html"))
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

/// Opens the window and returns when it closes.
///
/// With `preset`, the window opens on that result -- the one the command line
/// fetched -- with the form filled in from `initial` for refining it. Without,
/// it opens on the form and searches by itself when `initial` has a keyword.
pub fn run(client: Client, initial: SearchOptions, preset: Option<SearchResult>) -> io::Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(GuiState {
            client,
            initial,
            preset: Mutex::new(preset.map(Results::from)),
            busy: Mutex::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            initial_form,
            run_search,
            open_product
        ])
        .run(tauri::generate_context!())
        .map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_product_pages_of_the_searched_site_open() {
        // Arrange
        let host = "ja.aliexpress.com";
        // Act / Assert
        assert!(is_product_url(
            "https://ja.aliexpress.com/item/1005010567441252.html",
            host
        ));
        assert!(!is_product_url(
            "https://www.aliexpress.com/item/1005010567441252.html",
            host
        ));
        assert!(!is_product_url(
            "https://ja.aliexpress.com/item/../evil.html",
            host
        ));
        assert!(!is_product_url(
            "https://ja.aliexpress.com/item/.html",
            host
        ));
        assert!(!is_product_url("file:///etc/passwd", host));
    }

    #[test]
    fn the_form_round_trips_through_options() {
        // Arrange
        let query = Query {
            keyword: " usb c cable ".to_string(),
            kind: "hundred_yen_shop".to_string(),
            choice: true,
            free_shipping: false,
            min_price: Some(100.0),
            max_price: None,
            min_rating: Some(4.5),
            exclude_ads: true,
            dedupe: true,
            sort: "value".to_string(),
            pages: 99,
            limit: Some(5),
        };
        // Act
        let options = query.options().unwrap();
        let back = Query::from_options(&options);
        // Assert
        assert_eq!(options.keyword, "usb c cable");
        assert_eq!(options.pages, 10, "pages are capped");
        assert_eq!(options.filter.kind, Kind::HundredYenShop);
        assert_eq!(options.sort, Sort::Value);
        assert_eq!(options.limit, Some(5));
        assert_eq!(back.kind, "hundred_yen_shop");
        assert_eq!(back.sort, "value");
        assert_eq!(back.limit, Some(5));
        assert!(back.dedupe && back.choice && back.exclude_ads);
    }

    #[test]
    fn an_empty_keyword_is_refused() {
        // Arrange
        let query = Query {
            keyword: "  ".to_string(),
            kind: "any".to_string(),
            choice: false,
            free_shipping: false,
            min_price: None,
            max_price: None,
            min_rating: None,
            exclude_ads: false,
            dedupe: false,
            sort: "best".to_string(),
            pages: 1,
            limit: Some(0),
        };
        // Act / Assert
        assert!(query.options().is_err());
    }
}
