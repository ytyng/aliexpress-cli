//! What a search returns.

use serde::{Deserialize, Serialize};

/// One product as it appears on a search result page.
///
/// Prices are in the currency of the [`Site`](crate::Site) the page was fetched
/// for, as a plain number of that currency's main unit (yen, dollars, ...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub title: String,
    /// Product page, absolute.
    pub url: String,
    /// Main image, absolute.
    pub image_url: String,
    pub currency: String,
    /// The price shown on the card. For a Yoridori product this is the price
    /// when buying three or more Yoridori products together.
    pub price: f64,
    pub original_price: Option<f64>,
    pub discount_percent: Option<u32>,
    /// Star rating, 0 to 5. Absent on products nobody has rated.
    pub rating: Option<f64>,
    /// Units sold, as AliExpress reports it.
    pub sales: Option<u64>,
    /// Units sold as shown on the card, e.g. `4,000+ 点販売`.
    pub sales_text: Option<String>,
    /// In the "よりどり" (Yoridori) programme: free shipping from three products.
    pub yoridori: bool,
    /// In the "Choice" programme.
    pub choice: bool,
    /// A paid placement rather than a search hit.
    pub ad: bool,
    /// Country the product ships from, as a two letter code.
    pub ship_from: Option<String>,
    pub store_name: Option<String>,
    /// The bulk offer shown on the card, e.g. `3点以上注文で1点あたり142円`.
    pub bulk_offer: Option<String>,
    /// AliExpress' id of the product family this listing belongs to, when the
    /// listing is one of several for the same product.
    pub spu_id: Option<String>,
    /// AliExpress' id of the image set, shared by listings using the same images.
    pub pic_group_id: Option<String>,
}

/// One page of search results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchPage {
    pub page: u32,
    pub page_size: u32,
    pub total_results: u64,
    pub products: Vec<Product>,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A product with only the fields tests care about set; everything else neutral.
    pub(crate) fn product(id: &str) -> Product {
        Product {
            id: id.to_string(),
            title: format!("product {id}"),
            url: format!("https://ja.aliexpress.com/item/{id}.html"),
            image_url: String::new(),
            currency: "JPY".to_string(),
            price: 100.0,
            original_price: None,
            discount_percent: None,
            rating: None,
            sales: None,
            sales_text: None,
            yoridori: false,
            choice: false,
            ad: false,
            ship_from: None,
            store_name: None,
            bulk_offer: None,
            spu_id: None,
            pic_group_id: None,
        }
    }
}
