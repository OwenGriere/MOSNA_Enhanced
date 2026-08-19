//! Drawing the three panels.
//!
//! The layout reproduces `GUI_MOSNA.py`: a top bar carrying the working
//! directory, then a horizontal split of Browser, Viewer and Parameters, with
//! the status bar and progress bar under the viewer.

pub mod browser;
pub mod documentation;
pub mod layout;
pub mod modals;
pub mod network;
pub mod parameters;
pub mod top_bar;
pub mod viewer;

use crate::theme;

/// A panel heading, in the accent colour.
pub fn header(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(text)
            .color(theme::ACCENT)
            .size(theme::size::PANEL_TITLE)
            .strong(),
    );
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(2.0);
}

/// Which way a panel folds, which is the way its chevron points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fold {
    /// A left-hand panel, folding off the left edge.
    Left,
    /// A right-hand panel, folding off the right edge.
    Right,
}

impl Fold {
    /// The mark on the title, pointing the way the panel will go.
    fn chevron(self) -> &'static str {
        match self {
            Fold::Left => "\u{2039}",
            Fold::Right => "\u{203a}",
        }
    }
}

/// A panel heading that is also the control that folds the panel away.
///
/// The title *is* the button, rather than a separate widget beside it: a panel
/// has one obvious thing to click at the top of it, and giving that thing two
/// jobs is cheaper in space and in explanation than adding a second control to
/// a panel whose whole problem is that it is taking up room.
///
/// Returns `true` on the frame the user asked to fold it.
pub fn folding_header(ui: &mut egui::Ui, text: &str, fold: Fold) -> bool {
    ui.add_space(6.0);
    let title = egui::RichText::new(format!("{text}  {}", fold.chevron()))
        .color(theme::ACCENT)
        .size(theme::size::PANEL_TITLE)
        .strong();
    let clicked = ui
        .add(egui::Button::new(title).frame(false))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Fold this panel away")
        .clicked();
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(2.0);
    clicked
}

/// The whole of a folded panel: a band carrying its name, written up it.
///
/// Written bottom to top, which is the way a spine is read when a book is
/// standing up, and the only way that keeps the letters in order when the text
/// is turned onto its side.
///
/// Returns `true` on the frame the user asked to unfold it.
pub fn folded_spine(ui: &mut egui::Ui, text: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        ui.available_size(),
        egui::Sense::click(),
    );
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("Open {text}"));

    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, egui::CornerRadius::same(4), theme::SURFACE_HOVER);
    }

    // A step down from the panel title it stands in for: turned on its side,
    // a line of text is as wide as its glyphs are tall, and the band is
    // [`theme::FOLDED_WIDTH`] across. At the title's size the name would be
    // shaved on both sides by the clip.
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(theme::size::HEADING),
        theme::ACCENT,
    );
    // A quarter turn anticlockwise, about the position the shape is given. The
    // text then runs upwards from that point and downwards to the right of it,
    // which is why the anchor is the *bottom left* of where the line is to end
    // up rather than its top left.
    let size = galley.size();
    let anchor = egui::pos2(
        rect.center().x - size.y * 0.5,
        rect.center().y + size.x * 0.5,
    );
    painter.add(
        egui::epaint::TextShape::new(anchor, galley, theme::ACCENT)
            .with_angle(-std::f32::consts::FRAC_PI_2),
    );

    response.clicked()
}

/// A titled box, the equivalent of Qt's `QGroupBox`.
pub fn group<R>(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(8.0);
    // Graphite, not gold: a panel holds half a dozen of these, and gold on each
    // one would leave nothing for the panel's own title to be.
    ui.label(
        egui::RichText::new(title)
            .color(theme::STEEL)
            .size(theme::size::HEADING)
            .strong(),
    );
    ui.add_space(2.0);
    let frame = egui::Frame::new()
        .fill(theme::SURFACE)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(4))
        // Sized from the width the box was given rather than fixed: a constant
        // margin is a large fraction of a narrow panel and a rounding error in
        // a wide one.
        .inner_margin(egui::Margin::same(
            layout::margin(ui.available_width()) as i8
        ));
    frame.show(ui, contents).inner
}

/// Width one label needs to be shown on a single line.
///
/// Measured with the font that will draw it rather than estimated from the
/// character count, because a name in an interface is not a fixed-width string
/// and an estimate is how a column ends up half a word too narrow.
pub fn label_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::FontId::proportional(theme::size::LABEL);
    ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font, theme::TEXT)
            .size()
            .x
    })
}

/// Width a button needs to show `text` without growing past what it is given.
///
/// Buttons draw their caption in `TextStyle::Button`, which the theme sets to
/// the label size, inside [`theme::BUTTON_PADDING`] on each side.
pub fn button_width(ui: &egui::Ui, text: &str) -> f32 {
    label_width(ui, text) + 2.0 * theme::BUTTON_PADDING.x
}

