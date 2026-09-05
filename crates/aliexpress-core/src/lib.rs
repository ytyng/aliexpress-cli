//! Core of aliexpress-cli.
//!
//! The crate has no user interface of its own: the command line and the desktop
//! window both call [`search`] and show what it returns. Everything AliExpress
//! specific -- the URL of a search page, where the product JSON is hidden in
//! the HTML, which field marks a product as 100-yen Shop -- lives here, so
//! that the two front ends cannot drift apart.

mod client;
mod dedupe;
mod filter;
mod model;
mod parse;
mod score;
mod url;

pub use client::{Client, Site};
pub use dedupe::dedupe;
pub use filter::{Filter, Kind};
pub use model::{Product, SearchPage};
pub use parse::{ParseError, parse_search_html};
pub use score::value_score;

use std::fmt;
use std::thread;
use std::time::Duration;

/// Server side sort order of a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// AliExpress' own relevance ranking.
    #[default]
    Best,
    PriceAsc,
    PriceDesc,
    /// Most sold first.
    Orders,
    /// Best [`value_score`] first. Computed here, after the pages are fetched.
    Value,
}

impl Sort {
    /// The `sortType` query parameter AliExpress understands.
    fn server_param(self) -> &'static str {
        match self {
            Sort::Best | Sort::Value => "default",
            Sort::PriceAsc => "price_asc",
            Sort::PriceDesc => "price_desc",
            Sort::Orders => "orders_desc",
        }
    }
}

/// Everything a search needs, as the front ends collect it.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub keyword: String,
    /// How many result pages to fetch, starting at the first. 60 products each.
    pub pages: u32,
    pub sort: Sort,
    pub filter: Filter,
    /// Ask AliExpress for Choice products only.
    pub choice: bool,
    /// Ask AliExpress for products shipped for free only.
    pub free_shipping: bool,
    /// Collapse listings of the same product into the best one.
    pub dedupe: bool,
    /// Keep at most this many products after filtering.
    pub limit: Option<usize>,
}

/// What a search produced.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// How many results AliExpress reports for the query, before any local filtering.
    pub total_results: u64,
    /// How many products the fetched pages held before filtering.
    pub fetched: usize,
    pub products: Vec<Product>,
}

