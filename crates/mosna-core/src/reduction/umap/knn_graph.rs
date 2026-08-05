//! Exact k-nearest-neighbour search.

use rayon::prelude::*;

use crate::reduction::umap::metric::Metric;

/// The `k` nearest neighbours of every point, sorted by increasing distance.
#[derive(Debug, Clone, PartialEq)]
pub struct KnnGraph {
    /// `indices[i]` are the neighbours of point `i`, nearest first.
    pub indices: Vec<Vec<usize>>,
    /// `distances[i]` are the matching distances, ascending.
    pub distances: Vec<Vec<f64>>,
}

impl KnnGraph {
    /// Number of points.
    pub fn n_rows(&self) -> usize {
        self.indices.len()
    }

    /// Neighbours per point, or 0 for an empty graph.
    pub fn k(&self) -> usize {
        self.indices.first().map(Vec::len).unwrap_or(0)
    }
}

/// Find the `k` nearest neighbours of every row of `data` by exhaustive search.
///
/// `k` is clamped to `n_rows - 1`: a point is never its own neighbour, so there
/// are only that many candidates.
///
/// This is exact, and it costs `O(n^2 d)`.
///
/// # The cost is the ceiling on cohort size
///
/// There is no approximate fallback. An earlier version of this comment
/// claimed one — `nn_descent` — taking over above a few thousand points; that
/// module was never written, and the claim survived because `cargo doc` did not
/// gate the build. It is stated plainly instead: at fifty thousand cells this
/// is 2.5 billion distance evaluations per call, and step 3 calls it once for
/// the reduction and once more for the clustering graph.
///
/// Below roughly ten thousand cells the exhaustive search is the right trade —
/// it is exact, it parallelises perfectly, and it has no index to build. Above
/// that, an approximate index (NN-Descent, HNSW) is what makes the step
/// practical, and it would be measured against this routine as its
/// specification.
pub fn knn_graph(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    k: usize,
    metric: Metric,
) -> KnnGraph {
    let k = k.min(n_rows.saturating_sub(1));
    if n_rows == 0 || k == 0 {
        return KnnGraph {
            indices: vec![Vec::new(); n_rows],
            distances: vec![Vec::new(); n_rows],
        };
    }

    let rows: Vec<(Vec<usize>, Vec<f64>)> = (0..n_rows)
        .into_par_iter()
        .map(|i| {
            let point = &data[i * n_features..(i + 1) * n_features];
            // A bounded insertion list beats a heap here: `k` is small (tens),
            // so the linear insert is cheaper than heap bookkeeping, and it
            // keeps the result sorted for free.
            let mut best: Vec<(f64, usize)> = Vec::with_capacity(k + 1);
            let mut worst = f64::INFINITY;

            for j in 0..n_rows {
                if j == i {
                    continue;
                }
                let other = &data[j * n_features..(j + 1) * n_features];
                let rank = metric.rank_distance(point, other);
                if best.len() == k && rank >= worst {
                    continue;
                }
                // Ties keep the lower index, matching a stable sort by
                // (distance, index), so the graph is reproducible.
                let position = best
                    .iter()
                    .position(|&(d, idx)| rank < d || (rank == d && j < idx))
                    .unwrap_or(best.len());
                best.insert(position, (rank, j));
                best.truncate(k);
                worst = best.last().map(|&(d, _)| d).unwrap_or(f64::INFINITY);
            }

            let indices = best.iter().map(|&(_, idx)| idx).collect();
            let distances = best.iter().map(|&(d, _)| metric.from_rank(d)).collect();
            (indices, distances)
        })
        .collect();

    let mut graph = KnnGraph {
        indices: Vec::with_capacity(n_rows),
        distances: Vec::with_capacity(n_rows),
    };
    for (indices, distances) in rows {
        graph.indices.push(indices);
        graph.distances.push(distances);
    }
    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Points on a line at 0, 1, 2, 3, 4.
    fn line() -> Vec<f64> {
        (0..5).map(|i| i as f64).collect()
    }

    #[test]
    fn finds_the_nearest_neighbours_in_order() {
        let data = line();
        let graph = knn_graph(&data, 5, 1, 2, Metric::Euclidean);

        assert_eq!(graph.indices[0], vec![1, 2]);
        assert_eq!(graph.distances[0], vec![1.0, 2.0]);
        // Point 2 is equidistant from 1 and 3; the lower index wins.
        assert_eq!(graph.indices[2], vec![1, 3]);
    }

    #[test]
    fn a_point_is_never_its_own_neighbour() {
        let data = line();
        let graph = knn_graph(&data, 5, 1, 4, Metric::Euclidean);
        for (i, neighbours) in graph.indices.iter().enumerate() {
            assert!(!neighbours.contains(&i));
        }
    }

    #[test]
    fn k_is_clamped_to_the_available_candidates() {
        let data = line();
        let graph = knn_graph(&data, 5, 1, 99, Metric::Euclidean);
        assert!(graph.indices.iter().all(|n| n.len() == 4));
        assert_eq!(graph.k(), 4);
    }

    #[test]
    fn distances_use_the_requested_metric() {
        let data = vec![0.0, 0.0, 3.0, 4.0];
        let euclidean = knn_graph(&data, 2, 2, 1, Metric::Euclidean);
        assert_eq!(euclidean.distances[0], vec![5.0]);

        let manhattan = knn_graph(&data, 2, 2, 1, Metric::Manhattan);
        assert_eq!(manhattan.distances[0], vec![7.0]);
    }

    #[test]
    fn ties_are_broken_by_index_so_the_graph_is_reproducible() {
        // Four points at the same place.
        let data = vec![0.0; 8];
        let first = knn_graph(&data, 4, 2, 2, Metric::Euclidean);
        let second = knn_graph(&data, 4, 2, 2, Metric::Euclidean);
        assert_eq!(first, second);
        assert_eq!(first.indices[3], vec![0, 1]);
    }

    #[test]
    fn degenerate_inputs_produce_an_empty_graph() {
        let empty = knn_graph(&[], 0, 2, 3, Metric::Euclidean);
        assert_eq!(empty.n_rows(), 0);

        let single = knn_graph(&[1.0, 2.0], 1, 2, 3, Metric::Euclidean);
        assert_eq!(single.n_rows(), 1);
        assert!(single.indices[0].is_empty());
    }
}
