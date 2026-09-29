//! The sensitivity screen: a grid of niche analyses, and what came back.
//!
//! # Why it replaces the window rather than opening one
//!
//! Choosing a grid is not something done beside the main interface — it is done
//! instead of it, with the whole width, and it ends by sending the user back to
//! the figures the runs produced. A second window would have to be found,
//! raised and closed; a screen with a way back is one gesture in each
//! direction. The arrow in the top-left corner is that way back, and it is
//! where a reader's eye already goes for it.
//!
//! # What it is not
//!
//! It is not a second kind of analysis. Every point of the grid is an ordinary
//! `mosna niche-analysis`, with a few keys replaced — so it writes its labels
//! into the nodes files under the ordinary `niches_1-2-4` name, lands in the
//! ordinary register, and its figures show up in the ordinary viewer. What this
//! screen adds is the grid, the order the grid is walked in, and the table that
//! compares the results.

use crate::app::{MosnaApp, Screen};
use crate::model::sweep::{self, AxisEdit, Sampling, Scale, Stage};
use crate::panels;
use crate::theme;

/// Draw the whole screen into whatever the window has left.
pub fn show(app: &mut MosnaApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme::BACKGROUND).inner_margin(12.0))
        .show(ui, |ui| {
            header(app, ui);
            ui.add_space(6.0);
            progress(app, ui);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        let half = (ui.available_width() - 18.0).max(280.0) / 2.0;
                        ui.allocate_ui(egui::vec2(half, 0.0), |ui| grid(app, ui));
                        ui.add_space(12.0);
                        ui.allocate_ui(egui::vec2(half, 0.0), |ui| results(app, ui));
                    });
                });
        });
}

/// The way back, and what the screen is.
fn header(app: &mut MosnaApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        // The arrow is the first thing in the top-left corner, which is where
        // a reader's eye already goes to leave a screen.
        let back = ui
            .add(
                egui::Button::new(
                    egui::RichText::new("\u{2190}")
                        .size(theme::size::PANEL_TITLE + 4.0)
                        .color(theme::ACCENT),
                )
                .frame(false),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Back to the main interface");
        if back.clicked() {
            app.screen = Screen::Main;
        }

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Sensitivity analysis")
                .color(theme::ACCENT)
                .size(theme::size::PANEL_TITLE + 2.0)
                .strong(),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mode = sweep::swept_subsection(&app.config);
            ui.label(
                egui::RichText::new(format!("varying the `{mode}` settings"))
                    .color(theme::TEXT_MUTED),
            );
        });
    });
    ui.separator();
}

