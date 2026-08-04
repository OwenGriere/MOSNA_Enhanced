//! Port of `assortativity.py::attribute_ac`.

use crate::assortativity::mixing_matrix::MixMat;

/// Newman's attribute assortativity coefficient of a mixing matrix.
///
/// ```python
/// if M.sum() != 1.0:
///     M = M / float(M.sum())
/// M = np.asmatrix(M)
/// s = (M * M).sum()
/// t = M.trace()
/// r = (t - s) / (1 - s)
/// ```
///
/// Equation (2) of Newman, *Mixing patterns in networks*, Phys. Rev. E 67
/// 026126 (2003): the excess of same-attribute edges over what independent
/// mixing would give, normalised so that perfect assortativity is 1.
///
/// Note the `np.asmatrix` call: on a `np.matrix`, `M * M` is the **matrix
/// product**, not an element-wise square. So `s` is the sum of every element of
/// `M @ M`, which for a matrix with row sums `a` and column sums `b` equals
/// `sum_k a_k * b_k` — exactly the `||e^2||` of Newman's equation. Reading `M *
/// M` as an element-wise product, which is what the same expression means for a
/// plain `ndarray`, gives a different and wrong coefficient.
///
/// Returns `NaN` when the matrix is empty or when `s == 1`, the degenerate case
/// of a single attribute where the coefficient is undefined — the same `0/0`
/// numpy produces, and what `zscore` and the figures already treat as missing.
pub fn attribute_ac(mixmat: &MixMat) -> f64 {
    let total = mixmat.sum();
    if total == 0.0 {
        return f64::NAN;
    }

    // sum(M @ M) = sum_k (row_sum_k * col_sum_k), computed without forming the
    // product. Normalisation is folded in by dividing the sums by the total.
    let n = mixmat.n;
    let mut s = 0.0;
    for k in 0..n {
        let mut row_sum = 0.0;
        let mut col_sum = 0.0;
        for j in 0..n {
            row_sum += mixmat.get(k, j);
            col_sum += mixmat.get(j, k);
        }
        s += (row_sum / total) * (col_sum / total);
    }

    let t = mixmat.trace() / total;
    (t - s) / (1.0 - s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_rows(rows: &[&[f64]]) -> MixMat {
        let n = rows.len();
        let mut m = MixMat::zeros(n);
        for (i, row) in rows.iter().enumerate() {
            for (j, value) in row.iter().enumerate() {
                m.set(i, j, *value);
            }
        }
        m
    }

    #[test]
    fn perfectly_assortative_mixing_scores_one() {
        // Every edge joins like with like.
        let m = from_rows(&[&[0.5, 0.0], &[0.0, 0.5]]);
        assert!((attribute_ac(&m) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn perfectly_disassortative_mixing_scores_minus_one() {
        let m = from_rows(&[&[0.0, 0.5], &[0.5, 0.0]]);
        assert!(
            (attribute_ac(&m) + 1.0).abs() < 1e-12,
            "got {}",
            attribute_ac(&m)
        );
    }

    /// The coefficient is zero exactly when the mixing matrix equals the outer
    /// product of its marginals. This is the case that distinguishes the
    /// matrix-product reading of `(M * M).sum()` from the element-wise one:
    /// element-wise would give 1/3 here instead of 0.
    #[test]
    fn independent_mixing_scores_zero() {
        let m = from_rows(&[&[0.25, 0.25], &[0.25, 0.25]]);
        assert!(attribute_ac(&m).abs() < 1e-12, "got {}", attribute_ac(&m));
    }

    /// Independent mixing of attributes with *unequal* abundances is also zero,
    /// which pins the marginal-based formula rather than a symmetric shortcut.
    #[test]
    fn independent_mixing_of_unequal_abundances_scores_zero() {
        // Marginals 0.8 and 0.2, outer product.
        let m = from_rows(&[&[0.64, 0.16], &[0.16, 0.04]]);
        assert!(attribute_ac(&m).abs() < 1e-12, "got {}", attribute_ac(&m));
    }

    #[test]
    fn an_unnormalised_matrix_gives_the_same_answer() {
        let normalised = from_rows(&[&[0.25, 0.25], &[0.25, 0.25]]);
        let raw = from_rows(&[&[10.0, 10.0], &[10.0, 10.0]]);
        assert!((attribute_ac(&normalised) - attribute_ac(&raw)).abs() < 1e-12);
    }

    #[test]
    fn degenerate_matrices_yield_nan() {
        assert!(attribute_ac(&MixMat::zeros(3)).is_nan());
        // A single attribute: s == 1, so the coefficient is 0/0.
        assert!(attribute_ac(&from_rows(&[&[1.0]])).is_nan());
    }
}
