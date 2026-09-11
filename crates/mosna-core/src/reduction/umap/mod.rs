//! UMAP — Uniform Manifold Approximation and Projection.
//!
//! Replaces the `umap-learn` call in `mosna/clustering.py::get_reducer`. The
//! method has four stages:
//!
//! 1. [`fn@umap_neighbours`] — the `k` nearest neighbours of every point:
//!    [`fn@knn_graph`] computes them exactly while the input is small, and
//!    [`fn@nn_descent`] approximates them above the size at which umap-learn
//!    stops computing every distance.
//! 2. [`fn@smooth_knn_dist`] — a per-point bandwidth, so that neighbourhoods are
//!    comparable across regions of different density.
//! 3. [`fn@fuzzy_simplicial_set`] — a weighted, symmetrised graph.
//! 4. [`fn@optimize_layout`] — gradient descent placing the points in the
//!    low-dimensional space.
//!
//! # Relation to the Python
//!
//! `get_reducer` passes `random_state=None`, so umap-learn seeds itself from
//! the OS and parallelises its layout loop; two Python runs on identical input
//! give different embeddings, and therefore different niche labels. There is no
//! reference output to match. This implementation is deterministic instead:
//! same input and seed, same embedding, whatever the machine or thread count.

pub mod find_ab_params;
pub mod fuzzy_simplicial_set;
pub mod init_layout;
pub mod knn_graph;
pub mod metric;
pub mod nn_descent;
pub mod optimize_layout;
pub mod smooth_knn_dist;

pub use find_ab_params::find_ab_params;
pub use fuzzy_simplicial_set::fuzzy_simplicial_set;
pub use init_layout::init_layout;
pub use knn_graph::{knn_graph, KnnGraph};
pub use metric::Metric;
pub use nn_descent::{cluster_neighbours, nn_descent, umap_neighbours};
pub use optimize_layout::optimize_layout;
pub use smooth_knn_dist::smooth_knn_dist;

use crate::error::{CoreError, Result};

/// The arithmetic the reduction runs in.
///
/// # Why single precision
///
/// umap-learn is `float32` from end to end, and this is the stage that mirrors
/// it: `check_array(X, dtype=np.float32)` on the input matrix, `rho` and
/// `sigma` as `np.zeros(..., dtype=np.float32)`, the fuzzy graph's weights as
/// `vals = np.zeros(knn_indices.size, dtype=np.float32)`, `knn_dists` cast
/// before use, and an embedding that stays `float32` throughout its gradient
/// descent. Running the port in `f64` did not merely differ from the reference
/// in its algorithms — it differed in its arithmetic, and it was the port that
/// was out of step.
///
/// Nothing is lost. The matrix this reduces is the NAS feature table: means and
/// standard deviations of one-hot indicators over a Delaunay neighbourhood of
/// six or seven cells, so values in `[0, 1]` carrying three significant decimal
/// digits. Single precision carries seven.
///
/// # What stays in double precision
///
/// The boundary is this module. The NAS features, the niche composition and
/// everything written to disk as a result remain `f64`: they are the analysis'
/// output, not coordinates on the way to a clustering. Two accumulators inside
/// the stage stay wide as well, each for a stated reason — the cohort-wide mean
/// in [`fn@smooth_knn_dist`] and the covariance in [`fn@init_layout`] — and
/// [`fn@find_ab_params`] is fitted in `f64` the way `scipy.optimize.curve_fit`
/// is.
pub type Scalar = f32;

/// Settings of a UMAP run.
///
/// Defaults match umap-learn's, so a configuration that does not mention a
/// parameter behaves the way the Python did.
#[derive(Debug, Clone, PartialEq)]
pub struct UmapParams {
    /// Dimensionality of the embedding.
    pub n_components: usize,
    /// Size of the local neighbourhood; the balance between local and global
    /// structure.
    pub n_neighbors: usize,
    pub metric: Metric,
    /// How tightly points may pack together in the embedding.
    pub min_dist: f64,
    /// Scale of the embedded distances.
    pub spread: f64,
    /// Layout iterations. `0` selects umap-learn's rule: 500 for small inputs,
    /// 200 above ten thousand points.
    pub n_epochs: usize,
    pub learning_rate: f64,
    /// Negative samples drawn per positive one.
    pub negative_sample_rate: usize,
    /// Weight of the repulsive term.
    pub repulsion_strength: f64,
    /// Number of neighbours guaranteed a full-strength connection.
    pub local_connectivity: f64,
    pub seed: u64,
}

