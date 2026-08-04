//! The silver-and-gold palette.
//!
//! Silver carries the interface — the page, the panels, the boxes, the rules —
//! and gold is spent only on what the user is meant to act on or has already
//! chosen: the panel titles, the primary buttons, the selected row, the running
//! step. Everything that used to be gold merely to look expensive (group-box
//! titles, table headers, hover outlines, the manual's code blocks) is silver
//! or graphite, so the accent still means something when it appears.

use egui::{Color32, CornerRadius, Stroke, Vec2, Visuals};

/// Page background: the darkest silver, so the panels sitting on it read as
/// raised rather than as holes.
pub const BACKGROUND: Color32 = Color32::from_rgb(0xC4, 0xC8, 0xCD);
/// Panel background, one step lighter than the page.
pub const PANEL: Color32 = Color32::from_rgb(0xD8, 0xDB, 0xDF);
/// Raised surfaces: group boxes, table rows, the manual's navigation.
pub const SURFACE: Color32 = Color32::from_rgb(0xE7, 0xE9, 0xEC);
/// Hovered surface.
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0xF2, 0xF3, 0xF5);
/// What the user types into, and what the program writes back: text fields, the
/// log, the manual's code blocks. Near-white, so a field is legible as a field.
pub const FIELD: Color32 = Color32::from_rgb(0xF7, 0xF8, 0xFA);
/// Hairlines and separators.
pub const BORDER: Color32 = Color32::from_rgb(0xA5, 0xAB, 0xB2);
/// Darkened silver, for the emphasis that does not deserve the accent: group
/// titles, the outline under the pointer, the step that only clears files.
pub const STEEL: Color32 = Color32::from_rgb(0x6D, 0x74, 0x7C);

/// The accent: an antique gold, dark enough to be read on silver.
///
/// A metallic gold — the one in the logo — is barely two to one against these
/// backgrounds, so it is reserved for *fills*, where the text on top provides
/// the contrast instead. See [`ACCENT_SOFT`].
pub const ACCENT: Color32 = Color32::from_rgb(0x75, 0x59, 0x0C);
/// A deeper gold, for the element under the pointer or already chosen: on a
/// light background emphasis means more contrast, not more brightness.
pub const ACCENT_STRONG: Color32 = Color32::from_rgb(0x5E, 0x47, 0x08);
/// The metallic gold, used only as a fill — a pressed control, a selection,
/// the progress bar — with dark text over it.
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0xC7, 0xA5, 0x4A);

/// Body text: near-black with a cool cast, matching the silver.
pub const TEXT: Color32 = Color32::from_rgb(0x1B, 0x1F, 0x23);
/// Secondary text.
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x5B, 0x62, 0x6A);
/// Text drawn on a dark fill — a bronze step button, a failed run.
pub const TEXT_INVERSE: Color32 = Color32::from_rgb(0xF4, 0xF6, 0xF8);

/// Log colours.
///
/// Error, warning and success keep their conventional hues: they carry meaning
/// that a monochrome would destroy, and a user scanning a log needs red to mean
/// red. They are darkened from the values a dark theme wants, because the log
/// is now near-white. Information and progress, which have no conventional
/// colour, are on the accent axis — those were blue and purple in the Python.
pub const LOG_ERROR: Color32 = Color32::from_rgb(0xA3, 0x24, 0x1B);
pub const LOG_WARNING: Color32 = Color32::from_rgb(0x8A, 0x5A, 0x05);
pub const LOG_SUCCESS: Color32 = Color32::from_rgb(0x1D, 0x6B, 0x3F);
pub const LOG_INFO: Color32 = ACCENT;
pub const LOG_PROGRESS: Color32 = ACCENT_STRONG;
pub const LOG_PLAIN: Color32 = TEXT_MUTED;

