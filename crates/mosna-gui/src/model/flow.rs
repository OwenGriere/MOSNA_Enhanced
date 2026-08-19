//! The brushed gold of the progress bar.
//!
//! A bar that only grows says how far along a step is. It does not say whether
//! anything is still happening — and the steps here spend minutes between two
//! increments, on a cohort of two hundred samples. So the fill is a piece of
//! metal rather than a coloured rectangle: a gold ramp carrying a fixed grain,
//! with a single wide reflection travelling slowly across it. The position is
//! the progress, the light passing is the pulse. Stop the process and the light
//! stops with it; the metal stays.
//!
//! What this replaced was three highlights sliding along the bar on a cycle of
//! under two seconds. It read as a flux — something pouring through the bar —
//! rather than as work being done, because a pattern that repeats along its own
//! length and comes round again that quickly is a pattern, and the eye follows
//! patterns. Here nothing repeats along the bar: the grain does not move, and
//! there is only ever one reflection.
//!
//! All of it is arithmetic on a clock, which is why it lives here rather than
//! in the panel that paints it: a reflection that flickers, jumps at the seam of
//! its cycle or leaves the palette is a bug, and none of those can be seen in a
//! screenshot.

use egui::Color32;

use crate::theme;

/// How long one reflection takes to cross the bar, in seconds.
///
/// Slow: this is light moving over a surface, not a signal being sent. Six
/// seconds is unhurried enough that the eye never tries to follow it, and often
/// enough that a glance a few seconds later finds the bar in a different state.
pub const PERIOD: f32 = 6.0;

/// The reflection's half-width, as a share of the fill.
///
/// Wide, and deliberately so: a narrow band travelling along a bar is a marker,
/// and a marker on a progress bar claims to mean something. This one is broad
/// enough that no edge of it can be pointed at.
pub const SHEEN_WIDTH: f32 = 0.30;

/// How far past each end of the fill the reflection begins and ends, as a share
/// of the fill.
///
/// Far enough that it has faded to nothing before the cycle turns over: the
/// moment the phase wraps is the one place a travelling light can be caught
/// jumping, and it cannot jump if there is nothing there to jump.
const SHEEN_MARGIN: f32 = 0.9;

/// How far towards [`GLOW`] the reflection carries the metal at its brightest.
///
/// Not all the way: a reflection that reached the highlight would wash the ramp
/// out to near-white and lose the position it is drawn on top of.
pub const SHEEN: f32 = 0.28;

/// The same, on a bar with no count to show.
///
/// Stronger, because it is passing over a wash rather than over solid metal,
/// and because there it is the only thing saying anything at all.
pub const SHEEN_UNCOUNTED: f32 = 0.34;

/// How far from the track's silver towards the gold the wash sits.
///
/// A bar that is working but cannot say how far along it is shows the whole
/// track tinted, not a second bar filling it: the tint has to read as the track
/// under light, which means staying close enough to the silver that nobody
/// mistakes it for a fill at a hundred per cent.
pub const WASH: f32 = 0.42;

/// The dark end of the ramp.
pub const LOW: Color32 = Color32::from_rgb(0x7C, 0x5F, 0x10);
/// The light end.
pub const HIGH: Color32 = Color32::from_rgb(0xCF, 0xB0, 0x5F);
/// The reflection's own colour.
pub const GLOW: Color32 = Color32::from_rgb(0xFF, 0xF3, 0xD0);
/// The silver the wash is mixed into: the track the bar is drawn on.
pub const TRACK: Color32 = theme::SURFACE;

/// The striations of the brushed surface: wavelength in points, amplitude,
/// offset.
///
/// Two of them, at lengths that do not divide into one another, so the surface
/// never shows the same stretch of grain twice along one bar. The amplitudes
/// are under two per cent between them — at three, the stripes stop reading as
/// a surface and start reading as a texture someone applied.
const GRAIN: [(f32, f32, f32); 2] = [(6.3, 0.011, 0.0), (17.5, 0.008, 1.2)];

/// How much brighter the top edge of the metal is than its middle.
pub const BEVEL_TOP: f32 = 0.075;
/// How much darker the bottom edge is.
///
/// Less than the top gains: the light is above the bar, so the lit edge is the
/// event and the shaded one is only its consequence.
pub const BEVEL_BOTTOM: f32 = 0.055;
/// How quickly the bevel falls back to the middle.
///
/// Above two, the change is held near the edges and the middle of the bar stays
/// flat — which is what a bevel is. At one it would be a plain gradient from
/// top to bottom, and the bar would read as tilted rather than as raised.
const BEVEL_FALLOFF: f32 = 2.2;

