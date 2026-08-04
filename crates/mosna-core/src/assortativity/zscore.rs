//! Port of `assortativity.py::zscore`.

/// Mean, standard deviation and z-score of `observed` against a null sample.
///
/// ```python
/// rand_mean = mat_rand.mean(axis=0)
/// rand_std  = mat_rand.std(axis=0)
/// zscore    = (mat - rand_mean) / rand_std
/// ```
///
/// The Python suppresses the divide-by-zero warning and lets the result be
/// `inf` or `NaN`. That happens whenever a phenotype pair never occurs in any
/// randomisation, so its null distribution is a constant — the figures already
/// filter those out with `replace([np.inf, -np.inf], np.nan)`, and the same
/// values are produced here rather than being silently clamped.
///
/// The standard deviation is the population one (`ddof=0`), matching numpy's
/// default.
pub fn zscore(observed: f64, null_sample: &[f64]) -> (f64, f64, f64) {
    if null_sample.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    let n = null_sample.len() as f64;
    let mean = null_sample.iter().sum::<f64>() / n;
    let variance = null_sample
        .iter()
        .map(|v| {
            let d = v - mean;
            d * d
        })
        .sum::<f64>()
        / n;
    let std = variance.sqrt();
    (mean, std, (observed - mean) / std)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_a_value_against_its_null() {
        // Null sample with mean 2 and population std 1.
        let (mean, std, z) = zscore(4.0, &[1.0, 2.0, 3.0, 2.0, 2.0, 2.0]);
        assert!((mean - 2.0).abs() < 1e-12);
        assert!((std - (2.0f64 / 6.0).sqrt()).abs() < 1e-12);
        assert!(z > 0.0, "an above-null observation scores positive");
    }

    #[test]
    fn a_constant_null_gives_an_infinite_score() {
        let (mean, std, z) = zscore(1.0, &[0.0, 0.0, 0.0]);
        assert_eq!(mean, 0.0);
        assert_eq!(std, 0.0);
        assert!(z.is_infinite(), "the figures filter these out, got {z}");
    }

    #[test]
    fn a_constant_null_matching_the_observation_gives_nan() {
        let (_, _, z) = zscore(0.0, &[0.0, 0.0]);
        assert!(z.is_nan());
    }

    #[test]
    fn an_empty_null_yields_nan() {
        let (mean, std, z) = zscore(1.0, &[]);
        assert!(mean.is_nan() && std.is_nan() && z.is_nan());
    }

    #[test]
    fn uses_the_population_standard_deviation() {
        // np.std([1, 3]) == 1.0 (ddof=0), not 1.4142 (ddof=1).
        let (_, std, _) = zscore(0.0, &[1.0, 3.0]);
        assert!((std - 1.0).abs() < 1e-12);
    }
}
