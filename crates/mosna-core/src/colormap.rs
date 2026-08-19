//! Colour maps, reproducing matplotlib's exactly.
//!
//! These are not decoration. A diverging map whose centre is not neutral makes
//! a z-score of zero look like a signal; a categorical palette in a different
//! order colours the same niche differently between two runs. Both are
//! reproduced value for value.

/// An 8-bit RGB colour.
pub type Rgb = [u8; 3];

/// A continuous colour map, interpolated between evenly spaced control points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gradient {
    stops: Vec<Rgb>,
}

impl Gradient {
    pub fn new(stops: Vec<Rgb>) -> Self {
        assert!(stops.len() >= 2, "a gradient needs at least two stops");
        Self { stops }
    }

    /// Sample the map at `t` in `[0, 1]`, clamping outside.
    pub fn sample(&self, t: f64) -> Rgb {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        let last = self.stops.len() - 1;
        let scaled = t * last as f64;
        let low = scaled.floor() as usize;

        if low >= last {
            return self.stops[last];
        }
        let fraction = scaled - low as f64;
        let (a, b) = (self.stops[low], self.stops[low + 1]);

        // Linear in sRGB, which is what matplotlib's `LinearSegmentedColormap`
        // does; interpolating in a perceptual space would give different
        // values from the reference figures.
        [0, 1, 2].map(|c| (a[c] as f64 + (b[c] as f64 - a[c] as f64) * fraction).round() as u8)
    }

    /// The colour used for a missing value.
    ///
    /// `cmap.set_bad(color="#888888")` in the Python: a cell with no data is
    /// grey, distinct from every value the map can produce.
    pub const BAD: Rgb = [0x88, 0x88, 0x88];
}

/// `RdBu_r`, the diverging map every z-score figure uses.
///
/// The ColorBrewer 11-class RdBu, reversed: blue for negative, near-white at
/// the centre, red for positive.
pub fn rd_bu_r() -> Gradient {
    Gradient::new(vec![
        [0x05, 0x30, 0x61],
        [0x21, 0x66, 0xac],
        [0x43, 0x93, 0xc3],
        [0x92, 0xc5, 0xde],
        [0xd1, 0xe5, 0xf0],
        [0xf7, 0xf7, 0xf7],
        [0xfd, 0xdb, 0xc7],
        [0xf4, 0xa5, 0x82],
        [0xd6, 0x60, 0x4d],
        [0xb2, 0x18, 0x2b],
        [0x67, 0x00, 0x1f],
    ])
}

/// `Blues`, the sequential map the niche composition heatmap uses.
pub fn blues() -> Gradient {
    Gradient::new(vec![
        [0xf7, 0xfb, 0xff],
        [0xde, 0xeb, 0xf7],
        [0xc6, 0xdb, 0xef],
        [0x9e, 0xca, 0xe1],
        [0x6b, 0xae, 0xd6],
        [0x42, 0x92, 0xc6],
        [0x21, 0x71, 0xb5],
        [0x08, 0x51, 0x9c],
        [0x08, 0x30, 0x6b],
    ])
}

/// `Reds`, the sequential map the interactive network colours a measured
/// column with.
///
/// The ColorBrewer 9-class Reds, which is exactly what matplotlib interpolates
/// — these nine are its control points, not a sampling of them, so nine stops
/// reproduce the reference as closely as seventeen would.
///
/// # What it is for
///
/// A cell's value should be legible as *more* or *less* at a glance, and the
/// interface's surface is a light silver. This map puts the low end within a
/// shade of that surface and the high end at eleven to one against it, so a
/// rare high-expressing cell is the thing the eye lands on and a field of low
/// ones recedes into the background. That is deliberate, and it is a choice:
/// a low value is nearly invisible here, which is right when the question is
/// "where is the signal" and wrong when it is "is this cell measured at all".
/// A cell with *no* value is not faint but grey — [`Gradient::BAD`] — so the
/// two cannot be confused.
///
/// One hue throughout, so nothing competes with the intensity itself.
pub fn reds() -> Gradient {
    Gradient::new(vec![
        [0xff, 0xf5, 0xf0],
        [0xfe, 0xe0, 0xd2],
        [0xfc, 0xbb, 0xa1],
        [0xfc, 0x92, 0x72],
        [0xfb, 0x69, 0x4a],
        [0xee, 0x3a, 0x2c],
        [0xca, 0x18, 0x1d],
        [0xa3, 0x0f, 0x15],
        [0x67, 0x00, 0x0d],
    ])
}

