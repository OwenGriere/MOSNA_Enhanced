//! Fitting the output kernel of UMAP.

/// Fit `a` and `b` so that `1 / (1 + a * x^(2b))` approximates the piecewise
/// target curve
///
/// ```text
/// f(x) = 1                            for x <= min_dist
///        exp(-(x - min_dist) / spread) otherwise
/// ```
///
/// `min_dist` is how tightly points may pack in the embedding; the smooth
/// rational kernel is what the layout optimiser can actually differentiate.
///
/// umap-learn fits this with `scipy.optimize.curve_fit`; this uses
/// Levenberg-Marquardt over the same sampled curve, which reaches the same
/// minimum — for the defaults `spread = 1, min_dist = 0.1` both give
/// `a ≈ 1.577, b ≈ 0.895`.
pub fn find_ab_params(spread: f64, min_dist: f64) -> (f64, f64) {
    // The sampling grid umap-learn uses: 300 points over three spreads.
    const N_SAMPLES: usize = 300;
    let xs: Vec<f64> = (0..N_SAMPLES)
        .map(|i| i as f64 * 3.0 * spread / (N_SAMPLES - 1) as f64)
        .collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|&x| {
            if x <= min_dist {
                1.0
            } else {
                (-(x - min_dist) / spread).exp()
            }
        })
        .collect();

    let mut a = 1.0f64;
    let mut b = 1.0f64;
    let mut lambda = 1e-3f64;
    let mut error = sum_squared_error(&xs, &ys, a, b);

    for _ in 0..200 {
        // Normal equations of the Gauss-Newton step, damped by `lambda`.
        let (mut jtj00, mut jtj01, mut jtj11) = (0.0f64, 0.0f64, 0.0f64);
        let (mut jtr0, mut jtr1) = (0.0f64, 0.0f64);

        for (&x, &y) in xs.iter().zip(&ys) {
            if x <= 0.0 {
                // The kernel is exactly 1 at the origin whatever a and b are,
                // and `ln(0)` is undefined, so this sample carries no gradient.
                continue;
            }
            let x2b = x.powf(2.0 * b);
            let denominator = 1.0 + a * x2b;
            let f = 1.0 / denominator;
            let residual = f - y;

            let df_da = -x2b / (denominator * denominator);
            let df_db = -2.0 * a * x2b * x.ln() / (denominator * denominator);

            jtj00 += df_da * df_da;
            jtj01 += df_da * df_db;
            jtj11 += df_db * df_db;
            jtr0 += df_da * residual;
            jtr1 += df_db * residual;
        }

        // Solve the damped 2x2 system for the step.
        let m00 = jtj00 * (1.0 + lambda);
        let m11 = jtj11 * (1.0 + lambda);
        let determinant = m00 * m11 - jtj01 * jtj01;
        if determinant.abs() < 1e-300 {
            break;
        }
        let delta_a = (-jtr0 * m11 + jtr1 * jtj01) / determinant;
        let delta_b = (-jtr1 * m00 + jtr0 * jtj01) / determinant;

        // Both parameters must stay positive for the kernel to be a decreasing
        // function of distance.
        let candidate_a = (a + delta_a).max(1e-6);
        let candidate_b = (b + delta_b).max(1e-6);
        let candidate_error = sum_squared_error(&xs, &ys, candidate_a, candidate_b);

        if candidate_error < error {
            a = candidate_a;
            b = candidate_b;
            let improvement = error - candidate_error;
            error = candidate_error;
            // Converged: further steps would only chase floating point noise.
            if improvement < 1e-14 {
                break;
            }
            lambda = (lambda * 0.5).max(1e-12);
        } else {
            // Rejected: lean further towards gradient descent and retry.
            lambda *= 4.0;
            if lambda > 1e12 {
                break;
            }
        }
    }

    (a, b)
}

