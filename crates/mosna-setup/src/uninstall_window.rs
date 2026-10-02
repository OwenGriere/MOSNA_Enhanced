//! The uninstaller's window: what to remove, then the progress, then the
//! outcome.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use crate::install::Event;
use crate::place;
use crate::theme::{self, Backdrop};
use crate::uninstall::{self, Choices, Found};
use crate::window::{self, Journal};

const TITLE: &str = "Désinstallation de MOSNA Enhanced";

/// Open the window. `source` is the MOSNA folder the program was found in.
pub fn show(source: Option<PathBuf>) -> eframe::Result<()> {
    eframe::run_native(
        TITLE,
        window::options(TITLE),
        Box::new(|creation| {
            theme::apply(&creation.egui_ctx);
            Ok(Box::new(Uninstall::new(source, &creation.egui_ctx)))
        }),
    )
}

enum Stage {
    Choosing,
    Running(Receiver<Event>),
    Finished(Result<PathBuf, String>),
}

struct Uninstall {
    root: Option<PathBuf>,
    remove_folder: bool,
    /// Each prerequisite found, and whether it is ticked.
    prerequisites: Vec<(Found, bool)>,
    stage: Stage,
    journal: Journal,
    backdrop: Backdrop,
}

impl Uninstall {
    fn new(source: Option<PathBuf>, ctx: &egui::Context) -> Self {
        let root = source.filter(|s| place::is_mosna(s));
        // Ticked when the installer is known to have installed it; one that
        // was there before may serve other software.
        let prerequisites = uninstall::found(root.as_deref())
            .into_iter()
            .map(|found| (found, found.installed_by_mosna))
            .collect();
        Self {
            root,
            remove_folder: false,
            prerequisites,
            stage: Stage::Choosing,
            // The colours are for the build, which only the installer runs.
            journal: Journal::new(false),
            backdrop: Backdrop::new(ctx),
        }
    }

    fn start(&mut self) {
        let choices = Choices {
            root: self.root.clone(),
            remove_folder: self.remove_folder,
            prerequisites: self
                .prerequisites
                .iter()
                .filter(|(_, ticked)| *ticked)
                .map(|(found, _)| found.prerequisite)
                .collect(),
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || uninstall::run(choices, sender));
        self.stage = Stage::Running(receiver);
    }

    fn poll(&mut self) {
        let Stage::Running(receiver) = &self.stage else {
            return;
        };
        if let Some(outcome) = self.journal.poll(receiver) {
            self.stage = Stage::Finished(outcome);
        }
    }

    fn choosing(&mut self, ui: &mut egui::Ui) {
        ui.label(
            "MOSNA, ses raccourcis (bureau, menu Démarrer) et son module de figures seront \
             supprimés. Votre configuration est conservée.",
        );
        ui.add_space(8.0);
        if let Some(root) = &self.root {
            ui.checkbox(
                &mut self.remove_folder,
                format!("Supprimer aussi le dossier {}", root.display()),
            );
            if self.remove_folder {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "Tout ce qu'il contient sera supprimé, y compris les résultats que vous y \
                     auriez rangés.",
                );
            } else {
                ui.weak("Sa compilation (le dossier target, plusieurs Go) est supprimée dans tous les cas.");
            }
            ui.add_space(8.0);
        }

        if self.prerequisites.is_empty() {
            ui.weak("Aucun des outils que l'installation ajoute (outils C++, Rust, Python) n'est présent.");
        } else {
            ui.label("Désinstaller aussi les outils qui ont servi à compiler MOSNA :");
            for (found, ticked) in &mut self.prerequisites {
                let label = if found.installed_by_mosna {
                    format!("{} — installé par MOSNA", found.prerequisite.label())
                } else {
                    format!(
                        "{} — déjà présent, ou installé par une version précédente de \
                         l'installation ; d'autres logiciels peuvent s'en servir",
                        found.prerequisite.label()
                    )
                };
                ui.checkbox(ticked, label);
            }
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
            ui.horizontal(|ui| {
                if ui.button("Annuler").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if theme::primary_button(ui, "Désinstaller").clicked() {
                    self.start();
                }
            });
        });
    }

    fn progress(&mut self, ui: &mut egui::Ui) {
        let finished = match &self.stage {
            Stage::Finished(outcome) => Some(outcome.clone()),
            _ => None,
        };
        match &finished {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.strong(&self.journal.step);
                });
            }
            Some(Ok(_)) => {
                ui.strong("MOSNA est désinstallé.");
            }
            Some(Err(message)) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Tout n'a pas pu être supprimé :\n{message}"),
                );
                ui.label(format!(
                    "Le journal complet est dans {}.",
                    uninstall::log_file().display()
                ));
            }
        }
        ui.add_space(6.0);
        self.journal.show(ui);

        if finished.is_some() {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                if ui.button("Fermer").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        }
    }
}

impl eframe::App for Uninstall {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        if matches!(self.stage, Stage::Running(_)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            // Closing the window would not stop the uninstall, only hide it.
            if ctx.input(|input| input.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        egui::CentralPanel::default().show(ui, |ui| {
            self.backdrop.paint(ui);
            theme::heading(ui, "Désinstaller MOSNA Enhanced");
            ui.add_space(10.0);
            match self.stage {
                Stage::Choosing => self.choosing(ui),
                Stage::Running(_) | Stage::Finished(_) => self.progress(ui),
            }
        });
    }
}
