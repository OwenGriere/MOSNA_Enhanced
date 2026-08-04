//! Centred log-ratio transform — port of the `composition_stats` calls in
//! `mosna/niches.py` and `mosna/preprocessing.py`.

/// Normalise each row so it sums to one.
///
/// `composition_stats.closure(X)`. Rows summing to zero are left as zeros
/// rather than producing `NaN`; the callers feed this counts matrices where an
/// all-zero row means "no observation in this niche", and propagating a `NaN`
/// there would poison the whole figure.
pub fn closure(matrix: &mut [Vec<f64>]) {
    for row in matrix.iter_mut() {
        let sum: f64 = row.iter().sum();
        if sum > 0.0 {
            for value in row.iter_mut() {
                *value /= sum;
            }
        }
    }
}

/// Centred log-ratio transform of an already-closed matrix.
///
/// `composition_stats.clr(X)`: each row becomes `ln(x) - mean(ln(x))`.
pub fn clr(matrix: &mut [Vec<f64>]) {
    for row in matrix.iter_mut() {
        let logs: Vec<f64> = row.iter().map(|v| v.ln()).collect();
        let mean = logs.iter().sum::<f64>() / logs.len().max(1) as f64;
        for (value, log) in row.iter_mut().zip(logs) {
            *value = log - mean;
        }
    }
}

/// The full transform applied by `make_niches_composition(normalize='clr')` and
/// by `transform_CLR`.
///
/// ```python
/// X[X == 0] = X.max() / 100000
/// X_clr = cs.clr(cs.closure(X))
/// ```
///
/// Zeros are replaced before the closure because `ln(0)` is undefined; the
/// replacement value is five orders of magnitude below the largest count, which
/// keeps those cells far below every real observation.
pub fn transform_clr(matrix: &mut [Vec<f64>]) {
    let max = matrix
        .iter()
        .flat_map(|row| row.iter())
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    if max.is_finite() && max > 0.0 {
        let replacement = max / 100_000.0;
        for row in matrix.iter_mut() {
            for value in row.iter_mut() {
                if *value == 0.0 {
                    *value = replacement;
                }
            }
        }
    }
    closure(matrix);
    clr(matrix);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closure_makes_rows_sum_to_one() {
        let mut m = vec![vec![1.0, 3.0], vec![2.0, 2.0]];
        closure(&mut m);
        assert_eq!(m[0], vec![0.25, 0.75]);
        assert_eq!(m[1], vec![0.5, 0.5]);
    }

    #[test]
    fn closure_leaves_an_all_zero_row_alone() {
        let mut m = vec![vec![0.0, 0.0]];
        closure(&mut m);
        assert_eq!(m[0], vec![0.0, 0.0]);
    }

    #[test]
    fn clr_rows_sum_to_zero() {
        let mut m = vec![vec![0.25, 0.25, 0.5]];
        clr(&mut m);
        let sum: f64 = m[0].iter().sum();
        assert!(sum.abs() < 1e-12, "clr rows are centred, got {sum}");
    }

    #[test]
    fn clr_of_a_uniform_row_is_all_zeros() {
        let mut m = vec![vec![0.2; 5]];
        clr(&mut m);
        assert!(m[0].iter().all(|v| v.abs() < 1e-12));
    }

    #[test]
    fn transform_replaces_zeros_before_taking_logs() {
        let mut m = vec![vec![10.0, 0.0]];
        transform_clr(&mut m);
        assert!(
            m[0].iter().all(|v| v.is_finite()),
            "zeros must not produce -inf, got {:?}",
            m[0]
        );
        assert!(m[0][0] > m[0][1], "the observed category must dominate");
    }
}