/// How a search can fail.
#[derive(Debug)]
pub enum Error {
    Http(ureq::Error),
    Parse(ParseError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Http(error) => write!(f, "request failed: {error}"),
            Error::Parse(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ureq::Error> for Error {
    fn from(error: ureq::Error) -> Self {
        Error::Http(error)
    }
}

impl From<ParseError> for Error {
    fn from(error: ParseError) -> Self {
        Error::Parse(error)
    }
}

/// Pause between two page requests.
///
/// One search page is what a person loads at a time, and a short pause keeps a
/// multi-page search from looking like a crawler to the server.
const PAGE_PAUSE: Duration = Duration::from_millis(1200);

/// Pause before the one retry of a failed request.
const RETRY_PAUSE: Duration = Duration::from_millis(1500);

/// Fetches the requested pages and reduces them to the products asked for.
///
/// Pages are fetched in order. The search stops early when a page comes back
/// empty or when the pages fetched cover every result the server reports; a
/// page holding fewer than `page_size` products is *not* taken as the last
/// one, because the server routinely drops cards from a page.
pub fn search(client: &Client, options: &SearchOptions) -> Result<SearchResult, Error> {
    let mut total_results = 0;
    let mut fetched = 0;
    let mut products = Vec::new();
    for page in 1..=options.pages.max(1) {
        if page > 1 {
            thread::sleep(PAGE_PAUSE);
        }
        let result = fetch_page_with_retry(client, options, page)?;
        total_results = result.total_results;
        fetched += result.products.len();
        let covered = u64::from(page) * u64::from(result.page_size);
        let last = result.products.is_empty() || covered >= result.total_results;
        products.extend(result.products);
        if last {
            break;
        }
    }
    Ok(SearchResult {
        total_results,
        fetched,
        products: reduce(products, options),
    })
}

/// One page, with a single retry after a transport failure.
///
/// A dropped connection in the middle of a multi-page search would otherwise
/// throw away the pages already fetched. Neither an error status nor a parse
/// failure is retried: a refusal or a bot check page comes back the same the
/// second time, and asking again only makes the client look more like a bot.
fn fetch_page_with_retry(
    client: &Client,
    options: &SearchOptions,
    page: u32,
) -> Result<SearchPage, Error> {
    match client.search_page(options, page) {
        Err(Error::Http(error)) if !matches!(error, ureq::Error::StatusCode(_)) => {
            thread::sleep(RETRY_PAUSE);
            client.search_page(options, page)
        }
        result => result,
    }
}

/// The local half of a search: filter, deduplicate, sort, cut.
///
/// Separate from [`search`] so it can be tested without a server.
pub fn reduce(products: Vec<Product>, options: &SearchOptions) -> Vec<Product> {
    // The server repeats a few products across pages; the first copy that
    // passes the filter is kept. Filtering first, so that an ad copy dropped by
    // `exclude_ads` does not take the id with it.
    let mut seen = std::collections::HashSet::new();
    let mut products: Vec<Product> = products
        .into_iter()
        .filter(|product| options.filter.accepts(product))
        .filter(|product| seen.insert(product.id.clone()))
        .collect();
    if options.dedupe {
        products = dedupe(products);
    }
    // Every sort is stable, so products the sort cannot tell apart keep the
    // server's order. The server's own orders are re-applied after a dedupe,
    // since a group's representative can sit where an earlier listing was.
    match options.sort {
        Sort::Best => {}
        Sort::Value => products.sort_by(|a, b| value_score(b).total_cmp(&value_score(a))),
        Sort::PriceAsc if options.dedupe => products.sort_by(|a, b| a.price.total_cmp(&b.price)),
        Sort::PriceDesc if options.dedupe => products.sort_by(|a, b| b.price.total_cmp(&a.price)),
        Sort::Orders if options.dedupe => products.sort_by_key(|p| std::cmp::Reverse(p.sales)),
        Sort::PriceAsc | Sort::PriceDesc | Sort::Orders => {}
    }
    if let Some(limit) = options.limit {
        products.truncate(limit);
    }
    products
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::product;

    fn options() -> SearchOptions {
        SearchOptions {
            keyword: "x".to_string(),
            pages: 1,
            sort: Sort::Best,
            filter: Filter::default(),
            choice: false,
            free_shipping: false,
            dedupe: false,
            limit: None,
        }
    }

    #[test]
    fn reduce_filters_sorts_by_value_and_cuts() {
        // Arrange
        let good = Product {
            price: 100.0,
            rating: Some(4.9),
            sales: Some(5000),
            ..product("good")
        };
        let poor = Product {
            price: 100.0,
            rating: Some(3.0),
            sales: Some(5),
            ..product("poor")
        };
        let ad = Product {
            ad: true,
            rating: Some(5.0),
            sales: Some(99999),
            ..product("ad")
        };
        let options = SearchOptions {
            sort: Sort::Value,
            filter: Filter {
                exclude_ads: true,
                ..Filter::default()
            },
            limit: Some(1),
            ..options()
        };
        // Act
        let kept = reduce(vec![poor, ad, good.clone()], &options);
        // Assert
        assert_eq!(kept, vec![good]);
    }

    #[test]
    fn reduce_drops_repeated_ids_across_pages() {
        // Arrange
        let first = product("a");
        let again = Product {
            price: 1.0,
            ..product("a")
        };
        // Act
        let kept = reduce(vec![first.clone(), again], &options());
        // Assert
        assert_eq!(kept, vec![first]);
    }

    #[test]
    fn reduce_keeps_a_product_whose_ad_copy_was_filtered_out() {
        // Arrange
        let ad = Product {
            ad: true,
            ..product("a")
        };
        let plain = product("a");
        let options = SearchOptions {
            filter: Filter {
                exclude_ads: true,
                ..Filter::default()
            },
            ..options()
        };
        // Act
        let kept = reduce(vec![ad, plain.clone()], &options);
        // Assert
        assert_eq!(kept, vec![plain]);
    }

    #[test]
    fn reduce_restores_the_price_order_after_a_dedupe() {
        // Arrange: the cheap listing's group is represented by the dear, better rated one.
        let cheap = Product {
            price: 100.0,
            spu_id: Some("s".into()),
            ..product("cheap")
        };
        let mid = Product {
            price: 200.0,
            title: "something else entirely".into(),
            ..product("mid")
        };
        let dear = Product {
            price: 300.0,
            spu_id: Some("s".into()),
            rating: Some(5.0),
            sales: Some(10),
            ..product("dear")
        };
        let options = SearchOptions {
            sort: Sort::PriceAsc,
            dedupe: true,
            ..options()
        };
        // Act
        let kept = reduce(vec![cheap, mid.clone(), dear.clone()], &options);
        // Assert
        assert_eq!(kept, vec![mid, dear]);
    }

    #[test]
    fn reduce_keeps_the_server_order_by_default() {
        // Arrange
        let first = product("a");
        let second = product("b");
        // Act
        let kept = reduce(vec![first.clone(), second.clone()], &options());
        // Assert
        assert_eq!(kept, vec![first, second]);
    }
}
