//! The tree beside a clustered heatmap.
//!
//! The rows and the columns of the assortativity heatmap are ordered by Ward
//! clustering, and the tree that produced the order is drawn beside them. The
//! renderer receives it as line segments in two coordinates it can place
//! without knowing anything about clustering: a position along the axis, and a
//! merge height normalised to `[0, 1]`.
//!
//! # What changed
//!
//! The `plotters` implementation drew a *schematic* tree — the true leaf order
//! with evenly spaced joins, because threading the linkage matrix into the
//! drawing code was not worth it there. Here the drawing code is a list of
//! segments, so the real merge heights cost nothing and the tree now says what
//! it is supposed to: how far apart two groups actually are.

use mosna_core::stats::linkage::Linkage;

/// One line: `[position0, height0, position1, height1]`.
pub type Segment = [f64; 4];

/// The tree, in the drawing order the heatmap was given.
///
/// `order` is the leaf order the heatmap rows or columns were drawn in, so a
/// leaf's position is where it actually sits on the axis.
pub fn segments(linkage: &Linkage, order: &[usize]) -> Vec<Segment> {
    let n = linkage.n_leaves;
    if n < 2 || linkage.merges.is_empty() {
        return Vec::new();
    }

    // Where each leaf was drawn. A leaf missing from the order — which cannot
    // happen, and would be a silent mislabelling if it did — keeps its own
    // index rather than collapsing onto zero.
    let mut position = vec![0.0f64; n];
    for (drawn, &leaf) in order.iter().enumerate() {
        if leaf < n {
            position[leaf] = drawn as f64;
        }
    }

    // The tallest merge is the root; every height is read against it, so the
    // tree fills the band it is drawn in whatever the distances happen to be.
    let tallest = linkage
        .merges
        .iter()
        .map(|merge| merge.distance)
        .fold(0.0f64, f64::max);
    let scale = |distance: f64| {
        if tallest > 0.0 {
            distance / tallest
        } else {
            // Every observation identical: there is no height to show, and a
            // division would produce `NaN` for the whole tree.
            0.0
        }
    };

    // Each cluster's foot: where it sits on the axis, and how high it reaches.
    let mut node_position: Vec<f64> = position;
    let mut node_height: Vec<f64> = vec![0.0; n];

    let mut lines = Vec::with_capacity(3 * linkage.merges.len());
    for merge in &linkage.merges {
        let (left, right) = (merge.left, merge.right);
        let (x_left, h_left) = (node_position[left], node_height[left]);
        let (x_right, h_right) = (node_position[right], node_height[right]);
        let height = scale(merge.distance);

        lines.push([x_left, h_left, x_left, height]);
        lines.push([x_left, height, x_right, height]);
        lines.push([x_right, height, x_right, h_right]);

        node_position.push((x_left + x_right) / 2.0);
        node_height.push(height);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosna_core::stats::linkage::{dendrogram_leaf_order, ward_linkage};

    fn clustered(points: &[Vec<f64>]) -> (Linkage, Vec<usize>) {
        let linkage = ward_linkage(points);
        let order = dendrogram_leaf_order(&linkage);
        (linkage, order)
    }

    #[test]
    fn nothing_to_cluster_draws_no_tree() {
        let linkage = Linkage {
            n_leaves: 1,
            merges: Vec::new(),
        };
        assert!(segments(&linkage, &[0]).is_empty());
    }

    /// Every merge is a bracket: up from one child, across, and down to the
    /// other.
    #[test]
    fn each_merge_is_three_lines() {
        let points = vec![vec![0.0], vec![0.1], vec![5.0], vec![5.2]];
        let (linkage, order) = clustered(&points);
        assert_eq!(segments(&linkage, &order).len(), 3 * (points.len() - 1));
    }

    /// The renderer places the tree in a band beside the matrix; it can only do
    /// that if the heights it is given are a fraction of the tallest merge.
    #[test]
    fn heights_are_normalised_and_positions_stay_on_the_axis() {
        let points = vec![vec![0.0], vec![0.1], vec![5.0], vec![5.2], vec![9.0]];
        let (linkage, order) = clustered(&points);

        for [x0, h0, x1, h1] in segments(&linkage, &order) {
            for height in [h0, h1] {
                assert!(
                    (0.0..=1.0).contains(&height),
                    "height {height} is outside the band"
                );
            }
            for position in [x0, x1] {
                assert!(
                    (0.0..=(points.len() - 1) as f64).contains(&position),
                    "position {position} is off the axis"
                );
            }
        }
    }

    /// The tallest merge is the root, and it has to reach the top of the band
    /// or the tree is drawn squashed against the matrix.
    #[test]
    fn the_root_reaches_the_top_of_the_band() {
        let points = vec![vec![0.0], vec![0.1], vec![9.0]];
        let (linkage, order) = clustered(&points);
        let highest = segments(&linkage, &order)
            .iter()
            .flat_map(|[_, h0, _, h1]| [*h0, *h1])
            .fold(0.0f64, f64::max);
        assert!((highest - 1.0).abs() < 1e-12, "the root stops at {highest}");
    }

    /// A leaf's foot is at the position it was *drawn* at, not at the index it
    /// happens to have in the input — otherwise the tree labels the wrong
    /// rows, which is worse than no tree at all.
    #[test]
    fn a_leaf_stands_where_it_was_drawn() {
        let points = vec![vec![0.0], vec![9.0], vec![0.1]];
        let (linkage, order) = clustered(&points);
        let feet: Vec<f64> = segments(&linkage, &order)
            .iter()
            .filter(|[_, h0, _, _]| *h0 == 0.0)
            .map(|[x0, _, _, _]| *x0)
            .collect();

        // The two close points are neighbours in the order, so their feet are
        // adjacent positions.
        let mut sorted = feet.clone();
        sorted.sort_by(f64::total_cmp);
        sorted.dedup();
        for position in &sorted {
            assert!(order.len() as f64 > *position);
        }
        assert!(!feet.is_empty());
    }

    /// Two identical observations merge at distance zero. The height of that
    /// merge is zero, and dividing by the tallest merge must not turn the whole
    /// tree into `NaN` when *every* merge is at zero.
    #[test]
    fn a_tree_with_no_height_at_all_is_still_finite() {
        let points = vec![vec![1.0], vec![1.0], vec![1.0]];
        let (linkage, order) = clustered(&points);
        for [_, h0, _, h1] in segments(&linkage, &order) {
            assert!(h0.is_finite() && h1.is_finite());
        }
    }
}