/// The colour of each step's button and of the progress bar while it runs.
///
/// The Python gave the four steps four unrelated hues (blue, purple, green,
/// maroon) so that the progress bar told you which step was running. That cue
/// is kept, but expressed as a progression through the gold range — bronze,
/// gold, champagne — so it stays on palette while remaining distinguishable.
/// Clearing is not an analysis, so it stays off the accent, on graphite.
pub const STEP_TYSSERAND: Color32 = Color32::from_rgb(0x6B, 0x52, 0x0C);
pub const STEP_ASSORTATIVITY: Color32 = Color32::from_rgb(0xA8, 0x86, 0x2A);
pub const STEP_NICHES: Color32 = Color32::from_rgb(0xDC, 0xC2, 0x72);
pub const STEP_CLEAR: Color32 = Color32::from_rgb(0x4E, 0x55, 0x5C);
/// A failed run turns the progress bar this colour.
pub const STEP_FAILED: Color32 = Color32::from_rgb(0xA3, 0x24, 0x1B);

/// The sizing scale.
///
/// One named step per role, rather than a literal at each call site: that is
/// how an interface ends up with an eleven-pixel label beside a twelve-pixel
/// one for no reason anybody remembers. The steps are at least a pixel and a
/// half apart, so each reads as a level of the hierarchy rather than as a
/// mistake.
pub mod size {
    /// The title of a page of the manual.
    pub const PAGE_TITLE: f32 = 28.0;
    /// The name of a panel — Browser, Viewer, Parameters.
    ///
    /// Its own step rather than the section title's: these three name the whole
    /// column under them, and at the section size they read as one heading
    /// among the many inside the panel instead of as the panel's own title.
    pub const PANEL_TITLE: f32 = 24.0;
    /// A section title in the manual.
    pub const SECTION_TITLE: f32 = 20.0;
    /// A sub-heading inside a page, or a group box's title.
    pub const HEADING: f32 = 17.0;
    /// Running text.
    pub const BODY: f32 = 15.5;
    /// A control's label, a tab, a table cell.
    pub const LABEL: f32 = 14.0;
    /// A caption or a hint: secondary, never primary.
    pub const SMALL: f32 = 13.0;
    /// The log and the manual's code blocks.
    pub const MONO: f32 = 13.5;

