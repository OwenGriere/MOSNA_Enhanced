//! The installer's window: the choices, then the progress, then the outcome.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use crate::install::{self, Choices, Event};
use crate::place::{self, Placement};

const TITLE: &str = "Installation de MOSNA Enhanced";

/// Open the window. `source` is the MOSNA folder to install from, if one was
/// found beside the program.
pub fn show(source: Option<PathBuf>) -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(TITLE)
        .with_inner_size([640.0, 420.0])
        .with_min_inner_size([520.0, 360.0]);
    if let Some(icon) = source.as_deref().and_then(window_icon) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        options,
        Box::new(|_| Ok(Box::new(Setup::new(source)))),
    )
}

fn window_icon(source: &Path) -> Option<egui::IconData> {
    let image = image::open(source.join("assets").join("logo.ico"))
        .ok()?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

enum Stage {
    /// No MOSNA folder beside the program: nothing to install from.
    Lost,
    Choosing,
    Running(Receiver<Event>),
    Finished(Result<PathBuf, String>),
}

struct Setup {
    source: PathBuf,
    destination: String,
    desktop_shortcut: bool,
    figures: bool,
    /// Why the destination was refused, shown under it.
    problem: Option<String>,
    /// Set once an existing copy is found at the destination, until the user
    /// agrees to update it.
    confirm_update: bool,
    stage: Stage,
    step: String,
    log: Vec<String>,
}

impl Setup {
    fn new(source: Option<PathBuf>) -> Self {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_default();
        let (source, stage) = match source.filter(|s| place::is_mosna(s)) {
            Some(source) => (source, Stage::Choosing),
            None => (PathBuf::new(), Stage::Lost),
        };
        Self {
            destination: place::default_destination(&source, &home)
                .display()
                .to_string(),
            source,
            desktop_shortcut: true,
            figures: true,
            problem: None,
            confirm_update: false,
            stage,
            step: String::new(),
            log: Vec::new(),
        }
    }

    fn start(&mut self, placement: Placement) {
        let choices = Choices {
            source: self.source.clone(),
            destination: PathBuf::from(self.destination.trim()),
            placement,
            desktop_shortcut: self.desktop_shortcut,
            figures: self.figures,
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || install::run(choices, sender));
        self.stage = Stage::Running(receiver);
    }

    fn poll(&mut self) {
        let Stage::Running(receiver) = &self.stage else {
            return;
        };
        let mut finished = None;
        for event in receiver.try_iter() {
            match event {
                Event::Step(text) => {
                    self.log.push(String::new());
                    self.log.push(format!("==> {text}"));
                    self.step = text;
                }
                Event::Line(text) => self.log.push(text),
                Event::Done(root) => finished = Some(Ok(root)),
                Event::Failed(message) => {
                    self.log.push(String::new());
                    self.log.push(format!("ÉCHEC : {message}"));
                    finished = Some(Err(message));
                }
            }
        }
        if let Some(outcome) = finished {
            self.stage = Stage::Finished(outcome);
        }
    }

    fn choosing(&mut self, ui: &mut egui::Ui) {
        ui.label("Dossier où placer MOSNA :");
        ui.horizontal(|ui| {
            let field = egui::TextEdit::singleline(&mut self.destination)
                .desired_width(ui.available_width() - 110.0);
            if ui.add(field).changed() {
                self.problem = None;
                self.confirm_update = false;
            }
            if ui.button("Parcourir…").clicked() {
                let start = Path::new(self.destination.trim())
                    .parent()
                    .map(Path::to_path_buf);
                let mut dialog = rfd::FileDialog::new()
                    .set_title("Choisissez le dossier dans lequel placer MOSNA");
                if let Some(start) = start.filter(|s| s.is_dir()) {
                    dialog = dialog.set_directory(start);
                }
                if let Some(chosen) = dialog.pick_folder() {
                    self.destination = place::destination_in(&chosen, &self.source)
                        .display()
                        .to_string();
                    self.problem = None;
                    self.confirm_update = false;
                }
            }
        });
        if let Some(problem) = &self.problem {
            ui.colored_label(ui.visuals().error_fg_color, problem);
        }
        ui.add_space(10.0);
        ui.checkbox(
            &mut self.desktop_shortcut,
            "Créer un raccourci sur le bureau",
        );
        ui.checkbox(
            &mut self.figures,
            "Installer le module de figures (Python, environ 85 Mo)",
        );
        ui.add_space(10.0);
        ui.weak(
            "Ce qui manque (outils C++ de Microsoft, Rust, Python) est installé automatiquement. \
             La première installation peut prendre 20 à 40 minutes, et Windows demandera une \
             autorisation administrateur pour les outils C++.",
        );

        if self.confirm_update {
            ui.add_space(10.0);
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Ce dossier contient déjà une copie de MOSNA. Elle sera mise à jour : vos résultats \
                 et votre configuration sont conservés, seuls les fichiers de MOSNA sont remplacés.",
            );
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
            ui.horizontal(|ui| {
                if ui.button("Annuler").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let label = if self.confirm_update {
                    "Mettre à jour et installer"
                } else {
                    "Installer"
                };
                if ui.button(label).clicked() {
                    let destination = PathBuf::from(self.destination.trim());
                    match place::check(&self.source, &destination) {
                        Err(problem) => self.problem = Some(problem),
                        Ok(Placement::Update) if !self.confirm_update => self.confirm_update = true,
                        Ok(placement) => self.start(placement),
                    }
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
                    ui.strong(&self.step);
                });
            }
            Some(Ok(_)) => {
                ui.strong("MOSNA est installé.");
                let place = if self.desktop_shortcut {
                    "le raccourci du bureau ou le menu Démarrer"
                } else {
                    "le menu Démarrer"
                };
                ui.label(format!("Vous pouvez le lancer depuis {place}."));
            }
            Some(Err(message)) => {
                ui.colored_label(ui.visuals().error_fg_color, format!("Échec : {message}"));
                ui.label(format!(
                    "Le journal complet est dans {}.",
                    install::log_file().display()
                ));
            }
        }
        ui.add_space(6.0);

        let log_height = ui.available_height() - 40.0;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(log_height)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log {
                        ui.monospace(line);
                    }
                });
        });

        if let Some(outcome) = finished {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Fermer").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if outcome.is_ok() && ui.button("Lancer MOSNA").clicked() {
                        if let Some(gui) = installed_interface() {
                            let _ = std::process::Command::new(gui).spawn();
                        }
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        }
    }
}

/// Where `mosna-install` puts the interface by default on Windows.
fn installed_interface() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let gui = PathBuf::from(local).join(r"Programs\MOSNA\bin\mosna-gui.exe");
    gui.is_file().then_some(gui)
}

impl eframe::App for Setup {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        let running = matches!(self.stage, Stage::Running(_));
        if running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            // Closing the window would not stop an install already running,
            // only hide it.
            if ctx.input(|input| input.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Installer MOSNA Enhanced");
            ui.add_space(10.0);
            match self.stage {
                Stage::Lost => {
                    ui.label(
                        "INSTALLATION.exe doit rester dans le dossier de MOSNA, à côté de Cargo.toml.\n\n\
                         Si vous avez ouvert le ZIP sans l'extraire, faites d'abord clic droit sur le ZIP \
                         → « Extraire tout », puis relancez INSTALLATION.exe depuis le dossier extrait.",
                    );
                }
                Stage::Choosing => self.choosing(ui),
                Stage::Running(_) | Stage::Finished(_) => self.progress(ui),
            }
        });
    }
}
