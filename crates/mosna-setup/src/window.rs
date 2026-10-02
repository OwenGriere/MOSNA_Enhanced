//! The installer's window: the choices, then the progress, then the outcome.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use crate::console;
use crate::install::{self, Choices, Event};
use crate::place::{self, Placement};
use crate::theme::{self, Backdrop};

const TITLE: &str = "Installation de MOSNA Enhanced";

/// Open the window. `source` is the MOSNA folder to install from, if one was
/// found beside the program.
pub fn show(source: Option<PathBuf>) -> eframe::Result<()> {
    eframe::run_native(
        TITLE,
        options(TITLE),
        Box::new(|creation| {
            theme::apply(&creation.egui_ctx);
            Ok(Box::new(Setup::new(source, &creation.egui_ctx)))
        }),
    )
}

/// The window both programs open, with MOSNA's icon.
pub(crate) fn options(title: &str) -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size([720.0, 480.0])
        .with_min_inner_size([560.0, 400.0]);
    if let Some(icon) = theme::icon() {
        viewport = viewport.with_icon(icon);
    }
    eframe::NativeOptions {
        viewport,
        ..Default::default()
    }
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
    journal: Journal,
    backdrop: Backdrop,
}

/// What the worker has reported so far.
pub(crate) struct Journal {
    pub step: String,
    /// Each line, laid out once as it arrives.
    log: Vec<egui::text::LayoutJob>,
    /// Whether the console shows colours, or every line alike.
    colored: bool,
}

impl Journal {
    pub fn new(colored: bool) -> Self {
        Self {
            step: String::new(),
            log: Vec::new(),
            colored,
        }
    }

    fn push(&mut self, line: &str) {
        self.log.push(console::layout(line, self.colored));
    }

    /// Take in what the worker sent; its outcome, once it has finished.
    pub fn poll(&mut self, receiver: &Receiver<Event>) -> Option<Result<PathBuf, String>> {
        let mut finished = None;
        for event in receiver.try_iter() {
            match event {
                Event::Step(text) => {
                    self.push("");
                    self.push(&format!("==> {text}"));
                    self.step = text;
                }
                Event::Line(text) => self.push(&text),
                Event::Done(root) => finished = Some(Ok(root)),
                Event::Failed(message) => {
                    self.push("");
                    for line in format!("ÉCHEC : {message}").lines() {
                        self.push(line);
                    }
                    finished = Some(Err(message));
                }
            }
        }
        finished
    }

    /// The console, scrolled to its end, leaving room for a row of buttons.
    ///
    /// Lines are not wrapped — compiler output is read by its columns — and
    /// only those in view are drawn: a first build prints hundreds.
    pub fn show(&self, ui: &mut egui::Ui) {
        let log_height = ui.available_height() - 48.0;
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        egui::Frame::group(ui.style())
            .fill(theme::CONSOLE)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                egui::ScrollArea::both()
                    .max_height(log_height)
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show_rows(ui, row_height, self.log.len(), |ui, rows| {
                        for line in &self.log[rows] {
                            ui.add(
                                egui::Label::new(line.clone())
                                    .wrap_mode(egui::TextWrapMode::Extend),
                            );
                        }
                    });
            });
    }
}

impl Setup {
    fn new(source: Option<PathBuf>, ctx: &egui::Context) -> Self {
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
            journal: Journal::new(true),
            backdrop: Backdrop::new(ctx),
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
        if let Some(outcome) = self.journal.poll(receiver) {
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
                if theme::primary_button(ui, label).clicked() {
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
                    ui.strong(&self.journal.step);
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
        self.journal.show(ui);

        if let Some(outcome) = finished {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Fermer").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if outcome.is_ok() && theme::primary_button(ui, "Lancer MOSNA").clicked() {
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
            self.backdrop.paint(ui);
            theme::heading(ui, "Installer MOSNA Enhanced");
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
