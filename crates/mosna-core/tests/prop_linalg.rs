//! Property tests for the dense linear algebra.
//!
//! These routines replace LAPACK calls, so their contracts have to be pinned
//! against the mathematical definitions rather than against a reference
//! implementation.

use mosna_core::linalg::{cholesky, kmeans, symmetric_eigen};
use mosna_testkit::assertions::assert_symmetric;
use proptest::prelude::*;

/// Build a symmetric matrix from the generated upper triangle.
fn symmetrise(upper: &[f64], n: usize) -> Vec<f64> {
    let mut m = vec![0.0; n * n];
    let mut k = 0;
    for i in 0..n {
        for j in i..n {
            m[i * n + j] = upper[k];
            m[j * n + i] = upper[k];
            k += 1;
        }
    }
    m
}

/// A symmetric positive-definite matrix: `M Mᵀ + n·I` is SPD for any `M`.
fn spd(seed: &[f64], n: usize) -> Vec<f64> {
    let mut a = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            let mut sum = 0.0;
            for k in 0..n {
                sum += seed[i * n + k] * seed[j * n + k];
            }
            a[i * n + j] = sum;
        }
        a[i * n + i] += n as f64;
    }
    a
}

fn triangle(n: usize) -> impl Strategy<Value = Vec<f64>> {
    proptest::collection::vec(-10.0f64..10.0, (n * (n + 1) / 2)..=(n * (n + 1) / 2))
}

fn square(n: usize) -> impl Strategy<Value = Vec<f64>> {
    proptest::collection::vec(-3.0f64..3.0, (n * n)..=(n * n))
}

