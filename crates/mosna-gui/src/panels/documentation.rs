//! The manual, drawn inside the Viewer panel.
//!
//! Laid out like the Squidpy documentation — a navigation tree on the left, the
//! page on the right, a toolbar above with a search box and the language
//! button — but drawn with the interface's own widgets, so it wears the same
//! slate-and-teal palette as everything else.
//!
//! Every decision this file makes about *where the reader is* comes from
//! [`crate::docs::state::ManualState`], which is tested. What is left here is
//! only the drawing.

use crate::app::MosnaApp;
use crate::docs::model::{Block, CalloutKind, Section, Text};
use crate::docs::state::ManualState;
use crate::docs::Documentation;
use crate::theme;

/// Width of the navigation column.
const NAVIGATION_WIDTH: f32 = 210.0;

pub fn show(app: &mut MosnaApp, ui: &mut egui::Ui) {
    // Split the borrow: the document is immutable while the reader's position
    // is written to.
    let MosnaApp {
        documentation,
        manual,
        ..
    } = app;

    toolbar(documentation, manual, ui);
    ui.add_space(6.0);

    egui::containers::Panel::left("manual_navigation")
        .default_size(NAVIGATION_WIDTH)
        .size_range(160.0..=320.0)
        .frame(
            egui::Frame::new()
                .fill(theme::SURFACE)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .corner_radius(egui::CornerRadius::same(4)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("manual_navigation_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| navigation(documentation, manual, ui));
        });

    egui::ScrollArea::vertical()
        .id_salt("manual_page")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // A margin down the left, and a ceiling on the measure. Text that
            // starts where the panel starts is uncomfortable to read; text
            // stretched across a wide screen is hard to follow back to the next
            // line. On a narrow window the margin gives way before the text
            // does — a cramped margin beats a squeezed column.
            let margin = theme::DOC_MARGIN.min(ui.available_width() * 0.1);
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: margin as i8,
                    right: (margin * 0.5) as i8,
                    top: 8,
                    bottom: 16,
                })
                .show(ui, |ui| {
                    ui.set_max_width(theme::DOC_MAX_WIDTH);
                    page(documentation, manual, ui);
                });
        });
}

// ---------------------------------------------------------------------------
// Toolbar
// ---------------------------------------------------------------------------

fn toolbar(documentation: &Documentation, manual: &mut ManualState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        // Previous / next walk the manual in reading order; they go dead at the
        // ends rather than wrapping.
        ui.add_enabled_ui(manual.has_previous(documentation), |ui| {
            if ui.button("←").on_hover_text("Previous page").clicked() {
                manual.previous(documentation);
            }
        });
        ui.add_enabled_ui(manual.has_next(documentation), |ui| {
            if ui.button("→").on_hover_text("Next page").clicked() {
                manual.next(documentation);
            }
        });

        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Search")
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
        );
        // No handler: `results` reads the box directly on the next frame.
        ui.add(
            egui::TextEdit::singleline(&mut manual.query)
                .desired_width(180.0)
                .hint_text("filter the manual"),
        );
        if manual.is_searching() && ui.button("✕").on_hover_text("Clear").clicked() {
            manual.clear_search();
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // The button says the language it switches *to*.
            if ui
                .button(
                    egui::RichText::new(format!("🌐 {}", manual.language_button()))
                        .color(theme::ACCENT_STRONG),
                )
                .on_hover_text("Switch language")
                .clicked()
            {
                manual.toggle_language();
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

fn navigation(documentation: &Documentation, manual: &mut ManualState, ui: &mut egui::Ui) {
    if manual.is_searching() {
        let results = manual.results(documentation);
        ui.label(
            egui::RichText::new(match results.len() {
                0 => "No match".to_string(),
                1 => "1 match".to_string(),
                n => format!("{n} matches"),
            })
            .color(theme::TEXT_MUTED)
            .size(theme::size::SMALL),
        );
        ui.add_space(4.0);

        let found: Vec<(&'static str, &'static str)> = results
            .iter()
            .map(|section| (section.id, section.title.get(manual.language)))
            .collect();
        for (id, title) in found {
            if entry(ui, title, manual.section == id, 0.0) {
                manual.select(id);
            }
        }
        return;
    }

    let language = manual.language;
    for chapter in &documentation.chapters {
        let expanded = manual.is_expanded(chapter.id);
        let marker = if expanded { "▾" } else { "▸" };
        let heading = egui::RichText::new(format!("{marker}  {}", chapter.title.get(language)))
            .color(theme::ACCENT)
            .size(theme::size::HEADING)
            .strong();

        if ui
            .add(egui::Label::new(heading).sense(egui::Sense::click()))
            .clicked()
        {
            manual.toggle_chapter(chapter.id);
        }

        if expanded {
            for section in &chapter.sections {
                if entry(
                    ui,
                    section.title.get(language),
                    manual.section == section.id,
                    12.0,
                ) {
                    manual.select(section.id);
                }
            }
        }
        ui.add_space(6.0);
    }
}

/// One clickable line in the navigation. Returns whether it was clicked.
fn entry(ui: &mut egui::Ui, title: &str, selected: bool, indent: f32) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.add_space(indent);
        let text = egui::RichText::new(title).size(theme::size::BODY);
        let text = if selected {
            text.color(theme::ACCENT_STRONG).strong()
        } else {
            text.color(theme::TEXT_MUTED)
        };
        clicked = ui.selectable_label(selected, text).clicked();
    });
    clicked
}

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

