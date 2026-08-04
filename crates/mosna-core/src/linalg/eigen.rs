//! Symmetric eigendecomposition by the cyclic Jacobi method.

/// Eigenvalues and eigenvectors of a real symmetric matrix, sorted by
/// eigenvalue in ascending order.
#[derive(Debug, Clone)]
pub struct Eigen {
    /// Ascending eigenvalues.
    pub values: Vec<f64>,
    /// Eigenvectors, row-major: `vectors[i * n + k]` is component `i` of
    /// eigenvector `k`.
    pub vectors: Vec<f64>,
    pub n: usize,
}

impl Eigen {
    /// Borrow eigenvector `k` as a freshly collected vector.
    pub fn vector(&self, k: usize) -> Vec<f64> {
        (0..self.n).map(|i| self.vectors[i * self.n + k]).collect()
    }
}

/// Diagonalise the symmetric matrix `a` (row-major, `n x n`).
///
/// Uses the cyclic Jacobi rotation method: it is unconditionally stable, needs
/// no external LAPACK, and computes *all* eigenpairs to full accuracy. Its
/// `O(n^3)` cost per sweep is irrelevant here because `n` is the number of
/// clusters or the embedding dimensionality — at most a few tens — never the
/// number of cells.
///
/// The input is not modified.
pub fn symmetric_eigen(a: &[f64], n: usize) -> Eigen {
    debug_assert_eq!(a.len(), n * n);

    let mut m = a.to_vec();
    // Start from the identity; each rotation is accumulated into it.
    let mut v = vec![0.0f64; n * n];
    for i in 0..n {
        v[i * n + i] = 1.0;
    }

    if n <= 1 {
        return sorted(m, v, n);
    }

    const MAX_SWEEPS: usize = 100;
    for _ in 0..MAX_SWEEPS {
        // Convergence: the off-diagonal mass has become negligible.
        let off: f64 = (0..n)
            .flat_map(|i| (0..n).map(move |j| (i, j)))
            .filter(|(i, j)| i != j)
            .map(|(i, j)| m[i * n + j] * m[i * n + j])
            .sum();
        if off <= 1e-30 {
            break;
        }

        for p in 0..n - 1 {
            for q in (p + 1)..n {
                let apq = m[p * n + q];
                if apq.abs() < 1e-300 {
                    continue;
                }
                let app = m[p * n + p];
                let aqq = m[q * n + q];

                // Rotation angle zeroing element (p, q).
                let theta = (aqq - app) / (2.0 * apq);
                let t = if theta >= 0.0 {
                    1.0 / (theta + (1.0 + theta * theta).sqrt())
                } else {
                    -1.0 / (-theta + (1.0 + theta * theta).sqrt())
                };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = t * c;

                for k in 0..n {
                    let akp = m[k * n + p];
                    let akq = m[k * n + q];
                    m[k * n + p] = c * akp - s * akq;
                    m[k * n + q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let apk = m[p * n + k];
                    let aqk = m[q * n + k];
                    m[p * n + k] = c * apk - s * aqk;
                    m[q * n + k] = s * apk + c * aqk;
                }
                for k in 0..n {
                    let vkp = v[k * n + p];
                    let vkq = v[k * n + q];
                    v[k * n + p] = c * vkp - s * vkq;
                    v[k * n + q] = s * vkp + c * vkq;
                }
            }
        }
    }

    sorted(m, v, n)
}

/// Extract the diagonal as eigenvalues and reorder everything ascending.
fn sorted(m: Vec<f64>, v: Vec<f64>, n: usize) -> Eigen {
    let mut order: Vec<usize> = (0..n).collect();
    let diagonal: Vec<f64> = (0..n).map(|i| m[i * n + i]).collect();
    order.sort_by(|&a, &b| {
        diagonal[a]
            .partial_cmp(&diagonal[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let values: Vec<f64> = order.iter().map(|&k| diagonal[k]).collect();
    let mut vectors = vec![0.0f64; n * n];
    for (new_k, &old_k) in order.iter().enumerate() {
        // Fix the sign so the decomposition is reproducible: the component of
        // largest magnitude is made positive.
        let mut pivot = 0.0f64;
        for i in 0..n {
            if v[i * n + old_k].abs() > pivot.abs() {
                pivot = v[i * n + old_k];
            }
        }
        let sign = if pivot < 0.0 { -1.0 } else { 1.0 };
        for i in 0..n {
            vectors[i * n + new_k] = v[i * n + old_k] * sign;
        }
    }

    Eigen { values, vectors, n }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagonalises_a_diagonal_matrix() {
        let a = vec![3.0, 0.0, 0.0, 1.0];
        let eigen = symmetric_eigen(&a, 2);
        assert!((eigen.values[0] - 1.0).abs() < 1e-12);
        assert!((eigen.values[1] - 3.0).abs() < 1e-12);
    }

    #[test]
    fn recovers_a_known_decomposition() {
        // [[2, 1], [1, 2]] has eigenvalues 1 and 3.
        let a = vec![2.0, 1.0, 1.0, 2.0];
        let eigen = symmetric_eigen(&a, 2);
        assert!((eigen.values[0] - 1.0).abs() < 1e-10);
        assert!((eigen.values[1] - 3.0).abs() < 1e-10);

        // Eigenvector for 3 is (1, 1)/sqrt(2).
        let v = eigen.vector(1);
        assert!((v[0] - v[1]).abs() < 1e-10);
    }

    #[test]
    fn eigenvectors_satisfy_the_eigen_equation() {
        let n = 5;
        // A reproducible symmetric matrix.
        let mut a = vec![0.0; n * n];
        for i in 0..n {
            for j in 0..n {
                let value = ((i * 7 + j * 13) % 11) as f64 - 5.0;
                a[i * n + j] = value;
                a[j * n + i] = value;
            }
        }

        let eigen = symmetric_eigen(&a, n);
        for k in 0..n {
            let v = eigen.vector(k);
            let lambda = eigen.values[k];
            for i in 0..n {
                let av: f64 = (0..n).map(|j| a[i * n + j] * v[j]).sum();
                assert!(
                    (av - lambda * v[i]).abs() < 1e-8,
                    "A v != lambda v for eigenpair {k}"
                );
            }
        }
    }

    #[test]
    fn eigenvectors_are_orthonormal() {
        let n = 4;
        let mut a = vec![0.0; n * n];
        for i in 0..n {
            for j in i..n {
                let value = (i as f64 + 1.0) * (j as f64 + 2.0) % 7.0;
                a[i * n + j] = value;
                a[j * n + i] = value;
            }
        }
        let eigen = symmetric_eigen(&a, n);

        for k in 0..n {
            let vk = eigen.vector(k);
            let norm: f64 = vk.iter().map(|x| x * x).sum();
            assert!((norm - 1.0).abs() < 1e-9, "eigenvector {k} is not unit");
            for l in (k + 1)..n {
                let vl = eigen.vector(l);
                let dot: f64 = vk.iter().zip(&vl).map(|(a, b)| a * b).sum();
                assert!(
                    dot.abs() < 1e-9,
                    "eigenvectors {k} and {l} are not orthogonal"
                );
            }
        }
    }

    #[test]
    fn values_are_ascending_and_the_result_is_reproducible() {
        let a = vec![4.0, 1.0, 2.0, 1.0, 5.0, 3.0, 2.0, 3.0, 6.0];
        let first = symmetric_eigen(&a, 3);
        let second = symmetric_eigen(&a, 3);
        assert_eq!(first.values, second.values);
        assert_eq!(first.vectors, second.vectors);
        assert!(first.values.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn handles_trivial_sizes() {
        assert!(symmetric_eigen(&[], 0).values.is_empty());
        let one = symmetric_eigen(&[7.0], 1);
        assert_eq!(one.values, vec![7.0]);
        assert_eq!(one.vectors, vec![1.0]);
    }
}
