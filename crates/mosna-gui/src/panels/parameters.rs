//! The Parameters panel — port of `ParametersPanel`.

use crate::app::MosnaApp;
use crate::model::field::{FieldKind, NO_SELECTION};
use crate::model::runner::Step;
use crate::panels::{accent_button, field_row, group, header, label_column_for, path_field};
use crate::theme;

/// Draw the right panel: one tab per configuration section, then the actions.
pub fn show(app: &mut MosnaApp, ui: &mut egui::Ui) {
    egui::containers::Panel::right("parameters")
        .resizable(true)
        .default_size(theme::PARAMETERS_WIDTH)
        .size_range(theme::PARAMETERS_MIN_WIDTH..=theme::PARAMETERS_MAX_WIDTH)
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(egui::Margin::same(theme::PANEL_MARGIN as i8)),
        )
        .show(ui, |ui| {
            header(ui, "Parameters");

            egui::containers::Panel::bottom("actions")
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6)))
                .show(ui, |ui| actions(app, ui));

            sections(app, ui);
        });
}

fn sections(app: &mut MosnaApp, ui: &mut egui::Ui) {
    let names: Vec<String> = app.form.sections.iter().map(|s| s.name.clone()).collect();
    if names.is_empty() {
        ui.label(egui::RichText::new("No configuration loaded.").color(theme::TEXT_MUTED));
        return;
    }

    let mut section_index = ui
        .data(|d| d.get_temp::<usize>(egui::Id::new("section_tab")))
        .unwrap_or(0)
        .min(names.len() - 1);

    ui.horizontal_wrapped(|ui| {
        for (index, name) in names.iter().enumerate() {
            if ui
                .selectable_label(
                    index == section_index,
                    tab_label(name, index == section_index),
                )
                .clicked()
            {
                section_index = index;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(egui::Id::new("section_tab"), section_index));
    ui.separator();

    let tabs: Vec<String> = app.form.sections[section_index]
        .tabs
        .iter()
        .map(|t| t.name.clone())
        .collect();
    if tabs.is_empty() {
        return;
    }

    let tab_id = egui::Id::new(("inner_tab", section_index));
    let mut tab_index = ui
        .data(|d| d.get_temp::<usize>(tab_id))
        .unwrap_or(0)
        .min(tabs.len() - 1);

    ui.horizontal_wrapped(|ui| {
        for (index, name) in tabs.iter().enumerate() {
            if ui
                .selectable_label(index == tab_index, tab_label(name, index == tab_index))
                .clicked()
            {
                tab_index = index;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(tab_id, tab_index));
    ui.add_space(4.0);

    // The clusterer and the reducer both decide which other fields are of any
    // use, so the whole tab is re-evaluated after the pass rather than during
    // it — a field cannot be disabled while it is being drawn.
    let mut choice_changed = false;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let groups = app.form.sections[section_index].tabs[tab_index]
                .groups
                .len();
            for group_index in 0..groups {
                let title = app.form.sections[section_index].tabs[tab_index].groups[group_index]
                    .title
                    .clone();
                group(ui, &title, |ui| {
                    let fields = app.form.sections[section_index].tabs[tab_index].groups
                        [group_index]
                        .fields
                        .len();

                    // One column for the whole group, set by its longest name,
                    // so the controls line up instead of forming a ragged edge.
                    let names: Vec<String> = app.form.sections[section_index].tabs[tab_index]
                        .groups[group_index]
                        .fields
                        .iter()
                        .map(caption_of)
                        .collect();
                    let column = label_column_for(ui, names.iter().map(String::as_str));
                    for field_index in 0..fields {
                        let field = &mut app.form.sections[section_index].tabs[tab_index].groups
                            [group_index]
                            .fields[field_index];

                        let mut caption =
                            egui::RichText::new(caption_of(field)).size(theme::size::LABEL);
                        if !field.enabled {
                            caption = caption.color(theme::TEXT_MUTED);
                        }

                        let enabled = field.enabled;
                        let key = field.key.clone();
                        let changed = field_row(ui, caption, field.tooltip, column, |ui| {
                            let mut changed = false;
                            ui.add_enabled_ui(enabled, |ui| {
                                changed = draw_field(ui, field);
                            });
                            changed
                        });
                        if changed && matches!(key.as_str(), "clusterer_type" | "reducer_type") {
                            choice_changed = true;
                        }
                        ui.add_space(4.0);
                    }
                });
            }
        });

    if choice_changed {
        app.form.refresh();
    }
}

/// The text a field's label shows: its key, marked when a tooltip is attached.
fn caption_of(field: &crate::model::field::Field) -> String {
    match field.tooltip {
        Some(_) => format!("{} ⓘ", field.key),
        None => field.key.clone(),
    }
}

/// Draw one field. Returns `true` when the user changed it.
fn draw_field(ui: &mut egui::Ui, field: &mut crate::model::field::Field) -> bool {
    let id = egui::Id::new(("field", &field.key));
    match &mut field.kind {
        FieldKind::Text { text } => ui
            .add(egui::TextEdit::singleline(text).desired_width(ui.available_width()))
            .changed(),

        FieldKind::Choice { options, selected } => {
            let mut changed = false;
            egui::ComboBox::from_id_salt(id)
                .selected_text(options.get(*selected).cloned().unwrap_or_default())
                .width(ui.available_width())
                .wrap_mode(egui::TextWrapMode::Truncate)
                .show_ui(ui, |ui| {
                    for (index, option) in options.iter().enumerate() {
                        if ui.selectable_value(selected, index, option).clicked() {
                            changed = true;
                        }
                    }
                });
            changed
        }

        FieldKind::ColumnPicker { columns, selected } => {
            let mut changed = false;
            let caption = selected.clone().unwrap_or_else(|| NO_SELECTION.to_string());
            egui::ComboBox::from_id_salt(id)
                .selected_text(caption)
                .width(ui.available_width())
                .wrap_mode(egui::TextWrapMode::Truncate)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(selected.is_none(), NO_SELECTION)
                        .clicked()
                    {
                        *selected = None;
                        changed = true;
                    }
                    for column in columns.iter() {
                        let picked = selected.as_deref() == Some(column.as_str());
                        if ui.selectable_label(picked, column).clicked() {
                            *selected = Some(column.clone());
                            changed = true;
                        }
                    }
                });
            changed
        }

        FieldKind::MultiColumnPicker { columns, selected } => {
            let caption = match selected.len() {
                0 => "— select column(s) —".to_string(),
                1 => selected[0].clone(),
                n if n <= 3 => selected.join(", "),
                n => format!("{n} columns selected"),
            };
            let mut changed = false;
            egui::ComboBox::from_id_salt(id)
                .selected_text(caption)
                .width(ui.available_width())
                .wrap_mode(egui::TextWrapMode::Truncate)
                .show_ui(ui, |ui| {
                    // The menu stays open across clicks, as the Qt one does,
                    // because choosing several columns one at a time is the
                    // normal case.
                    if ui.button("Select all").clicked() {
                        *selected = columns.clone();
                        changed = true;
                    }
                    if ui.button("Clear all").clicked() {
                        selected.clear();
                        changed = true;
                    }
                    ui.separator();
                    for column in columns.iter() {
                        let mut picked = selected.contains(column);
                        if ui.checkbox(&mut picked, column).clicked() {
                            if picked {
                                selected.push(column.clone());
                            } else {
                                selected.retain(|c| c != column);
                            }
                            changed = true;
                        }
                    }
                });
            changed
        }

        FieldKind::IndexPicker { columns, custom } => {
            let mut changed = false;
            // The mode picker takes a third of the column, the value the rest:
            // both then follow the panel instead of a constant.
            let mode_width = (ui.available_width() * 0.36).clamp(70.0, 120.0);
            ui.horizontal(|ui| {
                let mut mode = if custom.is_some() { "Custom" } else { "index" };
                egui::ComboBox::from_id_salt(id.with("mode"))
                    .selected_text(mode)
                    .width(mode_width)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut mode, "index", "index");
                        ui.selectable_value(&mut mode, "Custom", "Custom");
                    });
                if mode == "index" && custom.is_some() {
                    *custom = None;
                    changed = true;
                }

                ui.add_enabled_ui(mode == "Custom", |ui| {
                    let caption = custom.clone().unwrap_or_else(|| NO_SELECTION.to_string());
                    egui::ComboBox::from_id_salt(id.with("value"))
                        .selected_text(caption)
                        .width(ui.available_width())
                        .wrap_mode(egui::TextWrapMode::Truncate)
                        .show_ui(ui, |ui| {
                            for column in columns.iter() {
                                let picked = custom.as_deref() == Some(column.as_str());
                                if ui.selectable_label(picked, column).clicked() {
                                    *custom = Some(column.clone());
                                    changed = true;
                                }
                            }
                        });
                });
            });
            changed
        }

        FieldKind::DirectoryPath { path } => path_field(ui, "Choose saving directory", path),
    }
}

