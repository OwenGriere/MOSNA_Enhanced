//! Per-point bandwidth of the fuzzy neighbourhood.

use crate::reduction::umap::Scalar;

use rayon::prelude::*;

/// Ratio below which a bandwidth is considered collapsed and raised to a floor.
///
/// `MIN_K_DIST_SCALE` in umap-learn, same value and same purpose: a `sigma` of
/// zero would make every membership either 1 or 0, throwing away the graded
/// structure the whole method rests on.
const MIN_K_DIST_SCALE: Scalar = 1e-3;

/// Solve for the local connectivity radius and bandwidth of every point.
///
/// For each point `i`, `sigma[i]` is chosen so that the membership strengths of
/// its neighbours sum to `log2(k)`:
///
/// ```text
/// sum_j exp( -max(d_ij - rho_i, 0) / sigma_i ) = log2(k)
/// ```
///
/// and `rho[i]` is the distance to its nearest neighbour, which guarantees that
/// every point keeps one connection at full strength and so cannot be stranded
/// in the embedding.
///
/// The equation is monotone decreasing in `sigma`, so a bisection converges
/// reliably; 64 iterations take it to machine precision.
///
/// `local_connectivity` selects which neighbour defines `rho`; the
/// configuration never changes it from 1, so the fractional interpolation
/// umap-learn performs for non-integer values is not reproduced — the value is
/// rounded down to a neighbour index instead.
pub fn smooth_knn_dist(
    distances: &[Vec<Scalar>],
    local_connectivity: Scalar,
) -> (Vec<Scalar>, Vec<Scalar>) {
    let n_rows = distances.len();
    let mut rho = vec![0.0; n_rows];
    let mut sigma = vec![1.0; n_rows];

    // The global mean distance backs the floor for points whose own
    // neighbourhood is entirely degenerate.
    // Accumulated in `f64` even though everything around it is single
    // precision: this is a sum over `n_rows * k` terms — a hundred and seventy
    // million of them on a large cohort — and a running `f32` total stops
    // absorbing a term once it has grown some ten million times larger than it,
    // which here happens well before the end. umap-learn reaches the same
    // number by another route: `np.mean` over a `float32` array sums pairwise,
    // so its error grows with `log(n)` rather than with `n`. Widening the
    // accumulator is the simpler way to the same place, and the result is
    // narrowed again before it is used.
    let (total, count) = distances
        .iter()
        .flat_map(|row| row.iter())
        .fold((0.0f64, 0usize), |(sum, n), &d| (sum + d as f64, n + 1));
    let mean_distance = if count > 0 {
        (total / count as f64) as Scalar
    } else {
        0.0
    };

    let connectivity_index = (local_connectivity.max(1.0) as usize).saturating_sub(1);

    // Every row is solved from its own distances and the two scalars above, so
    // the rows can be solved at once. The global mean stays sequential: it is a
    // sum over the whole cohort, and reassociating it would move its last bits.
    rho.par_iter_mut()
        .zip(sigma.par_iter_mut())
        .zip(distances.par_iter())
        .for_each(|((rho_i, sigma_i), row)| {
            if row.is_empty() {
                return;
            }
            let k = row.len();
            let target = (k as Scalar).log2();

            *rho_i = row[connectivity_index.min(k - 1)];

            // Bisection on sigma. `hi` starts unbounded and is discovered by
            // doubling, because a good upper bound depends on the local scale.
            let mut lo = 0.0;
            let mut hi = Scalar::INFINITY;
            let mut mid = 1.0;

            for _ in 0..64 {
                let psum: Scalar = row
                    .iter()
                    .map(|d| (-(d - *rho_i).max(0.0) / mid).exp())
                    .sum();

                if (psum - target).abs() < 1e-5 {
                    break;
                }
                if psum > target {
                    hi = mid;
                    mid = (lo + hi) / 2.0;
                } else {
                    lo = mid;
                    if hi.is_infinite() {
                        mid *= 2.0;
                    } else {
                        mid = (lo + hi) / 2.0;
                    }
                }
            }
            *sigma_i = mid;

            // Floor the bandwidth against the local scale, then against the global
            // one, so a point whose neighbours all coincide still gets a usable
            // positive value.
            let row_mean = row.iter().sum::<Scalar>() / k as Scalar;
            if *rho_i > 0.0 {
                *sigma_i = sigma_i.max(MIN_K_DIST_SCALE * row_mean);
            } else {
                *sigma_i = sigma_i.max(MIN_K_DIST_SCALE * mean_distance);
            }
            // `is_nan` is spelled out so a NaN bandwidth is caught too: every
            // comparison against NaN is false, so `sigma <= 0.0` alone would let it
            // through and poison every membership weight downstream.
            if sigma_i.is_nan() || *sigma_i <= 0.0 || sigma_i.is_infinite() {
                *sigma_i = 1.0;
            }
        });

    (rho, sigma)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn membership_sum(row: &[Scalar], rho: Scalar, sigma: Scalar) -> Scalar {
        row.iter()
            .map(|d| (-(d - rho).max(0.0) / sigma).exp())
            .sum()
    }

    #[test]
    fn solves_the_defining_equation() {
        let distances = vec![vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]];
        let (rho, sigma) = smooth_knn_dist(&distances, 1.0);

        let target = (8 as Scalar).log2();
        let sum = membership_sum(&distances[0], rho[0], sigma[0]);
        assert!((sum - target).abs() < 1e-4, "sum {sum} != {target}");
    }

    #[test]
    fn rho_is_the_nearest_neighbour_distance() {
        let distances = vec![vec![2.5, 3.0, 9.0]];
        let (rho, _) = smooth_knn_dist(&distances, 1.0);
        assert_eq!(rho[0], 2.5);
    }

    #[test]
    fn a_wider_neighbourhood_gets_a_wider_bandwidth() {
        let tight = vec![vec![1.0, 1.1, 1.2, 1.3]];
        let loose = vec![vec![1.0, 5.0, 9.0, 13.0]];
        let (_, sigma_tight) = smooth_knn_dist(&tight, 1.0);
        let (_, sigma_loose) = smooth_knn_dist(&loose, 1.0);
        assert!(
            sigma_loose[0] > sigma_tight[0],
            "{} should exceed {}",
            sigma_loose[0],
            sigma_tight[0]
        );
    }

    #[test]
    fn coincident_neighbours_still_get_a_positive_bandwidth() {
        // Every distance zero: the equation has no solution, so the floor must
        // take over rather than leaving sigma at zero.
        let distances = vec![vec![0.0; 5]];
        let (rho, sigma) = smooth_knn_dist(&distances, 1.0);
        assert_eq!(rho[0], 0.0);
        assert!(sigma[0] > 0.0 && sigma[0].is_finite(), "sigma {}", sigma[0]);
    }

    #[test]
    fn an_empty_row_is_left_at_its_defaults() {
        let (rho, sigma) = smooth_knn_dist(&[vec![]], 1.0);
        assert_eq!(rho[0], 0.0);
        assert!(sigma[0] > 0.0);
    }

    #[test]
    fn each_point_is_solved_independently() {
        let distances = vec![vec![1.0, 1.1, 1.2, 1.3], vec![10.0, 50.0, 90.0, 130.0]];
        let (rho, sigma) = smooth_knn_dist(&distances, 1.0);
        let target = (4 as Scalar).log2();
        for i in 0..2 {
            let sum = membership_sum(&distances[i], rho[i], sigma[i]);
            assert!((sum - target).abs() < 1e-4, "row {i}: sum {sum}");
        }
    }
}
