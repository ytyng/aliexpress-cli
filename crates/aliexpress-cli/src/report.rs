//! Text output for the terminal.

use std::fmt::Write as _;

use aliexpress_core::{Product, SearchResult, value_score};

/// How much of a title is shown. Titles are keyword soup and rarely say more
/// in their second half.
const TITLE_WIDTH: usize = 90;

/// Renders the result as one block per product.
///
/// One line per product does not fit a title, a price and a URL in a terminal,
/// so each product takes three: the numbers and tags, the title, the URL.
pub fn render(result: &SearchResult, show_score: bool) -> String {
    let mut out = String::new();
    if result.products.is_empty() {
        let _ = writeln!(
            out,
            "No products matched (AliExpress reports {} results; {} fetched).",
            result.total_results, result.fetched
        );
        return out;
    }
    for (index, product) in result.products.iter().enumerate() {
        let _ = writeln!(out, "{}", headline(index + 1, product, show_score));
        let _ = writeln!(out, "    {}", truncate(&plain(&product.title), TITLE_WIDTH));
        let _ = writeln!(out, "    {}", product.url);
    }
    let _ = writeln!(
        out,
        "{} products shown, {} fetched, {} results on AliExpress.",
        result.products.len(),
        result.fetched,
        result.total_results
    );
    out
}

fn headline(number: usize, product: &Product, show_score: bool) -> String {
    let mut line = format!("{number:>3}. {}", price(product));
    if let Some(original) = product.original_price
        && original > product.price
    {
        let _ = write!(line, " (was {})", money(original, &product.currency));
    }
    match product.rating {
        Some(rating) => {
            let _ = write!(line, "  ★{rating:.1}");
        }
        None => line.push_str("  ★-"),
    }
    if let Some(sales) = product.sales {
        let _ = write!(line, "  {sales} sold");
    }
    if show_score {
        let _ = write!(line, "  value {:.3}", value_score(product));
    }
    for tag in tags(product) {
        let _ = write!(line, "  [{tag}]");
    }
    if let Some(offer) = &product.bulk_offer {
        let _ = write!(line, "  {}", plain(offer));
    }
    line
}

/// Text with control characters replaced by spaces.
///
/// Titles and offers are text from AliExpress, written by sellers. An escape
/// sequence in one would be executed by the terminal: clearing the screen,
/// moving the cursor over earlier lines, or worse on terminals that accept
/// clipboard writes.
fn plain(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn price(product: &Product) -> String {
    money(product.price, &product.currency)
}

fn money(amount: f64, currency: &str) -> String {
    if currency == "JPY" {
        format!("¥{}", amount.round() as i64)
    } else if amount.fract() == 0.0 {
        format!("{} {}", amount as i64, currency)
    } else {
        format!("{amount:.2} {currency}")
    }
}

/// The programme tags of a product, in the order they are shown.
pub fn tags(product: &Product) -> Vec<&'static str> {
    let mut tags = Vec::new();
    if product.yoridori {
        tags.push("Yoridori");
    }
    if product.choice {
        tags.push("Choice");
    }
    if product.ad {
        tags.push("Ad");
    }
    tags
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut short: String = text.chars().take(width - 1).collect();
    short.push('…');
    short
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product() -> Product {
        Product {
            id: "1".to_string(),
            title: "USB C cable".to_string(),
            url: "https://ja.aliexpress.com/item/1.html".to_string(),
            image_url: String::new(),
            currency: "JPY".to_string(),
            price: 173.0,
            original_price: Some(367.0),
            discount_percent: Some(52),
            rating: Some(4.8),
            sales: Some(4041),
            sales_text: None,
            yoridori: true,
            choice: true,
            ad: false,
            ship_from: Some("CN".to_string()),
            store_name: None,
            bulk_offer: None,
            spu_id: None,
            pic_group_id: None,
        }
    }

    #[test]
    fn renders_one_block_per_product_and_a_summary() {
        // Arrange
        let result = SearchResult {
            total_results: 100,
            fetched: 60,
            products: vec![product()],
        };
        // Act
        let text = render(&result, false);
        // Assert
        assert_eq!(
            text,
            "  1. ¥173 (was ¥367)  ★4.8  4041 sold  [Yoridori]  [Choice]\n    USB C cable\n    https://ja.aliexpress.com/item/1.html\n1 products shown, 60 fetched, 100 results on AliExpress.\n"
        );
    }

    #[test]
    fn says_so_when_nothing_matched() {
        // Arrange
        let result = SearchResult {
            total_results: 5,
            fetched: 5,
            products: vec![],
        };
        // Act
        let text = render(&result, false);
        // Assert
        assert!(text.starts_with("No products matched"));
    }

    #[test]
    fn money_in_yen_and_in_decimal_currencies() {
        // Arrange / Act / Assert
        assert_eq!(money(173.0, "JPY"), "¥173");
        assert_eq!(money(1.5, "USD"), "1.50 USD");
        assert_eq!(money(2.0, "USD"), "2 USD");
    }

    #[test]
    fn control_characters_never_reach_the_terminal() {
        // Arrange
        let mut hostile = product();
        hostile.title = "clear\u{1b}[2J the screen\n and more".to_string();
        hostile.bulk_offer = Some("offer\u{7}".to_string());
        let result = SearchResult {
            total_results: 1,
            fetched: 1,
            products: vec![hostile],
        };
        // Act
        let text = render(&result, false);
        // Assert
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{7}'));
        assert!(text.contains("clear [2J the screen  and more"));
    }

    #[test]
    fn long_titles_are_cut_with_an_ellipsis() {
        // Arrange
        let title = "あ".repeat(100);
        // Act
        let short = truncate(&title, 10);
        // Assert
        assert_eq!(short.chars().count(), 10);
        assert!(short.ends_with('…'));
    }
}
