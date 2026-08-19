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
                // Tight, because the Network tab's canvas is the one thing in
                // this panel that can always use another few pixels.
                .inner_margin(egui::Margin::same(6)),
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
                    (ViewerTab::Network, "Network"),
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
                ViewerTab::Network => super::network::show(app, ui),
                ViewerTab::Log => log(app, ui),
                ViewerTab::Documentation => super::documentation::show(app, ui),
            }
        });
}

fn images(app: &mut MosnaApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for (tab, name) in [
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
        AnalysisTab::Assortativity => &app.images.assortativity,
        AnalysisTab::Niches => &app.images.niches,
    };

    let gallery = gallery_for(set, &mut app.selected_patient, ui);

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
    gallery.extend(set.global.iter().cloned());
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

/// Height of the bar, and how round its ends are.
const BAR_HEIGHT: f32 = theme::size::BODY + 10.0;
const BAR_RADIUS: f32 = 5.0;

/// How many strips the gradient is painted in.
///
/// `egui` fills a rectangle with one colour, so a gradient is a row of narrow
/// rectangles. Ninety-six of them is a strip of two or three pixels on any
/// panel this is drawn in — below what an eye resolves as a step — and ninety-
/// six rectangles is nothing to draw.
const SLICES: usize = 96;

fn status_bar(app: &mut MosnaApp, ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new(&app.status)
            .color(theme::TEXT)
            .size(theme::size::LABEL),
    );
    ui.add_space(5.0);

    let running = app.run.is_some();
    let caption = match app.progress {
        Some((current, total)) if total > 0 => format!("{current} / {total}"),
        _ => String::new(),
    };

    ui.horizontal(|ui| {
        // The count sits beside the bar rather than on it. On it, it would have
        // to be legible against both ends of a ramp that runs from dark bronze
        // to pale champagne, and no single colour is.
        let reserved = if caption.is_empty() {
            0.0
        } else {
            crate::panels::label_width(ui, &caption) + ui.spacing().item_spacing.x
        };
        let width =
            (crate::panels::layout::content_width(ui.available_width()) - reserved).max(48.0);

        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(width, BAR_HEIGHT),
            egui::Sense::focusable_noninteractive(),
        );
        paint_bar(app, ui, rect, running);

        if !caption.is_empty() {
            ui.label(
                egui::RichText::new(caption)
                    .color(theme::TEXT_MUTED)
                    .size(theme::size::LABEL)
                    .monospace(),
            );
        }
    });

    if running {
        // Nothing else asks for a frame while a step runs, so without this the
        // gold would freeze the moment the pointer stopped moving — which is
        // the opposite of what it is there to say. Thirty a second is smooth
        // and costs a fraction of a core.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
}

/// The track, and whatever is flowing along it.
fn paint_bar(app: &MosnaApp, ui: &egui::Ui, rect: egui::Rect, running: bool) {
    let painter = ui.painter();
    let radius = egui::CornerRadius::same(BAR_RADIUS as u8);
    painter.rect_filled(rect, radius, theme::SURFACE);
    painter.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );

    let phase = crate::model::flow::phase(ui.input(|input| input.time));

    match app.progress {
        // A position to show: the ramp fills it, and the highlight travels
        // along what has been done.
        Some((current, total)) if total > 0 => {
            let done = (current as f32 / total as f32).clamp(0.0, 1.0);
            let shade: Box<dyn Fn(f32) -> egui::Color32> = if app.last_run_failed {
                Box::new(|_| theme::STEP_FAILED)
            } else if running {
                Box::new(move |t| crate::model::flow::shade(t, phase))
            } else {
                // Finished, and still on screen: the ramp stays, the movement
                // stops. A bar that keeps flowing after the process exited
                // says the wrong thing.
                Box::new(crate::model::flow::base)
            };
            paint_ramp(painter, rect, (0.0, done), &shade);
        }
        // No count yet: a band sweeps the track. It says "working" without
        // claiming a position it does not know.
        _ if running => {
            let (start, end) = crate::model::flow::band(phase);
            paint_ramp(painter, rect, (start, end), &|t| {
                // Bright in the middle: the band is a comet, and its own
                // movement is the flow.
                crate::model::flow::shade(t, 0.5)
            });
        }
        _ => {}
    }
}

/// Fill `span` of `rect` with a colour that changes along it.
fn paint_ramp(
    painter: &egui::Painter,
    rect: egui::Rect,
    span: (f32, f32),
    shade: &dyn Fn(f32) -> egui::Color32,
) {
    let (start, end) = span;
    let left = rect.left() + rect.width() * start;
    let right = rect.left() + rect.width() * end;
    if right - left < 1.0 {
        return;
    }

    let step = (right - left) / SLICES as f32;
    for index in 0..SLICES {
        let from = left + step * index as f32;
        // Half a pixel of overlap, or a seam shows between two strips whose
        // edges land either side of a device pixel.
        let to = (from + step + 0.5).min(right);

        let corner = egui::CornerRadius {
            nw: if index == 0 { BAR_RADIUS as u8 } else { 0 },
            sw: if index == 0 { BAR_RADIUS as u8 } else { 0 },
            ne: if index + 1 == SLICES {
                BAR_RADIUS as u8
            } else {
                0
            },
            se: if index + 1 == SLICES {
                BAR_RADIUS as u8
            } else {
                0
            },
        };

        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(from, rect.top()), egui::pos2(to, rect.bottom())),
            corner,
            shade((index as f32 + 0.5) / SLICES as f32),
        );
    }
}

fn label(name: &str, selected: bool) -> egui::RichText {
    let text = egui::RichText::new(name).size(theme::size::LABEL);
    if selected {
        text.color(theme::ACCENT_STRONG).strong()
    } else {
        text.color(theme::TEXT_MUTED)
    }
}