/// Width of the label column a group of rows shares.
///
/// Shared, because rows whose columns each sized themselves would put their
/// controls at different places and leave a ragged edge down the panel.
pub fn label_column_for<'a>(ui: &egui::Ui, labels: impl IntoIterator<Item = &'a str>) -> f32 {
    let widest = labels
        .into_iter()
        .map(|text| label_width(ui, text))
        .fold(0.0f32, f32::max);
    layout::label_column(widest, ui.available_width(), ui.spacing().item_spacing.x)
}

/// One row of a group box: a label on the left, its control on the right.
///
/// `label_width` is the column the row's group agreed on, from
/// [`label_column_for`]. The control takes what is left, so the row fits the
/// space it was given at any panel width and the label is never clipped by the
/// frame — which is what used to happen to the longer parameter names, and why
/// widening the panel did not bring them back.
pub fn field_row<R>(
    ui: &mut egui::Ui,
    label: egui::RichText,
    tooltip: Option<&str>,
    label_width: f32,
    control: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let spacing = ui.spacing().item_spacing.x;
    let mut result = None;

    // Top-down, explicitly, in both arrangements. A plain `scope` inherits the
    // row's own left-to-right layout, so a control closure that draws *two*
    // widgets — the Network directory's mode picker and its path, for instance
    // — put them side by side instead of one above the other, and the pair ran
    // straight out of the panel. Worse, egui grows a `Ui`'s max rect to contain
    // what overflowed it, so the extra width came back as the panel's width on
    // the next frame, and the frame after that: the Browser ratcheted itself
    // open to its maximum in a quarter of a second and stayed there, whatever
    // its default said.
    let column = egui::UiBuilder::new().layout(egui::Layout::top_down(egui::Align::Min));

    match layout::row(ui.available_width(), spacing, label_width) {
        layout::Row::Beside(widths) => {
            ui.horizontal_top(|ui| {
                ui.scope(|ui| {
                    ui.set_width(widths.label);
                    caption(ui, label, tooltip);
                });
                ui.scope_builder(column, |ui| {
                    ui.set_width(widths.control);
                    result = Some(control(ui));
                });
            });
        }
        layout::Row::Stacked { width } => {
            caption(ui, label, tooltip);
            ui.scope_builder(column, |ui| {
                ui.set_width(width);
                result = Some(control(ui));
            });
        }
    }

    result.expect("the control closure runs exactly once")
}

/// A row's label, with its explanation on hover if it has one.
fn caption(ui: &mut egui::Ui, label: egui::RichText, tooltip: Option<&str>) {
    let response = ui.add(egui::Label::new(label).wrap());
    if let Some(tooltip) = tooltip {
        response.on_hover_text(tooltip);
    }
}

/// A text field with a browse button beside it, sized to fit together.
///
/// Returns whether the text changed.
pub fn path_field(ui: &mut egui::Ui, title: &str, path: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let button = layout::browse_button_width();
        let spacing = ui.spacing().item_spacing.x;
        let text = layout::text_and_button(ui.available_width(), button, spacing);

        changed |= ui
            .add(egui::TextEdit::singleline(path).desired_width(text))
            .changed();

        if ui
            .add_sized(
                [button, ui.spacing().interact_size.y],
                egui::Button::new("…"),
            )
            .clicked()
        {
            if let Some(directory) = rfd::FileDialog::new().set_title(title).pick_folder() {
                *path = directory.to_string_lossy().into_owned();
                changed = true;
            }
        }
    });
    changed
}

/// A button filled with a step's colour, across the width it is offered.
pub fn accent_button(ui: &mut egui::Ui, text: &str, colour: egui::Color32) -> egui::Response {
    accent_button_sized(
        ui,
        text,
        colour,
        layout::content_width(ui.available_width()),
    )
}

/// The same, at a chosen width, for the two buttons that share a row.
pub fn accent_button_sized(
    ui: &mut egui::Ui,
    text: &str,
    colour: egui::Color32,
    width: f32,
) -> egui::Response {
    // Dark text on a light fill, light text on a dark one, so the caption stays
    // readable across the whole step palette.
    let foreground = theme::text_on(colour);

    ui.add_sized(
        [width, theme::STEP_BUTTON_HEIGHT],
        egui::Button::new(
            egui::RichText::new(text)
                .color(foreground)
                .size(theme::size::HEADING)
                .strong(),
        )
        .fill(colour)
        // `Step 3 — Niche Analysis` is the longest of the four and does not fit
        // a panel dragged to its narrowest. Truncated it still says which step
        // it is; untruncated it would lay itself out at its own width and hang
        // out of the panel, because `add_sized` is a maximum in name only.
        .truncate()
        .corner_radius(egui::CornerRadius::same(4)),
    )
}