/// The left-hand half: what to vary.
///
/// Four collapsible boxes — the three stages, then the normalisation. Each row
/// is a bulleted parameter name, its controls, and the values it will produce
/// in a gold frame on the right. Nothing has to be enabled before it can be
/// typed into; a parameter set to one value is a parameter held still, which is
/// what the form opens on.
fn grid(app: &mut MosnaApp, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        let running = app.sweep.is_some();

        for stage in [Stage::Aggregation, Stage::Reduction] {
            // Which rows decide nothing as the form stands — read before the
            // loop, because the rows are drawn through a mutable borrow.
            let inert: Vec<bool> = app
                .sweep_form
                .stage(stage)
                .map(|axis| app.sweep_form.is_inert(&axis.key))
                .collect();
            let retired = stage == Stage::Reduction && app.sweep_form.reduction_is_inert();

            section(ui, stage.title(), |ui| {
                if retired {
                    ui.label(
                        egui::RichText::new(
                            "No reducer is chosen, so nothing is projected: the parameters \
                             the reducer alone reads are retired. `n_neighbors` stays — it \
                             also caps leiden's graph degree.",
                        )
                        .color(theme::TEXT_MUTED)
                        .size(theme::size::SMALL),
                    );
                    ui.add_space(4.0);
                }

                let keys: Vec<String> = app
                    .sweep_form
                    .stage(stage)
                    .map(|axis| axis.key.clone())
                    .collect();
                for (position, key) in keys.into_iter().enumerate() {
                    let dead = inert.get(position).copied().unwrap_or(false);
                    if let Some(axis) = app.sweep_form.shared.iter_mut().find(|a| a.key == key) {
                        axis_row(ui, axis, running || dead, dead);
                    }
                }
            });
        }

        section(ui, Stage::Clustering.title(), |ui| {
            ui.label(
                egui::RichText::new(
                    "Tick the clusterers to try. Each varies only the parameters it reads.",
                )
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
            );
            ui.add_space(4.0);

            for index in 0..app.sweep_form.clusterers.len() {
                let clusterer = app.sweep_form.clusterers[index].clusterer.clone();
                ui.add_enabled(
                    !running,
                    egui::Checkbox::new(
                        &mut app.sweep_form.clusterers[index].selected,
                        egui::RichText::new(&clusterer)
                            .size(theme::size::HEADING)
                            .strong(),
                    ),
                );
                // A clusterer that is on has its parameters, and they are not
                // optional: leiden reads `resolution` whatever the user does,
                // so there is nothing to opt into — only values to give.
                if app.sweep_form.clusterers[index].selected {
                    ui.indent(clusterer, |ui| {
                        for axis in &mut app.sweep_form.clusterers[index].axes {
                            axis_row(ui, axis, running, false);
                        }
                    });
                }
                ui.add_space(2.0);
            }
        });

        section(ui, "Normalisation", |ui| {
            ui.label(
                egui::RichText::new(
                    "Which composition figures every run draws. It changes no part of the \
                     partition, so it is one choice for the whole sweep rather than an axis \
                     of it — `all` draws the five in one pass.",
                )
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
            );
            ui.add_space(4.0);
            ui.add_enabled_ui(!running, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for option in sweep::NORMALIZATIONS {
                        ui.radio_value(
                            &mut app.sweep_form.normalize,
                            (*option).to_string(),
                            *option,
                        );
                    }
                });
            });
        });

        ui.add_space(8.0);
        summary(app, ui);
    });
}

/// A titled box that folds away.
///
/// Four of them stacked is more than a screen holds once a clusterer is open,
/// and the stage a user is working on is rarely all of them at once.
fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(
        egui::RichText::new(title)
            .color(theme::ACCENT)
            .size(theme::size::PANEL_TITLE)
            .strong(),
    )
    .id_salt(title)
    .default_open(true)
    .show_unindented(ui, |ui| {
        ui.add_space(2.0);
        body(ui);
        ui.add_space(4.0);
    });
    ui.add_space(2.0);
}

/// One parameter: its name, the controls, and what it will run.
///
/// The name is a label rather than a tick-box. Every parameter is in the grid;
/// what a row says is how many values it contributes, and one value is a
/// parameter held at the figure beside it.
fn axis_row(ui: &mut egui::Ui, axis: &mut AxisEdit, disabled: bool, retired: bool) {
    /// Wide enough for the longest name — `min_cluster_size` — so every row's
    /// controls start at the same place and the column reads as a column.
    const NAME_WIDTH: f32 = 150.0;

    ui.horizontal(|ui| {
        ui.allocate_ui(egui::vec2(NAME_WIDTH, 0.0), |ui| {
            ui.horizontal(|ui| {
                bullet(ui, retired);
                ui.label(
                    egui::RichText::new(&axis.key)
                        .size(theme::size::HEADING)
                        .color(if retired {
                            theme::TEXT_MUTED
                        } else {
                            theme::TEXT
                        }),
                );
            });
        });

        ui.add_enabled_ui(!disabled, |ui| controls(ui, axis));
        values(ui, axis, retired);
    });
    ui.add_space(2.0);
}

