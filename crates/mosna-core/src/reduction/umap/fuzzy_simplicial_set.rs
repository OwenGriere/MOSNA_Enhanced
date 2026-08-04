//! The weighted graph UMAP optimises the layout of.

use std::collections::HashMap;

use crate::reduction::umap::knn_graph::KnnGraph;
use crate::reduction::umap::smooth_knn_dist::smooth_knn_dist;

/// Build the symmetrised fuzzy neighbourhood graph.
///
/// Each point contributes a directed membership to each of its neighbours,
///
/// ```text
/// w_ij = exp( -max(d_ij - rho_i, 0) / sigma_i )
/// ```
///
/// and the two directions are merged with the probabilistic t-conorm
///
/// ```text
/// w = w_ij + w_ji - w_ij * w_ji
/// ```
///
/// which is the union of the two fuzzy sets. That keeps the result in `(0, 1]`
/// and makes an edge strong when *either* endpoint considers the other a close
/// neighbour — the asymmetry of a k-nearest-neighbour graph would otherwise
/// leave points in dense regions weakly attached.
///
/// Returned as a deduplicated edge list `(a, b, weight)` with `a < b`.
pub fn fuzzy_simplicial_set(
    graph: &KnnGraph,
    n_rows: usize,
    local_connectivity: f64,
) -> Vec<(usize, usize, f64)> {
    let (rho, sigma) = smooth_knn_dist(&graph.distances, local_connectivity);

    // Directed memberships, keyed by the unordered pair.
    let mut merged: HashMap<(usize, usize), (f64, f64)> = HashMap::new();

    for i in 0..n_rows.min(graph.indices.len()) {
        for (slot, &j) in graph.indices[i].iter().enumerate() {
            if i == j {
                continue;
            }
            let d = graph.distances[i][slot];
            let weight = (-(d - rho[i]).max(0.0) / sigma[i]).exp();

            let (key, forward) = if i < j {
                ((i, j), true)
            } else {
                ((j, i), false)
            };
            let entry = merged.entry(key).or_insert((0.0, 0.0));
            if forward {
                entry.0 = weight;
            } else {
                entry.1 = weight;
            }
        }
    }

    let mut edges: Vec<(usize, usize, f64)> = merged
        .into_iter()
        .map(|((a, b), (forward, backward))| (a, b, forward + backward - forward * backward))
        .filter(|&(_, _, w)| w > 0.0)
        .collect();

    // Sorted so the edge list — and therefore the SGD's visit order — does not
    // depend on the hash map's iteration order.
    edges.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reduction::umap::knn_graph::knn_graph;
    use crate::reduction::umap::metric::Metric;

    fn line_graph(n: usize, k: usize) -> (KnnGraph, usize) {
        let data: Vec<f64> = (0..n).map(|i| i as f64).collect();
        (knn_graph(&data, n, 1, k, Metric::Euclidean), n)
    }

    #[test]
    fn weights_stay_in_the_unit_interval() {
        let (graph, n) = line_graph(20, 5);
        for &(_, _, w) in &fuzzy_simplicial_set(&graph, n, 1.0) {
            assert!(w > 0.0 && w <= 1.0 + 1e-12, "weight {w}");
        }
    }

    #[test]
    fn each_pair_appears_exactly_once_with_a_le_b() {
        let (graph, n) = line_graph(20, 5);
        let edges = fuzzy_simplicial_set(&graph, n, 1.0);

        let mut seen = std::collections::HashSet::new();
        for &(a, b, _) in &edges {
            assert!(a < b, "pair ({a}, {b}) is not ordered");
            assert!(seen.insert((a, b)), "pair ({a}, {b}) is duplicated");
        }
    }

    #[test]
    fn every_point_keeps_at_least_one_edge() {
        let (graph, n) = line_graph(30, 4);
        let edges = fuzzy_simplicial_set(&graph, n, 1.0);

        let mut connected = vec![false; n];
        for &(a, b, _) in &edges {
            connected[a] = true;
            connected[b] = true;
        }
        assert!(connected.iter().all(|&c| c));
    }

    /// A mutual nearest-neighbour pair reaches weight 1: both directions are
    /// at full strength, and `1 + 1 - 1 = 1`.
    #[test]
    fn a_mutual_nearest_pair_has_full_weight() {
        let (graph, n) = line_graph(6, 2);
        let edges = fuzzy_simplicial_set(&graph, n, 1.0);
        let (_, _, w) = edges.iter().find(|&&(a, b, _)| a == 0 && b == 1).unwrap();
        assert!((w - 1.0).abs() < 1e-12, "weight {w}");
    }

    #[test]
    fn the_edge_list_is_sorted_and_reproducible() {
        let (graph, n) = line_graph(25, 5);
        let first = fuzzy_simplicial_set(&graph, n, 1.0);
        let second = fuzzy_simplicial_set(&graph, n, 1.0);
        assert_eq!(first, second);
        assert!(first
            .windows(2)
            .all(|w| (w[0].0, w[0].1) < (w[1].0, w[1].1)));
    }

    #[test]
    fn an_empty_graph_yields_no_edges() {
        let graph = KnnGraph {
            indices: vec![Vec::new()],
            distances: vec![Vec::new()],
        };
        assert!(fuzzy_simplicial_set(&graph, 1, 1.0).is_empty());
    }
}