    /// The scale in descending order, which is the order it has to be in.
    ///
    /// Returned as data rather than compared constant against constant, so the
    /// property being checked is "this hierarchy descends" and not "26 is more
    /// than 20" — which the compiler already knows and no test needs to say.
    pub fn hierarchy() -> [(&'static str, f32); 7] {
        [
            ("PAGE_TITLE", PAGE_TITLE),
            ("PANEL_TITLE", PANEL_TITLE),
            ("SECTION_TITLE", SECTION_TITLE),
            ("HEADING", HEADING),
            ("BODY", BODY),
            ("LABEL", LABEL),
            ("SMALL", SMALL),
        ]
    }

    /// Every step, including the monospace size, which sits outside the
    /// hierarchy: code is not a level of emphasis.
    pub fn all() -> [(&'static str, f32); 8] {
        let [a, b, c, d, e, f, g] = hierarchy();
        [a, b, c, d, e, f, g, ("MONO", MONO)]
    }
}

/// Space inside a button, around its label.
pub const BUTTON_PADDING: Vec2 = Vec2::new(14.0, 8.0);
/// Minimum height of anything clickable.
///
/// egui's default is eighteen pixels, which is a target you have to aim at.
pub const MIN_INTERACT_HEIGHT: f32 = 32.0;
/// Space between two stacked widgets.
pub const ITEM_SPACING: Vec2 = Vec2::new(9.0, 7.0);
/// Height of the four step buttons.
///
/// Taller than an ordinary control: these are the four things the interface
/// exists to do, and they should look like it.
pub const STEP_BUTTON_HEIGHT: f32 = 42.0;

/// Margin inside a panel, between its edge and its contents.
///
/// Every pixel here is taken from the figures in the middle, so it is the
/// smallest that still keeps the contents off the edge. Group boxes size their
/// own margin from the width they are given — see [`crate::panels::layout::margin`];
/// this one is the outer frame and stays modest.
pub const PANEL_MARGIN: f32 = 6.0;
/// Nominal margin inside a group box, used by the layout arithmetic to reason
/// about the space a row will really have. The drawing code asks
/// [`crate::panels::layout::margin`] for the actual figure, which follows the
/// panel's width.
pub const GROUP_MARGIN: f32 = 9.0;

/// Default width of the left panel, and the range it can be dragged through.
///
/// The side panels drive the analysis; the figures between them are what the
/// user actually looks at, so any width beyond what the contents need is width
/// taken from the picture. The defaults are therefore set to the *narrow* end:
/// a long parameter name takes two lines here rather than a wide panel taking
/// a quarter of the window. Nothing is lost by that — a wrapped label still
/// reads in full, and a panel that wants to be wider can be dragged there.
pub const BROWSER_WIDTH: f32 = 260.0;
pub const BROWSER_MIN_WIDTH: f32 = 220.0;
pub const BROWSER_MAX_WIDTH: f32 = 900.0;

/// The same for the right panel, which is wider only because its parameter
/// names are longer.
pub const PARAMETERS_WIDTH: f32 = 300.0;
pub const PARAMETERS_MIN_WIDTH: f32 = 250.0;
pub const PARAMETERS_MAX_WIDTH: f32 = 1000.0;

/// Space to the left of the manual's text.
///
/// Text that starts where the panel starts is uncomfortable to read and looks
/// unfinished.
pub const DOC_MARGIN: f32 = 48.0;
/// Widest a line of the manual may get.
///
/// A line stretched across a wide screen is hard to follow back to its
/// successor; typography puts the comfortable maximum around 60 to 90
/// characters, which at this body size is close to this.
pub const DOC_MAX_WIDTH: f32 = 820.0;

/// Corner radius used throughout.
const RADIUS: u8 = 4;

/// Install the palette on a context.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::light();

    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = PANEL;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = FIELD;
    visuals.faint_bg_color = SURFACE;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.selection.bg_fill = ACCENT_SOFT;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT_STRONG);
    visuals.hyperlink_color = ACCENT_STRONG;

    let rounding = CornerRadius::same(RADIUS);

    // Idle controls stay quiet, and the pointer only lifts them a shade of
    // silver: the gold is kept for the control being *pressed*, and for what is
    // already chosen, so it stays rare enough to mean something.
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_MUTED);
    visuals.widgets.noninteractive.corner_radius = rounding;

    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.corner_radius = rounding;

    visuals.widgets.hovered.bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, STEEL);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.corner_radius = rounding;

    visuals.widgets.active.bg_fill = ACCENT_SOFT;
    visuals.widgets.active.weak_bg_fill = ACCENT_SOFT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    // Dark text on the metallic gold: the gold is the fill, not the writing.
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.active.corner_radius = rounding;

    visuals.widgets.open.bg_fill = SURFACE;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, STEEL);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.open.corner_radius = rounding;

    ctx.set_visuals(visuals);

    // The sizes, applied once here rather than at every call site: a label
    // written without an explicit size inherits the scale.
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                egui::TextStyle::Heading,
                egui::FontId::proportional(size::SECTION_TITLE),
            ),
            (
                egui::TextStyle::Body,
                egui::FontId::proportional(size::BODY),
            ),
            (
                egui::TextStyle::Button,
                egui::FontId::proportional(size::LABEL),
            ),
            (
                egui::TextStyle::Small,
                egui::FontId::proportional(size::SMALL),
            ),
            (
                egui::TextStyle::Monospace,
                egui::FontId::monospace(size::MONO),
            ),
        ]
        .into();

        style.spacing.button_padding = BUTTON_PADDING;
        style.spacing.item_spacing = ITEM_SPACING;
        style.spacing.interact_size.y = MIN_INTERACT_HEIGHT;
        style.spacing.combo_height = 400.0;
        // A control too narrow to show its own value is one the user has to
        // guess at.
        style.spacing.slider_width = 160.0;
        style.spacing.icon_width = 18.0;
        style.spacing.icon_width_inner = 10.0;
    });
}

