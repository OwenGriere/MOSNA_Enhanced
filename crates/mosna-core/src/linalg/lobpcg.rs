//! Leading eigenvectors of a large symmetric operator, by LOBPCG.
//!
//! [`fn@crate::linalg::symmetric_eigen`] diagonalises a dense matrix in
//! `O(n^3)`: right for a few tens of rows, hopeless for the graph of a cohort,
//! where `n` is the number of cells. The spectral initialisation of UMAP needs
//! only a handful of eigenvectors of such a graph, and this computes them from
//! matrix-vector products alone — the matrix is never formed.
//!
//! # The method
//!
//! LOBPCG (Knyazev, 2001) — the solver `scipy.sparse.linalg.lobpcg` provides,
//! and the one umap-learn itself switches to above two million points. Each
//! iteration builds the subspace spanned by
//!
//! * `X`, the current approximations of the eigenvectors,
//! * `W = M X - X diag(mu)`, their residuals,
//! * `P`, the direction the previous iteration moved in,
//!
//! orthonormalises it, and solves the small projected eigenproblem in it
//! (Rayleigh-Ritz). The best `b` Ritz vectors become the next `X`. Keeping `P`
//! is what makes this a conjugate-gradient method rather than a steepest
//! ascent, and it converges at the rate of a block Lanczos for a fraction of
//! the memory.
//!
//! # Deterministic
//!
//! Every inner product is summed over fixed blocks of rows, and the block
//! totals are added in order: the result is the same on one core or sixty-four.
//! The starting block is drawn from the seed.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand_distr::{Distribution, StandardNormal};
use rayon::prelude::*;

use crate::linalg::eigen::symmetric_eigen;

/// Rows per block in the inner products: fixed, so the sum does not depend on
/// how many threads computed it.
const CHUNK: usize = 4096;

/// Below this length a vector operation runs on the calling thread. The
/// arithmetic is the same either way — the same blocks, summed in the same
/// order — but a hundred small operations per iteration each handed to the
/// thread pool cost more in dispatch than in work.
const PARALLEL_MIN: usize = 1 << 14;

/// Iterations between two re-orthonormalisations of the current block.
const REORTHONORMALISE: usize = 10;

/// Leading eigenpairs of a symmetric operator.
#[derive(Debug, Clone)]
pub struct Leading {
    /// Eigenvalues, largest first.
    pub values: Vec<f64>,
    /// The matching eigenvectors, unit norm, each of length `n`.
    pub vectors: Vec<Vec<f64>>,
    /// Whether every wanted pair reached the tolerance before `max_iter`.
    pub converged: bool,
    /// Iterations taken.
    pub iterations: usize,
}

/// The `k` algebraically largest eigenpairs of the symmetric operator `apply`.
///
/// `apply(x, y)` must write `M x` into `y`; it is called on vectors of length
/// `n`. `start` optionally fixes the first starting vectors — umap-learn seeds
/// the trivial eigenvector of the Laplacian this way — and the rest of the
/// block is drawn from `seed`.
///
/// Convergence is declared when every wanted residual `||M x - mu x||` is below
/// `tol`, the vectors being of unit norm. Returns `None` when the operator
/// produces a non-finite value or `n` is too small for the block; a caller
/// with a small matrix should diagonalise it densely instead.
pub fn leading_eigenpairs(
    n: usize,
    k: usize,
    apply: &(dyn Fn(&[f64], &mut [f64]) + Sync),
    start: &[Vec<f64>],
    seed: u64,
    tol: f64,
    max_iter: usize,
) -> Option<Leading> {
    // A couple of guard vectors beyond the wanted ones: the k-th pair then
    // converges at the gap to the (b+1)-th eigenvalue rather than to the
    // (k+1)-th, which is what keeps the last wanted vector from stalling.
    let b = (k + 2).min(n / 3);
    if k == 0 || b < k {
        return None;
    }

    // Starting block.
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut x: Vec<Vec<f64>> = Vec::with_capacity(b);
    for vector in start.iter().take(b) {
        x.push(vector.clone());
    }
    while x.len() < b {
        x.push((0..n).map(|_| StandardNormal.sample(&mut rng)).collect());
    }
    orthonormalise(&mut x, &[]);
    if x.len() < k {
        return None;
    }

    // Rayleigh-Ritz on the starting block alone.
    let mut ax = apply_all(apply, &x, n)?;
    let (mut mu, c) = ritz(&x, &ax, x.len(), x.len())?;
    x = combine(&x, &c);
    ax = combine(&ax, &c);

    let mut p: Vec<Vec<f64>> = Vec::new();
    let mut converged = false;
    let mut iterations = 0;

    for iteration in 0..max_iter {
        iterations = iteration + 1;

        // Residuals of the current approximations.
        let mut w: Vec<Vec<f64>> = x
            .iter()
            .zip(&ax)
            .zip(&mu)
            .map(|((xi, axi), &m)| axi.iter().zip(xi).map(|(a, v)| a - m * v).collect())
            .collect();
        let worst = w.iter().take(k).map(|r| norm(r)).fold(0.0f64, f64::max);
        if !worst.is_finite() {
            return None;
        }
        if worst < tol {
            converged = true;
            break;
        }

        // The search subspace [X, W, P], orthonormal, X first and untouched.
        //
        // X comes out of the Rayleigh-Ritz step orthonormal, and `AX` with it
        // as the same combination of the images. Rounding erodes that slowly,
        // so X is re-orthonormalised and its image recomputed only every
        // `REORTHONORMALISE` iterations rather than at each one — which was
        // a third of the products and of the Gram-Schmidt work.
        if iteration % REORTHONORMALISE == 0 {
            orthonormalise(&mut x, &[]);
            ax = apply_all(apply, &x, n)?;
        }
        orthonormalise(&mut w, &x);
        let mut against: Vec<Vec<f64>> = x.clone();
        against.extend(w.iter().cloned());
        orthonormalise(&mut p, &against);

        let n_x = x.len();
        let mut basis = x.clone();
        basis.extend(w);
        basis.extend(p);
        let mut image = ax.clone();
        image.extend(apply_all(apply, &basis[n_x..], n)?);

        let m = basis.len();
        let take = b.min(m);
        let (values, c) = ritz(&basis, &image, m, take)?;

        // The next directions: the part of the new X outside the old X.
        let rest: Vec<Vec<f64>> = (0..take)
            .into_par_iter()
            .map(|col| {
                let mut v = vec![0.0f64; n];
                for (row, vector) in basis.iter().enumerate().skip(n_x) {
                    axpy(c[row * take + col], vector, &mut v);
                }
                v
            })
            .collect();

        x = combine(&basis, &c);
        ax = combine(&image, &c);
        mu = values;
        p = rest;
    }

    Some(Leading {
        values: mu.into_iter().take(k).collect(),
        vectors: x.into_iter().take(k).collect(),
        converged,
        iterations,
    })
}