/// The gold disc that marks a parameter, hollow when it is retired.
fn bullet(ui: &mut egui::Ui, retired: bool) {
    let size = theme::size::HEADING * 0.42;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size * 2.2, size), egui::Sense::hover());
    if retired {
        ui.painter().circle_stroke(
            rect.center(),
            size / 2.0,
            egui::Stroke::new(1.0, theme::TEXT_MUTED),
        );
    } else {
        ui.painter()
            .circle_filled(rect.center(), size / 2.0, theme::ACCENT);
    }
}

/// The fields a parameter is given its values through.
fn controls(ui: &mut egui::Ui, axis: &mut AxisEdit) {
    match axis.sampling {
        // A fixed vocabulary: the values are the choices.
        Sampling::Choice(_) => {
            ui.horizontal_wrapped(|ui| {
                for (name, picked) in &mut axis.choices {
                    ui.checkbox(picked, name.as_str());
                }
            });
        }

        // A whole number: the bounds give the candidates, and the menu is where
        // they are ticked — in the place a count would otherwise sit.
        Sampling::Integer => {
            let mut bounds_moved = false;
            bounds_moved |= ui
                .add(egui::TextEdit::singleline(&mut axis.from).desired_width(48.0))
                .changed();
            ui.label(egui::RichText::new("…").color(theme::TEXT_MUTED));
            bounds_moved |= ui
                .add(egui::TextEdit::singleline(&mut axis.to).desired_width(48.0))
                .changed();
            if bounds_moved {
                axis.refresh_candidates();
            }
            menu(ui, axis);
        }

        // A decimal: an interval, and how many points to take in it.
        Sampling::Decimal(_) => {
            let spans = axis.spans_an_interval();
            ui.add(egui::TextEdit::singleline(&mut axis.from).desired_width(58.0));

            // The second bound is only in play above one value. Greyed rather
            // than hidden: a field that vanishes takes the row's shape with it,
            // and the user is about to raise the count again.
            ui.add_enabled(
                spans,
                egui::TextEdit::singleline(&mut axis.to).desired_width(58.0),
            );
            ui.label(egui::RichText::new("×").color(theme::TEXT_MUTED));
            ui.add(egui::TextEdit::singleline(&mut axis.count).desired_width(32.0))
                .on_hover_text("How many values to take. One holds the parameter still.");

            // Offered only where it changes something: two points of an
            // interval are its ends however they are spaced, so a box that
            // could be ticked at two values would do nothing when clicked.
            let matters = axis.scale_matters();
            let mut log = axis.scale == Scale::Log;
            if ui
                .add_enabled(matters, egui::Checkbox::new(&mut log, "log"))
                .on_hover_text(if matters {
                    "Constant ratio rather than constant step.\n\
                     0.005 to 0.05 in four values is 0.005, 0.0108, 0.0232, 0.05 \
                     — a decade covered evenly.\nLinearly, three of the four sit \
                     in the top half."
                } else {
                    "Two values of an interval are its two ends however they are \
                     spaced.\nAsk for three or more and the scale starts to matter."
                })
                .changed()
            {
                axis.scale = if log { Scale::Log } else { Scale::Linear };
            }
        }
    }
}

/// The drop-down a whole number's values are ticked in.
///
/// Select all and Clear all first, as the column picker of the main interface
/// has them, and the menu stays open across clicks because ticking several is
/// the normal case.
fn menu(ui: &mut egui::Ui, axis: &mut AxisEdit) {
    let picked: Vec<String> = axis
        .choices
        .iter()
        .filter(|(_, picked)| *picked)
        .map(|(name, _)| name.clone())
        .collect();
    let caption = match picked.len() {
        0 => "— none —".to_string(),
        n if n <= 3 => picked.join(", "),
        n => format!("{n} values"),
    };

    // Stays open across clicks: ticking four cluster counts is four clicks, and
    // a menu that shut after each of them would be opened four times.
    panels::multi_select(
        ui,
        format!("menu_{}", axis.key),
        caption,
        Some(122.0),
        |ui| {
            if ui.button("Select all").clicked() {
                for (_, picked) in &mut axis.choices {
                    *picked = true;
                }
            }
            if ui.button("Clear all").clicked() {
                for (_, picked) in &mut axis.choices {
                    *picked = false;
                }
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for (name, picked) in &mut axis.choices {
                        ui.checkbox(picked, name.as_str());
                    }
                });
        },
    );
}