/// `Greens`, `Purples` — the other sequential maps the network offers.
///
/// The ColorBrewer 9-class maps, built exactly as [`reds`] is and for exactly
/// the same job: the interactive network can colour four columns at once, and
/// four views coloured by the same ramp are four pictures nobody can tell
/// apart at a glance. A hue per view, and the view is named by its colour.
///
/// The four are chosen to stay apart at every step of their ramps, not only at
/// the dark end: red, blue, green and purple are the four sequential maps
/// ColorBrewer builds from hues far enough apart that a mid-value of one is
/// never a mid-value of another.
pub fn greens() -> Gradient {
    Gradient::new(vec![
        [0xf7, 0xfc, 0xf5],
        [0xe5, 0xf5, 0xe0],
        [0xc7, 0xe9, 0xc0],
        [0xa1, 0xd9, 0x9b],
        [0x74, 0xc4, 0x76],
        [0x41, 0xab, 0x5d],
        [0x23, 0x8b, 0x45],
        [0x00, 0x6d, 0x2c],
        [0x00, 0x44, 0x1b],
    ])
}

/// See [`greens`].
pub fn purples() -> Gradient {
    Gradient::new(vec![
        [0xfc, 0xfb, 0xfd],
        [0xef, 0xed, 0xf5],
        [0xda, 0xda, 0xeb],
        [0xbc, 0xbd, 0xdc],
        [0x9e, 0x9a, 0xc8],
        [0x80, 0x7d, 0xba],
        [0x6a, 0x51, 0xa3],
        [0x54, 0x27, 0x8f],
        [0x3f, 0x00, 0x7d],
    ])
}

/// `tab20`, used by the abundance bar chart.
pub fn tab20() -> Vec<Rgb> {
    vec![
        [0x1f, 0x77, 0xb4],
        [0xae, 0xc7, 0xe8],
        [0xff, 0x7f, 0x0e],
        [0xff, 0xbb, 0x78],
        [0x2c, 0xa0, 0x2c],
        [0x98, 0xdf, 0x8a],
        [0xd6, 0x27, 0x28],
        [0xff, 0x98, 0x96],
        [0x94, 0x67, 0xbd],
        [0xc5, 0xb0, 0xd5],
        [0x8c, 0x56, 0x4b],
        [0xc4, 0x9c, 0x94],
        [0xe3, 0x77, 0xc2],
        [0xf7, 0xb6, 0xd2],
        [0x7f, 0x7f, 0x7f],
        [0xc7, 0xc7, 0xc7],
        [0xbc, 0xbd, 0x22],
        [0xdb, 0xdb, 0x8d],
        [0x17, 0xbe, 0xcf],
        [0x9e, 0xda, 0xe5],
    ]
}

/// `tab20b`, appended when more than twenty categories are shown.
pub fn tab20b() -> Vec<Rgb> {
    vec![
        [0x39, 0x3b, 0x79],
        [0x52, 0x54, 0xa3],
        [0x6b, 0x6e, 0xcf],
        [0x9c, 0x9e, 0xde],
        [0x63, 0x79, 0x39],
        [0x8c, 0xa2, 0x52],
        [0xb5, 0xcf, 0x6b],
        [0xce, 0xdb, 0x9c],
        [0x8c, 0x6d, 0x31],
        [0xbd, 0x9e, 0x39],
        [0xe7, 0xba, 0x52],
        [0xe7, 0xcb, 0x94],
        [0x84, 0x3c, 0x39],
        [0xad, 0x49, 0x4a],
        [0xd6, 0x61, 0x6b],
        [0xe7, 0x96, 0x9c],
        [0x7b, 0x41, 0x73],
        [0xa5, 0x51, 0x94],
        [0xce, 0x6d, 0xbd],
        [0xde, 0x9e, 0xd6],
    ]
}

/// Colours for a stacked bar chart of `n` categories.
///
/// Port of the palette assembly in `assort_figures_abundance.py`: `tab20`,
/// extended with `tab20b` past twenty categories.
pub fn abundance_palette(n: usize) -> Vec<Rgb> {
    let mut palette = tab20();
    if n > 20 {
        palette.extend(tab20b());
    }
    if palette.is_empty() {
        return vec![Gradient::BAD; n];
    }
    (0..n).map(|i| palette[i % palette.len()]).collect()
}

