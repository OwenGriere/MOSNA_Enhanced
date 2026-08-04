//! Gaussian mixture model with full covariances.
//!
//! Replaces the `torchgmm.bayes.GaussianMixture` call in
//! `clustering.py::get_clusterer`, which is the clusterer the shipped
//! configuration selects (`clusterer_type: gmm`).

use crate::clustering::relabel_clusters::relabel_clusters;
use crate::error::{CoreError, Result};
use crate::linalg::cholesky::{cholesky, Cholesky};
use crate::linalg::kmeans::kmeans;

/// Settings of a mixture fit.
#[derive(Debug, Clone, PartialEq)]
pub struct GmmParams {
    pub n_clusters: usize,
    pub max_iter: usize,
    /// Convergence threshold on the mean log-likelihood between two iterations.
    pub tol: f64,
    /// Added to the diagonal of each covariance, keeping it invertible.
    pub reg_covar: f64,
    /// Restarts; the best likelihood wins.
    pub n_init: usize,
    pub seed: u64,
}

impl Default for GmmParams {
    /// scikit-learn's defaults, which `torchgmm` also follows.
    fn default() -> Self {
        Self {
            n_clusters: 8,
            max_iter: 100,
            tol: 1e-3,
            reg_covar: 1e-6,
            n_init: 1,
            seed: 0,
        }
    }
}

/// A fitted mixture.
#[derive(Debug, Clone)]
pub struct GmmResult {
    /// Most likely component of each point, renumbered contiguously.
    pub labels: Vec<u32>,
    /// Component means, row-major `n_clusters * n_features`.
    pub means: Vec<f64>,
    /// Mean log-likelihood of the data under the fitted model.
    pub log_likelihood: f64,
    pub n_iter: usize,
    /// Mean log-likelihood after each iteration, for the winning restart.
    pub log_likelihood_history: Vec<f64>,
}

/// Fit a Gaussian mixture by expectation-maximisation.
///
/// `data` is row-major, `n_rows` by `n_features`.
///
/// Initialised from k-means, as scikit-learn does, then alternating E and M
/// steps until the mean log-likelihood stops improving by more than `tol`.
/// Everything is computed in log space through a Cholesky factorisation: a
/// covariance determinant in twenty dimensions underflows to zero in linear
/// space, which would turn every responsibility into `NaN`.
pub fn gaussian_mixture(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    params: &GmmParams,
) -> Result<GmmResult> {
    if data.len() != n_rows * n_features {
        return Err(CoreError::shape(format!(
            "data has {} values, expected {n_rows} x {n_features}",
            data.len()
        )));
    }
    if n_rows == 0 || n_features == 0 {
        return Err(CoreError::invalid(
            "cannot fit a mixture to an empty dataset",
        ));
    }

    // More components than points has no meaning; the extra ones would stay
    // empty and their covariances undefined.
    let k = params.n_clusters.clamp(1, n_rows);

    let mut best: Option<GmmResult> = None;
    for attempt in 0..params.n_init.max(1) {
        let candidate = fit_once(
            data,
            n_rows,
            n_features,
            k,
            params,
            params.seed.wrapping_add(attempt as u64),
        )?;
        if best
            .as_ref()
            .is_none_or(|b| candidate.log_likelihood > b.log_likelihood)
        {
            best = Some(candidate);
        }
    }
    Ok(best.expect("n_init is at least one"))
}

fn fit_once(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    k: usize,
    params: &GmmParams,
    seed: u64,
) -> Result<GmmResult> {
    // k-means gives a far better starting point than random responsibilities,
    // and is what scikit-learn's default `init_params='kmeans'` does.
    let seeded = kmeans(data, n_rows, n_features, k, 50, 1, seed);

    let mut responsibilities = vec![0.0f64; n_rows * k];
    for (row, &label) in seeded.labels.iter().enumerate() {
        responsibilities[row * k + label as usize] = 1.0;
    }

    let mut weights = vec![0.0f64; k];
    let mut means = vec![0.0f64; k * n_features];
    let mut covariances = vec![0.0f64; k * n_features * n_features];
    maximisation(
        data,
        n_rows,
        n_features,
        k,
        &responsibilities,
        params.reg_covar,
        &mut weights,
        &mut means,
        &mut covariances,
    );

    let mut history = Vec::with_capacity(params.max_iter);
    let mut previous = f64::NEG_INFINITY;
    let mut n_iter = 0;

    for iteration in 0..params.max_iter.max(1) {
        let log_likelihood = expectation(
            data,
            n_rows,
            n_features,
            k,
            &weights,
            &means,
            &covariances,
            params.reg_covar,
            &mut responsibilities,
        )?;
        history.push(log_likelihood);
        n_iter = iteration + 1;

        maximisation(
            data,
            n_rows,
            n_features,
            k,
            &responsibilities,
            params.reg_covar,
            &mut weights,
            &mut means,
            &mut covariances,
        );

        if (log_likelihood - previous).abs() < params.tol {
            break;
        }
        previous = log_likelihood;
    }

    // One last E step so the labels reflect the final parameters.
    let log_likelihood = expectation(
        data,
        n_rows,
        n_features,
        k,
        &weights,
        &means,
        &covariances,
        params.reg_covar,
        &mut responsibilities,
    )?;

    let raw_labels: Vec<u32> = (0..n_rows)
        .map(|row| {
            let slice = &responsibilities[row * k..(row + 1) * k];
            slice
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(component, _)| component as u32)
                .unwrap_or(0)
        })
        .collect();

    // Components that ended up empty leave gaps; the rest of the pipeline
    // requires contiguous ids.
    let labels = relabel_clusters(&raw_labels);

    Ok(GmmResult {
        labels,
        means,
        log_likelihood,
        n_iter,
        log_likelihood_history: history,
    })
}

