//! Starting positions for the layout optimisation.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::linalg::eigen::symmetric_eigen;

/// Half-width of the initial layout.
///
/// umap-learn draws its random initialisation uniformly from `[-10, 10]` and
/// rescales a spectral one to a comparable extent. The learning rate and the
/// gradient clipping are tuned for that scale, so the PCA initialisation is
/// stretched to match rather than left at the data's own scale.
const INIT_EXTENT: f64 = 10.0;

/// Initial embedding, from the leading principal components of the data.
///
/// # Why not the spectral initialisation
///
/// umap-learn initialises from the eigenvectors of the graph Laplacian. That
/// needs a *sparse* eigensolver on an `n x n` matrix where `n` is the number of
/// cells — hundreds of thousands here — which would mean pulling in an ARPACK
/// binding or writing a Lanczos solver with restarts.
///
/// PCA is an initialisation umap-learn itself offers (`init='pca'`), it costs a
/// covariance over the *features* — a few tens or hundreds of columns, which
/// the dense solver already in this crate handles — and it preserves global
/// structure far better than a random start. The layout optimiser converges to
/// comparable embeddings from either.
///
/// Falls back to a seeded random layout when the data has no spread to project
/// onto, which happens when every point coincides.
pub fn init_layout(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    n_components: usize,
    seed: u64,
) -> Vec<f64> {
    if n_rows == 0 || n_components == 0 {
        return Vec::new();
    }
    if n_rows == 1 {
        return vec![0.0; n_components];
    }

    let projected = pca_project(data, n_rows, n_features, n_components);
    match projected {
        Some(mut embedding) => {
            rescale(&mut embedding, n_rows, n_components);
            embedding
        }
        None => random_layout(n_rows, n_components, seed),
    }
}

/// Project onto the leading principal components, or `None` when the data has
/// no variance to speak of.
fn pca_project(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    n_components: usize,
) -> Option<Vec<f64>> {
    if n_features == 0 {
        return None;
    }

    // Centre the columns.
    let mut mean = vec![0.0f64; n_features];
    for row in 0..n_rows {
        for f in 0..n_features {
            mean[f] += data[row * n_features + f];
        }
    }
    mean.iter_mut().for_each(|m| *m /= n_rows as f64);

    // Covariance over features: `n_features` is the width of the NAS table, a
    // few tens to a few hundreds, never the number of cells.
    let mut covariance = vec![0.0f64; n_features * n_features];
    for row in 0..n_rows {
        let offset = row * n_features;
        for i in 0..n_features {
            let di = data[offset + i] - mean[i];
            if di == 0.0 {
                continue;
            }
            for j in i..n_features {
                let dj = data[offset + j] - mean[j];
                covariance[i * n_features + j] += di * dj;
            }
        }
    }
    let denominator = (n_rows - 1).max(1) as f64;
    for i in 0..n_features {
        for j in i..n_features {
            let value = covariance[i * n_features + j] / denominator;
            covariance[i * n_features + j] = value;
            covariance[j * n_features + i] = value;
        }
    }

    let eigen = symmetric_eigen(&covariance, n_features);
    // `symmetric_eigen` sorts ascending, so the leading components are last.
    let leading: Vec<usize> = (0..n_components)
        .map(|c| n_features.saturating_sub(1 + c))
        .collect();

    let total_variance: f64 = leading.iter().map(|&k| eigen.values[k].max(0.0)).sum();
    if total_variance.is_nan() || total_variance <= 1e-24 {
        return None;
    }

    let mut embedding = vec![0.0f64; n_rows * n_components];
    for row in 0..n_rows {
        for (c, &k) in leading.iter().enumerate() {
            // A component beyond the data's dimensionality contributes nothing.
            if eigen.values[k] <= 0.0 {
                continue;
            }
            let mut value = 0.0;
            for f in 0..n_features {
                value += (data[row * n_features + f] - mean[f]) * eigen.vectors[f * n_features + k];
            }
            embedding[row * n_components + c] = value;
        }
    }
    Some(embedding)
}