/// The colour of a log line.
pub fn log_colour(kind: crate::model::log::LogKind) -> Color32 {
    use crate::model::log::LogKind;
    match kind {
        LogKind::Error => LOG_ERROR,
        LogKind::Warning => LOG_WARNING,
        LogKind::Success => LOG_SUCCESS,
        LogKind::Info => LOG_INFO,
        LogKind::Progress => LOG_PROGRESS,
        LogKind::Plain => LOG_PLAIN,
    }
}

/// Perceived luminance, on the WCAG definition.
fn luminance(colour: Color32) -> f32 {
    let channel = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(colour.r()) + 0.7152 * channel(colour.g()) + 0.0722 * channel(colour.b())
}

/// Contrast ratio between two colours, 1:1 to 21:1.
fn contrast(a: Color32, b: Color32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    let (high, low) = if x > y { (x, y) } else { (y, x) };
    (high + 0.05) / (low + 0.05)
}

/// The caption colour for a coloured fill: whichever of the two text colours is
/// easier to read on it.
///
/// Measured rather than guessed from the sum of the channels, which is what
/// this used to do: the step colours span bronze to champagne, and the sum puts
/// the boundary in the middle of that range, where the answer is genuinely
/// close and the cheap rule gets it wrong.
pub fn text_on(fill: Color32) -> Color32 {
    if contrast(TEXT, fill) >= contrast(TEXT_INVERSE, fill) {
        TEXT
    } else {
        TEXT_INVERSE
    }
}