/// Where in the cycle the reflection is, from the clock, in `[0, 1)`.
///
/// The clock is `egui`'s, which has been running since the interface started —
/// hours, on a long cohort — so the remainder is taken rather than assumed.
pub fn phase(seconds: f64) -> f32 {
    let turns = seconds / f64::from(PERIOD);
    (turns - turns.floor()) as f32
}

/// The ramp at `t`, with no light on it and no grain.
pub fn base(t: f32) -> Color32 {
    mix(LOW, HIGH, t.clamp(0.0, 1.0))
}

/// How strongly the reflection falls at `t`, in `[0, 1]`.
///
/// One reflection, crossing once per cycle, entering and leaving well off the
/// ends of the fill. A Gaussian rather than a band with edges: an edge is
/// something the eye can lock onto, and a light that can be locked onto is
/// being followed rather than noticed.
pub fn sheen(t: f32, phase: f32) -> f32 {
    let travel = 1.0 + 2.0 * SHEEN_MARGIN;
    let centre = phase.clamp(0.0, 1.0) * travel - SHEEN_MARGIN;
    let distance = (t.clamp(0.0, 1.0) - centre) / SHEEN_WIDTH;
    (-distance * distance).exp()
}

/// The grain's lightness at `x` points from the bar's left edge.
///
/// Measured from the *bar*, not from the end of the fill: a brushed surface is
/// brushed once and stays that way, so the stripes have to sit still while the
/// fill grows past them. Taking `x` from the fill instead would drag the whole
/// grain along behind every increment — a flux again, and a slower, stranger
/// one.
pub fn grain(x: f32) -> f32 {
    1.0 + GRAIN
        .iter()
        .map(|(wavelength, amplitude, offset)| {
            amplitude * (std::f32::consts::TAU * x / wavelength + offset).sin()
        })
        .sum::<f32>()
}

/// The vertical profile of the metal, as a lightness factor.
///
/// `u` runs from the top edge of the bar to the bottom. Neutral through the
/// middle, so the profile can be faded out at the round ends without the middle
/// of the bar changing colour as it goes.
pub fn bevel(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    1.0 + BEVEL_TOP * (1.0 - u).powf(BEVEL_FALLOFF) - BEVEL_BOTTOM * u.powf(BEVEL_FALLOFF)
}

/// The colour of the metal at `t` along the fill, `x` points from the bar's
/// left edge, with the light where the phase puts it.
pub fn shade(t: f32, x: f32, phase: f32) -> Color32 {
    lit(mix(base(t), GLOW, SHEEN * sheen(t, phase)), grain(x))
}

/// The metal with no light crossing it.
///
/// What a finished run leaves on the screen: the ramp and its grain stay, the
/// movement stops. A bar that keeps catching the light after the process has
/// exited says the wrong thing.
pub fn still(t: f32, x: f32) -> Color32 {
    lit(base(t), grain(x))
}

/// The colour at `t` of a bar that is working but has no count to show.
///
/// The track, tinted gold, with the same grain and the same reflection crossing
/// it. It claims no position — the whole track is tinted — and says the one
/// thing it does know, which is that something is still happening.
pub fn wash(t: f32, x: f32, phase: f32) -> Color32 {
    let tint = mix(TRACK, base(t), WASH);
    lit(mix(tint, GLOW, SHEEN_UNCOUNTED * sheen(t, phase)), grain(x))
}