fn page(documentation: &Documentation, manual: &ManualState, ui: &mut egui::Ui) {
    let Some(section) = manual.current(documentation) else {
        ui.label(egui::RichText::new("The manual is empty.").color(theme::TEXT_MUTED));
        return;
    };
    let language = manual.language;

    // A breadcrumb, so a reader who arrived by search knows where they landed.
    if let Some(chapter) = documentation.chapter_of(section.id) {
        ui.label(
            egui::RichText::new(chapter.title.get(language))
                .color(theme::TEXT_MUTED)
                .size(theme::size::LABEL),
        );
    }
    ui.label(
        egui::RichText::new(section.title.get(language))
            .color(theme::ACCENT_STRONG)
            .size(theme::size::PAGE_TITLE)
            .strong(),
    );
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(6.0);

    for block in &section.blocks {
        draw_block(block, manual, ui);
        ui.add_space(14.0);
    }

    footer(documentation, section, manual, ui);
}

fn draw_block(block: &Block, manual: &ManualState, ui: &mut egui::Ui) {
    let language = manual.language;
    match block {
        Block::Heading(text) => {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(text.get(language))
                    .color(theme::ACCENT)
                    .size(theme::size::HEADING)
                    .strong(),
            );
        }
        Block::Paragraph(text) => {
            ui.label(
                egui::RichText::new(text.get(language))
                    .color(theme::TEXT)
                    .size(theme::size::BODY),
            );
        }
        Block::List(items) => {
            for item in items {
                ui.horizontal_top(|ui| {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("•")
                            .color(theme::ACCENT)
                            .size(theme::size::BODY),
                    );
                    ui.label(
                        egui::RichText::new(item.get(language))
                            .color(theme::TEXT)
                            .size(theme::size::BODY),
                    );
                });
            }
        }
        Block::Callout { kind, text } => callout(*kind, *text, language, ui),
        Block::Code { caption, lines } => code(caption.get(language), lines, ui),
        Block::Table { headers, rows } => table(headers, rows, language, ui),
        Block::Image { asset, caption } => image(asset, caption.get(language), ui),
        Block::Citations(citations) => self::citations(citations, language, ui),
    }
}

/// The credits list: one row per crate, its name set in the code face so it
/// can be typed into a search box as it appears.
fn citations(
    citations: &[crate::docs::Citation],
    language: crate::docs::Language,
    ui: &mut egui::Ui,
) {
    for citation in citations {
        ui.horizontal_top(|ui| {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(citation.name)
                    .color(theme::ACCENT)
                    .monospace()
                    .size(theme::size::MONO)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(citation.role.get(language))
                    .color(theme::TEXT)
                    .size(theme::size::BODY),
            );
        });
        ui.add_space(4.0);
    }
}

fn callout(kind: CalloutKind, text: Text, language: crate::docs::Language, ui: &mut egui::Ui) {
    let (accent, marker) = match kind {
        CalloutKind::Tip => (theme::ACCENT, "Tip"),
        CalloutKind::Warning => (theme::STEP_FAILED, "Warning"),
        CalloutKind::Note => (theme::TEXT_MUTED, "Note"),
    };

    egui::Frame::new()
        .fill(theme::SURFACE)
        // A left rule instead of a coloured background: it reads as an aside
        // without fighting the palette.
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(marker)
                    .color(accent)
                    .size(theme::size::LABEL)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(text.get(language))
                    .color(theme::TEXT)
                    .size(theme::size::BODY),
            );
        });
}

