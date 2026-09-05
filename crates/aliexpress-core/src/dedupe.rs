//! Collapsing listings of the same product.
//!
//! A search for a popular product returns the same thing from many sellers,
//! often with the same photos and a near-identical title. Two listings are taken
//! to be the same product when AliExpress itself groups them (a shared `spu_id`
//! or image set), or failing that when their titles are nearly the same.

use std::collections::HashSet;

use crate::model::Product;
use crate::score::value_score;

/// How alike two titles have to be to count as the same product, as the
/// Jaccard index of their character bigrams. Chosen by eye on cable searches:
/// 0.5 merged different lengths of the same cable, 0.7 kept copies apart.
const TITLE_SIMILARITY: f64 = 0.6;

/// Keeps one listing per product: the one with the best [`value_score`], or the
/// first one when scores tie. The result is in the order the *groups* first
/// appeared, which is not the order of the kept listings: a group's
/// representative can be a listing that came later than the group's first.
/// Callers that need a particular order sort afterwards.
pub fn dedupe(products: Vec<Product>) -> Vec<Product> {
    let mut groups: Vec<Group> = Vec::new();
    for (position, product) in products.into_iter().enumerate() {
        let bigrams = title_bigrams(&product.title);
        let matching: Vec<usize> = groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.holds(&product, &bigrams))
            .map(|(index, _)| index)
            .collect();
        let Some((&first, rest)) = matching.split_first() else {
            groups.push(Group::new(product, position, bigrams));
            continue;
        };
        // A listing can bridge groups that had nothing in common until now (one
        // shares its family id, another its images). They are one product, so
        // the later groups fold into the first.
        for &index in rest.iter().rev() {
            let group = groups.remove(index);
            groups[first].absorb(group);
        }
        groups[first].offer(product, position, bigrams);
    }
    groups.into_iter().map(|group| group.best).collect()
}

struct Group {
    best: Product,
    best_score: f64,
    /// Where `best` was in the input. Ties go to the earlier listing, and after
    /// groups merge the earlier one is not always the one already held.
    best_position: usize,
    spu_ids: HashSet<String>,
    pic_group_ids: HashSet<String>,
    titles: Vec<HashSet<[char; 2]>>,
}

impl Group {
    fn new(product: Product, position: usize, bigrams: HashSet<[char; 2]>) -> Self {
        let mut group = Group {
            best_score: value_score(&product),
            best_position: position,
            best: product,
            spu_ids: HashSet::new(),
            pic_group_ids: HashSet::new(),
            titles: Vec::new(),
        };
        group.remember_best(bigrams);
        group
    }

    fn holds(&self, product: &Product, bigrams: &HashSet<[char; 2]>) -> bool {
        if product
            .spu_id
            .as_ref()
            .is_some_and(|id| self.spu_ids.contains(id))
        {
            return true;
        }
        if product
            .pic_group_id
            .as_ref()
            .is_some_and(|id| self.pic_group_ids.contains(id))
        {
            return true;
        }
        self.titles
            .iter()
            .any(|title| jaccard(title, bigrams) >= TITLE_SIMILARITY)
    }

    /// Adds a listing to the group, keeping it as the representative if it is
    /// the better deal.
    fn offer(&mut self, product: Product, position: usize, bigrams: HashSet<[char; 2]>) {
        self.remember(&product, bigrams);
        let score = value_score(&product);
        if self.beaten_by(score, position) {
            self.best = product;
            self.best_score = score;
            self.best_position = position;
        }
    }

    /// Takes over another group's listings and identifiers.
    fn absorb(&mut self, other: Group) {
        self.spu_ids.extend(other.spu_ids);
        self.pic_group_ids.extend(other.pic_group_ids);
        self.titles.extend(other.titles);
        if self.beaten_by(other.best_score, other.best_position) {
            self.best = other.best;
            self.best_score = other.best_score;
            self.best_position = other.best_position;
        }
    }

    /// Whether a listing with this score, at this position in the input, should
    /// replace the representative: a better score, or the same score earlier on.
    fn beaten_by(&self, score: f64, position: usize) -> bool {
        score > self.best_score || (score == self.best_score && position < self.best_position)
    }

    fn remember(&mut self, product: &Product, bigrams: HashSet<[char; 2]>) {
        if let Some(id) = &product.spu_id {
            self.spu_ids.insert(id.clone());
        }
        if let Some(id) = &product.pic_group_id {
            self.pic_group_ids.insert(id.clone());
        }
        self.titles.push(bigrams);
    }

    fn remember_best(&mut self, bigrams: HashSet<[char; 2]>) {
        if let Some(id) = &self.best.spu_id {
            self.spu_ids.insert(id.clone());
        }
        if let Some(id) = &self.best.pic_group_id {
            self.pic_group_ids.insert(id.clone());
        }
        self.titles.push(bigrams);
    }
}

/// Character bigrams of a title, lower-cased and without punctuation or spaces.
///
/// Bigrams rather than words because Japanese titles have no spaces to split on,
/// and they work just as well for the English ones.
fn title_bigrams(title: &str) -> HashSet<[char; 2]> {
    let chars: Vec<char> = title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    chars.windows(2).map(|pair| [pair[0], pair[1]]).collect()
}