/// The values this row will run, framed in gold on the right.
///
/// Beside the controls rather than under them: it is the answer to what the
/// fields say, and reading it means looking along the row rather than down to
/// the next one.
fn values(ui: &mut egui::Ui, axis: &AxisEdit, retired: bool) {
    if retired {
        // Nothing reads it, so it runs nothing — saying "not used" is more
        // honest than showing values that will never reach a run.
        egui::Frame::NONE
            .stroke(egui::Stroke::new(1.0, theme::TEXT_MUTED))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(7, 3))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("not used")
                        .color(theme::TEXT_MUTED)
                        .size(theme::size::LABEL),
                );
            });
        return;
    }

    let (text, colour) = match axis.problem() {
        Some(problem) => (problem, theme::LOG_WARNING),
        None => {
            let points = axis.to_axis().points();
            let shown: Vec<String> = points.iter().take(10).map(|p| p.label()).collect();
            let ellipsis = if points.len() > 10 { ", …" } else { "" };
            (format!("{}{ellipsis}", shown.join(", ")), theme::TEXT)
        }
    };

    egui::Frame::NONE
        .stroke(egui::Stroke::new(1.0, theme::ACCENT))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .color(colour)
                    .size(theme::size::LABEL),
            );
        });
}

/// What the grid costs, and the controls that start and stop it.
fn summary(app: &mut MosnaApp, ui: &mut egui::Ui) {
    let plan = app.sweep_form.plan();
    ui.separator();

    if plan.is_empty() {
        for problem in app.sweep_form.problems() {
            ui.label(egui::RichText::new(problem).color(theme::TEXT_MUTED));
        }
    } else {
        ui.label(
            egui::RichText::new(format!("{} run(s)", plan.len()))
                .strong()
                .size(theme::size::PANEL_TITLE),
        );
        if plan.requested > plan.len() {
            ui.label(
                egui::RichText::new(format!(
                    "{} combination(s) asked for; the repeats were collapsed",
                    plan.requested
                ))
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
            );
        }
        // Where the time actually goes. The runs of a sweep do not cost the
        // same: only the first of each block recomputes its aggregation and
        // its projection, and the rest read both from the cache.
        ui.label(
            egui::RichText::new(format!(
                "{} aggregation(s) and {} projection(s) to compute — the other runs \
                 read theirs from the intermediate files",
                plan.aggregations(),
                plan.reductions()
            ))
            .color(theme::TEXT_MUTED)
            .size(theme::size::SMALL),
        );
        for problem in app.sweep_form.problems() {
            ui.label(egui::RichText::new(problem).color(theme::LOG_WARNING));
        }
    }

    ui.add_space(6.0);
    let running = app.sweep.as_ref().is_some_and(|sweep| sweep.is_running());
    ui.horizontal(|ui| {
        if !running {
            let ready = app.sweep_form.is_runnable() && app.working_dir().is_some();
            // Gold: it is the one thing on this screen that starts work, and
            // the accent is what the interface already uses to say so.
            if ui
                .add_enabled(
                    ready,
                    egui::Button::new(
                        egui::RichText::new("Start the sensitivity analysis")
                            .color(theme::TEXT_INVERSE)
                            .size(theme::size::HEADING)
                            .strong(),
                    )
                    .fill(theme::ACCENT)
                    .corner_radius(6.0)
                    // The width of the column and a height a thumb could find:
                    // it is the one control on this screen that starts work.
                    .min_size(egui::vec2(ui.available_width(), 44.0)),
                )
                .on_disabled_hover_text(if app.working_dir().is_none() {
                    "Choose a working directory first"
                } else {
                    "Choose a clusterer and at least one parameter to vary"
                })
                .clicked()
            {
                app.start_sweep();
            }
        } else {
            let stopping = app.sweep.as_ref().is_some_and(|sweep| sweep.stopping);
            if ui
                .add_enabled(
                    !stopping,
                    egui::Button::new(
                        egui::RichText::new("Stop")
                            .color(theme::TEXT_INVERSE)
                            .size(theme::size::LABEL)
                            .strong(),
                    )
                    .fill(theme::LOG_WARNING)
                    .min_size(egui::vec2(120.0, 30.0)),
                )
                .on_hover_text(
                    "Ends the sweep and the run it has in flight.\nThe runs already \
                     finished keep their results; the one cut short leaves none.",
                )
                .clicked()
            {
                app.stop_sweep();
            }
            if stopping {
                ui.label(egui::RichText::new("stopping…").color(theme::TEXT_MUTED));
            }
        }
    });
}