/// The colour of a step's button and progress bar.
pub fn step_colour(step: crate::model::runner::Step) -> Color32 {
    use crate::model::runner::Step;
    match step {
        Step::Tysserand => STEP_TYSSERAND,
        Step::Assortativity => STEP_ASSORTATIVITY,
        Step::NicheAnalysis => STEP_NICHES,
        Step::ClearTemporary => STEP_CLEAR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::log::LogKind;
    use crate::model::runner::Step;

    /// Body text on the panel must clear the 4.5:1 the accessibility guidelines
    /// ask for; a low-contrast theme is easy to get wrong here.
    #[test]
    fn body_text_is_readable_on_the_panel() {
        assert!(
            contrast(TEXT, PANEL) >= 4.5,
            "contrast is only {:.1}:1",
            contrast(TEXT, PANEL)
        );
    }

    #[test]
    fn the_accent_is_readable_on_the_panel() {
        assert!(
            contrast(ACCENT, PANEL) >= 4.5,
            "the accent on panel is only {:.1}:1",
            contrast(ACCENT, PANEL)
        );
    }

    /// The metallic gold is a fill, so what has to be legible is the writing on
    /// top of it.
    #[test]
    fn text_on_a_pressed_control_stays_legible() {
        assert!(
            contrast(TEXT, ACCENT_SOFT) >= 4.5,
            "text on a pressed control is only {:.1}:1",
            contrast(TEXT, ACCENT_SOFT)
        );
    }

    /// Every fill the interface picks a caption colour for must end up with a
    /// readable one — the step buttons span bronze to champagne, and a rule
    /// that got the middle wrong would put pale text on pale gold.
    #[test]
    fn every_fill_has_a_readable_caption() {
        let fills = [
            STEP_TYSSERAND,
            STEP_ASSORTATIVITY,
            STEP_NICHES,
            STEP_CLEAR,
            STEP_FAILED,
            ACCENT,
            ACCENT_SOFT,
        ];
        for fill in fills {
            let ratio = contrast(text_on(fill), fill);
            assert!(ratio >= 4.0, "{fill:?} gets a {ratio:.1}:1 caption");
        }
    }

    /// Silver has to carry more of the interface than gold does, or the accent
    /// stops being an accent. The chrome — page, panels, boxes, rules, the
    /// emphasis that is not the accent — is neutral: no channel apart from the
    /// others by more than a hair.
    #[test]
    fn the_chrome_is_silver_not_gold() {
        for colour in [
            BACKGROUND,
            PANEL,
            SURFACE,
            SURFACE_HOVER,
            FIELD,
            BORDER,
            STEEL,
            TEXT,
            TEXT_MUTED,
            TEXT_INVERSE,
        ] {
            let (r, g, b) = (colour.r() as i32, colour.g() as i32, colour.b() as i32);
            let spread = r.max(g).max(b) - r.min(g).min(b);
            assert!(spread <= 16, "{colour:?} is tinted, not silver");
        }
    }

    /// A colour in CIE L\*a\*b\*, where a Euclidean distance approximates how
    /// different two colours *look*.
    fn lab(colour: Color32) -> [f32; 3] {
        let linear = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = (linear(colour.r()), linear(colour.g()), linear(colour.b()));

        // Through XYZ, normalised to the D65 white point.
        let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;

        let f = |t: f32| {
            if t > 0.008856 {
                t.cbrt()
            } else {
                7.787 * t + 16.0 / 116.0
            }
        };
        let (fx, fy, fz) = (f(x), f(y), f(z));
        [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
    }

    /// How different two colours look, on the CIE76 scale: about 2.3 is the
    /// smallest difference an eye can see, and 20 is unmistakable.
    fn perceptual_distance(a: Color32, b: Color32) -> f32 {
        let (a, b) = (lab(a), lab(b));
        (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
    }

    /// The four steps must stay distinguishable, or the progress bar no longer
    /// tells the user which step is running.
    ///
    /// Measured perceptually rather than by contrast ratio, which knows only
    /// about lightness: it passed a bronze and a gold that were plainly the
    /// same button twice, and would fail the bronze against the graphite that
    /// nobody could confuse it with.
    #[test]
    fn the_step_colours_are_distinguishable() {
        let steps = Step::all();
        for (i, a) in steps.iter().enumerate() {
            for b in steps.iter().skip(i + 1) {
                let distance = perceptual_distance(step_colour(*a), step_colour(*b));
                assert!(
                    distance >= 20.0,
                    "{a:?} and {b:?} are too close: {distance:.0}"
                );
            }
        }
    }

    /// Error, warning and success keep their conventional hues.
    #[test]
    fn severity_colours_keep_their_meaning() {
        assert!(LOG_ERROR.r() > LOG_ERROR.g() && LOG_ERROR.r() > LOG_ERROR.b());
        assert!(LOG_SUCCESS.g() > LOG_SUCCESS.r());
        assert_eq!(log_colour(LogKind::Error), LOG_ERROR);
        assert_eq!(log_colour(LogKind::Plain), LOG_PLAIN);
    }

    /// Information and progress sit on the accent axis: red channel highest,
    /// blue channel lowest.
    #[test]
    fn informational_colours_are_on_the_accent_axis() {
        for colour in [LOG_INFO, LOG_PROGRESS] {
            assert!(
                colour.r() > colour.b() && colour.g() > colour.b(),
                "{colour:?} is not a gold"
            );
        }
    }

    /// The log is drawn on the field colour, not on the panel.
    #[test]
    fn every_log_line_has_a_colour() {
        for kind in [
            LogKind::Error,
            LogKind::Warning,
            LogKind::Success,
            LogKind::Info,
            LogKind::Progress,
            LogKind::Plain,
        ] {
            let colour = log_colour(kind);
            assert!(contrast(colour, FIELD) >= 4.5, "{kind:?} is hard to read");
        }
    }
}
