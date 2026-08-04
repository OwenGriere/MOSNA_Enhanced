//! `numpy.percentile` with the default linear interpolation.

/// Return the `q`-th percentile of `values`, with `q` in `[0, 100]`.
///
/// Reproduces `np.percentile(a, q)` exactly, including its default
/// `method='linear'`: the result is interpolated between the two order
/// statistics bracketing the position `(n - 1) * q / 100`.
///
/// Getting this right matters because [`fn@crate::geometry::find_trim_dist`] uses
/// it to pick the edge-length cutoff; a nearest-rank percentile would trim a
/// different set of edges and change every downstream figure.
///
/// Returns `None` for an empty slice, where numpy raises.
pub fn percentile(values: &[f64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| !v.is_nan()).collect();
    if sorted.is_empty() {
        return Some(f64::NAN);
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("NaNs were filtered out"));

    let n = sorted.len();
    if n == 1 {
        return Some(sorted[0]);
    }

    let q = q.clamp(0.0, 100.0);
    let position = (n - 1) as f64 * q / 100.0;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return Some(sorted[lower]);
    }
    let weight = position - lower as f64;
    Some(sorted[lower] * (1.0 - weight) + sorted[upper] * weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_numpy_on_a_known_example() {
        // np.percentile([1, 2, 3, 4], 50) == 2.5
        let values = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile(&values, 50.0), Some(2.5));
        // np.percentile([1, 2, 3, 4], 25) == 1.75
        assert_eq!(percentile(&values, 25.0), Some(1.75));
        // np.percentile([1, 2, 3, 4], 99) == 3.97
        let p99 = percentile(&values, 99.0).unwrap();
        assert!((p99 - 3.97).abs() < 1e-12, "got {p99}");
    }

    #[test]
    fn endpoints_are_min_and_max() {
        let values = [5.0, 1.0, 3.0];
        assert_eq!(percentile(&values, 0.0), Some(1.0));
        assert_eq!(percentile(&values, 100.0), Some(5.0));
    }

    #[test]
    fn handles_degenerate_inputs() {
        assert_eq!(percentile(&[], 50.0), None);
        assert_eq!(percentile(&[7.0], 42.0), Some(7.0));
    }

    #[test]
    fn nans_are_ignored_like_nanpercentile() {
        let values = [1.0, f64::NAN, 3.0];
        assert_eq!(percentile(&values, 50.0), Some(2.0));
    }
}
