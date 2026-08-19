//! Colours, in the form the renderer takes them.
//!
//! `xy` maps a value onto a colour by interpolating linearly between the stops
//! it is given, over the domain it is given. Every normalisation this project
//! uses — the diverging one centred on zero, the symmetric-log one — is
//! therefore expressed *as stops*: resampling the colour map through the
//! normalisation reproduces it to within a quantisation step, and leaves the
//! renderer with real data values on its axes and its colour bar rather than a
//! pre-normalised `[0, 1]` nobody can read.

use mosna_core::colormap::{Gradient, Rgb};

/// How many stops a resampled map carries.
pub const STOPS: usize = 256;

/// A colour as the renderer spells it.
pub fn hex(colour: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2])
}

/// The map itself, evenly sampled: for a normalisation that is already linear.
pub fn linear(gradient: &Gradient, stops: usize) -> Vec<String> {
    let stops = stops.max(2);
    (0..stops)
        .map(|index| hex(gradient.sample(index as f64 / (stops - 1) as f64)))
        .collect()
}

/// The map as seen *through* a normalisation, over `[vmin, vmax]`.
///
/// Stop `i` is the colour the normalisation gives to the value the renderer
/// will place there, so reading the stops linearly reproduces the curve.
pub fn resample(
    gradient: &Gradient,
    normalise: impl Fn(f64) -> f64,
    vmin: f64,
    vmax: f64,
    stops: usize,
) -> Vec<String> {
    let stops = stops.max(2);
    let span = vmax - vmin;
    (0..stops)
        .map(|index| {
            let fraction = index as f64 / (stops - 1) as f64;
            let value = vmin + span * fraction;
            // `Gradient::sample` already clamps and survives a `NaN`, so an
            // exotic normalisation cannot produce something that is not a
            // colour.
            hex(gradient.sample(normalise(value)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::norm::TwoSlopeNorm;
    use mosna_core::colormap::{blues, rd_bu_r};

    #[test]
    fn a_colour_is_six_hexadecimal_digits_behind_a_hash() {
        assert_eq!(hex([0x05, 0x30, 0x61]), "#053061");
        assert_eq!(hex([0, 0, 0]), "#000000");
        assert_eq!(hex([255, 255, 255]), "#ffffff");
    }

    #[test]
    fn a_linear_map_keeps_its_ends() {
        let stops = linear(&blues(), 16);
        assert_eq!(stops.len(), 16);
        assert_eq!(stops[0], hex(blues().sample(0.0)));
        assert_eq!(stops[15], hex(blues().sample(1.0)));
    }

    /// The point of resampling: a value of zero has to land on the neutral
    /// centre of the map even when the positive tail is four times the
    /// negative one. Read linearly over `[vmin, vmax]`, the stop at zero is the
    /// one that decides that.
    #[test]
    fn a_two_slope_normalisation_survives_being_expressed_as_stops() {
        let (vmin, vmax) = (-1.0, 4.0);
        let norm = TwoSlopeNorm::new(vmin, 0.0, vmax);
        let stops = resample(&rd_bu_r(), |v| norm.normalise(v), vmin, vmax, STOPS);

        assert_eq!(stops.len(), STOPS);
        // Where zero falls, read the way the renderer reads it.
        let at_zero = ((0.0 - vmin) / (vmax - vmin) * (STOPS - 1) as f64).round() as usize;
        assert_eq!(
            stops[at_zero],
            hex(rd_bu_r().sample(0.5)),
            "zero is not on the neutral centre of the map"
        );
        assert_eq!(stops[0], hex(rd_bu_r().sample(0.0)));
        assert_eq!(stops[STOPS - 1], hex(rd_bu_r().sample(1.0)));
    }

    /// Every stop is a colour, whatever the normalisation does with a value:
    /// a `NaN` out of the norm must not become the string "#NaNNaN".
    #[test]
    fn every_stop_is_a_colour() {
        let stops = resample(&rd_bu_r(), |_| f64::NAN, -1.0, 1.0, 8);
        for stop in &stops {
            assert_eq!(stop.len(), 7, "{stop} is not a colour");
            assert!(stop.starts_with('#'));
            assert!(stop[1..].chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    /// A degenerate range — every value identical — must still produce a map
    /// rather than divide by zero.
    #[test]
    fn a_range_of_no_width_still_produces_a_map() {
        let stops = resample(&blues(), |v| v, 1.0, 1.0, 4);
        assert_eq!(stops.len(), 4);
    }

    #[test]
    fn a_single_stop_is_refused_rather_than_drawn_from() {
        // `xy` interpolates between stops; one stop is not a map. Two is the
        // floor, and asking for fewer gets two.
        assert_eq!(linear(&blues(), 1).len(), 2);
        assert_eq!(linear(&blues(), 0).len(), 2);
    }
}
