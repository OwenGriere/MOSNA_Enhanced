//! Per-point bandwidth of the fuzzy neighbourhood.

/// Ratio below which a bandwidth is considered collapsed and raised to a floor.
///
/// `MIN_K_DIST_SCALE` in umap-learn, same value and same purpose: a `sigma` of
/// zero would make every membership either 1 or 0, throwing away the graded
/// structure the whole method rests on.
const MIN_K_DIST_SCALE: f64 = 1e-3;

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
pub fn smooth_knn_dist(distances: &[Vec<f64>], local_connectivity: f64) -> (Vec<f64>, Vec<f64>) {
    let n_rows = distances.len();
    let mut rho = vec![0.0f64; n_rows];
    let mut sigma = vec![1.0f64; n_rows];

    // The global mean distance backs the floor for points whose own
    // neighbourhood is entirely degenerate.
    let (total, count) = distances
        .iter()
        .flat_map(|row| row.iter())
        .fold((0.0f64, 0usize), |(sum, n), &d| (sum + d, n + 1));
    let mean_distance = if count > 0 { total / count as f64 } else { 0.0 };

    let connectivity_index = (local_connectivity.max(1.0) as usize).saturating_sub(1);

    for (i, row) in distances.iter().enumerate() {
        if row.is_empty() {
            continue;
        }
        let k = row.len();
        let target = (k as f64).log2();

        rho[i] = row[connectivity_index.min(k - 1)];

        // Bisection on sigma. `hi` starts unbounded and is discovered by
        // doubling, because a good upper bound depends on the local scale.
        let mut lo = 0.0f64;
        let mut hi = f64::INFINITY;
        let mut mid = 1.0f64;

        for _ in 0..64 {
            let psum: f64 = row
                .iter()
                .map(|d| (-(d - rho[i]).max(0.0) / mid).exp())
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
        sigma[i] = mid;

        // Floor the bandwidth against the local scale, then against the global
        // one, so a point whose neighbours all coincide still gets a usable
        // positive value.
        let row_mean = row.iter().sum::<f64>() / k as f64;
        if rho[i] > 0.0 {
            sigma[i] = sigma[i].max(MIN_K_DIST_SCALE * row_mean);
        } else {
            sigma[i] = sigma[i].max(MIN_K_DIST_SCALE * mean_distance);
        }
        // `is_nan` is spelled out so a NaN bandwidth is caught too: every
        // comparison against NaN is false, so `sigma <= 0.0` alone would let it
        // through and poison every membership weight downstream.
        if sigma[i].is_nan() || sigma[i] <= 0.0 || sigma[i].is_infinite() {
            sigma[i] = 1.0;
        }
    }

    (rho, sigma)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn membership_sum(row: &[f64], rho: f64, sigma: f64) -> f64 {
        row.iter()
            .map(|d| (-(d - rho).max(0.0) / sigma).exp())
            .sum()
    }

    #[test]
    fn solves_the_defining_equation() {
        let distances = vec![vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]];
        let (rho, sigma) = smooth_knn_dist(&distances, 1.0);

        let target = 8f64.log2();
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
        let target = 4f64.log2();
        for i in 0..2 {
            let sum = membership_sum(&distances[i], rho[i], sigma[i]);
            assert!((sum - target).abs() < 1e-4, "row {i}: sum {sum}");
        }
    }
}
