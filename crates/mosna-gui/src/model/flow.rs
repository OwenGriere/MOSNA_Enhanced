//! The moving gold of the progress bar.
//!
//! A bar that only grows says how far along a step is. It does not say whether
//! anything is still happening — and the steps here spend minutes between two
//! increments, on a cohort of two hundred samples. So the fill is a gold ramp
//! with a highlight travelling along it: the position is the progress, the
//! movement is the pulse. Stop the process and the movement stops with it.
//!
//! All of it is arithmetic on a clock, which is why it lives here rather than
//! in the panel that paints it: a shimmer that flickers, jumps at the seam or
//! leaves the palette is a bug, and none of those can be seen in a screenshot.

use egui::Color32;

/// How long the highlight takes to cross the bar once, in seconds.
///
/// Slow enough to read as a flow rather than a strobe, quick enough that a
/// glance says "alive".
pub const PERIOD: f32 = 1.8;

/// Width of a travelling highlight, as a share of the distance between two.
pub const GLOW_WIDTH: f32 = 0.55;

/// How many highlights are on the bar at once.
///
/// One would be a pulse crossing now and then; three is a current. What the
/// bar has to say is that work is still flowing through it, and a single
/// travelling bump on a ramp that is already bright at one end says it too
/// faintly to notice out of the corner of an eye.
pub const REPEATS: f32 = 3.0;

/// Width of the band shown when there is no position to show, as a share of
/// the track.
pub const BAND_WIDTH: f32 = 0.32;

/// The dark end of the ramp.
pub const LOW: Color32 = Color32::from_rgb(0x6B, 0x52, 0x0C);
/// The light end.
pub const HIGH: Color32 = Color32::from_rgb(0xDC, 0xC2, 0x72);
/// The highlight that travels along it.
pub const GLOW: Color32 = Color32::from_rgb(0xFF, 0xF3, 0xD0);

/// Where in the cycle the animation is, from the clock, in `[0, 1)`.
///
/// The clock is `egui`'s, which has been running since the interface started —
/// hours, on a long cohort — so the remainder is taken rather than assumed.
pub fn phase(seconds: f64) -> f32 {
    let turns = seconds / f64::from(PERIOD);
    (turns - turns.floor()) as f32
}

/// The ramp at `t`, ignoring the highlight.
pub fn base(t: f32) -> Color32 {
    mix(LOW, HIGH, t.clamp(0.0, 1.0))
}

/// How strongly the highlight falls at `t`, in `[0, 1]`.
///
/// Highlights repeat along the bar and slide along it as the phase advances.
/// The distance to the nearest one is measured *around* the cycle rather than
/// along it, so a highlight leaving by one end arrives at the other without a
/// seam — and the seam is exactly where a shimmer betrays itself.
pub fn glow(t: f32, phase: f32) -> f32 {
    let position = t.clamp(0.0, 1.0) * REPEATS - phase;
    let within = position - position.floor();
    let around = within.min(1.0 - within);

    let near = 1.0 - (around * 2.0 / GLOW_WIDTH).min(1.0);
    // Smoothstep, so the edges of the band fade instead of ending: a linear
    // falloff has a corner, and a corner travelling along a bar is visible as a
    // ripple rather than a glow.
    near * near * (3.0 - 2.0 * near)
}

/// The colour of the bar at `t`.
pub fn shade(t: f32, phase: f32) -> Color32 {
    // Two thirds of the way to the highlight at most: a band that reached it
    // would wash the ramp out to near-white and lose the position it is drawn
    // on top of.
    mix(base(t), GLOW, 0.66 * glow(t, phase))
}

/// The part of the track the band covers when there is no position to show.
///
/// Returns `(start, end)` in `[0, 1]`, empty when the band is entirely off the
/// left of the track. It enters from the left and leaves by the right, rather
/// than appearing in the middle.
pub fn band(phase: f32) -> (f32, f32) {
    let travel = 1.0 + BAND_WIDTH;
    let start = phase.clamp(0.0, 1.0) * travel - BAND_WIDTH;
    let end = (start + BAND_WIDTH).clamp(0.0, 1.0);
    (start.clamp(0.0, 1.0), end)
}

