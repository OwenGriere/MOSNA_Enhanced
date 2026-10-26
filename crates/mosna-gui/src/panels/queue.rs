//! The Queue tab: the analyses waiting their turn, the one running, and the
//! ones that have ended.
//!
//! The arithmetic of the queue is in [`crate::model::queue`]; what is here is
//! one row per job — its name, its bar, and the red cross that takes it out of
//! the queue or stops it where it is.

use crate::app::MosnaApp;
use crate::model::queue::{Job, JobState};
use crate::theme;

/// Width of the red cross beside each bar.
const CROSS_WIDTH: f32 = 28.0;
/// Height of a job's bar.
const BAR_HEIGHT: f32 = 18.0;

/// Draw the tab.
pub fn show(app: &mut MosnaApp, ui: &mut egui::Ui) {
    let waiting = app.queue.waiting();
    let running = app
        .queue
        .jobs
        .iter()
        .any(|job| matches!(job.state, JobState::Running | JobState::Cancelling));
    let finished = app
        .queue
        .jobs
        .iter()
        .filter(|job| job.state.is_finished())
        .count();

    ui.horizontal(|ui| {
        let summary = match (running, waiting) {
            (false, 0) => "Nothing queued.".to_string(),
            (true, 0) => "1 running.".to_string(),
            (true, n) => format!("1 running, {n} waiting."),
            (false, n) => format!("{n} waiting."),
        };
        ui.label(egui::RichText::new(summary).color(theme::TEXT_MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(finished > 0, |ui| {
                if ui
                    .button("Clear finished")
                    .on_hover_text("Forget the analyses that have ended")
                    .clicked()
                {
                    app.queue.clear_finished();
                }
            });
        });
    });
    ui.add_space(4.0);

    if app.queue.jobs.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(
                    "Press an analysis button in the Parameters panel to add it here.\n\
                     Analyses run one after the other, in the order they were added.",
                )
                .color(theme::TEXT_MUTED),
            );
        });
        return;
    }

    let mut cancelled: Option<u64> = None;
    egui::ScrollArea::vertical()
        .id_salt("queue")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (position, job) in app.queue.jobs.iter().enumerate() {
                if row(ui, position + 1, job) {
                    cancelled = Some(job.id);
                }
                ui.add_space(6.0);
            }
        });

    if let Some(id) = cancelled {
        app.cancel_job(id);
    }
}

/// One job. Returns `true` when its cross was pressed.
fn row(ui: &mut egui::Ui, position: usize, job: &Job) -> bool {
    let mut pressed = false;
    egui::Frame::new()
        .fill(theme::SURFACE)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{position}."))
                        .color(theme::TEXT_MUTED)
                        .size(theme::size::LABEL),
                );
                let mut name = egui::RichText::new(job.step.label())
                    .color(theme::step_colour(job.step))
                    .size(theme::size::LABEL)
                    .strong();
                if job.state == JobState::Cancelled {
                    name = name.strikethrough();
                }
                ui.label(name);
                if job.without_figures {
                    ui.label(
                        egui::RichText::new("without figures")
                            .color(theme::TEXT_MUTED)
                            .size(theme::size::SMALL)
                            .italics(),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(state_caption(job))
                            .color(state_colour(job))
                            .size(theme::size::SMALL),
                    );
                });
            });

            ui.horizontal(|ui| {
                let spacing = ui.spacing().item_spacing.x;
                let width = (ui.available_width() - CROSS_WIDTH - spacing).max(40.0);
                let mut bar = egui::ProgressBar::new(job.fraction().unwrap_or(0.0))
                    .desired_width(width)
                    .desired_height(BAR_HEIGHT)
                    .fill(state_colour(job))
                    .corner_radius(egui::CornerRadius::same(4));
                if let Some((current, total)) = job.progress.filter(|(_, total)| *total > 0) {
                    if !job.state.is_finished() {
                        bar = bar.text(format!("{current} / {total}"));
                    }
                }
                // Running, but no count yet: the bar says "working" without
                // claiming a position it does not know.
                if job.state == JobState::Running && job.fraction().is_none() {
                    bar = bar.animate(true);
                }
                ui.add(bar);

                let cross =
                    egui::Button::new(egui::RichText::new("✕").color(theme::TEXT_INVERSE).strong())
                        .fill(theme::STEP_FAILED)
                        .corner_radius(egui::CornerRadius::same(4));
                let hint = match job.state {
                    JobState::Running => "Cancel this analysis",
                    JobState::Cancelling => "Cancelling…",
                    JobState::Waiting => "Remove from the queue",
                    _ => "Remove from the list",
                };
                let enabled = job.state != JobState::Cancelling;
                if ui.add_enabled(enabled, cross).on_hover_text(hint).clicked() {
                    pressed = true;
                }
            });

            let detail = match &job.state {
                JobState::Failed(reason) => Some((reason.as_str(), theme::LOG_ERROR)),
                JobState::Running | JobState::Cancelling if !job.message.is_empty() => {
                    Some((job.message.as_str(), theme::TEXT_MUTED))
                }
                _ => None,
            };
            if let Some((text, colour)) = detail {
                ui.label(
                    egui::RichText::new(text)
                        .color(colour)
                        .size(theme::size::SMALL),
                );
            }
        });
    pressed
}

/// The state, with how long the job took once it has ended.
fn state_caption(job: &Job) -> String {
    match job.elapsed {
        Some(seconds) if job.state.is_finished() => format!(
            "{} · {}",
            job.state.caption(),
            crate::model::runner::format_duration(seconds)
        ),
        _ => job.state.caption().to_string(),
    }
}

fn state_colour(job: &Job) -> egui::Color32 {
    match job.state {
        JobState::Failed(_) => theme::STEP_FAILED,
        JobState::Cancelled | JobState::Cancelling | JobState::Waiting => theme::TEXT_MUTED,
        JobState::Running | JobState::Done => theme::step_colour(job.step),
    }
}