/// How far the sweep has got, across the whole window.
///
/// Above the two columns rather than inside the right-hand one: it is the thing
/// a user glances at while a grid runs, and a bar half a screen wide is a bar
/// whose filled part is hard to read against its empty part.
fn progress(app: &mut MosnaApp, ui: &mut egui::Ui) {
    let Some(sweep) = &app.sweep else { return };

    ui.add(
        egui::ProgressBar::new(sweep.fraction())
            .text(
                egui::RichText::new(format!("{}%", sweep.percent()))
                    .size(theme::size::LABEL)
                    .strong(),
            )
            .desired_width(ui.available_width())
            .desired_height(26.0),
    );
    ui.label(
        egui::RichText::new(sweep.caption()).color(if sweep.failure.is_some() {
            theme::LOG_WARNING
        } else {
            theme::TEXT_MUTED
        }),
    );

    // A grid usually fails for one reason and fails that way from end to end,
    // so the reason is worth more than the count.
    if let Some(reason) = &sweep.failure {
        ui.add_space(4.0);
        egui::Frame::NONE
            .stroke(egui::Stroke::new(1.0, theme::LOG_WARNING))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!("The first run failed: {reason}"))
                        .color(theme::LOG_WARNING)
                        .size(theme::size::LABEL),
                );
            });
    }
    ui.add_space(8.0);
    ui.separator();
}

/// The right-hand half: how far the sweep has got, and what it found.
fn results(app: &mut MosnaApp, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        panels::header(ui, "Results");

        let Some(sweep) = &app.sweep else {
            ui.label(
                egui::RichText::new(
                    "Nothing has run yet.\n\nEvery point of the grid is an ordinary niche \
                     analysis: it writes its labels into the nodes files under the usual \
                     niches_1-2-4 name and reads back whatever the intermediate files \
                     already hold.\n\nIts figures go to Niche_Analysis/sensitivity_analysis/, \
                     so a sweep does not bury the runs you started by hand.",
                )
                .color(theme::TEXT_MUTED),
            );
            return;
        };

        if !sweep.is_running() && !sweep.rows.is_empty() {
            ui.label(
                egui::RichText::new(format!(
                    "The runs and the comparison table are in Niche_Analysis/{}/",
                    crate::app::SENSITIVITY_DIR
                ))
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
            );
            ui.add_space(4.0);
        }

        table(app, ui);
        ui.add_space(10.0);
        comparison(app, ui);
    });
}

