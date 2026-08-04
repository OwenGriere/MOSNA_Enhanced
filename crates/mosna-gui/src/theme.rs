//! The black-and-gold palette.

use egui::{Color32, CornerRadius, Stroke, Vec2, Visuals};

/// Page background, near-black with a warm cast so gold sits on it comfortably.
pub const BACKGROUND: Color32 = Color32::from_rgb(0x0E, 0x0D, 0x0B);
/// Panel background, one step lighter than the page.
pub const PANEL: Color32 = Color32::from_rgb(0x16, 0x15, 0x12);
/// Raised surfaces: group boxes, table rows, the log view.
pub const SURFACE: Color32 = Color32::from_rgb(0x1E, 0x1C, 0x18);
/// Hovered surface.
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x2A, 0x27, 0x20);
/// Hairlines and separators.
pub const BORDER: Color32 = Color32::from_rgb(0x35, 0x31, 0x27);

/// The accent: a classic metallic gold.
pub const GOLD: Color32 = Color32::from_rgb(0xD4, 0xAF, 0x37);
/// A brighter gold, for the element under the pointer.
pub const GOLD_BRIGHT: Color32 = Color32::from_rgb(0xF0, 0xC7, 0x5E);
/// A muted gold, for borders and inactive accents.
pub const GOLD_DIM: Color32 = Color32::from_rgb(0x8A, 0x73, 0x26);

/// Body text: warm off-white rather than pure white, which glares on black.
pub const TEXT: Color32 = Color32::from_rgb(0xE8, 0xE2, 0xD4);
/// Secondary text.
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x9A, 0x93, 0x82);

/// Log colours.
///
/// Error, warning and success keep their conventional hues: they carry meaning
/// that a gold monochrome would destroy, and a user scanning a log needs red to
/// mean red. Information and progress, which have no conventional colour, are
/// moved onto the gold axis — those were blue and purple in the Python.
pub const LOG_ERROR: Color32 = Color32::from_rgb(0xE5, 0x5B, 0x4C);
pub const LOG_WARNING: Color32 = Color32::from_rgb(0xE0, 0xA8, 0x30);
pub const LOG_SUCCESS: Color32 = Color32::from_rgb(0x6B, 0xC2, 0x8A);
pub const LOG_INFO: Color32 = GOLD;
pub const LOG_PROGRESS: Color32 = GOLD_DIM;
pub const LOG_PLAIN: Color32 = TEXT_MUTED;

/// The colour of each step's button and of the progress bar while it runs.
///
/// The Python gave the four steps four unrelated hues (blue, purple, green,
/// maroon) so that the progress bar told you which step was running. That cue
/// is kept, but expressed as a progression through the gold range — bronze,
/// gold, champagne — so it stays on palette while remaining distinguishable.
pub const STEP_TYSSERAND: Color32 = Color32::from_rgb(0x8C, 0x6D, 0x1F);
pub const STEP_ASSORTATIVITY: Color32 = Color32::from_rgb(0xB8, 0x86, 0x0B);
pub const STEP_NICHES: Color32 = Color32::from_rgb(0xE3, 0xC0, 0x5C);
pub const STEP_CLEAR: Color32 = Color32::from_rgb(0x4A, 0x43, 0x33);
/// A failed run turns the progress bar this colour.
pub const STEP_FAILED: Color32 = Color32::from_rgb(0x8B, 0x2E, 0x24);

/// The sizing scale.
///
/// One named step per role, rather than a literal at each call site: that is
/// how an interface ends up with an eleven-pixel label beside a twelve-pixel
/// one for no reason anybody remembers. The steps are at least a pixel and a
/// half apart, so each reads as a level of the hierarchy rather than as a
/// mistake.
pub mod size {
    /// The title of a page of the manual.
    pub const PAGE_TITLE: f32 = 26.0;
    /// A panel header, or a section title in the manual.
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
    pub fn hierarchy() -> [(&'static str, f32); 6] {
        [
            ("PAGE_TITLE", PAGE_TITLE),
            ("SECTION_TITLE", SECTION_TITLE),
            ("HEADING", HEADING),
            ("BODY", BODY),
            ("LABEL", LABEL),
            ("SMALL", SMALL),
        ]
    }

    /// Every step, including the monospace size, which sits outside the
    /// hierarchy: code is not a level of emphasis.
    pub fn all() -> [(&'static str, f32); 7] {
        let [a, b, c, d, e, f] = hierarchy();
        [a, b, c, d, e, f, ("MONO", MONO)]
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
    let mut visuals = Visuals::dark();

    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = PANEL;
    visuals.window_fill = BACKGROUND;
    visuals.extreme_bg_color = BACKGROUND;
    visuals.faint_bg_color = SURFACE;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.selection.bg_fill = GOLD_DIM;
    visuals.selection.stroke = Stroke::new(1.0, GOLD_BRIGHT);
    visuals.hyperlink_color = GOLD_BRIGHT;

    let rounding = CornerRadius::same(RADIUS);

    // Idle controls stay quiet; the gold appears on interaction, so the eye is
    // drawn to what it can act on rather than to everything at once.
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
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, GOLD_DIM);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, GOLD_BRIGHT);
    visuals.widgets.hovered.corner_radius = rounding;

    visuals.widgets.active.bg_fill = GOLD_DIM;
    visuals.widgets.active.weak_bg_fill = GOLD_DIM;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, GOLD);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, BACKGROUND);
    visuals.widgets.active.corner_radius = rounding;

    visuals.widgets.open.bg_fill = SURFACE;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, GOLD_DIM);
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

    /// Perceived luminance, for contrast checks.
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

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (high, low) = {
            let (x, y) = (luminance(a), luminance(b));
            if x > y {
                (x, y)
            } else {
                (y, x)
            }
        };
        (high + 0.05) / (low + 0.05)
    }

    /// Body text on the panel must clear the 4.5:1 the accessibility guidelines
    /// ask for; a dark theme is easy to get wrong here.
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
            contrast(GOLD, PANEL) >= 4.5,
            "gold on panel is only {:.1}:1",
            contrast(GOLD, PANEL)
        );
    }

    /// Text drawn on an active, gold-filled control must be dark.
    #[test]
    fn text_on_a_pressed_control_stays_legible() {
        assert!(contrast(BACKGROUND, GOLD_DIM) >= 3.0);
    }

    /// The four steps must stay distinguishable, or the progress bar no longer
    /// tells the user which step is running.
    #[test]
    fn the_step_colours_are_distinguishable() {
        let steps = Step::all();
        for (i, a) in steps.iter().enumerate() {
            for b in steps.iter().skip(i + 1) {
                let ratio = contrast(step_colour(*a), step_colour(*b));
                assert!(ratio >= 1.3, "{a:?} and {b:?} are too close: {ratio:.2}:1");
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

    /// Information and progress sit on the gold axis: red channel highest,
    /// blue channel lowest.
    #[test]
    fn informational_colours_are_gold() {
        for colour in [LOG_INFO, LOG_PROGRESS] {
            assert!(
                colour.r() > colour.b() && colour.g() > colour.b(),
                "{colour:?} is not a gold"
            );
        }
    }

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
            assert!(contrast(colour, SURFACE) >= 3.0, "{kind:?} is hard to read");
        }
    }
}