/// Scale a colour's lightness, keeping its hue.
///
/// Multiplicative on each channel, so gold stays gold: the ratios between red,
/// green and blue are what make the colour, and adding a constant to all three
/// would drain it towards grey at one end and towards white at the other.
pub fn lit(colour: Color32, factor: f32) -> Color32 {
    let channel = |value: u8| (f32::from(value) * factor).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(channel(colour.r()), channel(colour.g()), channel(colour.b()))
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
        for seconds in [0.0, 0.3, 5.99, 6.0, 12.7, 3600.0, 86_400.0] {
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

    /// Gold is red first, green second, blue last. Anything else on this bar is
    /// a colour that does not belong to the interface — and the grain and the
    /// bevel both scale the channels, which is exactly the operation that keeps
    /// that order true.
    #[test]
    fn every_shade_is_a_gold() {
        for step in 0..=20 {
            let t = step as f32 / 20.0;
            for turn in 0..=10 {
                let phase = turn as f32 / 10.0;
                for point in 0..=20 {
                    let x = point as f32 * 3.0;
                    for row in 0..=4 {
                        let u = row as f32 / 4.0;
                        for colour in [
                            lit(shade(t, x, phase), bevel(u)),
                            lit(wash(t, x, phase), bevel(u)),
                            lit(still(t, x), bevel(u)),
                        ] {
                            let [r, g, b] = channels(colour);
                            assert!(
                                r >= g && g >= b,
                                "({t}, {x}, {phase}, {u}) is not gold: {r},{g},{b}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// The point of the change: one reflection, not three. A second peak
    /// anywhere on the bar is the pattern coming back.
    #[test]
    fn only_one_reflection_is_ever_on_the_bar() {
        for turn in 0..=40 {
            let phase = turn as f32 / 40.0;
            let sampled: Vec<f32> = (0..=400)
                .map(|step| sheen(step as f32 / 400.0, phase))
                .collect();
            let peaks = sampled
                .windows(3)
                .filter(|window| {
                    // A peak worth counting: above its neighbours, and bright
                    // enough to be seen at all once it is mixed in at `SHEEN`.
                    window[1] > window[0] && window[1] >= window[2] && window[1] > 0.05
                })
                .count();
            assert!(peaks <= 1, "{peaks} reflections at phase {phase}");
        }
    }

    /// And it crosses forwards, the same way every cycle: a light that reverses
    /// reads as an error rather than as progress.
    #[test]
    fn the_reflection_crosses_forwards() {
        let brightest = |phase: f32| {
            (0..=400)
                .map(|step| step as f32 / 400.0)
                .max_by(|a, b| sheen(*a, phase).total_cmp(&sheen(*b, phase)))
                .unwrap()
        };
        // Sampled inside the stretch of the cycle where the reflection is on
        // the bar at all; off either end every point of the bar is equally dark
        // and there is no brightest one to speak of.
        assert!(brightest(0.35) < brightest(0.5), "the light went backwards");
        assert!(brightest(0.5) < brightest(0.65), "the light went backwards");
    }

    /// Over a cycle the light reaches everywhere: a stretch of bar that is
    /// never lit is a stretch that looks dead while the rest moves.
    #[test]
    fn the_reflection_reaches_the_whole_bar() {
        for step in 0..=40 {
            let t = step as f32 / 40.0;
            let best = (0..=200)
                .map(|turn| sheen(t, turn as f32 / 200.0))
                .fold(0.0f32, f32::max);
            assert!(best > 0.9, "{t} is only ever lit to {best}");
        }
    }

    /// A reflection that jumps is one that reads as a fault in the display. The
    /// cycle turns over at the seam, so the seam is the place to check: at both
    /// ends of it the light has to be off the bar entirely.
    #[test]
    fn the_reflection_is_continuous_including_at_the_seam() {
        for turn in 0..40 {
            let phase = turn as f32 / 40.0;
            let mut previous = sheen(0.0, phase);
            for step in 1..=200 {
                let next = sheen(step as f32 / 200.0, phase);
                assert!(
                    (next - previous).abs() < 0.1,
                    "the light jumps at {step}/200 of phase {phase}: {previous} to {next}"
                );
                previous = next;
            }
        }
        for step in 0..=200 {
            let t = step as f32 / 200.0;
            let before = sheen(t, 0.999);
            let after = sheen(t, 0.0);
            assert!(
                (before - after).abs() < 0.01,
                "the light jumps at the seam of the cycle at {t}: {before} to {after}"
            );
        }
    }

    #[test]
    fn the_reflection_stays_within_its_bounds() {
        for step in 0..=50 {
            for turn in 0..=50 {
                let value = sheen(step as f32 / 50.0, turn as f32 / 50.0);
                assert!((0.0..=1.0).contains(&value), "{value} is outside the range");
            }
        }
    }

    /// The light lightens the metal; it never darkens it. A dark band crossing
    /// a gold bar reads as damage, not as movement.
    #[test]
    fn the_reflection_only_ever_lightens() {
        for step in 0..=20 {
            let t = step as f32 / 20.0;
            let plain = lightness(still(t, 0.0));
            for turn in 0..=20 {
                let lit = lightness(shade(t, 0.0, turn as f32 / 20.0));
                assert!(
                    lit >= plain - 0.5,
                    "({t}, {turn}) went darker: {lit} < {plain}"
                );
            }
        }
    }

    /// The grain is a property of the bar, not of the light on it: its stripes
    /// have to be in the same places at every moment of the cycle. This is what
    /// separates a surface from a second, slower flux.
    #[test]
    fn the_grain_stays_where_it_is() {
        let stripes = |phase: f32| {
            (0..400)
                .map(|step| lightness(shade(0.5, step as f32 * 0.25, phase)))
                .collect::<Vec<_>>()
        };
        let early = stripes(0.2);
        let later = stripes(0.7);
        let brightest = |sample: &[f32]| {
            sample
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                .unwrap()
                .0
        };
        assert_eq!(
            brightest(&early),
            brightest(&later),
            "the grain moved with the light"
        );
    }

    /// And it is a grain, not a wave: shallow enough to be a surface, deep
    /// enough to be there at all.
    #[test]
    fn the_grain_is_shallow() {
        let mut lowest = f32::MAX;
        let mut highest = f32::MIN;
        for step in 0..2000 {
            let value = grain(step as f32 * 0.1);
            lowest = lowest.min(value);
            highest = highest.max(value);
        }
        assert!(highest - lowest > 0.01, "the grain is invisible");
        assert!(highest - lowest < 0.05, "the grain is a texture, not a metal");
        assert!((0.95..=1.05).contains(&lowest) && (0.95..=1.05).contains(&highest));
    }

    /// The bevel is faded out at the round ends of the bar, and it can only be
    /// faded to nothing if nothing is what the middle of the bar already has.
    #[test]
    fn the_bevel_is_neutral_through_the_middle() {
        assert!(
            (bevel(0.5) - 1.0).abs() < 0.01,
            "the middle of the bar is {} of its own colour",
            bevel(0.5)
        );
    }

    /// Lit from above: brightest at the top edge, darkest at the bottom, and
    /// never the other way round anywhere in between.
    #[test]
    fn the_bevel_is_lit_from_above() {
        assert!(bevel(0.0) > 1.0 && bevel(1.0) < 1.0);
        let mut previous = bevel(0.0);
        for step in 1..=100 {
            let next = bevel(step as f32 / 100.0);
            assert!(next <= previous + 1e-6, "the bevel rises again at {step}/100");
            previous = next;
        }
    }

    #[test]
    fn the_bevel_stays_shallow() {
        for step in 0..=100 {
            let value = bevel(step as f32 / 100.0);
            assert!((0.9..=1.1).contains(&value), "{value} is too strong");
        }
    }

    /// A run with no count still has to look like a bar that is doing
    /// something, which means the wash has to be visibly gold — and still like
    /// a bar that is not claiming to be finished, which means it has to stay
    /// well short of the metal.
    #[test]
    fn the_wash_sits_between_the_track_and_the_metal() {
        for step in 0..=20 {
            let t = step as f32 / 20.0;
            let washed = wash(t, 0.0, 0.0);
            assert!(
                (lightness(TRACK) - lightness(washed)).abs() > 10.0,
                "the wash at {t} is invisible against the track"
            );
            assert!(
                lightness(washed) > lightness(still(t, 0.0)),
                "the wash at {t} is as dark as the metal"
            );
        }
    }

    /// The failed run reuses `lit` on a colour that is not gold at all, so the
    /// scaling has to be safe on any colour and at any factor the grain and the
    /// bevel can produce.
    #[test]
    fn scaling_a_colour_keeps_it_in_range() {
        for colour in [LOW, HIGH, GLOW, TRACK, Color32::WHITE, Color32::BLACK] {
            for factor in [0.0, 0.9, 1.0, 1.1, 4.0] {
                let scaled = lit(colour, factor);
                if (factor - 1.0).abs() < f32::EPSILON {
                    assert_eq!(channels(scaled), channels(colour), "1.0 changed {colour:?}");
                }
                assert!(lightness(scaled) <= lightness(Color32::WHITE) + 0.5);
            }
        }
    }
}
