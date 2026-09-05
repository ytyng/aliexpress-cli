//! Ranking by value for money.

use crate::model::Product;

/// How good a deal a product is: higher is better.
///
/// `(rating / 5)² × ln(1 + sales) / price`. A cheap product with a high rating and
/// many sales scores highest; the rating is squared so that a 4.0 is noticeably
/// behind a 4.8, and sales are logarithmic so that a product with a million
/// sales does not swamp everything else. A product with no rating or no sales
/// scores 0: there is no evidence it is any good, so it ranks last.
pub fn value_score(product: &Product) -> f64 {
    let (Some(rating), Some(sales)) = (product.rating, product.sales) else {
        return 0.0;
    };
    if product.price <= 0.0 {
        return 0.0;
    }
    let quality = (rating / 5.0).clamp(0.0, 1.0);
    quality * quality * (1.0 + sales as f64).ln() / product.price
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::product;

    #[test]
    fn cheaper_better_rated_and_more_sold_scores_higher() {
        // Arrange
        let base = Product {
            price: 100.0,
            rating: Some(4.5),
            sales: Some(1000),
            ..product("a")
        };
        let cheaper = Product {
            price: 50.0,
            ..base.clone()
        };
        let better = Product {
            rating: Some(4.9),
            ..base.clone()
        };
        let popular = Product {
            sales: Some(100_000),
            ..base.clone()
        };
        // Act / Assert
        assert!(value_score(&cheaper) > value_score(&base));
        assert!(value_score(&better) > value_score(&base));
        assert!(value_score(&popular) > value_score(&base));
    }

    #[test]
    fn no_evidence_means_no_score() {
        // Arrange / Act / Assert
        assert_eq!(
            value_score(&Product {
                rating: None,
                sales: Some(10),
                ..product("a")
            }),
            0.0
        );
        assert_eq!(
            value_score(&Product {
                rating: Some(5.0),
                sales: None,
                ..product("a")
            }),
            0.0
        );
        assert_eq!(
            value_score(&Product {
                price: 0.0,
                rating: Some(5.0),
                sales: Some(10),
                ..product("a")
            }),
            0.0
        );
    }
}
