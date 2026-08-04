//! The two modal dialogs: the mandatory working directory, and notices.

use crate::app::MosnaApp;
use crate::theme;

/// Draw whichever modal is due.
pub fn show(app: &mut MosnaApp, ctx: &egui::Context) {
    if let Some(message) = app.notice.clone() {
        egui::Window::new("MOSNA")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(message).color(theme::TEXT));
                ui.add_space(8.0);
                if ui.button("OK").clicked() {
                    app.notice = None;
                }
            });
        return;
    }

    // The Python asks for the working directory before anything else and closes
    // if the user declines; the same requirement is expressed as a modal that
    // cannot be dismissed without choosing.
    if app.needs_working_dir {
        egui::Window::new("Choose a working directory")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "MOSNA writes its results into a working directory.\n\
                         Choose one to continue.",
                    )
                    .color(theme::TEXT),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let button = egui::Button::new(
                        egui::RichText::new("Choose…")
                            .color(theme::TEXT_INVERSE)
                            .strong(),
                    )
                    .fill(theme::ACCENT);
                    if ui.add(button).clicked() {
                        if let Some(directory) = rfd::FileDialog::new()
                            .set_title("Choose working directory")
                            .pick_folder()
                        {
                            app.set_working_dir(directory);
                        }
                    }
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
    }
}
