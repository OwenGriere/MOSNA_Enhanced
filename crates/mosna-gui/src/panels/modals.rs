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

    // Before the sensitivity screen opens: the settings every run of a grid
    // will share, and what is missing from them.
    if app.sweep_gate.is_some() {
        sweep_gate(app, ctx);
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

/// Confirm the settings a sweep will be built on, before it is built.
///
/// # Why it is shown even when nothing is wrong
///
/// Every run of a grid reads the same General settings, so one of them wrong is
/// a grid that fails from end to end — eighty-four runs in a few seconds,
/// reporting a count and nothing else. They are also the settings the user is
/// least likely to have looked at recently: they were chosen once, on another
/// screen, possibly in another session.
///
/// One glance before minutes of work is cheap. A dialog that only appeared when
/// something was already wrong would not have been.
fn sweep_gate(app: &mut MosnaApp, ctx: &egui::Context) {
    let Some(gate) = app.sweep_gate.clone() else {
        return;
    };

    egui::Window::new("Sensitivity analysis")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Do you confirm these settings?")
                    .color(theme::TEXT)
                    .size(theme::size::HEADING)
                    .strong(),
            );
            ui.label(
                egui::RichText::new("Every run of the grid will share them.")
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(10.0);

            egui::Grid::new("sweep_gate_summary")
                .striped(true)
                .spacing(egui::vec2(14.0, 4.0))
                .show(ui, |ui| {
                    for (name, value) in &gate.summary {
                        ui.label(egui::RichText::new(name).color(theme::TEXT_MUTED));
                        setting(ui, name, value);
                        ui.end_row();
                    }
                });

            if !gate.is_ready() {
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("Fill these in on the main screen first:")
                        .color(theme::LOG_WARNING)
                        .strong(),
                );
                for problem in &gate.problems {
                    ui.label(egui::RichText::new(format!("• {problem}")).color(theme::LOG_WARNING));
                }
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    app.sweep_gate = None;
                }
                let go = egui::Button::new(
                    egui::RichText::new("Continue")
                        .color(theme::TEXT_INVERSE)
                        .strong(),
                )
                .fill(theme::ACCENT);
                if ui
                    .add_enabled(gate.is_ready(), go)
                    .on_disabled_hover_text("Some of the settings above are still empty")
                    .clicked()
                {
                    app.enter_sweep_screen();
                }
            });
        });
}

/// One setting's value, abbreviated when it is a long list.
///
/// `Column to aggregate` on a real cohort is thirty-four phenotypes. Spelt out,
/// they push the five other settings this dialog exists to show off the screen.
/// Past three the count stands in for them and the names are one click away —
/// the rule the main interface's column picker already follows, so the two read
/// alike.
fn setting(ui: &mut egui::Ui, name: &str, value: &crate::model::sweep::SettingValue) {
    let caption = value.caption();
    let colour = if value.is_missing() {
        theme::LOG_WARNING
    } else {
        theme::TEXT
    };

    if !value.is_abbreviated() {
        ui.label(egui::RichText::new(caption).color(colour));
        return;
    }

    // Stays open until the reader clicks away: scrolling thirty-four names is
    // not something to be interrupted by the list closing itself.
    crate::panels::multi_select(
        ui,
        format!("sweep_gate_{name}"),
        egui::RichText::new(caption).color(colour),
        None,
        |ui| {
            // Names only: nothing here is chosen, the dialog is confirming what
            // was chosen elsewhere.
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    for item in value.items() {
                        ui.label(egui::RichText::new(item).color(theme::TEXT));
                    }
                });
        },
    );
}