/// E step: posterior probability of each component for each point.
///
/// Returns the mean log-likelihood.
#[allow(clippy::too_many_arguments)]
fn expectation(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    k: usize,
    weights: &[f64],
    means: &[f64],
    covariances: &[f64],
    reg_covar: f64,
    responsibilities: &mut [f64],
) -> Result<f64> {
    let factorisations: Vec<Cholesky> = (0..k)
        .map(|component| {
            let block = &covariances
                [component * n_features * n_features..(component + 1) * n_features * n_features];
            factorise(block, n_features, reg_covar)
        })
        .collect::<Result<Vec<_>>>()?;

    let normaliser = -0.5 * n_features as f64 * (2.0 * std::f64::consts::PI).ln();
    let mut total = 0.0;
    let mut delta = vec![0.0f64; n_features];

    for row in 0..n_rows {
        let point = &data[row * n_features..(row + 1) * n_features];

        // Log of the joint probability of the point and each component.
        let mut log_joint = vec![f64::NEG_INFINITY; k];
        for (component, factorisation) in factorisations.iter().enumerate() {
            if weights[component] <= 0.0 {
                continue;
            }
            for f in 0..n_features {
                delta[f] = point[f] - means[component * n_features + f];
            }
            log_joint[component] = weights[component].ln() + normaliser
                - 0.5 * factorisation.log_det()
                - 0.5 * factorisation.mahalanobis_squared(&delta);
        }

        // Log-sum-exp, shifted by the maximum so the exponentials cannot
        // overflow or all underflow to zero.
        let max = log_joint.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let sum_exp: f64 = log_joint.iter().map(|v| (v - max).exp()).sum();
        let log_prob = max + sum_exp.ln();
        total += log_prob;

        for component in 0..k {
            responsibilities[row * k + component] = (log_joint[component] - log_prob).exp();
        }
    }

    Ok(total / n_rows as f64)
}

/// M step: re-estimate the weights, means and covariances.
#[allow(clippy::too_many_arguments)]
fn maximisation(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    k: usize,
    responsibilities: &[f64],
    reg_covar: f64,
    weights: &mut [f64],
    means: &mut [f64],
    covariances: &mut [f64],
) {
    // Effective count of each component. The floor is scikit-learn's
    // `10 * eps`: a component that has lost every point would otherwise divide
    // by zero here.
    let floor = 10.0 * f64::EPSILON;
    let mut counts = vec![0.0f64; k];
    for row in 0..n_rows {
        for (component, count) in counts.iter_mut().enumerate() {
            *count += responsibilities[row * k + component];
        }
    }
    counts.iter_mut().for_each(|c| *c = c.max(floor));

    means.iter_mut().for_each(|m| *m = 0.0);
    for row in 0..n_rows {
        for component in 0..k {
            let r = responsibilities[row * k + component];
            if r == 0.0 {
                continue;
            }
            for f in 0..n_features {
                means[component * n_features + f] += r * data[row * n_features + f];
            }
        }
    }
    for component in 0..k {
        for f in 0..n_features {
            means[component * n_features + f] /= counts[component];
        }
    }

    covariances.iter_mut().for_each(|c| *c = 0.0);
    let mut delta = vec![0.0f64; n_features];
    for row in 0..n_rows {
        for component in 0..k {
            let r = responsibilities[row * k + component];
            if r == 0.0 {
                continue;
            }
            for f in 0..n_features {
                delta[f] = data[row * n_features + f] - means[component * n_features + f];
            }
            let block = component * n_features * n_features;
            for i in 0..n_features {
                let scaled = r * delta[i];
                for j in i..n_features {
                    covariances[block + i * n_features + j] += scaled * delta[j];
                }
            }
        }
    }
    for (component, &count) in counts.iter().enumerate().take(k) {
        let block = component * n_features * n_features;
        for i in 0..n_features {
            for j in i..n_features {
                let mut value = covariances[block + i * n_features + j] / count;
                if i == j {
                    value += reg_covar;
                }
                covariances[block + i * n_features + j] = value;
                covariances[block + j * n_features + i] = value;
            }
        }
    }

    let total: f64 = counts.iter().sum();
    for component in 0..k {
        weights[component] = counts[component] / total;
    }
}