/// Rayleigh-Ritz: the `take` largest eigenpairs of `Sᵀ M S`.
///
/// `image` holds `M s` for each column `s` of `basis`. Returns the values,
/// largest first, and the coefficients row-major `m x take`.
fn ritz(
    basis: &[Vec<f64>],
    image: &[Vec<f64>],
    m: usize,
    take: usize,
) -> Option<(Vec<f64>, Vec<f64>)> {
    // The upper triangle, one entry per task — each a sum over the rows in a
    // fixed order, so the thread that computed it does not show. The operator
    // is symmetric, so `s_i · M s_j` is the matrix; it is mirrored rather than
    // averaged with `s_j · M s_i`, which would double the work for a
    // difference at the last bits.
    let pairs: Vec<(usize, usize)> = (0..m).flat_map(|i| (i..m).map(move |j| (i, j))).collect();
    let entries: Vec<f64> = pairs
        .par_iter()
        .map(|&(i, j)| dot(&basis[i], &image[j]))
        .collect();
    let mut gram = vec![0.0f64; m * m];
    for (&(i, j), &value) in pairs.iter().zip(&entries) {
        gram[i * m + j] = value;
        gram[j * m + i] = value;
    }
    if !gram.iter().all(|v| v.is_finite()) {
        return None;
    }
    let eigen = symmetric_eigen(&gram, m);
    // Ascending, so the largest are last.
    let mut values = Vec::with_capacity(take);
    let mut coefficients = vec![0.0f64; m * take];
    for col in 0..take {
        let source = m - 1 - col;
        values.push(eigen.values[source]);
        for row in 0..m {
            coefficients[row * take + col] = eigen.vectors[row * m + source];
        }
    }
    Some((values, coefficients))
}

/// The columns of `basis · c`, `c` row-major `basis.len() x take`.
fn combine(basis: &[Vec<f64>], c: &[f64]) -> Vec<Vec<f64>> {
    let m = basis.len();
    let take = c.len() / m.max(1);
    let n = basis.first().map_or(0, Vec::len);
    (0..take)
        .into_par_iter()
        .map(|col| {
            let mut v = vec![0.0f64; n];
            for (row, vector) in basis.iter().enumerate() {
                axpy(c[row * take + col], vector, &mut v);
            }
            v
        })
        .collect()
}

fn apply_all(
    apply: &(dyn Fn(&[f64], &mut [f64]) + Sync),
    vectors: &[Vec<f64>],
    n: usize,
) -> Option<Vec<Vec<f64>>> {
    // One product per vector, side by side: each is a sum over its own row,
    // so the order the vectors are computed in changes nothing.
    let out: Vec<Vec<f64>> = vectors
        .par_iter()
        .map(|v| {
            let mut y = vec![0.0f64; n];
            apply(v, &mut y);
            y
        })
        .collect();
    out.iter()
        .all(|y| y.iter().all(|v| v.is_finite()))
        .then_some(out)
}

