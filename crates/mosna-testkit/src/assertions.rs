//! Assertions expressing an invariant once rather than at each call site.

/// Assert two floats agree to `tolerance`, reporting both values on failure.
///
/// `NaN` equals `NaN` here: several MOSNA routines legitimately return `NaN`
/// for a degenerate input (an assortativity coefficient with a single
/// attribute, a z-score against a constant null), and a test pinning that
/// behaviour must be able to say so.
#[track_caller]
pub fn assert_close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    if actual.is_nan() && expected.is_nan() {
        return;
    }
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: got {actual}, expected {expected} (tolerance {tolerance})"
    );
}

/// Assert two slices agree element-wise.
#[track_caller]
pub fn assert_slice_close(actual: &[f64], expected: &[f64], tolerance: f64, what: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{what}: length {} != {}",
        actual.len(),
        expected.len()
    );
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert_close(*a, *e, tolerance, &format!("{what}[{i}]"));
    }
}

/// Assert a row-major `n x n` matrix is symmetric.
#[track_caller]
pub fn assert_symmetric(matrix: &[f64], n: usize, tolerance: f64, what: &str) {
    assert_eq!(matrix.len(), n * n, "{what}: not an n x n matrix");
    for i in 0..n {
        for j in (i + 1)..n {
            let upper = matrix[i * n + j];
            let lower = matrix[j * n + i];
            if upper.is_nan() && lower.is_nan() {
                continue;
            }
            assert!(
                (upper - lower).abs() <= tolerance,
                "{what}: ({i}, {j}) = {upper} but ({j}, {i}) = {lower}"
            );
        }
    }
}

/// Assert `labels` is a valid clustering of `n_items`.
///
/// Valid means: one label per item, and the distinct labels form the contiguous
/// range `0..k`. Every clustering routine in the port promises this — the niche
/// labels are written into parquet and used to index colour maps and
/// composition matrices, so a gap in the numbering would silently mis-colour a
/// figure or index out of bounds.
#[track_caller]
pub fn assert_valid_partition(labels: &[u32], n_items: usize, what: &str) {
    assert_eq!(labels.len(), n_items, "{what}: expected {n_items} labels");
    if labels.is_empty() {
        return;
    }
    let mut distinct: Vec<u32> = labels.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    let expected: Vec<u32> = (0..distinct.len() as u32).collect();
    assert_eq!(
        distinct, expected,
        "{what}: labels must be 0..k with no gaps, got {distinct:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_accepts_within_tolerance() {
        assert_close(1.0, 1.0 + 1e-12, 1e-9, "x");
        assert_close(f64::NAN, f64::NAN, 0.0, "both nan");
    }

    #[test]
    #[should_panic(expected = "expected 2")]
    fn close_rejects_beyond_tolerance() {
        assert_close(1.0, 2.0, 1e-9, "x");
    }

    #[test]
    fn symmetric_accepts_a_symmetric_matrix() {
        assert_symmetric(&[1.0, 2.0, 2.0, 3.0], 2, 1e-12, "m");
    }

    #[test]
    #[should_panic(expected = "(0, 1)")]
    fn symmetric_rejects_an_asymmetric_matrix() {
        assert_symmetric(&[1.0, 2.0, 5.0, 3.0], 2, 1e-12, "m");
    }

    #[test]
    fn partition_accepts_contiguous_labels() {
        assert_valid_partition(&[0, 1, 1, 2, 0], 5, "labels");
        assert_valid_partition(&[], 0, "empty");
    }

    #[test]
    #[should_panic(expected = "no gaps")]
    fn partition_rejects_a_gap() {
        assert_valid_partition(&[0, 2], 2, "labels");
    }
}
