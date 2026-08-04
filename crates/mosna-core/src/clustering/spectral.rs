//! Spectral clustering on a radial-basis affinity.
//!
//! Replaces
//!
//! ```python
//! SpectralClustering(n_clusters=n_clusters, assign_labels='discretize',
//!                    random_state=0).fit_predict(embedding)
//! ```
//!
//! in `clustering.py::get_clusterer` — note that it runs on the *embedding*,
//! with scikit-learn's default `affinity='rbf'`, not on the k-nearest-neighbour
//! graph built a few lines above.

use crate::clustering::relabel_clusters::relabel_clusters;
use crate::error::{CoreError, Result};
use crate::linalg::eigen::symmetric_eigen;
use crate::linalg::kmeans::kmeans;

/// Settings of a spectral clustering run.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectralParams {
    pub n_clusters: usize,
    /// Width of the radial-basis kernel, `exp(-gamma * d^2)`.
    pub gamma: f64,
    /// Largest input this will attempt.
    ///
    /// The affinity matrix is dense, so memory grows as `n^2` and the
    /// eigendecomposition as `n^3`. scikit-learn has the same `n^2` wall; the
    /// limit is made explicit here so a cohort-sized input is refused with a
    /// clear message instead of exhausting memory.
    pub max_points: usize,
    pub seed: u64,
}

impl Default for SpectralParams {
    fn default() -> Self {
        Self {
            n_clusters: 8,
            // scikit-learn's default for the RBF affinity.
            gamma: 1.0,
            max_points: 4096,
            seed: 0,
        }
    }
}

