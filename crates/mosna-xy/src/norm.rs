//! Mapping data values onto the `[0, 1]` a colour map is sampled with.

/// A diverging normalisation with an explicit centre.
///
/// Port of `matplotlib.colors.TwoSlopeNorm`. Each side of the centre is scaled
/// independently, so the centre lands on the middle of the map whatever the
/// range — which is what makes a z-score of zero read as neutral even when the
/// positive tail is far longer than the negative one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwoSlopeNorm {
    vmin: f64,
    vcenter: f64,
    vmax: f64,
}

impl TwoSlopeNorm {
    pub fn new(vmin: f64, vcenter: f64, vmax: f64) -> Self {
        Self {
            vmin,
            vcenter,
            vmax,
        }
    }

    /// A normalisation over `values`, centred on zero.
    ///
    /// Reproduces the guards the Python applies before building the norm: a
    /// range that does not straddle zero is widened so `TwoSlopeNorm` accepts
    /// it, since it requires `vmin < vcenter < vmax`.
    pub fn centred_on_zero(values: impl IntoIterator<Item = f64>) -> Self {
        let (mut vmin, mut vmax) = values
            .into_iter()
            .filter(|v| v.is_finite())
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            });

        if !vmin.is_finite() || !vmax.is_finite() {
            vmin = -1e-6;
            vmax = 1e-6;
        }
        if vmin >= 0.0 {
            vmin = -1e-6;
        }
        if vmax <= 0.0 {
            vmax = 1e-6;
        }
        if vmin == vmax {
            vmin -= 1e-6;
            vmax += 1e-6;
        }
        Self::new(vmin, 0.0, vmax)
    }

    /// The ends of the range, which the renderer needs as its domain.
    pub fn vmin(&self) -> f64 {
        self.vmin
    }

    pub fn vmax(&self) -> f64 {
        self.vmax
    }

    /// Map a value to `[0, 1]`.
    pub fn normalise(&self, value: f64) -> f64 {
        if value.is_nan() {
            return f64::NAN;
        }
        if value <= self.vmin {
            return 0.0;
        }
        if value >= self.vmax {
            return 1.0;
        }

        if value < self.vcenter {
            let span = self.vcenter - self.vmin;
            if span <= 0.0 {
                return 0.5;
            }
            0.5 * (value - self.vmin) / span
        } else {
            let span = self.vmax - self.vcenter;
            if span <= 0.0 {
                return 0.5;
            }
            0.5 + 0.5 * (value - self.vcenter) / span
        }
    }
}

/// A symmetric logarithmic normalisation.
///
/// Port of `matplotlib.colors.SymLogNorm`. Linear within `linthresh` of zero,
/// logarithmic beyond. The mean-assortativity figure needs it: a handful of
/// enormous z-scores would otherwise flatten every other cell onto the centre
/// colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SymLogNorm {
    linthresh: f64,
    vmin: f64,
    vmax: f64,
}

impl SymLogNorm {
    pub fn new(linthresh: f64, vmin: f64, vmax: f64) -> Self {
        Self {
            linthresh: linthresh.max(f64::MIN_POSITIVE),
            vmin,
            vmax,
        }
    }

    /// Map a value to `[0, 1]`.
    pub fn normalise(&self, value: f64) -> f64 {
        if value.is_nan() {
            return f64::NAN;
        }
        let clamped = value.clamp(self.vmin, self.vmax);

        let transform = |v: f64| -> f64 {
            let magnitude = v.abs();
            let scaled = if magnitude <= self.linthresh {
                magnitude / self.linthresh
            } else {
                // Continuous at the threshold: the linear region ends at 1, and
                // the logarithmic one starts there.
                1.0 + (magnitude / self.linthresh).ln()
            };
            scaled * v.signum()
        };

        let low = transform(self.vmin);
        let high = transform(self.vmax);
        let span = high - low;
        if span <= 0.0 {
            return 0.5;
        }
        ((transform(clamped) - low) / span).clamp(0.0, 1.0)
    }

    /// The threshold the Python derives from the data.
    ///
    /// `linthresh = max(0.1, zlim * 0.05)` in
    /// `assort_figures_mean_std_across_samples.py`.
    pub fn threshold_for(zlim: f64) -> f64 {
        (zlim * 0.05).max(0.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_slope_norm_is_piecewise_linear() {
        let norm = TwoSlopeNorm::new(-4.0, 0.0, 2.0);
        assert_eq!(norm.normalise(-2.0), 0.25);
        assert_eq!(norm.normalise(0.0), 0.5);
        assert_eq!(norm.normalise(1.0), 0.75);
    }

    #[test]
    fn a_range_that_misses_zero_is_widened() {
        // Every value positive: the Python nudges vmin below zero so the norm
        // can be built at all.
        let norm = TwoSlopeNorm::centred_on_zero([1.0, 5.0, 9.0]);
        assert!(norm.vmin < 0.0);
        assert!((norm.normalise(0.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn an_all_nan_range_still_produces_a_usable_norm() {
        let norm = TwoSlopeNorm::centred_on_zero([f64::NAN, f64::INFINITY]);
        assert!(norm.normalise(0.0).is_finite());
    }

    #[test]
    fn a_nan_value_stays_nan_so_it_can_be_painted_grey() {
        assert!(TwoSlopeNorm::new(-1.0, 0.0, 1.0)
            .normalise(f64::NAN)
            .is_nan());
        assert!(SymLogNorm::new(1.0, -1.0, 1.0).normalise(f64::NAN).is_nan());
    }

    #[test]
    fn the_symmetric_log_norm_compresses_the_tails() {
        let norm = SymLogNorm::new(1.0, -1000.0, 1000.0);
        // Ten and a hundred are a decade apart, as are a hundred and a
        // thousand; on a log scale those steps are equal.
        let a = norm.normalise(10.0) - norm.normalise(1.0);
        let b = norm.normalise(100.0) - norm.normalise(10.0);
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }

    #[test]
    fn the_threshold_never_falls_below_a_tenth() {
        assert_eq!(SymLogNorm::threshold_for(0.0), 0.1);
        assert_eq!(SymLogNorm::threshold_for(100.0), 5.0);
    }

    #[test]
    fn a_degenerate_range_does_not_divide_by_zero() {
        let norm = SymLogNorm::new(1.0, 0.0, 0.0);
        assert!(norm.normalise(0.0).is_finite());
    }
}