/// Factorise a covariance, escalating the regularisation until it succeeds.
///
/// A component that has collapsed onto fewer points than dimensions has a
/// singular covariance. scikit-learn raises in that situation; here the
/// regularisation is raised instead, which keeps the fit going and is what the
/// caller wants — a niche run should not abort because one of twenty components
/// briefly degenerated.
fn factorise(block: &[f64], n_features: usize, reg_covar: f64) -> Result<Cholesky> {
    if let Some(factorisation) = cholesky(block, n_features) {
        return Ok(factorisation);
    }

    let mut boost = reg_covar.max(f64::EPSILON);
    let mut patched = block.to_vec();
    for _ in 0..60 {
        boost *= 10.0;
        patched.copy_from_slice(block);
        for i in 0..n_features {
            patched[i * n_features + i] += boost;
        }
        if let Some(factorisation) = cholesky(&patched, n_features) {
            return Ok(factorisation);
        }
    }
    Err(CoreError::numeric(
        "gaussian mixture",
        "a component covariance stayed singular under regularisation",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two tight groups, ten units apart.
    fn two_groups() -> (Vec<f64>, usize) {
        let mut data = Vec::new();
        for centre in [0.0, 10.0] {
            for i in 0..15 {
                let t = i as f64 * 0.4;
                data.push(centre + t.sin() * 0.2);
                data.push(t.cos() * 0.2);
            }
        }
        (data, 30)
    }

    #[test]
    fn separates_two_groups() {
        let (data, n) = two_groups();
        let params = GmmParams {
            n_clusters: 2,
            seed: 1,
            ..Default::default()
        };
        let result = gaussian_mixture(&data, n, 2, &params).unwrap();

        let first = result.labels[0];
        assert!(result.labels[..15].iter().all(|&l| l == first));
        assert!(result.labels[15..].iter().all(|&l| l != first));
    }

    #[test]
    fn the_means_land_on_the_group_centres() {
        let (data, n) = two_groups();
        let params = GmmParams {
            n_clusters: 2,
            seed: 1,
            ..Default::default()
        };
        let result = gaussian_mixture(&data, n, 2, &params).unwrap();

        let mut centres = [result.means[0], result.means[2]];
        centres.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((centres[0] - 0.0).abs() < 0.5, "got {centres:?}");
        assert!((centres[1] - 10.0).abs() < 0.5, "got {centres:?}");
    }

    #[test]
    fn the_likelihood_never_decreases() {
        let (data, n) = two_groups();
        let params = GmmParams {
            n_clusters: 3,
            n_init: 1,
            seed: 2,
            ..Default::default()
        };
        let result = gaussian_mixture(&data, n, 2, &params).unwrap();
        assert!(
            result
                .log_likelihood_history
                .windows(2)
                .all(|w| w[1] >= w[0] - 1e-9),
            "{:?}",
            result.log_likelihood_history
        );
    }

    #[test]
    fn identical_points_do_not_produce_a_singular_failure() {
        let data = vec![3.0; 40];
        let params = GmmParams {
            n_clusters: 3,
            seed: 0,
            ..Default::default()
        };
        let result = gaussian_mixture(&data, 10, 4, &params).unwrap();
        assert!(result.log_likelihood.is_finite());
        assert_eq!(result.labels.len(), 10);
    }

    #[test]
    fn the_component_count_is_clamped_to_the_sample_size() {
        let data = vec![0.0, 0.0, 1.0, 1.0];
        let params = GmmParams {
            n_clusters: 9,
            seed: 0,
            ..Default::default()
        };
        let result = gaussian_mixture(&data, 2, 2, &params).unwrap();
        assert_eq!(result.labels.len(), 2);
        assert!(result.means.len() <= 2 * 2);
    }

    #[test]
    fn rejects_a_mismatched_shape() {
        let err = gaussian_mixture(&[1.0, 2.0, 3.0], 2, 2, &GmmParams::default()).unwrap_err();
        assert!(err.to_string().contains("expected 2 x 2"));
    }

    #[test]
    fn rejects_an_empty_dataset() {
        assert!(gaussian_mixture(&[], 0, 2, &GmmParams::default()).is_err());
    }

    #[test]
    fn is_reproducible() {
        let (data, n) = two_groups();
        let params = GmmParams {
            n_clusters: 3,
            seed: 5,
            ..Default::default()
        };
        let first = gaussian_mixture(&data, n, 2, &params).unwrap();
        let second = gaussian_mixture(&data, n, 2, &params).unwrap();
        assert_eq!(first.labels, second.labels);
        assert_eq!(first.log_likelihood, second.log_likelihood);
    }
}