fn sum_squared_error(xs: &[f64], ys: &[f64], a: f64, b: f64) -> f64 {
    xs.iter()
        .zip(ys)
        .map(|(&x, &y)| {
            let f = 1.0 / (1.0 + a * x.powf(2.0 * b));
            (f - y) * (f - y)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(x: f64, min_dist: f64, spread: f64) -> f64 {
        if x <= min_dist {
            1.0
        } else {
            (-(x - min_dist) / spread).exp()
        }
    }

    fn worst_error(a: f64, b: f64, min_dist: f64, spread: f64) -> f64 {
        (0..=300)
            .map(|i| {
                let x = i as f64 * 3.0 * spread / 300.0;
                let fitted = 1.0 / (1.0 + a * x.powf(2.0 * b));
                (fitted - target(x, min_dist, spread)).abs()
            })
            .fold(0.0f64, f64::max)
    }

    fn sum_of_squares(a: f64, b: f64, min_dist: f64, spread: f64) -> f64 {
        (0..300)
            .map(|i| {
                let x = i as f64 * 3.0 * spread / 299.0;
                let residual = 1.0 / (1.0 + a * x.powf(2.0 * b)) - target(x, min_dist, spread);
                residual * residual
            })
            .sum()
    }

    #[test]
    fn matches_the_reference_values_for_the_umap_defaults() {
        let (a, b) = find_ab_params(1.0, 0.1);
        assert!((a - 1.577).abs() < 0.05, "a = {a}");
        assert!((b - 0.895).abs() < 0.05, "b = {b}");
    }

    /// The real specification is that the optimiser finds the minimum, not that
    /// the minimum is small. It is not: a rational kernel cannot follow a pure
    /// exponential near the origin, so the best possible worst-case error at
    /// `min_dist = 0` is around 0.052 — for umap-learn's own parameters too.
    ///
    /// So this compares against the values `scipy.optimize.curve_fit` reaches,
    /// rather than an invented tolerance. Fitting at least as well as the
    /// reference is the property that matters.
    #[test]
    fn the_fit_is_at_least_as_good_as_the_reference_implementation() {
        // (spread, min_dist, reference a, reference b) from umap-learn.
        for (spread, min_dist, ref_a, ref_b) in
            [(1.0, 0.0, 1.9285, 0.7915), (1.0, 0.1, 1.5769, 0.8951)]
        {
            let (a, b) = find_ab_params(spread, min_dist);
            let ours = sum_of_squares(a, b, min_dist, spread);
            let theirs = sum_of_squares(ref_a, ref_b, min_dist, spread);
            assert!(
                ours <= theirs * (1.0 + 1e-6),
                "spread {spread} min_dist {min_dist}: our fit {ours} is worse than {theirs}"
            );
        }
    }

    #[test]
    fn the_fit_tracks_the_target_curve() {
        for (spread, min_dist) in [(1.0, 0.0), (1.0, 0.1), (1.0, 0.5), (2.0, 0.1)] {
            let (a, b) = find_ab_params(spread, min_dist);
            let worst = worst_error(a, b, min_dist, spread);
            assert!(worst < 0.11, "spread {spread} min_dist {min_dist}: {worst}");
        }
    }

    #[test]
    fn both_parameters_stay_positive() {
        for min_dist in [0.0, 0.05, 0.25, 0.9] {
            let (a, b) = find_ab_params(1.0, min_dist);
            assert!(a > 0.0 && b > 0.0, "min_dist {min_dist}: a {a}, b {b}");
        }
    }

    #[test]
    fn a_larger_min_dist_flattens_the_kernel() {
        let (tight, _) = find_ab_params(1.0, 0.0);
        let (loose, _) = find_ab_params(1.0, 0.5);
        assert!(loose < tight, "{loose} should be below {tight}");
    }

    #[test]
    fn the_result_is_reproducible() {
        assert_eq!(find_ab_params(1.0, 0.1), find_ab_params(1.0, 0.1));
    }
}
