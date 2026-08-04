//! The Viewer panel — port of `ImageViewerPanel` and `AnalysisImageTab`.

use std::path::PathBuf;

use crate::app::{AnalysisTab, MosnaApp, ViewerTab};
use crate::model::viewer::AnalysisImages;
use crate::panels::header;
use crate::theme;

/// Draw the centre panel: figures, log, documentation, and the status bar.
pub fn show(app: &mut MosnaApp, ui: &mut egui::Ui) {
    egui::containers::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(egui::Margin::same(10)),
        )
        .show(ui, |ui| {
            header(ui, "Viewer");

            egui::containers::Panel::bottom("status")
                .frame(
                    egui::Frame::new()
                        .fill(theme::SURFACE)
                        .inner_margin(egui::Margin::symmetric(10, 8)),
                )
                .show(ui, |ui| status_bar(app, ui));

            ui.horizontal(|ui| {
                for (tab, name) in [
                    (ViewerTab::Images, "Images"),
                    (ViewerTab::Log, "Log"),
                    (ViewerTab::Documentation, "Documentation"),
                ] {
                    let selected = app.viewer_tab == tab;
                    if ui
                        .selectable_label(selected, label(name, selected))
                        .clicked()
                    {
                        app.viewer_tab = tab;
                    }
                }
            });
            ui.separator();

            match app.viewer_tab {
                ViewerTab::Images => images(app, ui),
                ViewerTab::Log => log(app, ui),
                ViewerTab::Documentation => super::documentation::show(app, ui),
            }
        });
}

fn images(app: &mut MosnaApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for (tab, name) in [
            (AnalysisTab::Tysserand, "Tysserand"),
            (AnalysisTab::Assortativity, "Assortativity"),
            (AnalysisTab::Niches, "Niches"),
        ] {
            let selected = app.analysis_tab == tab;
            if ui
                .selectable_label(selected, label(name, selected))
                .clicked()
            {
                app.analysis_tab = tab;
                app.selected_image = 0;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Refresh").clicked() {
                app.refresh_images();
            }
        });
    });
    ui.add_space(4.0);

    let set = match app.analysis_tab {
        AnalysisTab::Tysserand => &app.images.tysserand,
        AnalysisTab::Assortativity => &app.images.assortativity,
        AnalysisTab::Niches => &app.images.niches,
    };

    // Step 1 produces only per-patient figures, so its tab has no Global list —
    // the same asymmetry the Python builds.
    let per_patient_only = app.analysis_tab == AnalysisTab::Tysserand;
    let gallery = gallery_for(set, &mut app.selected_patient, per_patient_only, ui);

    if gallery.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new("No figure yet.\nRun an analysis, then press Refresh.")
                    .color(theme::TEXT_MUTED),
            );
        });
        return;
    }

    app.selected_image = app.selected_image.min(gallery.len() - 1);

    egui::ScrollArea::horizontal()
        .id_salt("figure_tabs")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (index, path) in gallery.iter().enumerate() {
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let selected = index == app.selected_image;
                    if ui
                        .selectable_label(selected, label(&name, selected))
                        .clicked()
                    {
                        app.selected_image = index;
                    }
                }
            });
        });
    ui.add_space(4.0);

    let path = &gallery[app.selected_image];
    egui::ScrollArea::both()
        .id_salt("figure")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                egui::Image::new(crate::model::viewer::file_uri(path))
                    .fit_to_original_size(1.0)
                    .max_width(ui.available_width()),
            );
        });
}

/// The figures to show, and the patient selector above them.
fn gallery_for(
    set: &AnalysisImages,
    selected_patient: &mut Option<String>,
    per_patient_only: bool,
    ui: &mut egui::Ui,
) -> Vec<PathBuf> {
    let patients: Vec<&String> = set.patients.keys().collect();

    if !patients.is_empty() {
        if selected_patient
            .as_ref()
            .map(|current| !set.patients.contains_key(current))
            .unwrap_or(true)
        {
            *selected_patient = Some(patients[0].clone());
        }

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Patient").color(theme::TEXT_MUTED));
            let caption = selected_patient.clone().unwrap_or_default();
            egui::ComboBox::from_id_salt("patient")
                .selected_text(caption)
                .show_ui(ui, |ui| {
                    for patient in &patients {
                        let picked = selected_patient.as_ref() == Some(*patient);
                        if ui.selectable_label(picked, *patient).clicked() {
                            *selected_patient = Some((*patient).clone());
                        }
                    }
                });
        });
    }

    let mut gallery = Vec::new();
    if !per_patient_only {
        gallery.extend(set.global.iter().cloned());
    }
    if let Some(patient) = selected_patient {
        if let Some(images) = set.patients.get(patient) {
            gallery.extend(images.iter().cloned());
        }
    }
    gallery
}

fn log(app: &mut MosnaApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Script output").color(theme::TEXT_MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Clear").clicked() {
                app.log.clear();
            }
        });
    });
    ui.add_space(4.0);

    egui::Frame::new()
        .fill(theme::FIELD)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if app.log.is_empty() {
                        ui.label(
                            egui::RichText::new("Script output will appear here…")
                                .color(theme::TEXT_MUTED)
                                .italics(),
                        );
                    }
                    for (kind, line) in &app.log {
                        ui.label(
                            egui::RichText::new(line)
                                .color(theme::log_colour(*kind))
                                .monospace()
                                .size(theme::size::MONO),
                        );
                    }
                });
        });
}

fn status_bar(app: &mut MosnaApp, ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new(&app.status)
            .color(theme::TEXT)
            .size(theme::size::LABEL),
    );
    ui.add_space(5.0);

    let colour = if app.last_run_failed {
        theme::STEP_FAILED
    } else {
        app.active_step
            .map(theme::step_colour)
            .unwrap_or(theme::ACCENT_SOFT)
    };

    let bar = match app.progress {
        Some((current, total)) if total > 0 => {
            egui::ProgressBar::new((current as f32 / total as f32).clamp(0.0, 1.0))
                .text(format!("{current} / {total}"))
        }
        // No count yet: an animated bar says "working" without claiming a
        // position it does not know.
        _ if app.run.is_some() => egui::ProgressBar::new(0.0).animate(true),
        _ => egui::ProgressBar::new(0.0),
    };

    ui.add(
        bar.fill(colour)
            .desired_height(theme::size::BODY + 10.0)
            .corner_radius(egui::CornerRadius::same(4)),
    );
}

fn label(name: &str, selected: bool) -> egui::RichText {
    let text = egui::RichText::new(name).size(theme::size::LABEL);
    if selected {
        text.color(theme::ACCENT_STRONG).strong()
    } else {
        text.color(theme::TEXT_MUTED)
    }
}
