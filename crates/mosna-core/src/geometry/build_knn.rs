//! Port of `tysserand::build_knn` and `tysserand::pairs_from_knn`.

use kiddo::float::kdtree::KdTree;
use kiddo::SquaredEuclidean;

use crate::geometry::remove_duplicate_pairs::remove_duplicate_pairs;
use crate::{Pair, Point2};

/// Connect each node to its `k` nearest neighbours.
///
/// The Python version switches to an approximate `pynndescent` index above
/// 3000 nodes. This always computes the exact neighbours with a k-d tree: for
/// the network sizes MOSNA works with (tens of thousands of cells per sample)
/// an exact query is fast enough, and being exact removes the run-to-run
/// variation the approximate index introduces.
///
/// `pairs_from_knn` in Python pairs `ind[:, 0]` — each point itself, since the
/// query includes the point — with each of its `k` other neighbours, then
/// deduplicates. The same happens here by skipping the self-match.
pub fn build_knn(coords: &[Point2], k: usize) -> Vec<Pair> {
    let n = coords.len();
    if n < 2 || k == 0 {
        return Vec::new();
    }

    let mut tree: KdTree<f64, u32, 2, 32, u32> = KdTree::with_capacity(n);
    for (idx, point) in coords.iter().enumerate() {
        tree.add(point, idx as u32);
    }

    // `k + 1` because the nearest neighbour of a point is the point itself.
    let query_k = (k + 1).min(n);
    let mut pairs = Vec::with_capacity(n * k);
    for (idx, point) in coords.iter().enumerate() {
        let idx = idx as u32;
        for neighbour in tree.nearest_n::<SquaredEuclidean>(point, query_k) {
            if neighbour.item != idx {
                pairs.push((idx, neighbour.item));
            }
        }
    }
    remove_duplicate_pairs(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_node_reaches_its_nearest_neighbour() {
        // Points on a line, unit spacing.
        let coords: Vec<Point2> = (0..5).map(|i| [i as f64, 0.0]).collect();
        let pairs = build_knn(&coords, 1);
        // Each point links to its closest neighbour; after deduplication the
        // consecutive pairs remain.
        assert_eq!(pairs, vec![(0, 1), (1, 2), (2, 3), (3, 4)]);
    }

    #[test]
    fn k_two_adds_the_second_neighbour() {
        let coords: Vec<Point2> = (0..5).map(|i| [i as f64, 0.0]).collect();
        let pairs = build_knn(&coords, 2);
        assert!(pairs.contains(&(0, 2)), "got {pairs:?}");
        assert!(pairs.contains(&(0, 1)));
        assert!(pairs.iter().all(|&(a, b)| a < b));
    }

    #[test]
    fn no_node_is_linked_to_itself() {
        let coords: Vec<Point2> = (0..8).map(|i| [i as f64, (i % 3) as f64]).collect();
        let pairs = build_knn(&coords, 3);
        assert!(pairs.iter().all(|&(a, b)| a != b));
    }

    #[test]
    fn k_larger_than_the_network_is_clamped() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]];
        let pairs = build_knn(&coords, 100);
        // Every possible undirected pair, and no more.
        assert_eq!(pairs, vec![(0, 1), (0, 2), (1, 2)]);
    }

    #[test]
    fn degenerate_inputs_yield_no_edges() {
        assert!(build_knn(&[], 3).is_empty());
        assert!(build_knn(&[[0.0, 0.0]], 3).is_empty());
        assert!(build_knn(&[[0.0, 0.0], [1.0, 0.0]], 0).is_empty());
    }
}