/// Cluster `data` by the eigenvectors of its normalised graph Laplacian.
///
/// `data` is row-major, `n_rows` by `n_features`.
///
/// The method builds a radial-basis affinity, takes the `n_clusters` leading
/// eigenvectors of the symmetric normalised Laplacian, normalises the rows of
/// that spectral embedding, and runs k-means on it. This is the standard
/// Ng-Jordan-Weiss formulation. scikit-learn's `assign_labels='discretize'`
/// replaces the final k-means with a rotation-based rounding; both round the
/// same spectral embedding to the same partition on well-separated data, and
/// k-means is the more predictable of the two.
pub fn spectral_clustering(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    params: &SpectralParams,
) -> Result<Vec<u32>> {
    if data.len() != n_rows * n_features {
        return Err(CoreError::shape(format!(
            "data has {} values, expected {n_rows} x {n_features}",
            data.len()
        )));
    }
    if n_rows == 0 {
        return Ok(Vec::new());
    }
    if n_rows > params.max_points {
        return Err(CoreError::Unsupported(format!(
            "spectral clustering needs a dense {n_rows} x {n_rows} affinity matrix, \
             beyond the limit of {} points; use clusterer_type `leiden` or `gmm`, \
             which scale to a whole cohort",
            params.max_points
        )));
    }

    let k = params.n_clusters.clamp(1, n_rows);

    // Affinity: exp(-gamma * squared distance).
    let mut affinity = vec![0.0f64; n_rows * n_rows];
    for i in 0..n_rows {
        affinity[i * n_rows + i] = 1.0;
        let a = &data[i * n_features..(i + 1) * n_features];
        for j in (i + 1)..n_rows {
            let b = &data[j * n_features..(j + 1) * n_features];
            let squared: f64 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum();
            let value = (-params.gamma * squared).exp();
            affinity[i * n_rows + j] = value;
            affinity[j * n_rows + i] = value;
        }
    }

    // Symmetric normalisation: D^{-1/2} A D^{-1/2}. Its leading eigenvectors
    // are those of the smallest eigenvalues of the Laplacian I - that, so the
    // Laplacian never has to be formed.
    let inverse_sqrt_degree: Vec<f64> = (0..n_rows)
        .map(|i| {
            let degree: f64 = affinity[i * n_rows..(i + 1) * n_rows].iter().sum();
            // An isolated point has degree zero; leaving it at zero keeps its
            // row out of the spectral embedding instead of producing infinities.
            if degree > 0.0 {
                1.0 / degree.sqrt()
            } else {
                0.0
            }
        })
        .collect();

    for i in 0..n_rows {
        for j in 0..n_rows {
            affinity[i * n_rows + j] *= inverse_sqrt_degree[i] * inverse_sqrt_degree[j];
        }
    }

    let eigen = symmetric_eigen(&affinity, n_rows);

    // The `k` largest eigenvalues, which sit at the end of the ascending list.
    let mut spectral = vec![0.0f64; n_rows * k];
    for (column, offset) in (0..k).enumerate() {
        let index = n_rows - 1 - offset;
        for row in 0..n_rows {
            spectral[row * k + column] = eigen.vectors[row * n_rows + index];
        }
    }

    // Row normalisation projects each point onto the unit sphere, which is what
    // makes the clusters angularly separated and k-means appropriate.
    for row in 0..n_rows {
        let slice = &mut spectral[row * k..(row + 1) * k];
        let norm: f64 = slice.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm > 1e-12 {
            slice.iter_mut().for_each(|v| *v /= norm);
        }
    }

    let result = kmeans(&spectral, n_rows, k, k, 100, 10, params.seed);
    Ok(relabel_clusters(&result.labels))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n_clusters(labels: &[u32]) -> usize {
        let mut d = labels.to_vec();
        d.sort_unstable();
        d.dedup();
        d.len()
    }

    /// Two tight groups, far apart.
    fn two_groups() -> (Vec<f64>, usize) {
        let mut data = Vec::new();
        for centre in [0.0f64, 8.0] {
            for i in 0..12 {
                let t = i as f64 * 0.5;
                data.push(centre + t.sin() * 0.2);
                data.push(t.cos() * 0.2);
            }
        }
        (data, 24)
    }

    #[test]
    fn separates_two_groups() {
        let (data, n) = two_groups();
        let params = SpectralParams {
            n_clusters: 2,
            ..Default::default()
        };
        let labels = spectral_clustering(&data, n, 2, &params).unwrap();

        let first = labels[0];
        assert!(labels[..12].iter().all(|&l| l == first), "{labels:?}");
        assert!(labels[12..].iter().all(|&l| l != first), "{labels:?}");
    }

    #[test]
    fn labels_are_contiguous() {
        let (data, n) = two_groups();
        let params = SpectralParams {
            n_clusters: 2,
            ..Default::default()
        };
        let labels = spectral_clustering(&data, n, 2, &params).unwrap();
        let mut distinct = labels.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct, (0..distinct.len() as u32).collect::<Vec<_>>());
    }

    #[test]
    fn the_size_limit_is_enforced_with_a_helpful_message() {
        let params = SpectralParams {
            n_clusters: 2,
            max_points: 10,
            ..Default::default()
        };
        let err = spectral_clustering(&vec![0.0; 40], 20, 2, &params).unwrap_err();
        assert!(err.to_string().contains("leiden"));
    }

    #[test]
    fn the_cluster_count_is_clamped_to_the_sample_size() {
        let params = SpectralParams {
            n_clusters: 10,
            ..Default::default()
        };
        let labels = spectral_clustering(&[0.0, 0.0, 5.0, 5.0], 2, 2, &params).unwrap();
        assert!(n_clusters(&labels) <= 2);
    }

    #[test]
    fn coincident_points_do_not_produce_nan_labels() {
        let params = SpectralParams {
            n_clusters: 2,
            ..Default::default()
        };
        let data = [1.0f64; 20];
        let labels = spectral_clustering(&data, 10, 2, &params).unwrap();
        assert_eq!(labels.len(), 10);
    }

    #[test]
    fn an_empty_dataset_yields_no_labels() {
        assert!(spectral_clustering(&[], 0, 2, &SpectralParams::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rejects_a_mismatched_shape() {
        let err =
            spectral_clustering(&[1.0, 2.0, 3.0], 2, 2, &SpectralParams::default()).unwrap_err();
        assert!(err.to_string().contains("expected 2 x 2"));
    }
}