/// Blend two colours, `amount` of the way from the first to the second.
fn mix(from: Color32, to: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let channel = |from: u8, to: u8| {
        (f32::from(from) + (f32::from(to) - f32::from(from)) * amount).round() as u8
    };
    Color32::from_rgb(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels(colour: Color32) -> [u8; 3] {
        [colour.r(), colour.g(), colour.b()]
    }

    fn lightness(colour: Color32) -> f32 {
        0.2126 * colour.r() as f32 + 0.7152 * colour.g() as f32 + 0.0722 * colour.b() as f32
    }

    #[test]
    fn the_phase_runs_from_nought_to_one_over_a_cycle() {
        assert!((phase(0.0) - 0.0).abs() < 1e-6);
        assert!((phase(f64::from(PERIOD) / 2.0) - 0.5).abs() < 1e-4);
    }

    /// The clock behind this is `egui`'s, which has been running since the
    /// interface started — hours, on a long cohort. The phase has to stay in
    /// its cycle whatever it is handed.
    #[test]
    fn the_phase_never_leaves_its_cycle() {
        for seconds in [0.0, 0.3, 1.79, 1.8, 12.7, 3600.0, 86_400.0] {
            let phase = phase(seconds);
            assert!(
                (0.0..1.0).contains(&phase),
                "{seconds} s gives a phase of {phase}"
            );
        }
    }

    #[test]
    fn the_cycle_repeats() {
        for seconds in [0.0, 0.4, 1.1] {
            let once = phase(seconds);
            let again = phase(seconds + f64::from(PERIOD));
            assert!((once - again).abs() < 1e-3, "{once} then {again}");
        }
    }

    /// Gold: dark bronze at one end, champagne at the other.
    #[test]
    fn the_ramp_runs_from_bronze_to_champagne() {
        assert_eq!(channels(base(0.0)), channels(LOW));
        assert_eq!(channels(base(1.0)), channels(HIGH));
        assert!(lightness(base(0.0)) < lightness(base(0.5)));
        assert!(lightness(base(0.5)) < lightness(base(1.0)));
    }

    /// Gold is red first, green second, blue last. Anything else on this ramp
    /// is a colour that does not belong to the interface.
    #[test]
    fn every_shade_is_a_gold() {
        for step in 0..=20 {
            let t = step as f32 / 20.0;
            for turn in 0..=10 {
                let [r, g, b] = channels(shade(t, turn as f32 / 10.0));
                assert!(r >= g && g >= b, "({t}, {turn}) is not gold: {r},{g},{b}");
            }
        }
    }

    /// The point of the whole thing: the pattern is somewhere else a moment
    /// later. Measured over the whole bar rather than on one peak, because
    /// there are several peaks and the brightest of them changes hands.
    #[test]
    fn the_pattern_travels() {
        let sample = |phase: f32| -> Vec<f32> {
            (0..=100)
                .map(|step| glow(step as f32 / 100.0, phase))
                .collect()
        };

        let early = sample(0.0);
        let later = sample(0.25);
        let moved: f32 = early
            .iter()
            .zip(&later)
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
            / early.len() as f32;

        assert!(moved > 0.1, "the pattern sat still: {moved:.3} of change");
    }

    /// And it travels in one direction, the same one every cycle: a shimmer
    /// that reverses reads as an error rather than as progress.
    #[test]
    fn the_pattern_travels_forwards() {
        // A peak sits where the sliding position lands on a whole turn. At
        // phase p the first one is at t = p / REPEATS, which grows with p.
        let peak = |phase: f32| {
            (0..=400)
                .map(|step| step as f32 / 400.0)
                .filter(|t| *t < 1.0 / REPEATS)
                .max_by(|a, b| glow(*a, phase).total_cmp(&glow(*b, phase)))
                .unwrap()
        };
        assert!(peak(0.1) < peak(0.2), "the highlight moved backwards");
        assert!(peak(0.2) < peak(0.3), "the highlight moved backwards");
    }

    /// A shimmer that jumps is a shimmer that reads as a fault in the display.
    /// The band wraps around the ends, so the seam is the place to check.
    #[test]
    fn the_highlight_is_continuous_including_at_the_seam() {
        for turn in 0..20 {
            let phase = turn as f32 / 20.0;
            let mut previous = glow(0.0, phase);
            for step in 1..=200 {
                let next = glow(step as f32 / 200.0, phase);
                assert!(
                    (next - previous).abs() < 0.1,
                    "the highlight jumps at {step}/200 of phase {phase}: {previous} to {next}"
                );
                previous = next;
            }
            // And across the wrap: the last point of the bar and the first are
            // neighbours on the circle the highlight travels.
            let seam = (glow(1.0, phase) - glow(0.0, phase)).abs();
            assert!(
                seam < 0.1,
                "the highlight jumps at the seam of phase {phase}"
            );
        }
    }

    #[test]
    fn the_highlight_stays_within_its_bounds() {
        for step in 0..=50 {
            for turn in 0..=50 {
                let value = glow(step as f32 / 50.0, turn as f32 / 50.0);
                assert!((0.0..=1.0).contains(&value), "{value} is outside the range");
            }
        }
    }

    /// The highlight lightens the ramp; it never darkens it. A dark band
    /// travelling along a gold bar reads as damage, not as movement.
    #[test]
    fn the_highlight_only_ever_lightens() {
        for step in 0..=20 {
            let t = step as f32 / 20.0;
            let plain = lightness(base(t));
            for turn in 0..=10 {
                let lit = lightness(shade(t, turn as f32 / 10.0));
                assert!(
                    lit >= plain - 0.5,
                    "({t}, {turn}) went darker: {lit} < {plain}"
                );
            }
        }
    }

    /// With no count to show, a band sweeps the track instead. It has to start
    /// off the left and leave by the right, or it appears out of nowhere in the
    /// middle.
    #[test]
    fn the_band_sweeps_the_whole_track() {
        let (start, end) = band(0.0);
        assert!(end - start < BAND_WIDTH, "the band starts fully on screen");

        let mut covered = [false; 50];
        for turn in 0..=200 {
            let (start, end) = band(turn as f32 / 200.0);
            assert!(start <= end, "the band is inside out at {turn}");
            assert!(
                end - start <= BAND_WIDTH + 1e-6,
                "the band grew: {start}..{end}"
            );
            assert!((0.0..=1.0).contains(&start) && (0.0..=1.0).contains(&end));

            for (index, seen) in covered.iter_mut().enumerate() {
                let point = (index as f32 + 0.5) / 50.0;
                if point >= start && point <= end {
                    *seen = true;
                }
            }
        }
        assert!(
            covered.iter().all(|seen| *seen),
            "part of the track is never reached by the band"
        );
    }

    #[test]
    fn the_band_leaves_by_the_right() {
        let (start, end) = band(0.999);
        assert!(
            start > 0.5,
            "the band is still on the left at the end of a cycle"
        );
        assert!(end <= 1.0);
    }
}