/// The three figures a finished sweep is read through, and how to read them.
///
/// Below the table because they answer a question the table cannot: the table
/// says how many niches each run found, and these say whether two runs that
/// found the same number found the *same niches*.
fn comparison(app: &MosnaApp, ui: &mut egui::Ui) {
    let Some(sweep) = &app.sweep else { return };

    if app.comparing {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(egui::RichText::new("Comparing the runs…").color(theme::TEXT_MUTED));
        });
        return;
    }
    if sweep.is_running() {
        return;
    }

    let Some(directory) = app
        .working_dir()
        .map(|dir| dir.join("Niche_Analysis").join(crate::app::SENSITIVITY_DIR))
    else {
        return;
    };

    // The three, in the order they are read: where the answer moves, which
    // runs agree, and what became of the niches.
    let figures = [
        ("Sensitivity_Agreement", "Agreement along the grid"),
        (
            "Sensitivity_Agreement_Matrix",
            "Every run against every other",
        ),
        ("Sensitivity_Niche_Stability", "What became of each niche"),
    ];
    let drawn: Vec<(&str, &str, std::path::PathBuf)> = figures
        .into_iter()
        .filter_map(|(stem, caption)| {
            let png = directory.join(format!("{stem}.png"));
            png.is_file().then_some((stem, caption, png))
        })
        .collect();

    if drawn.is_empty() {
        if sweep.total() >= 2 {
            ui.label(
                egui::RichText::new(
                    "No comparison figures: the renderer is not installed, or the runs' \
                     label columns are no longer in the network files.",
                )
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
            );
        }
        return;
    }

    panels::header(ui, "How the runs compare");
    for (stem, caption, png) in &drawn {
        ui.label(
            egui::RichText::new(*caption)
                .color(theme::TEXT)
                .size(theme::size::HEADING),
        );
        ui.add(
            egui::Image::new(format!("file://{}", png.display()))
                .max_width(ui.available_width())
                .corner_radius(4.0),
        );

        // The interactive copy, which `xy` wrote beside the PNG: hovering a
        // cell of a matrix of fifty runs is the only way to read it.
        let html = directory.join(format!("{stem}.html"));
        if html.is_file()
            && ui
                .link(
                    egui::RichText::new("open the interactive version")
                        .color(theme::ACCENT)
                        .size(theme::size::SMALL),
                )
                .on_hover_text(html.display().to_string())
                .clicked()
        {
            let _ = open_in_browser(&html);
        }
        ui.add_space(10.0);
    }

    note(ui);
}

/// Open a file with whatever the desktop uses for it.
fn open_in_browser(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";

    std::process::Command::new(opener)
        .arg(path)
        .spawn()
        .map(|_| ())
}

/// What the three numbers mean, beside the figures that show them.
///
/// # Why this is on the screen and not in the manual
///
/// A reader meets these three the moment a sweep ends, and the one thing that
/// would mislead them — that an adjusted index near zero on a cohort cut into
/// hundreds of niches is normal rather than alarming — is exactly what a
/// manual they have not opened cannot tell them.
fn note(ui: &mut egui::Ui) {
    egui::Frame::NONE
        .stroke(egui::Stroke::new(1.0, theme::ACCENT))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Reading these three")
                    .color(theme::ACCENT)
                    .size(theme::size::HEADING)
                    .strong(),
            );
            ui.add_space(4.0);

            for (name, text) in [
                (
                    "ARI",
                    "Adjusted Rand index. Counts pairs of cells: for every pair, do the two \
                     runs agree about whether they belong together? 1 is the same partition \
                     — niche numbers need not match, only which cells fall together — and 0 \
                     is what two unrelated partitions score. It can go negative when two \
                     runs agree less than chance.",
                ),
                (
                    "AMI",
                    "Adjusted mutual information. How much knowing one labelling tells you \
                     about the other, corrected the same way. Read it beside the ARI rather \
                     than instead of it: the ARI is dominated by the large niches and this \
                     is not, so the two parting company means the niches are very uneven.",
                ),
                (
                    "Jaccard",
                    "Per niche, not per run: of the cells in this niche and the cells in the \
                     best-matching niche of the other run, what share is in both. 1 is a \
                     niche found again intact, 0.5 a niche split down the middle or merged \
                     into one twice its size. A row that stays bright across the sweep is a \
                     feature of the tissue; one that darkens is a feature of the settings.",
                ),
            ] {
                ui.horizontal_top(|ui| {
                    ui.allocate_ui(egui::vec2(62.0, 0.0), |ui| {
                        ui.label(
                            egui::RichText::new(name)
                                .color(theme::ACCENT)
                                .size(theme::size::LABEL)
                                .strong(),
                        );
                    });
                    ui.label(
                        egui::RichText::new(text)
                            .color(theme::TEXT)
                            .size(theme::size::SMALL),
                    );
                });
                ui.add_space(6.0);
            }

            ui.separator();
            ui.label(
                egui::RichText::new(
                    "On a cohort of tens of thousands of cells cut into hundreds of niches, \
                     both adjusted indices are crushed towards zero — there are so many \
                     pairs in different niches that agreeing about them earns almost \
                     nothing. Compare them with each other, not against 1.",
                )
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
            );
        });
}