proptest! {
    /// Every returned pair satisfies `A v = lambda v`. This is the definition,
    /// and it catches both a wrong rotation and a mismatched sort order.
    #[test]
    fn prop_eigenpairs_satisfy_the_definition(upper in triangle(5)) {
        let n = 5;
        let a = symmetrise(&upper, n);
        let eigen = symmetric_eigen(&a, n);

        for k in 0..n {
            let v = eigen.vector(k);
            let lambda = eigen.values[k];
            for i in 0..n {
                let av: f64 = (0..n).map(|j| a[i * n + j] * v[j]).sum();
                prop_assert!(
                    (av - lambda * v[i]).abs() < 1e-7,
                    "pair {k}: (Av)[{i}] = {av}, lambda*v[{i}] = {}",
                    lambda * v[i]
                );
            }
        }
    }

    #[test]
    fn prop_eigenvectors_are_orthonormal(upper in triangle(4)) {
        let n = 4;
        let eigen = symmetric_eigen(&symmetrise(&upper, n), n);

        for k in 0..n {
            let vk = eigen.vector(k);
            let norm: f64 = vk.iter().map(|x| x * x).sum();
            prop_assert!((norm - 1.0).abs() < 1e-8, "vector {k} has norm^2 {norm}");
            for l in (k + 1)..n {
                let vl = eigen.vector(l);
                let dot: f64 = vk.iter().zip(&vl).map(|(a, b)| a * b).sum();
                prop_assert!(dot.abs() < 1e-8, "vectors {k},{l} overlap by {dot}");
            }
        }
    }

    /// The trace equals the sum of the eigenvalues, and the eigenvalues come
    /// back ascending.
    #[test]
    fn prop_eigenvalues_sum_to_the_trace(upper in triangle(4)) {
        let n = 4;
        let a = symmetrise(&upper, n);
        let eigen = symmetric_eigen(&a, n);

        let trace: f64 = (0..n).map(|i| a[i * n + i]).sum();
        let sum: f64 = eigen.values.iter().sum();
        prop_assert!((trace - sum).abs() < 1e-8, "trace {trace} != sum {sum}");
        prop_assert!(eigen.values.windows(2).all(|w| w[0] <= w[1] + 1e-12));
    }

    /// The decomposition is deterministic, including eigenvector signs. Without
    /// a sign convention a spectral embedding would flip between runs.
    #[test]
    fn prop_eigen_is_reproducible(upper in triangle(4)) {
        let a = symmetrise(&upper, 4);
        let first = symmetric_eigen(&a, 4);
        let second = symmetric_eigen(&a, 4);
        prop_assert_eq!(first.values, second.values);
        prop_assert_eq!(first.vectors, second.vectors);
    }

    /// `L Lᵀ` reconstructs the input for any symmetric positive-definite matrix.
    #[test]
    fn prop_cholesky_reconstructs_the_input(seed in square(4)) {
        let n = 4;
        let a = spd(&seed, n);
        assert_symmetric(&a, n, 1e-9, "constructed SPD matrix");

        let chol = cholesky(&a, n).expect("M Mt + nI is positive definite");
        for i in 0..n {
            for j in 0..n {
                let reconstructed: f64 =
                    (0..n).map(|k| chol.l[i * n + k] * chol.l[j * n + k]).sum();
                let relative = (reconstructed - a[i * n + j]).abs()
                    / a[i * n + j].abs().max(1.0);
                prop_assert!(relative < 1e-9, "element ({i}, {j}) diverged");
            }
        }
    }

    /// The factor is lower triangular with a positive diagonal.
    #[test]
    fn prop_cholesky_factor_is_lower_triangular(seed in square(4)) {
        let n = 4;
        let chol = cholesky(&spd(&seed, n), n).expect("positive definite");
        for i in 0..n {
            prop_assert!(chol.l[i * n + i] > 0.0, "diagonal {i} is not positive");
            for j in (i + 1)..n {
                prop_assert_eq!(chol.l[i * n + j], 0.0, "upper triangle is not zero");
            }
        }
    }

    /// The Mahalanobis distance is non-negative and vanishes only at the mean.
    #[test]
    fn prop_mahalanobis_is_a_squared_norm(seed in square(3), delta in square(3)) {
        let n = 3;
        let chol = cholesky(&spd(&seed, n), n).expect("positive definite");

        let zero = chol.mahalanobis_squared(&[0.0; 3]);
        prop_assert!(zero.abs() < 1e-12, "distance at the mean is {zero}");

        let d = &delta[..n];
        let distance = chol.mahalanobis_squared(d);
        prop_assert!(distance >= 0.0, "distance {distance} is negative");

        // It is a quadratic form: scaling the offset scales it by the square.
        let doubled: Vec<f64> = d.iter().map(|v| v * 2.0).collect();
        let scaled = chol.mahalanobis_squared(&doubled);
        prop_assert!(
            (scaled - 4.0 * distance).abs() < 1e-6 * (1.0 + distance),
            "{scaled} != 4 * {distance}"
        );
    }

    /// K-means always returns one label per point, within range, and centroids
    /// for every cluster.
    #[test]
    fn prop_kmeans_output_is_well_formed(
        data in proptest::collection::vec(-50.0f64..50.0, 20..=60),
        k in 1usize..5,
    ) {
        let n_features = 2;
        let n_rows = data.len() / n_features;
        prop_assume!(n_rows >= 2);
        let data = &data[..n_rows * n_features];

        let result = kmeans(data, n_rows, n_features, k, 50, 2, 7);
        prop_assert_eq!(result.labels.len(), n_rows);
        prop_assert!(result.labels.iter().all(|&l| (l as usize) < result.k));
        prop_assert_eq!(result.centroids.len(), result.k * n_features);
        prop_assert!(result.inertia >= 0.0 && result.inertia.is_finite());
    }

    /// More clusters can never fit the data worse: the inertia of `k + 1`
    /// clusters is at most that of `k`, up to the local-minimum slack the
    /// restarts leave.
    #[test]
    fn prop_kmeans_inertia_decreases_with_k(
        data in proptest::collection::vec(-20.0f64..20.0, 40..=40),
    ) {
        let (n_rows, n_features) = (20, 2);
        let two = kmeans(&data, n_rows, n_features, 2, 100, 8, 3);
        let four = kmeans(&data, n_rows, n_features, 4, 100, 8, 3);
        prop_assert!(
            four.inertia <= two.inertia + 1e-6,
            "k=4 fits worse ({}) than k=2 ({})",
            four.inertia,
            two.inertia
        );
    }

    #[test]
    fn prop_kmeans_is_reproducible(
        data in proptest::collection::vec(-20.0f64..20.0, 30..=30),
    ) {
        let a = kmeans(&data, 15, 2, 3, 50, 3, 11);
        let b = kmeans(&data, 15, 2, 3, 50, 3, 11);
        prop_assert_eq!(a.labels, b.labels);
        prop_assert_eq!(a.centroids, b.centroids);
    }
}
