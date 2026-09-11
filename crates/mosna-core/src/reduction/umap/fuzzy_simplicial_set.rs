//! The weighted graph UMAP optimises the layout of.

use crate::reduction::umap::Scalar;

use rayon::prelude::*;

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
///
/// # Merging by sorting rather than by hashing
///
/// The two directions of a pair used to be brought together in a
/// `HashMap<(usize, usize), (Scalar, Scalar)>`. That map holds one entry per edge —
/// on a twelve-million-cell cohort with `k = 20`, upwards of a hundred and
/// seventy million of them, six gigabytes of table before the edge list itself
/// exists, and briefly twice that whenever it outgrows its capacity and
/// rehashes. Every insertion is also a random probe into a structure far larger
/// than any cache.
///
/// The directed memberships are collected into two flat vectors instead, sorted
/// and walked in step. The peak is the vectors themselves, the access pattern is
/// sequential, and the sort is the one this function already had to perform at
/// the end to make its output independent of the map's iteration order.
pub fn fuzzy_simplicial_set(
    graph: &KnnGraph,
    n_rows: usize,
    local_connectivity: Scalar,
) -> Vec<(usize, usize, Scalar)> {
    let (rho, sigma) = smooth_knn_dist(&graph.distances, local_connectivity);

    // A membership is `forward` when it was contributed by the lower-numbered
    // endpoint of the pair. Split on that here so the merge below is a walk
    // along two sorted runs rather than a lookup per entry.
    let rows = n_rows.min(graph.indices.len());
    let mut forward: Vec<(usize, usize, Scalar)> = Vec::new();
    let mut backward: Vec<(usize, usize, Scalar)> = Vec::new();

    for i in 0..rows {
        for (slot, &j) in graph.indices[i].iter().enumerate() {
            if i == j {
                continue;
            }
            let d = graph.distances[i][slot];
            let weight = (-(d - rho[i]).max(0.0) / sigma[i]).exp();

            if i < j {
                forward.push((i, j, weight));
            } else {
                backward.push((j, i, weight));
            }
        }
    }

    // A pair occurs at most once in each vector — a row lists a neighbour once —
    // so the key is unique within each and an unstable sort is deterministic.
    forward.par_sort_unstable_by_key(|&(a, b, _)| (a, b));
    backward.par_sort_unstable_by_key(|&(a, b, _)| (a, b));

    let mut edges: Vec<(usize, usize, Scalar)> =
        Vec::with_capacity(forward.len().max(backward.len()));
    let (mut f, mut b) = (0usize, 0usize);
    while f < forward.len() || b < backward.len() {
        // Whichever pair comes first, taking both weights when they agree.
        let (a, z, weight_forward, weight_backward) = match (forward.get(f), backward.get(b)) {
            (Some(&(fa, fb, fw)), Some(&(ba, bb, bw))) => match (fa, fb).cmp(&(ba, bb)) {
                std::cmp::Ordering::Less => {
                    f += 1;
                    (fa, fb, fw, 0.0)
                }
                std::cmp::Ordering::Greater => {
                    b += 1;
                    (ba, bb, 0.0, bw)
                }
                std::cmp::Ordering::Equal => {
                    f += 1;
                    b += 1;
                    (fa, fb, fw, bw)
                }
            },
            (Some(&(fa, fb, fw)), None) => {
                f += 1;
                (fa, fb, fw, 0.0)
            }
            (None, Some(&(ba, bb, bw))) => {
                b += 1;
                (ba, bb, 0.0, bw)
            }
            (None, None) => unreachable!("the loop condition holds one of them"),
        };

        let weight = weight_forward + weight_backward - weight_forward * weight_backward;
        if weight > 0.0 {
            edges.push((a, z, weight));
        }
    }

    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reduction::umap::knn_graph::knn_graph;
    use crate::reduction::umap::metric::Metric;

    fn line_graph(n: usize, k: usize) -> (KnnGraph, usize) {
        let data: Vec<Scalar> = (0..n).map(|i| i as Scalar).collect();
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