/// How many runs the table shows before it starts scrolling.
const VISIBLE_ROWS: usize = 10;

/// One line per run: what was asked for, and what came back.
fn table(app: &MosnaApp, ui: &mut egui::Ui) {
    let Some(sweep) = &app.sweep else { return };
    let columns = app.sweep_form.plan().columns();

    // The height is asked for in rows rather than taken from what is left of
    // the screen: the table is drawn inside a column allocated with no height
    // of its own, so `available_height` there is whatever the outer scroll
    // area happens to have left — often one row's worth. Ten rows and the
    // header, and the scroll bar carries the rest.
    let row = theme::size::SMALL * 1.4 + 4.0;
    let shown = sweep.rows.len().clamp(1, VISIBLE_ROWS) + 1;
    let height = row * shown as f32 + ui.spacing().scroll.bar_width + 6.0;

    egui::ScrollArea::both()
        // Horizontally it fills the column; vertically it grows with the runs
        // until it hits the ten-row ceiling, so a sweep of three does not
        // leave seven empty lines below it.
        .auto_shrink([false, true])
        .max_height(height)
        .show(ui, |ui| {
            egui::Grid::new("sensitivity_table")
                .striped(true)
                .spacing(egui::vec2(10.0, 4.0))
                .show(ui, |ui| {
                    let head = |ui: &mut egui::Ui, text: &str| {
                        ui.label(
                            egui::RichText::new(text)
                                .color(theme::ACCENT)
                                .size(theme::size::SMALL)
                                .strong(),
                        );
                    };
                    head(ui, "run");
                    for column in &columns {
                        head(ui, column);
                    }
                    head(ui, "niches");
                    head(ui, "largest");
                    head(ui, "floor");
                    ui.end_row();

                    for row in &sweep.rows {
                        let colour = match row.state {
                            sweep::RunState::Done => theme::TEXT,
                            sweep::RunState::Running => theme::ACCENT,
                            sweep::RunState::Failed => theme::LOG_WARNING,
                            sweep::RunState::Pending => theme::TEXT_MUTED,
                        };
                        let cell = |ui: &mut egui::Ui, text: String| {
                            ui.label(
                                egui::RichText::new(text)
                                    .color(colour)
                                    .size(theme::size::SMALL),
                            );
                        };

                        cell(
                            ui,
                            row.run
                                .clone()
                                .unwrap_or_else(|| row.state.as_str().to_string()),
                        );
                        for column in &columns {
                            cell(
                                ui,
                                row.combination
                                    .get(column)
                                    .map(|p| p.label())
                                    .unwrap_or_default(),
                            );
                        }
                        cell(ui, row.niches.map(|n| n.to_string()).unwrap_or_default());
                        cell(ui, row.largest.map(|n| n.to_string()).unwrap_or_default());
                        // The floor: leiden cannot find fewer niches than its
                        // graph has connected components, whatever the
                        // resolution. A column that equals `niches` says the
                        // sweep is saturated and the lever is elsewhere.
                        cell(
                            ui,
                            row.graph_components
                                .map(|n| n.to_string())
                                .unwrap_or_default(),
                        );
                        ui.end_row();
                    }
                });
        });
}