/// Orthonormalise `vectors` in place against `fixed` (already orthonormal) and
/// against each other, by classical Gram-Schmidt run twice (CGS2) — as stable
/// as the modified variant, and its projections are independent of one
/// another, so they are computed side by side. They are then subtracted in a
/// fixed order. A vector that has nothing left once projected is dropped.
fn orthonormalise(vectors: &mut Vec<Vec<f64>>, fixed: &[Vec<f64>]) {
    let mut kept: Vec<Vec<f64>> = Vec::with_capacity(vectors.len());
    for mut v in vectors.drain(..) {
        let original = norm(&v);
        if original.is_nan() || original <= 0.0 || !original.is_finite() {
            continue;
        }
        for _ in 0..2 {
            let basis: Vec<&Vec<f64>> = fixed.iter().chain(kept.iter()).collect();
            let projections: Vec<f64> = basis.par_iter().map(|u| dot(u, &v)).collect();
            for (u, &projection) in basis.iter().zip(&projections) {
                axpy(-projection, u, &mut v);
            }
        }
        let remaining = norm(&v);
        if remaining > 1e-10 * original {
            v.iter_mut().for_each(|value| *value /= remaining);
            kept.push(v);
        }
    }
    *vectors = kept;
}

/// `y += a x`.
fn axpy(a: f64, x: &[f64], y: &mut [f64]) {
    let step = |(ys, xs): (&mut [f64], &[f64])| {
        for (yi, xi) in ys.iter_mut().zip(xs) {
            *yi += a * xi;
        }
    };
    if y.len() >= PARALLEL_MIN {
        y.par_chunks_mut(CHUNK)
            .zip(x.par_chunks(CHUNK))
            .for_each(step);
    } else {
        y.chunks_mut(CHUNK).zip(x.chunks(CHUNK)).for_each(step);
    }
}

/// The inner product, summed block by block and then in block order — the
/// same additions whether the blocks were computed in parallel or not.
pub(crate) fn dot(a: &[f64], b: &[f64]) -> f64 {
    let block = |(xs, ys): (&[f64], &[f64])| xs.iter().zip(ys).map(|(x, y)| x * y).sum::<f64>();
    let partial: Vec<f64> = if a.len() >= PARALLEL_MIN {
        a.par_chunks(CHUNK)
            .zip(b.par_chunks(CHUNK))
            .map(block)
            .collect()
    } else {
        a.chunks(CHUNK).zip(b.chunks(CHUNK)).map(block).collect()
    };
    partial.iter().sum()
}

fn norm(a: &[f64]) -> f64 {
    dot(a, a).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path graph's normalised adjacency has known eigenvalues
    /// `cos(pi j / (n - 1))`; the leading ones must be found.
    #[test]
    fn finds_the_leading_eigenvalues_of_a_path() {
        let n = 300;
        let degree: Vec<f64> = (0..n)
            .map(|i| if i == 0 || i == n - 1 { 1.0 } else { 2.0 })
            .collect();
        let apply = |x: &[f64], y: &mut [f64]| {
            for i in 0..n {
                let mut sum = 0.0;
                if i > 0 {
                    sum += x[i - 1] / (degree[i] * degree[i - 1]).sqrt();
                }
                if i + 1 < n {
                    sum += x[i + 1] / (degree[i] * degree[i + 1]).sqrt();
                }
                y[i] = sum;
            }
        };
        let found = leading_eigenpairs(n, 3, &apply, &[], 0, 1e-6, 5000).unwrap();
        assert!(found.converged, "{} iterations", found.iterations);
        for (j, value) in found.values.iter().enumerate() {
            let exact = (std::f64::consts::PI * j as f64 / (n - 1) as f64).cos();
            assert!(
                (value - exact).abs() < 1e-8,
                "λ{j} = {value}, expected {exact}"
            );
        }
    }

    #[test]
    fn the_vectors_are_orthonormal_eigenvectors() {
        let n = 120;
        // A diagonal operator: eigenvectors are the unit vectors.
        let apply = |x: &[f64], y: &mut [f64]| {
            for i in 0..n {
                y[i] = (i as f64) * x[i];
            }
        };
        let found = leading_eigenpairs(n, 2, &apply, &[], 3, 1e-8, 2000).unwrap();
        assert!(found.converged);
        assert!((found.values[0] - (n - 1) as f64).abs() < 1e-6);
        assert!((found.values[1] - (n - 2) as f64).abs() < 1e-6);
        assert!(dot(&found.vectors[0], &found.vectors[1]).abs() < 1e-8);
        assert!((found.vectors[0][n - 1].abs() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_result_depends_on_the_seed_alone() {
        let n = 200;
        let apply = |x: &[f64], y: &mut [f64]| {
            for i in 0..n {
                y[i] = ((i % 17) as f64) * x[i] + 0.1 * x[(i + 1) % n] + 0.1 * x[(i + n - 1) % n];
            }
        };
        let a = leading_eigenpairs(n, 2, &apply, &[], 9, 1e-7, 1000).unwrap();
        let b = leading_eigenpairs(n, 2, &apply, &[], 9, 1e-7, 1000).unwrap();
        assert_eq!(a.vectors, b.vectors);
    }

    #[test]
    fn a_block_larger_than_the_matrix_is_refused() {
        let apply = |x: &[f64], y: &mut [f64]| y.copy_from_slice(x);
        assert!(leading_eigenpairs(4, 3, &apply, &[], 0, 1e-6, 10).is_none());
    }
}
