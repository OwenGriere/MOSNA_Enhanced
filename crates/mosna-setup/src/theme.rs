//! The look of both windows: the logo's black and gold, with the logo itself
//! drawn faintly behind the contents.
//!
//! The logo is built into the program rather than read from the MOSNA folder:
//! the uninstaller may run once that folder is gone, and a background that
//! depends on where the program was started is a background that goes missing.

use egui::{Color32, CornerRadius, Stroke, Visuals};

/// The logo, a PNG whatever its extension says.
const LOGO: &[u8] = include_bytes!("../../../assets/logo.ico");

/// The window, the near-black of the logo's disc.
pub const BACKGROUND: Color32 = Color32::from_rgb(0x14, 0x11, 0x0B);
/// Controls at rest.
pub const SURFACE: Color32 = Color32::from_rgb(0x26, 0x20, 0x15);
/// Controls under the pointer.
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x34, 0x2B, 0x1B);
/// Hairlines.
pub const BORDER: Color32 = Color32::from_rgb(0x4A, 0x3E, 0x26);
/// What the user types into.
pub const FIELD: Color32 = Color32::from_rgb(0x0C, 0x0A, 0x06);
/// The console, translucent so the logo still shows through it.
pub const CONSOLE: Color32 = Color32::from_rgba_premultiplied(0x09, 0x08, 0x05, 0xD8);

/// The logo's gold: titles, the main button, the selection.
pub const GOLD: Color32 = Color32::from_rgb(0xD5, 0xA8, 0x46);
/// A deeper gold, the fill of a pressed control, under light text.
pub const GOLD_DEEP: Color32 = Color32::from_rgb(0x93, 0x70, 0x2B);

/// Text, warm to sit with the gold.
pub const TEXT: Color32 = Color32::from_rgb(0xEC, 0xE6, 0xD8);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0xA8, 0x9F, 0x8C);

/// The console's code: the conventional hues, light enough for a dark
/// background.
pub const LOG_ERROR: Color32 = Color32::from_rgb(0xF0, 0x6A, 0x5C);
pub const LOG_WARNING: Color32 = Color32::from_rgb(0xF2, 0xC2, 0x4B);
pub const LOG_INFO: Color32 = Color32::from_rgb(0x7F, 0xB2, 0xE5);
pub const LOG_SUCCESS: Color32 = Color32::from_rgb(0x8B, 0xD0, 0x7A);
/// The start of a stage, on the accent.
pub const LOG_STEP: Color32 = GOLD;
pub const LOG_PLAIN: Color32 = Color32::from_rgb(0xC9, 0xC1, 0xAF);

/// The console's type size.
pub const MONO_SIZE: f32 = 12.5;

/// How much of the logo shows behind the contents.
const LOGO_OPACITY: u8 = 34;

/// Install the palette on a context.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = FIELD;
    visuals.faint_bg_color = SURFACE;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.selection.bg_fill = GOLD_DEEP;
    visuals.selection.stroke = Stroke::new(1.0, GOLD);
    visuals.hyperlink_color = GOLD;
    visuals.warn_fg_color = LOG_WARNING;
    visuals.error_fg_color = LOG_ERROR;
    visuals.weak_text_color = Some(TEXT_MUTED);

    let rounding = CornerRadius::same(4);
    let widgets = &mut visuals.widgets;

    widgets.noninteractive.bg_fill = BACKGROUND;
    widgets.noninteractive.weak_bg_fill = BACKGROUND;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.noninteractive.corner_radius = rounding;

    widgets.inactive.bg_fill = SURFACE;
    widgets.inactive.weak_bg_fill = SURFACE;
    widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.inactive.corner_radius = rounding;

    widgets.hovered.bg_fill = SURFACE_HOVER;
    widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    widgets.hovered.bg_stroke = Stroke::new(1.0, GOLD);
    widgets.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    widgets.hovered.corner_radius = rounding;

    // Also the colour of strong text, so it stays light.
    widgets.active.bg_fill = GOLD_DEEP;
    widgets.active.weak_bg_fill = GOLD_DEEP;
    widgets.active.bg_stroke = Stroke::new(1.0, GOLD);
    widgets.active.fg_stroke = Stroke::new(1.5, TEXT);
    widgets.active.corner_radius = rounding;

    widgets.open.bg_fill = SURFACE;
    widgets.open.bg_stroke = Stroke::new(1.0, GOLD);
    widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.open.corner_radius = rounding;

    // Whatever the system's preference, these windows are dark.
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals);
    ctx.all_styles_mut(|style| {
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        if let Some(mono) = style.text_styles.get_mut(&egui::TextStyle::Monospace) {
            mono.size = MONO_SIZE;
        }
    });
}

fn logo_image() -> Option<image::RgbaImage> {
    image::load_from_memory(LOGO)
        .ok()
        .map(|logo| logo.into_rgba8())
}

/// The logo as the window's icon.
pub fn icon() -> Option<egui::IconData> {
    let image = logo_image()?;
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

/// The logo behind the contents.
pub struct Backdrop {
    texture: Option<egui::TextureHandle>,
}

impl Backdrop {
    pub fn new(ctx: &egui::Context) -> Self {
        let texture = logo_image().map(|image| {
            let size = [image.width() as usize, image.height() as usize];
            let pixels = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            ctx.load_texture("mosna-logo", pixels, egui::TextureOptions::LINEAR)
        });
        Self { texture }
    }

    /// Draw the logo, faint and centred, over the whole of `ui`. Called before
    /// the contents, so they are drawn over it.
    pub fn paint(&self, ui: &egui::Ui) {
        let Some(texture) = &self.texture else {
            return;
        };
        let area = ui.max_rect();
        let side = area.width().min(area.height()) * 0.92;
        let rect = egui::Rect::from_center_size(area.center(), egui::vec2(side, side));
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::from_white_alpha(LOGO_OPACITY),
        );
    }
}

/// The window's title, in gold.
pub fn heading(ui: &mut egui::Ui, text: &str) {
    ui.heading(egui::RichText::new(text).color(GOLD).strong());
}

/// The button that does what the window is for: gold, with dark text.
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(BACKGROUND).strong())
            .fill(GOLD)
            .stroke(Stroke::new(1.0, GOLD)),
    )
}
