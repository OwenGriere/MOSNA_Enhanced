//! Cholesky factorisation of a symmetric positive-definite matrix.

/// The lower-triangular factor `L` of `A = L Lᵀ`.
#[derive(Debug, Clone)]
pub struct Cholesky {
    /// Row-major lower triangle; the upper triangle is zero.
    pub l: Vec<f64>,
    pub n: usize,
}

impl Cholesky {
    /// `log(det(A))`, computed as `2 * sum(log(diag(L)))`.
    ///
    /// Working in log space is what keeps the Gaussian mixture's
    /// log-likelihood finite: for a 20-dimensional covariance the determinant
    /// itself readily underflows to zero.
    pub fn log_det(&self) -> f64 {
        2.0 * (0..self.n)
            .map(|i| self.l[i * self.n + i].ln())
            .sum::<f64>()
    }

    /// Solve `L y = b` by forward substitution.
    pub fn solve_lower(&self, b: &[f64]) -> Vec<f64> {
        let n = self.n;
        let mut y = vec![0.0; n];
        for i in 0..n {
            let mut sum = b[i];
            for (j, y_j) in y.iter().take(i).enumerate() {
                sum -= self.l[i * n + j] * y_j;
            }
            y[i] = sum / self.l[i * n + i];
        }
        y
    }

    /// The squared Mahalanobis distance `(x - mu)ᵀ A⁻¹ (x - mu)`.
    pub fn mahalanobis_squared(&self, delta: &[f64]) -> f64 {
        self.solve_lower(delta).iter().map(|v| v * v).sum()
    }
}

/// Factorise `a` (row-major, `n x n`, symmetric positive definite).
///
/// Returns `None` when the matrix is not positive definite, which for a
/// Gaussian mixture means a component has collapsed onto fewer points than
/// dimensions. The caller regularises and retries rather than propagating a
/// `NaN` through the whole likelihood, which is what scikit-learn's
/// `reg_covar` does.
pub fn cholesky(a: &[f64], n: usize) -> Option<Cholesky> {
    debug_assert_eq!(a.len(), n * n);
    let mut l = vec![0.0f64; n * n];

    for i in 0..n {
        for j in 0..=i {
            let mut sum = a[i * n + j];
            for k in 0..j {
                sum -= l[i * n + k] * l[j * n + k];
            }
            if i == j {
                // `<= 0.0` is deliberately written so that a NaN pivot — which
                // a matrix containing NaN produces — also rejects, since every
                // comparison against NaN is false.
                if sum.is_nan() || sum <= 0.0 {
                    return None;
                }
                l[i * n + j] = sum.sqrt();
            } else {
                l[i * n + j] = sum / l[j * n + j];
            }
        }
    }
    Some(Cholesky { l, n })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factorises_the_identity() {
        let a = vec![1.0, 0.0, 0.0, 1.0];
        let chol = cholesky(&a, 2).unwrap();
        assert_eq!(chol.l, vec![1.0, 0.0, 0.0, 1.0]);
        assert!(chol.log_det().abs() < 1e-15);
    }

    #[test]
    fn l_times_l_transpose_reconstructs_the_input() {
        let n = 3;
        let a = vec![4.0, 2.0, 1.0, 2.0, 5.0, 3.0, 1.0, 3.0, 6.0];
        let chol = cholesky(&a, n).unwrap();

        for i in 0..n {
            for j in 0..n {
                let reconstructed: f64 =
                    (0..n).map(|k| chol.l[i * n + k] * chol.l[j * n + k]).sum();
                assert!(
                    (reconstructed - a[i * n + j]).abs() < 1e-12,
                    "element ({i}, {j}) diverged"
                );
            }
        }
    }

    #[test]
    fn log_det_matches_a_direct_determinant() {
        // Diagonal matrix, determinant 2 * 8 = 16.
        let a = vec![2.0, 0.0, 0.0, 8.0];
        let chol = cholesky(&a, 2).unwrap();
        assert!((chol.log_det() - 16.0f64.ln()).abs() < 1e-12);
    }

    #[test]
    fn mahalanobis_reduces_to_euclidean_for_the_identity() {
        let chol = cholesky(&[1.0, 0.0, 0.0, 1.0], 2).unwrap();
        assert!((chol.mahalanobis_squared(&[3.0, 4.0]) - 25.0).abs() < 1e-12);
    }

    #[test]
    fn mahalanobis_scales_with_the_variance() {
        // Variance 4 along each axis: a distance of 2 counts as 1 sigma.
        let chol = cholesky(&[4.0, 0.0, 0.0, 4.0], 2).unwrap();
        assert!((chol.mahalanobis_squared(&[2.0, 0.0]) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_non_positive_definite_matrix_is_rejected() {
        // Singular.
        assert!(cholesky(&[1.0, 1.0, 1.0, 1.0], 2).is_none());
        // Negative eigenvalue.
        assert!(cholesky(&[1.0, 2.0, 2.0, 1.0], 2).is_none());
    }
}