fn jaccard(a: &HashSet<[char; 2]>, b: &HashSet<[char; 2]>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.len() + b.len() - intersection;
    intersection as f64 / union as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::product;

    #[test]
    fn listings_sharing_a_family_id_collapse_to_the_best_deal() {
        // Arrange
        let dear = Product {
            price: 300.0,
            rating: Some(4.8),
            sales: Some(1000),
            spu_id: Some("s1".into()),
            ..product("a")
        };
        let cheap = Product {
            price: 100.0,
            rating: Some(4.8),
            sales: Some(1000),
            spu_id: Some("s1".into()),
            ..product("b")
        };
        let other = Product {
            spu_id: Some("s2".into()),
            title: "completely different thing".into(),
            ..product("c")
        };
        // Act
        let kept = dedupe(vec![dear.clone(), cheap.clone(), other.clone()]);
        // Assert
        assert_eq!(kept, vec![cheap, other]);
    }

    #[test]
    fn listings_sharing_images_collapse() {
        // Arrange
        let first = Product {
            pic_group_id: Some("p1".into()),
            title: "USB cable".into(),
            ..product("a")
        };
        let second = Product {
            pic_group_id: Some("p1".into()),
            title: "phone case".into(),
            ..product("b")
        };
        // Act
        let kept = dedupe(vec![first.clone(), second]);
        // Assert
        assert_eq!(kept, vec![first]);
    }

    #[test]
    fn near_identical_titles_collapse_and_different_ones_do_not() {
        // Arrange
        let a = Product {
            title: "Baseus 100W USB C to USB C Cable PD Fast Charging for iPhone 15".into(),
            ..product("a")
        };
        let a_copy = Product {
            title: "Baseus 100W USB C to USB C Cable PD Fast Charging for iPhone 15 16".into(),
            ..product("b")
        };
        let b = Product {
            title: "Toocki 3 in 1 Retractable Car Charger Cable Lightning Micro".into(),
            ..product("c")
        };
        // Act
        let kept = dedupe(vec![a.clone(), a_copy, b.clone()]);
        // Assert
        assert_eq!(kept, vec![a, b]);
    }

    #[test]
    fn ties_keep_the_first_listing() {
        // Arrange
        let first = Product {
            spu_id: Some("s".into()),
            ..product("a")
        };
        let second = Product {
            spu_id: Some("s".into()),
            ..product("b")
        };
        // Act
        let kept = dedupe(vec![first.clone(), second]);
        // Assert
        assert_eq!(kept, vec![first]);
    }

    #[test]
    fn a_listing_bridging_two_groups_merges_them() {
        // Arrange: A and B share nothing; C shares a family id with A and images with B.
        let a = Product {
            spu_id: Some("s1".into()),
            title: "aaaa".into(),
            ..product("a")
        };
        let b = Product {
            pic_group_id: Some("p2".into()),
            title: "bbbb".into(),
            ..product("b")
        };
        let c = Product {
            spu_id: Some("s1".into()),
            pic_group_id: Some("p2".into()),
            title: "cccc".into(),
            rating: Some(5.0),
            sales: Some(100),
            ..product("c")
        };
        // Act
        let kept = dedupe(vec![a, b, c.clone()]);
        // Assert
        assert_eq!(kept, vec![c]);
    }

    #[test]
    fn a_merge_keeps_the_earliest_of_tied_representatives() {
        // Arrange: A, B and C are separate; B and C tie and beat A; D bridges all three.
        let a = Product {
            spu_id: Some("s1".into()),
            title: "aaaa".into(),
            ..product("a")
        };
        let b = Product {
            spu_id: Some("s2".into()),
            title: "bbbb".into(),
            rating: Some(5.0),
            sales: Some(10),
            ..product("b")
        };
        let c = Product {
            spu_id: Some("s3".into()),
            title: "cccc".into(),
            rating: Some(5.0),
            sales: Some(10),
            ..product("c")
        };
        let d = Product {
            spu_id: Some("s1".into()),
            pic_group_id: Some("p".into()),
            title: "dddd".into(),
            ..product("d")
        };
        let b_bridge = Product {
            spu_id: Some("s2".into()),
            pic_group_id: Some("p".into()),
            title: "eeee".into(),
            ..product("e")
        };
        let c_bridge = Product {
            spu_id: Some("s3".into()),
            pic_group_id: Some("p".into()),
            title: "ffff".into(),
            ..product("f")
        };
        // Act
        let kept = dedupe(vec![a, b.clone(), c, d, b_bridge, c_bridge]);
        // Assert
        assert_eq!(kept, vec![b]);
    }

    #[test]
    fn a_zero_group_id_is_not_a_group() {
        // Arrange: parse never produces "0", but the rule is worth pinning.
        let a = Product {
            pic_group_id: None,
            title: "aaaa bbbb".into(),
            ..product("a")
        };
        let b = Product {
            pic_group_id: None,
            title: "cccc dddd".into(),
            ..product("b")
        };
        // Act
        let kept = dedupe(vec![a, b]);
        // Assert
        assert_eq!(kept.len(), 2);
    }
}