fn actions(app: &mut MosnaApp, ui: &mut egui::Ui) {
    let running = app.run.is_some();

    ui.horizontal(|ui| {
        let row = crate::panels::layout::content_width(ui.available_width());
        let save_width = row * 0.72;
        let save = egui::Button::new(
            egui::RichText::new("Save Config")
                .color(theme::TEXT_INVERSE)
                .strong(),
        )
        .fill(theme::ACCENT)
        .corner_radius(egui::CornerRadius::same(4))
        .truncate();
        if ui.add_sized([save_width, 28.0], save).clicked() {
            match app.save_config() {
                Ok(()) => app.status = "Configuration saved successfully.".to_string(),
                Err(error) => app.notice = Some(format!("Failed to save config:\n{error}")),
            }
        }

        ui.add_enabled_ui(running, |ui| {
            let stop_width = crate::panels::layout::content_width(ui.available_width());
            if ui
                .add_sized([stop_width, 28.0], egui::Button::new("Stop").truncate())
                .clicked()
            {
                app.stop();
            }
        });
    });

    ui.add_space(4.0);
    for step in Step::all() {
        ui.add_enabled_ui(!running, |ui| {
            if accent_button(ui, step.label(), theme::step_colour(step)).clicked() {
                app.start(step);
            }
        });
        ui.add_space(3.0);
    }
}

fn tab_label(name: &str, selected: bool) -> egui::RichText {
    let text = egui::RichText::new(name).size(theme::size::LABEL);
    if selected {
        text.color(theme::ACCENT_STRONG).strong()
    } else {
        text.color(theme::TEXT_MUTED)
    }
}