impl Default for UmapParams {
    fn default() -> Self {
        Self {
            n_components: 2,
            n_neighbors: 15,
            metric: Metric::Euclidean,
            min_dist: 0.1,
            spread: 1.0,
            n_epochs: 0,
            learning_rate: 1.0,
            negative_sample_rate: 5,
            repulsion_strength: 1.0,
            local_connectivity: 1.0,
            seed: 42,
        }
    }
}

impl UmapParams {
    /// Layout iterations for a dataset of `n_rows` points.
    fn epochs(&self, n_rows: usize) -> usize {
        if self.n_epochs > 0 {
            self.n_epochs
        } else if n_rows <= 10_000 {
            500
        } else {
            200
        }
    }
}

/// Embed `data` into `params.n_components` dimensions.
///
/// `data` is row-major, `n_rows` by `n_features`. The result is row-major,
/// `n_rows` by `n_components`.
///
/// Both are [`Scalar`]: this is where the caller's `f64` feature matrix is
/// narrowed and where its coordinates come back. The settings stay `f64`
/// because they come from the configuration file, and are narrowed at the one
/// place each is used.
pub fn umap(
    data: &[Scalar],
    n_rows: usize,
    n_features: usize,
    params: &UmapParams,
) -> Result<Vec<Scalar>> {
    if data.len() != n_rows * n_features {
        return Err(CoreError::shape(format!(
            "data has {} values, expected {n_rows} x {n_features}",
            data.len()
        )));
    }
    if params.n_components == 0 {
        return Err(CoreError::invalid("n_components must be at least 1"));
    }
    if !data.iter().all(|v| v.is_finite()) {
        return Err(CoreError::numeric(
            "umap",
            "the input contains a non-finite value",
        ));
    }

    let mut embedding = init_layout(data, n_rows, n_features, params.n_components, params.seed);

    // Below three points there is no neighbourhood structure to model, and the
    // initial layout is already the answer.
    if n_rows < 3 {
        return Ok(embedding);
    }

    let k = params.n_neighbors.max(2).min(n_rows - 1);
    let graph = umap_neighbours(data, n_rows, n_features, k, params.metric, params.seed);
    let edges = fuzzy_simplicial_set(&graph, n_rows, params.local_connectivity as Scalar);

    // Fitted in `f64`, as `scipy.optimize.curve_fit` is, and applied in single
    // precision like the rest of the layout.
    let (a, b) = find_ab_params(params.spread, params.min_dist);
    let (a, b) = (a as Scalar, b as Scalar);

    optimize_layout(
        &mut embedding,
        n_rows,
        params.n_components,
        &edges,
        params.epochs(n_rows),
        a,
        b,
        params.learning_rate as Scalar,
        params.negative_sample_rate,
        params.repulsion_strength as Scalar,
        params.seed,
    );

    Ok(embedding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_mismatched_shape() {
        let err = umap(&[1.0, 2.0, 3.0], 2, 2, &UmapParams::default()).unwrap_err();
        assert!(err.to_string().contains("expected 2 x 2"));
    }

    #[test]
    fn rejects_non_finite_input() {
        let data = vec![1.0, Scalar::NAN, 3.0, 4.0];
        let err = umap(&data, 2, 2, &UmapParams::default()).unwrap_err();
        assert!(err.to_string().contains("non-finite"));
    }

    #[test]
    fn rejects_a_zero_dimensional_embedding() {
        let params = UmapParams {
            n_components: 0,
            ..Default::default()
        };
        assert!(umap(&[1.0, 2.0], 1, 2, &params).is_err());
    }

    #[test]
    fn the_epoch_schedule_follows_the_umap_learn_rule() {
        let auto = UmapParams::default();
        assert_eq!(auto.epochs(500), 500);
        assert_eq!(auto.epochs(50_000), 200);

        let explicit = UmapParams {
            n_epochs: 37,
            ..Default::default()
        };
        assert_eq!(explicit.epochs(50_000), 37);
    }

    #[test]
    fn n_neighbors_is_clamped_to_the_dataset() {
        // Four points, fifteen neighbours requested: must not panic.
        let data: Vec<Scalar> = (0..8).map(|i| i as Scalar).collect();
        let embedding = umap(&data, 4, 2, &UmapParams::default()).unwrap();
        assert_eq!(embedding.len(), 8);
        assert!(embedding.iter().all(|v| v.is_finite()));
    }
}
