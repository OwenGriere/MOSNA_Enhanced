//! The working-directory bar.

use crate::app::MosnaApp;
use crate::theme;

/// Draw the bar carrying the working directory and its picker.
pub fn show(app: &mut MosnaApp, ui: &mut egui::Ui) {
    egui::containers::Panel::top("top_bar")
        .frame(
            egui::Frame::new()
                .fill(theme::BACKGROUND)
                .inner_margin(egui::Margin::symmetric(14, 9)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let text = match app.working_dir() {
                    Some(path) => format!("Working directory: {}", path.display()),
                    None => "Working directory: not set".to_string(),
                };
                ui.label(
                    egui::RichText::new(text)
                        .color(theme::TEXT_MUTED)
                        .size(theme::size::BODY),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let button = egui::Button::new(
                        egui::RichText::new("📁 Set working dir")
                            .color(theme::TEXT_INVERSE)
                            .size(theme::size::LABEL)
                            .strong(),
                    )
                    .fill(theme::ACCENT)
                    .corner_radius(egui::CornerRadius::same(4));

                    if ui
                        .add_sized([210.0, theme::MIN_INTERACT_HEIGHT], button)
                        .clicked()
                    {
                        if let Some(directory) = rfd::FileDialog::new()
                            .set_title("Choose working directory")
                            .pick_folder()
                        {
                            app.set_working_dir(directory);
                        }
                    }
                });
            });
        });
}