/// Stretch the layout so its widest axis spans `[-INIT_EXTENT, INIT_EXTENT]`.
fn rescale(embedding: &mut [f64], n_rows: usize, n_components: usize) {
    let extent = embedding.iter().fold(0.0f64, |acc, v| acc.max(v.abs()));
    if extent > 1e-12 {
        let scale = INIT_EXTENT / extent;
        embedding.iter_mut().for_each(|v| *v *= scale);
    } else {
        // Degenerate projection; leave the caller's fallback to handle it.
        debug_assert!(embedding.len() == n_rows * n_components);
    }
}

/// A seeded uniform layout, matching umap-learn's `init='random'`.
fn random_layout(n_rows: usize, n_components: usize, seed: u64) -> Vec<f64> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..n_rows * n_components)
        .map(|_| rng.gen_range(-INIT_EXTENT..INIT_EXTENT))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_the_requested_shape() {
        let data: Vec<f64> = (0..30).map(|i| i as f64).collect();
        for n_components in [1, 2, 3] {
            let layout = init_layout(&data, 10, 3, n_components, 0);
            assert_eq!(layout.len(), 10 * n_components);
            assert!(layout.iter().all(|v| v.is_finite()));
        }
    }

    /// The leading component must line up with the direction the data actually
    /// varies along; a PCA that picked the wrong axis would start the layout
    /// from a projection that has thrown the structure away.
    #[test]
    fn the_first_component_follows_the_direction_of_greatest_variance() {
        // Spread wide along x, narrow along y.
        let mut data = Vec::new();
        for i in 0..20 {
            data.push(i as f64);
            data.push((i % 2) as f64 * 0.01);
        }
        let layout = init_layout(&data, 20, 2, 2, 0);

        // The first embedded coordinate must be monotone in the input index.
        let first: Vec<f64> = (0..20).map(|i| layout[i * 2]).collect();
        let ascending = first.windows(2).all(|w| w[0] < w[1]);
        let descending = first.windows(2).all(|w| w[0] > w[1]);
        assert!(
            ascending || descending,
            "the leading component did not capture the x axis: {first:?}"
        );
    }

    #[test]
    fn the_layout_is_scaled_to_the_expected_extent() {
        let data: Vec<f64> = (0..40).map(|i| (i as f64 * 0.3).sin()).collect();
        let layout = init_layout(&data, 20, 2, 2, 0);
        let extent = layout.iter().fold(0.0f64, |acc, v| acc.max(v.abs()));
        assert!(
            (extent - INIT_EXTENT).abs() < 1e-9,
            "extent {extent} should be {INIT_EXTENT}"
        );
    }

    #[test]
    fn coincident_points_fall_back_to_a_random_layout() {
        let data = vec![7.0; 40];
        let layout = init_layout(&data, 20, 2, 2, 3);
        assert_eq!(layout.len(), 40);
        assert!(layout.iter().all(|v| v.is_finite()));
        // A random fallback must actually spread the points out.
        let extent = layout.iter().fold(0.0f64, |acc, v| acc.max(v.abs()));
        assert!(extent > 1.0, "the fallback collapsed to {extent}");
    }

    #[test]
    fn the_layout_is_reproducible() {
        let data = vec![1.0; 40];
        assert_eq!(
            init_layout(&data, 20, 2, 2, 5),
            init_layout(&data, 20, 2, 2, 5)
        );
    }

    #[test]
    fn asking_for_more_components_than_features_still_works() {
        let data: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let layout = init_layout(&data, 10, 1, 3, 0);
        assert_eq!(layout.len(), 30);
        assert!(layout.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn degenerate_sizes_are_handled() {
        assert!(init_layout(&[], 0, 2, 2, 0).is_empty());
        assert_eq!(init_layout(&[1.0, 2.0], 1, 2, 2, 0), vec![0.0, 0.0]);
    }
}
