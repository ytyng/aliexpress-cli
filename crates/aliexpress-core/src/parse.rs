//! Reading the products out of a search page.
//!
//! The page is rendered in the browser from a JSON document the server embeds
//! in a `<script>` tag: `window._dida_config_._init_data_= { data: {...} }`.
//! Reading that document is both simpler and more complete than reading the
//! HTML around it, whose class names change with every deployment.

use std::fmt;

use serde_json::Value;

use crate::client::Site;
use crate::model::{Product, SearchPage};

/// Why a page could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The page has no embedded search data. Usually a bot check page or a
    /// redirect to the home page.
    NoData,
    /// The embedded data is not JSON. The page format changed.
    BadJson(String),
    /// The JSON is there but not in the shape this reader knows.
    Shape(&'static str),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::NoData => write!(
                f,
                "the page holds no search results (a bot check or an unexpected page)"
            ),
            ParseError::BadJson(error) => {
                write!(f, "the embedded search data is not valid JSON: {error}")
            }
            ParseError::Shape(what) => write!(
                f,
                "the embedded search data has an unexpected shape: {what}"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// Where the embedded document starts, up to the JSON object itself.
const DATA_MARKER: &str = "_init_data_";

/// Reads the products out of the HTML of a search page.
pub fn parse_search_html(html: &str, site: &Site) -> Result<SearchPage, ParseError> {
    let json = embedded_json(html).ok_or(ParseError::NoData)?;
    let data: Value = serde_json::Deserializer::from_str(json)
        .into_iter::<Value>()
        .next()
        .ok_or(ParseError::NoData)?
        .map_err(|error| ParseError::BadJson(error.to_string()))?;
    let fields = data
        .pointer("/data/root/fields")
        .ok_or(ParseError::Shape("no data.root.fields"))?;
    let page_info = fields
        .get("pageInfo")
        .ok_or(ParseError::Shape("no pageInfo"))?;
    // A search with no hits has no item list at all (`searchResultType` is
    // `zero_result`); that is an empty page, not a page of another shape.
    let items = fields
        .pointer("/mods/itemList/content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let products = items
        .iter()
        .filter_map(|item| product(item, site))
        .collect();
    Ok(SearchPage {
        page: as_u64(page_info.get("page")).unwrap_or(1) as u32,
        page_size: as_u64(page_info.get("pageSize")).unwrap_or(60) as u32,
        total_results: as_u64(page_info.get("totalResults")).unwrap_or(0),
        products,
    })
}

/// The text from the opening brace of the embedded document onwards.
///
/// The assignment is `_init_data_= { data: {`, and `data` is a bare identifier,
/// not JSON. The object *after* `data:` is JSON, so that is what is returned;
/// the deserializer stops at its closing brace and ignores the rest of the page.
fn embedded_json(html: &str) -> Option<&str> {
    let mut rest = html;
    while let Some(at) = rest.find(DATA_MARKER) {
        rest = &rest[at + DATA_MARKER.len()..];
        let after = rest.trim_start();
        let Some(after) = after.strip_prefix('=') else {
            continue;
        };
        let Some(after) = after.trim_start().strip_prefix('{') else {
            continue;
        };
        let Some(after) = after.trim_start().strip_prefix("data") else {
            continue;
        };
        let Some(after) = after.trim_start().strip_prefix(':') else {
            continue;
        };
        let after = after.trim_start();
        if after.starts_with('{') {
            return Some(after);
        }
    }
    None
}

/// One product card, or `None` for a card without a price: the page holds a
/// few of those (banners rendered as items), and they are not products.
fn product(item: &Value, site: &Site) -> Option<Product> {
    let id = item.get("productId")?.as_str()?.to_string();
    let prices = item.get("prices")?;
    let sale = prices.get("salePrice")?;
    let price = as_f64(sale.get("minPrice"))?;
    let currency = sale
        .get("currencyCode")
        .and_then(Value::as_str)
        .unwrap_or(&site.currency)
        .to_string();
    let ut_log = item.pointer("/trace/utLogMap");
    let ut = |key: &str| ut_log.and_then(|log| log.get(key)).and_then(Value::as_str);
    let sales = ut("real_trade_count")
        .and_then(|count| count.parse::<u64>().ok())
        .or_else(|| {
            item.pointer("/trade/tradeDesc")
                .and_then(Value::as_str)
                .and_then(parse_sales_text)
        });
    let selling_points = item
        .get("sellingPoints")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let has_selling_point = |source: &str| {
        selling_points
            .iter()
            .any(|point| point.get("source").and_then(Value::as_str) == Some(source))
    };
    Some(Product {
        url: site.product_url(&id),
        id,
        title: item
            .pointer("/title/displayTitle")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        image_url: absolute(
            item.pointer("/image/imgUrl")
                .and_then(Value::as_str)
                .unwrap_or(""),
        ),
        currency,
        price,
        original_price: as_f64(prices.pointer("/originalPrice/minPrice")),
        discount_percent: as_u64(sale.get("discount")).map(|d| d as u32),
        rating: as_f64(item.pointer("/evaluation/starRating")),
        sales,
        sales_text: item
            .pointer("/trade/tradeDesc")
            .and_then(Value::as_str)
            .map(str::to_string),
        yoridori: is_yoridori(item),
        choice: ut("isChoice") == Some("true") || has_selling_point("choice_atm"),
        ad: item.get("productType").and_then(Value::as_str) == Some("ad")
            || item.get("p4p").is_some(),
        ship_from: ship_from(item),
        store_name: store_name(item),
        bulk_offer: selling_points
            .iter()
            .find(|point| {
                point.get("source").and_then(Value::as_str) == Some("common_b_item_bulk_choice_atm")
            })
            .and_then(|point| point.pointer("/tagContent/tagText"))
            .and_then(Value::as_str)
            .map(str::to_string),
        spu_id: ut("spu_id")
            .filter(|id| !id.is_empty() && *id != "-1")
            .map(str::to_string),
        pic_group_id: ut("pic_group_id")
            .filter(|id| !id.is_empty() && *id != "0")
            .map(str::to_string),
    })
}

/// Yoridori products carry a "rainbow" ribbon linking to the programme's channel
/// page. The ribbon type is the stable signal; the title is the localized word.
fn is_yoridori(item: &Value) -> bool {
    let Some(rainbow) = item.get("rainbow") else {
        return false;
    };
    rainbow.get("rainbowType").and_then(Value::as_str) == Some("channelProduct")
        && rainbow.get("title").and_then(Value::as_str) == Some("よりどり")
}

/// The ship-from country hides in `pdp_cdi`, a URL encoded JSON string passed
/// to the product page.
fn ship_from(item: &Value) -> Option<String> {
    let encoded = item.pointer("/trace/pdpParams/pdp_cdi")?.as_str()?;
    let decoded = percent_decode(encoded);
    let value: Value = serde_json::from_str(&decoded).ok()?;
    value.get("shipFrom")?.as_str().map(str::to_string)
}

/// The store name hides in `p4pExtendParam`, a JSON string.
fn store_name(item: &Value) -> Option<String> {
    let text = item.pointer("/trace/custom/p4pExtendParam")?.as_str()?;
    let value: Value = serde_json::from_str(text).ok()?;
    value.get("store_name")?.as_str().map(str::to_string)
}

/// `//host/path` image URLs are scheme relative; the front ends need absolute ones.
fn absolute(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        url.to_string()
    }
}

/// Units sold from the card's text: `4,000+ 点販売`, `100K+ 点販売`, `1.5K+ sold`,
/// `63 点販売`. The number is the first token; the rest is a localized word.
fn parse_sales_text(text: &str) -> Option<u64> {
    let token = text.split_whitespace().next()?;
    let mut digits = String::new();
    let mut multiplier = 1.0;
    for c in token.chars() {
        match c {
            '0'..='9' | '.' => digits.push(c),
            ',' | '+' => {}
            'K' | 'k' => multiplier = 1_000.0,
            'M' | 'm' => multiplier = 1_000_000.0,
            _ => break,
        }
    }
    let number: f64 = digits.parse().ok()?;
    Some((number * multiplier).round() as u64)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // Sliced as bytes, not as `str`: a `%` followed by a multi-byte character
        // would put the slice inside that character and panic.
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn as_f64(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

fn as_u64(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/search.html");

    fn page() -> SearchPage {
        parse_search_html(FIXTURE, &Site::japan()).expect("the fixture parses")
    }

    #[test]
    fn reads_the_page_summary() {
        // Arrange / Act
        let page = page();
        // Assert
        assert_eq!(page.page, 1);
        assert_eq!(page.page_size, 60);
        assert_eq!(page.total_results, 67538);
    }

    #[test]
    fn skips_cards_without_a_price() {
        // Arrange / Act
        let page = page();
        // Assert: the fixture holds five cards, one of them a banner without prices.
        assert_eq!(page.products.len(), 4);
        assert!(page.products.iter().all(|p| p.price > 0.0));
    }

    #[test]
    fn reads_a_yoridori_product() {
        // Arrange / Act
        let page = page();
        let product = page
            .products
            .iter()
            .find(|p| p.id == "1005010567441252")
            .expect("the Yoridori product is there");
        // Assert
        assert!(product.yoridori);
        assert!(product.choice);
        assert!(!product.ad);
        assert_eq!(product.price, 50.0);
        assert_eq!(product.original_price, Some(304.0));
        assert_eq!(product.discount_percent, Some(83));
        assert_eq!(product.currency, "JPY");
        assert_eq!(product.rating, Some(4.8));
        assert_eq!(product.sales, Some(75730));
        assert_eq!(product.sales_text.as_deref(), Some("50,000+ 点販売"));
        assert_eq!(product.ship_from.as_deref(), Some("CN"));
        assert_eq!(product.store_name.as_deref(), Some("Shop1104633993 Store"));
        assert_eq!(product.spu_id.as_deref(), Some("6000000001703831"));
        assert_eq!(product.pic_group_id.as_deref(), Some("1005008674452595"));
        assert_eq!(
            product.url,
            "https://ja.aliexpress.com/item/1005010567441252.html"
        );
        assert!(
            product
                .image_url
                .starts_with("https://ae-pic-a1.aliexpress-media.com/")
        );
        assert!(product.title.contains("USB-C"));
    }

    #[test]
    fn reads_an_ad_and_a_plain_product() {
        // Arrange / Act
        let page = page();
        let ad = page
            .products
            .iter()
            .find(|p| p.id == "1005008570336761")
            .unwrap();
        let second = page
            .products
            .iter()
            .find(|p| p.id == "1005007539883736")
            .unwrap();
        let bulk = page
            .products
            .iter()
            .find(|p| p.id == "1005010599451051")
            .unwrap();
        // Assert
        assert!(ad.ad);
        assert!(!ad.yoridori);
        assert_eq!(ad.pic_group_id, None, "a zero group id means no group");
        assert_eq!(ad.bulk_offer, None);
        assert!(!second.ad);
        assert!(second.yoridori);
        assert_eq!(
            bulk.bulk_offer.as_deref(),
            Some("3点以上注文で1点あたり142円")
        );
    }

    #[test]
    fn a_page_without_data_is_reported_as_such() {
        // Arrange
        let html = "<html><body>Please verify you are human</body></html>";
        // Act
        let result = parse_search_html(html, &Site::japan());
        // Assert
        assert_eq!(result.unwrap_err(), ParseError::NoData);
    }

    #[test]
    fn the_marker_inside_scripts_does_not_confuse_the_reader() {
        // Arrange: the page mentions _init_data_ in other scripts before the assignment.
        let html = "<script>if(window._dida_config_._init_data_&&x){}</script>\
            <script>window._dida_config_._init_data_= { data: {\"data\":{\"root\":{\"fields\":{\"pageInfo\":{\"page\":3,\"pageSize\":60,\"totalResults\":\"7\"},\"mods\":{\"itemList\":{\"content\":[]}}}}}} };</script>";
        // Act
        let page = parse_search_html(html, &Site::japan()).unwrap();
        // Assert
        assert_eq!(page.page, 3);
        assert_eq!(page.total_results, 7);
        assert!(page.products.is_empty());
    }

    #[test]
    fn a_search_without_hits_is_an_empty_page() {
        // Arrange: the zero-result page carries the filters but no item list.
        let html = "<script>window._dida_config_._init_data_= { data: {\"data\":{\"root\":{\"fields\":{\"pageInfo\":{\"page\":1,\"pageSize\":60,\"totalResults\":0,\"searchResultType\":\"zero_result\"},\"mods\":{\"searchRefineFilters\":{}}}}}} };</script>";
        // Act
        let page = parse_search_html(html, &Site::japan()).unwrap();
        // Assert
        assert_eq!(page.total_results, 0);
        assert!(page.products.is_empty());
    }

    #[test]
    fn sales_text_in_every_form_it_takes() {
        // Arrange / Act / Assert
        assert_eq!(parse_sales_text("4,000+ 点販売"), Some(4000));
        assert_eq!(parse_sales_text("100K+ 点販売"), Some(100_000));
        assert_eq!(parse_sales_text("1.5K+ sold"), Some(1500));
        assert_eq!(parse_sales_text("63 点販売"), Some(63));
        assert_eq!(parse_sales_text("1,000+ sold"), Some(1000));
        assert_eq!(parse_sales_text("点販売"), None);
    }

    #[test]
    fn percent_decoding() {
        // Arrange / Act / Assert
        assert_eq!(percent_decode("%7B%22a%22%3A1%7D"), "{\"a\":1}");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("bad%zz%4"), "bad%zz%4");
        assert_eq!(percent_decode("%あ%E3%81%82"), "%ああ");
    }
}