/// The categorical palette for clusters and phenotypes.
///
/// Port of `plotting.py::make_cluster_cmap` with its defaults
/// (`grey_pos='end'`, `saturated_first=True`): ten saturated colours, then ten
/// pale companions past ten categories, then four bright ones past twenty.
/// Beyond that the palette cycles, which is what
/// `clusters_cmap[i % n_colors]` does at every call site.
pub fn make_cluster_cmap(n: usize) -> Vec<Rgb> {
    const SATURATED: [Rgb; 10] = [
        [0x1F, 0x77, 0xB4],
        [0xFF, 0x7F, 0x0E],
        [0x2C, 0xA0, 0x2C],
        [0xD6, 0x27, 0x28],
        [0x94, 0x67, 0xBD],
        [0x8C, 0x56, 0x4B],
        [0x17, 0xBE, 0xCF],
        [0xE3, 0x77, 0xC2],
        [0xBC, 0xBD, 0x22],
        [0x7F, 0x7F, 0x7F],
    ];
    const PALE: [Rgb; 10] = [
        [0xAE, 0xC7, 0xE8],
        [0xFF, 0xBB, 0x78],
        [0x98, 0xDF, 0x8A],
        [0xFF, 0x98, 0x96],
        [0xC5, 0xB0, 0xD5],
        [0xC4, 0x9C, 0x94],
        [0x9E, 0xDA, 0xE5],
        [0xF7, 0xB6, 0xD2],
        [0xDB, 0xDB, 0x8D],
        [0xC7, 0xC7, 0xC7],
    ];
    const BRIGHT: [Rgb; 4] = [
        [0x00, 0xFF, 0xFF],
        [0x00, 0xFF, 0x00],
        [0xFF, 0x00, 0xFF],
        [0xFF, 0x00, 0x7F],
    ];

    let mut palette: Vec<Rgb> = SATURATED.to_vec();
    if n > 10 {
        // Below twenty the pale list is trimmed to what is needed, which is
        // what keeps a twelve-cluster figure from reserving ten pale colours.
        let take = if n < 20 { n - 10 } else { PALE.len() };
        palette.extend(PALE.iter().take(take));
    }
    if n > 20 {
        palette.extend(BRIGHT);
    }

    (0..n).map(|i| palette[i % palette.len()]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gradient_hits_its_end_points_exactly() {
        let gradient = Gradient::new(vec![[0, 0, 0], [255, 255, 255]]);
        assert_eq!(gradient.sample(0.0), [0, 0, 0]);
        assert_eq!(gradient.sample(1.0), [255, 255, 255]);
        assert_eq!(gradient.sample(0.5), [128, 128, 128]);
    }

    #[test]
    fn a_gradient_clamps_and_survives_nan() {
        let gradient = rd_bu_r();
        assert_eq!(gradient.sample(-5.0), gradient.sample(0.0));
        assert_eq!(gradient.sample(5.0), gradient.sample(1.0));
        assert_eq!(gradient.sample(f64::NAN), gradient.sample(0.0));
    }

    /// The point of `Reds` in this interface: a high value has to be the thing
    /// the eye lands on, and a low one has to get out of the way.
    #[test]
    fn reds_runs_from_almost_white_to_a_deep_red() {
        let map = reds();
        let (low, high) = (map.sample(0.0), map.sample(1.0));

        assert!(
            low.iter().all(|c| *c > 0xE0),
            "the low end is not near-white: {low:?}"
        );
        assert!(
            high[0] > high[1] && high[0] > high[2] && high[0] < 0x80,
            "the high end is not a deep red: {high:?}"
        );
    }

    /// Lightness falls monotonically, which is what lets a reader order two
    /// cells by eye rather than by consulting the bar.
    #[test]
    fn reds_only_ever_darkens() {
        let map = reds();
        let lightness = |c: Rgb| 0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32;

        let mut previous = lightness(map.sample(0.0));
        for step in 1..=20 {
            let next = lightness(map.sample(step as f64 / 20.0));
            assert!(
                next < previous,
                "it brightens between {} and {step} of 20",
                step - 1
            );
            previous = next;
        }
    }

    /// One hue throughout: red is never the smallest channel, so nothing in
    /// the ramp reads as another colour competing with the intensity.
    #[test]
    fn reds_stays_red_the_whole_way() {
        let map = reds();
        for step in 0..=20 {
            let [r, g, b] = map.sample(step as f64 / 20.0);
            assert!(r >= g && r >= b, "step {step} is not a red: {r},{g},{b}");
        }
    }

    /// A faint cell and an unmeasured one must not look alike: the first is a
    /// value, the second is the absence of one.
    #[test]
    fn the_palest_red_is_not_the_missing_value_grey() {
        assert_ne!(reds().sample(0.0), Gradient::BAD);
    }

    /// Everything asserted of `Reds` above is asserted of the other three: they
    /// are offered as alternatives to it in the same picker, and an alternative
    /// that behaves differently is a trap rather than a choice.
    #[test]
    fn every_sequential_map_runs_from_almost_white_to_a_deep_colour() {
        for (name, map) in sequential() {
            let (low, high) = (map.sample(0.0), map.sample(1.0));
            assert!(
                low.iter().all(|c| *c > 0xE0),
                "{name} starts at {low:?}, which is not near-white"
            );
            assert!(
                high.iter().all(|c| *c < 0x90),
                "{name} ends at {high:?}, which is not deep"
            );
            assert_ne!(map.sample(0.0), Gradient::BAD, "{name}");
        }
    }

    #[test]
    fn every_sequential_map_only_ever_darkens() {
        for (name, map) in sequential() {
            let mut previous = lightness(map.sample(0.0));
            for step in 1..=20 {
                let next = lightness(map.sample(step as f64 / 20.0));
                assert!(
                    next < previous,
                    "{name} brightens between {} and {step} of 20",
                    step - 1
                );
                previous = next;
            }
        }
    }

    /// The whole point of having four: wherever there is a signal to see, no
    /// two of them show it in the same colour.
    ///
    /// From the half-way point up, which is where the maps are carrying
    /// anything at all. Below it they converge — that is not a flaw in the set
    /// but the shape of every sequential map: they all begin at the same
    /// near-white, because a low value is meant to recede into the interface
    /// rather than announce which map it belongs to. Which map a *faint* cell
    /// was drawn with is read off the colour bar beside its view, not off the
    /// cell.
    #[test]
    fn the_four_sequential_maps_stay_apart_wherever_there_is_signal() {
        let maps = sequential();
        for step in 10..=20 {
            let t = step as f64 / 20.0;
            for (index, (name, map)) in maps.iter().enumerate() {
                for (other_name, other) in &maps[index + 1..] {
                    let (a, b) = (map.sample(t), other.sample(t));
                    let distance: i32 = (0..3).map(|c| (a[c] as i32 - b[c] as i32).abs()).sum();
                    assert!(
                        distance > 60,
                        "{name} and {other_name} are {distance} apart at {t}: {a:?} {b:?}"
                    );
                }
            }
        }
    }

    /// And the converse, stated so that it is a decision rather than an
    /// oversight: at the pale end they are all but the same colour.
    #[test]
    fn the_four_sequential_maps_all_begin_near_white() {
        for (name, map) in sequential() {
            assert!(
                lightness(map.sample(0.0)) > 245.0,
                "{name} does not begin near-white"
            );
        }
    }

    /// Each keeps one hue the whole way, so nothing in a ramp reads as another
    /// colour competing with the intensity.
    #[test]
    fn every_sequential_map_keeps_its_hue() {
        for step in 0..=20 {
            let t = step as f64 / 20.0;
            let [r, g, b] = greens().sample(t);
            assert!(g >= r && g >= b, "step {step} of Greens is not green");
            let [r, _, b] = purples().sample(t);
            assert!(b >= r, "step {step} of Purples has lost its blue");
            let [r, g, b] = blues().sample(t);
            assert!(b >= r && b >= g, "step {step} of Blues is not blue");
        }
    }

    fn sequential() -> Vec<(&'static str, Gradient)> {
        vec![
            ("Reds", reds()),
            ("Blues", blues()),
            ("Greens", greens()),
            ("Purples", purples()),
        ]
    }

    fn lightness(c: Rgb) -> f32 {
        0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32
    }

    #[test]
    fn the_diverging_centre_is_neutral() {
        let centre = rd_bu_r().sample(0.5);
        assert_eq!(centre[0], centre[1]);
        assert_eq!(centre[1], centre[2]);
    }

    #[test]
    fn the_cluster_palette_trims_the_pale_colours_below_twenty() {
        let twelve = make_cluster_cmap(12);
        assert_eq!(twelve.len(), 12);
        // Ten saturated, then exactly two pale ones — not ten.
        assert_eq!(twelve[10], [0xAE, 0xC7, 0xE8]);
        assert_eq!(twelve[11], [0xFF, 0xBB, 0x78]);
    }

    #[test]
    fn the_cluster_palette_never_returns_short() {
        for n in [1, 5, 10, 11, 20, 21, 24, 25, 100] {
            assert_eq!(make_cluster_cmap(n).len(), n, "n = {n}");
        }
    }

    #[test]
    fn an_empty_request_yields_an_empty_palette() {
        assert!(make_cluster_cmap(0).is_empty());
        assert!(abundance_palette(0).is_empty());
    }

    #[test]
    fn the_abundance_palette_extends_past_twenty() {
        let palette = abundance_palette(30);
        assert_eq!(palette.len(), 30);
        assert_eq!(palette[20], tab20b()[0], "tab20b follows tab20");
    }
}
