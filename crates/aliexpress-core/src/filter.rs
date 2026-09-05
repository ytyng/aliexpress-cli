//! Local filtering of products.

use crate::model::Product;

/// Which of the two kinds of product to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Any,
    /// Yoridori products only.
    Yoridori,
    /// Everything but Yoridori products.
    Normal,
}

/// Conditions a product must meet to be shown.
///
/// The price bounds are sent to the server too, but applied here as well: the
/// server matches them against every variant of a product, so a card whose
/// shown price is out of range can still come back.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filter {
    pub kind: Kind,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub min_rating: Option<f64>,
    /// Drop paid placements.
    pub exclude_ads: bool,
}

impl Filter {
    pub fn accepts(&self, product: &Product) -> bool {
        match self.kind {
            Kind::Any => {}
            Kind::Yoridori if !product.yoridori => return false,
            Kind::Normal if product.yoridori => return false,
            _ => {}
        }
        if self.min_price.is_some_and(|min| product.price < min) {
            return false;
        }
        if self.max_price.is_some_and(|max| product.price > max) {
            return false;
        }
        // An unrated product cannot meet a minimum rating: the buyer asked for
        // evidence, and there is none.
        if let Some(min) = self.min_rating
            && !product.rating.is_some_and(|rating| rating >= min)
        {
            return false;
        }
        if self.exclude_ads && product.ad {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::product;

    #[test]
    fn kind_splits_yoridori_from_the_rest() {
        // Arrange
        let yoridori = Product {
            yoridori: true,
            ..product("a")
        };
        let normal = product("b");
        // Act / Assert
        let only = Filter {
            kind: Kind::Yoridori,
            ..Filter::default()
        };
        assert!(only.accepts(&yoridori));
        assert!(!only.accepts(&normal));
        let none = Filter {
            kind: Kind::Normal,
            ..Filter::default()
        };
        assert!(!none.accepts(&yoridori));
        assert!(none.accepts(&normal));
        assert!(Filter::default().accepts(&yoridori));
    }

    #[test]
    fn price_bounds_are_inclusive() {
        // Arrange
        let filter = Filter {
            min_price: Some(100.0),
            max_price: Some(200.0),
            ..Filter::default()
        };
        // Act / Assert
        assert!(filter.accepts(&Product {
            price: 100.0,
            ..product("a")
        }));
        assert!(filter.accepts(&Product {
            price: 200.0,
            ..product("a")
        }));
        assert!(!filter.accepts(&Product {
            price: 99.0,
            ..product("a")
        }));
        assert!(!filter.accepts(&Product {
            price: 201.0,
            ..product("a")
        }));
    }

    #[test]
    fn an_unrated_product_fails_a_minimum_rating() {
        // Arrange
        let filter = Filter {
            min_rating: Some(4.5),
            ..Filter::default()
        };
        // Act / Assert
        assert!(filter.accepts(&Product {
            rating: Some(4.5),
            ..product("a")
        }));
        assert!(!filter.accepts(&Product {
            rating: Some(4.4),
            ..product("a")
        }));
        assert!(!filter.accepts(&Product {
            rating: None,
            ..product("a")
        }));
    }

    #[test]
    fn ads_can_be_dropped() {
        // Arrange
        let filter = Filter {
            exclude_ads: true,
            ..Filter::default()
        };
        // Act / Assert
        assert!(!filter.accepts(&Product {
            ad: true,
            ..product("a")
        }));
        assert!(filter.accepts(&product("a")));
    }
}
