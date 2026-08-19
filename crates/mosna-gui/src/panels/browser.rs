//! The Browser panel — port of `BrowserPanel`.

use crate::app::MosnaApp;
use crate::panels::{
    field_row, folded_spine, folding_header, group, label_column_for, path_field, Fold,
};
use crate::theme;

/// Draw the left panel: data sources, naming pattern, and the files found.
///
/// Or, when it has been folded away, the band that brings it back. The panel
/// is folded and unfolded under two different ids, so that the width it was
/// dragged to is still there when it comes back — egui remembers a panel's
/// size against its id, and one id cannot remember two.
pub fn show(app: &mut MosnaApp, ui: &mut egui::Ui) {
    if app.browser_folded {
        folded(app, ui);
        return;
    }

    egui::containers::Panel::left("browser")
        .resizable(true)
        .default_size(theme::BROWSER_WIDTH)
        .size_range(theme::BROWSER_MIN_WIDTH..=theme::BROWSER_MAX_WIDTH)
        .frame(
            // The panel's own margin, not one derived from `ui`: `ui` here is
            // the whole window, so `margin(ui.available_width())` returned the
            // upper bound of fourteen pixels on any normal screen — twenty-eight
            // of a two-hundred-and-sixty pixel panel, spent on nothing. Group
            // boxes still size their margin from the width they are given,
            // which is theirs to measure.
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(egui::Margin::same(theme::PANEL_MARGIN as i8)),
        )
        .show(ui, |ui| {
            if folding_header(ui, "Browser", Fold::Left) {
                app.browser_folded = true;
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                sources(app, ui);
                pattern(app, ui);
                actions(app, ui);
                results(app, ui);
            });
        });
}

/// The folded panel: a band down the left edge, with its name up it.
fn folded(app: &mut MosnaApp, ui: &mut egui::Ui) {
    egui::containers::Panel::left("browser_folded")
        .resizable(false)
        .exact_size(theme::FOLDED_WIDTH)
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(egui::Margin::symmetric(2, theme::PANEL_MARGIN as i8)),
        )
        .show(ui, |ui| {
            if folded_spine(ui, "Browser") {
                app.browser_folded = false;
            }
        });
}

fn sources(app: &mut MosnaApp, ui: &mut egui::Ui) {
    group(ui, "Data sources", |ui| {
        let column = label_column_for(ui, ["Nodes directory", "Network directory"]);

        field_row(ui, label("Nodes directory"), None, column, |ui| {
            path_field(
                ui,
                "Choose nodes directory",
                &mut app.browser.nodes_directory,
            );
        });
        ui.add_space(6.0);

        field_row(ui, label("Network directory"), None, column, |ui| {
            // The mode picker sits above the path rather than beside it: on a
            // narrow panel the two side by side left the path a few characters
            // wide, which is not a path.
            let mut mode = if app.browser.network_directory_is_default {
                "Default"
            } else {
                "Custom"
            };
            egui::ComboBox::from_id_salt("network_mode")
                .selected_text(mode)
                .width(ui.available_width())
                // Without this the caption is laid out at its natural width and
                // the box grows past the width it was just given — `width()` is
                // a *minimum* for the text, not a maximum for the widget. The
                // Parameters panel's pickers already truncate; these two did
                // not, which is the whole of why the Browser overflowed and the
                // Parameters panel did not.
                .wrap_mode(egui::TextWrapMode::Truncate)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut mode, "Default", "Default");
                    ui.selectable_value(&mut mode, "Custom", "Custom");
                });

            let is_default = mode == "Default";
            if is_default != app.browser.network_directory_is_default {
                app.browser.network_directory_is_default = is_default;
                if is_default {
                    app.browser.network_directory.clear();
                }
            }

            ui.add_enabled_ui(!is_default, |ui| {
                path_field(
                    ui,
                    "Choose network directory",
                    &mut app.browser.network_directory,
                );
            });
        });
    });
}

/// A field label at the interface's label size.
fn label(text: &str) -> egui::RichText {
    egui::RichText::new(text).size(theme::size::LABEL)
}