fn code(caption: &str, lines: &[&'static str], ui: &mut egui::Ui) {
    let body = lines.join("\n");

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(caption)
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Copy").clicked() {
                ui.ctx().copy_text(body.clone());
            }
        });
    });

    egui::Frame::new()
        .fill(theme::FIELD)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            for line in lines {
                // A comment is dimmed, as a terminal would show it.
                let colour = if line.trim_start().starts_with('#') {
                    theme::TEXT_MUTED
                } else {
                    theme::TEXT
                };
                ui.label(
                    egui::RichText::new(*line)
                        .color(colour)
                        .monospace()
                        .size(theme::size::MONO),
                );
            }
        });
}

fn table(
    headers: &[Text; 3],
    rows: &[crate::docs::ParameterRow],
    language: crate::docs::Language,
    ui: &mut egui::Ui,
) {
    use egui_extras::{Column, TableBuilder};

    let height = ui.text_style_height(&egui::TextStyle::Body);

    TableBuilder::new(ui)
        .id_salt(rows.first().map(|row| row.name).unwrap_or("table"))
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::TOP))
        .column(Column::auto().at_least(170.0))
        .column(Column::auto().at_least(80.0))
        .column(Column::remainder())
        .header(height + 6.0, |mut header| {
            for text in headers {
                header.col(|ui| {
                    ui.label(
                        egui::RichText::new(text.get(language))
                            .color(theme::ACCENT)
                            .size(theme::size::LABEL)
                            .strong(),
                    );
                });
            }
        })
        .body(|body| {
            // The descriptions wrap, so a row is as tall as its text is long.
            // Roughly sixty characters fit on a line at this width.
            let heights = rows
                .iter()
                .map(|row| {
                    let length = row.description.get(language).chars().count();
                    height * (1.0 + (length / 55) as f32).max(1.5) + 8.0
                })
                .collect::<Vec<f32>>();

            body.heterogeneous_rows(heights.into_iter(), |mut line| {
                let row = &rows[line.index()];
                {
                    line.col(|ui| {
                        ui.label(
                            egui::RichText::new(row.name)
                                .color(theme::TEXT)
                                .monospace()
                                .size(theme::size::MONO),
                        );
                    });
                    line.col(|ui| {
                        ui.label(
                            egui::RichText::new(row.kind)
                                .color(theme::TEXT_MUTED)
                                .size(theme::size::LABEL),
                        );
                    });
                    line.col(|ui| {
                        ui.label(
                            egui::RichText::new(row.description.get(language))
                                .color(theme::TEXT)
                                .size(theme::size::BODY),
                        );
                    });
                }
            });
        });
}

fn image(asset: &'static str, caption: &str, ui: &mut egui::Ui) {
    match crate::docs::assets::image(asset) {
        Some(bytes) => {
            ui.add(
                egui::Image::from_bytes(format!("bytes://{asset}"), bytes)
                    .max_width(ui.available_width().min(720.0))
                    .corner_radius(egui::CornerRadius::same(4)),
            );
            ui.label(
                egui::RichText::new(caption)
                    .color(theme::TEXT_MUTED)
                    .size(theme::size::SMALL)
                    .italics(),
            );
        }
        // Shipping is checked by the test suite, so this is unreachable in a
        // built binary; saying so is still better than an empty gap.
        None => {
            ui.label(
                egui::RichText::new(format!("[missing figure: {asset}]"))
                    .color(theme::TEXT_MUTED)
                    .italics(),
            );
        }
    }
}

/// The previous/next pair repeated at the bottom, as a documentation site does.
fn footer(
    documentation: &Documentation,
    section: &Section,
    manual: &ManualState,
    ui: &mut egui::Ui,
) {
    let order: Vec<&Section> = documentation
        .chapters
        .iter()
        .flat_map(|chapter| chapter.sections.iter())
        .collect();
    let Some(index) = order.iter().position(|s| s.id == section.id) else {
        return;
    };

    ui.add_space(6.0);
    ui.separator();
    ui.horizontal(|ui| {
        if let Some(previous) = index.checked_sub(1).and_then(|i| order.get(i)) {
            ui.label(
                egui::RichText::new(format!("← {}", previous.title.get(manual.language)))
                    .color(theme::TEXT_MUTED)
                    .size(theme::size::SMALL),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(next) = order.get(index + 1) {
                ui.label(
                    egui::RichText::new(format!("{} →", next.title.get(manual.language)))
                        .color(theme::TEXT_MUTED)
                        .size(theme::size::SMALL),
                );
            }
        });
    });
}