fn pattern(app: &mut MosnaApp, ui: &mut egui::Ui) {
    group(ui, "Pattern used to find files", |ui| {
        let column = label_column_for(
            ui,
            ["Patient column name", "Sample column name", "Extension"],
        );

        field_row(ui, label("Patient column name"), None, column, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.browser.patient_column)
                    .desired_width(ui.available_width()),
            );
        });
        ui.add_space(6.0);

        field_row(ui, label("Sample column name"), None, column, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.browser.sample_column)
                    .desired_width(ui.available_width()),
            );
        });
        ui.add_space(6.0);

        field_row(ui, label("Extension"), None, column, |ui| {
            egui::ComboBox::from_id_salt("extension")
                .selected_text(app.browser.extension.clone())
                .width(ui.available_width())
                .wrap_mode(egui::TextWrapMode::Truncate)
                .show_ui(ui, |ui| {
                    for option in ["csv", "parquet", "tsv"] {
                        ui.selectable_value(&mut app.browser.extension, option.to_string(), option);
                    }
                });
        });
    });
}

/// The two Refresh buttons: side by side when both captions fit, one per line
/// when they do not.
///
/// Halving the row unconditionally is what pushed the second button out of the
/// panel: `add_sized` does not shrink a button below its caption, so each one
/// took the hundred and forty pixels `Refresh Networks` needs whatever it was
/// offered, and the pair ran past the frame. At the panel's default width they
/// stack, which is also how the Parameters panel arranges its own actions.
fn actions(app: &mut MosnaApp, ui: &mut egui::Ui) {
    const CAPTIONS: [&str; 2] = ["Refresh Nodes", "Refresh Networks"];

    ui.add_space(8.0);
    let widest = CAPTIONS
        .iter()
        .map(|caption| crate::panels::button_width(ui, caption))
        .fold(0.0f32, f32::max);
    let spacing = ui.spacing().item_spacing.x;
    let height = ui.spacing().interact_size.y;

    let mut clicked = [false; CAPTIONS.len()];
    let mut draw = |ui: &mut egui::Ui, width: f32| {
        for (index, caption) in CAPTIONS.iter().enumerate() {
            clicked[index] = ui
                .add_sized([width, height], egui::Button::new(*caption).truncate())
                .clicked();
        }
    };

    match crate::panels::layout::buttons_in_a_row(
        ui.available_width(),
        spacing,
        CAPTIONS.len(),
        widest,
    ) {
        Some(width) => {
            ui.horizontal(|ui| draw(ui, width));
        }
        None => {
            let width = crate::panels::layout::content_width(ui.available_width());
            draw(ui, width);
        }
    }

    if clicked[0] {
        app.refresh_nodes();
    }
    if clicked[1] {
        app.refresh_networks();
    }
}

fn results(app: &mut MosnaApp, ui: &mut egui::Ui) {
    group(ui, "Files found", |ui| {
        let caption = if app.rows.is_empty() {
            "No file matches the current pattern.".to_string()
        } else {
            format!("{} file(s) found.", app.rows.len())
        };
        ui.label(
            egui::RichText::new(caption)
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
        );
        ui.add_space(4.0);

        let mut clicked_row = None;
        egui::ScrollArea::both()
            .max_height(360.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("results")
                    .num_columns(4)
                    .striped(true)
                    .spacing([14.0, 6.0])
                    .show(ui, |ui| {
                        for title in ["Patient", "Sample", "Nodes files", "Edges files"] {
                            ui.label(
                                egui::RichText::new(title)
                                    .color(theme::TEXT_MUTED)
                                    .size(theme::size::LABEL)
                                    .strong(),
                            );
                        }
                        ui.end_row();

                        for (index, row) in app.rows.iter().enumerate() {
                            let selected = app.selected_row == Some(index);
                            let paint = |text: &str| {
                                let mut rich = egui::RichText::new(text).size(theme::size::LABEL);
                                if selected {
                                    rich = rich.color(theme::ACCENT_STRONG).strong();
                                }
                                rich
                            };

                            if ui.selectable_label(selected, paint(&row.patient)).clicked() {
                                clicked_row = Some(index);
                            }
                            ui.label(paint(row.sample.as_deref().unwrap_or("")));
                            ui.label(paint(&row.nodes_file));
                            ui.label(paint(row.edges_file.as_deref().unwrap_or("")));
                            ui.end_row();
                        }
                    });
            });

        if let Some(index) = clicked_row {
            app.selected_row = Some(index);
            app.load_columns_of_selection();
        }
    });
}
