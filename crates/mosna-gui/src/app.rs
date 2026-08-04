//! The application state and frame loop — port of `MosnaGUI`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Instant;

use mosna_config::RawConfig;

use crate::docs::state::ManualState;
use crate::docs::Documentation;
use crate::model::browser::{BrowserState, SampleRow};
use crate::model::form::Form;
use crate::model::log::{classify, LogKind};
use crate::model::runner::{format_duration, parse_output_line, OutputLine, Step};
use crate::model::viewer::{collect_analysis_images, AnalysisImageSet};
use crate::panels;
use crate::theme;

/// Which page of the viewer is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerTab {
    Images,
    /// The network itself, drawn from the files rather than from a figure.
    Network,
    Log,
    Documentation,
}

/// Which analysis's figures are showing.
///
/// Step 1 has no entry: its only figure was a picture of the network, and the
/// Network tab draws the network itself — from the same files, at any zoom,
/// with the attributes still attached. A PNG of it is a worse copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisTab {
    Assortativity,
    Niches,
}

/// A running analysis.
pub struct Run {
    pub step: Step,
    pub child: std::process::Child,
    pub output: Receiver<String>,
    pub started: Instant,
}

/// The whole interface.
pub struct MosnaApp {
    pub config_path: PathBuf,
    pub config: RawConfig,
    pub form: Form,
    pub browser: BrowserState,

    pub rows: Vec<SampleRow>,
    pub selected_row: Option<usize>,

    pub images: AnalysisImageSet,
    /// The interactive network's state: which sample is drawn, what it is
    /// coloured by, where the camera is.
    pub network: crate::panels::network::NetworkState,
    pub viewer_tab: ViewerTab,
    pub analysis_tab: AnalysisTab,
    pub selected_patient: Option<String>,
    pub selected_image: usize,

    pub log: Vec<(LogKind, String)>,
    pub status: String,
    /// `(current, total)`, or `None` for an indeterminate bar.
    pub progress: Option<(usize, usize)>,
    pub active_step: Option<Step>,
    pub last_run_failed: bool,
    pub run: Option<Run>,

    /// The manual, and where the reader is in it.
    pub documentation: Documentation,
    pub manual: ManualState,

    /// Shown as a modal until a working directory is chosen.
    pub needs_working_dir: bool,
    /// A message the user must acknowledge.
    pub notice: Option<String>,
}

impl MosnaApp {
    /// Load the configuration and build the interface.
    pub fn new(config_path: PathBuf) -> Self {
        let (config, notice) = match mosna_config::get_config(&config_path) {
            Ok(config) => (config, None),
            Err(error) => (
                RawConfig::default(),
                Some(format!("Failed to load config:\n{error}")),
            ),
        };

        let form = Form::from_config(&config);
        let browser = BrowserState::from_config(&config);

        Self {
            config_path,
            config,
            form,
            browser,
            rows: Vec::new(),
            selected_row: None,
            images: AnalysisImageSet::default(),
            network: Default::default(),
            viewer_tab: ViewerTab::Images,
            analysis_tab: AnalysisTab::Assortativity,
            selected_patient: None,
            selected_image: 0,
            log: Vec::new(),
            status: "Ready.".to_string(),
            progress: None,
            active_step: None,
            last_run_failed: false,
            run: None,
            documentation: Documentation::build(),
            manual: ManualState::default(),
            needs_working_dir: true,
            notice,
        }
    }

    /// The chosen working directory, if any.
    pub fn working_dir(&self) -> Option<&Path> {
        self.browser.working_dir.as_deref()
    }

    /// Adopt a working directory and refresh everything that depends on it.
    pub fn set_working_dir(&mut self, directory: PathBuf) {
        self.browser.working_dir = Some(directory);
        self.needs_working_dir = false;
        // The network on screen belongs to the directory being left, down to
        // its camera and its colouring; keeping any of it would show one
        // dataset labelled as another.
        self.network.clear();
        self.refresh_images();
        self.refresh_nodes();
    }

    /// Re-scan the nodes directory and fill the sample table.
    pub fn refresh_nodes(&mut self) {
        match self.browser.discover_nodes() {
            Ok(rows) => {
                self.status = format!("{} file(s) found.", rows.len());
                self.rows = rows;
                self.selected_row = (!self.rows.is_empty()).then_some(0);
                self.load_columns_of_selection();
            }
            Err(error) => {
                self.rows.clear();
                self.selected_row = None;
                self.status = error.to_string();
            }
        }
    }

    /// Re-scan the network directory, which also resolves the edges files.
    pub fn refresh_networks(&mut self) {
        match self.browser.discover_networks() {
            Ok(rows) => {
                self.status = format!("{} file(s) found.", rows.len());
                self.rows = rows;
                self.selected_row = (!self.rows.is_empty()).then_some(0);
                self.load_columns_of_selection();
            }
            Err(error) => {
                self.rows.clear();
                self.selected_row = None;
                self.status = error.to_string();
            }
        }
    }

    /// Read the columns of the selected nodes file and offer them to the
    /// column pickers, so the user chooses from real column names.
    pub fn load_columns_of_selection(&mut self) {
        let Some(row) = self.selected_row.and_then(|i| self.rows.get(i)) else {
            return;
        };

        let extension =
            match mosna_io::read::get_opener::Extension::parse(self.browser.extension.trim()) {
                Ok(extension) => extension,
                Err(error) => {
                    self.status = error.to_string();
                    return;
                }
            };

        match mosna_io::read::get_opener::read_table(&row.nodes_path, extension) {
            Ok(table) => {
                let columns: Vec<String> = table
                    .column_names()
                    .into_iter()
                    .map(str::to_string)
                    .collect();
                self.form.set_available_columns(&columns);
            }
            Err(error) => {
                self.status = format!("Could not read nodes file: {error}");
            }
        }
    }

    /// Re-scan the working directory for figures.
    pub fn refresh_images(&mut self) {
        if let Some(root) = self.working_dir() {
            self.images = collect_analysis_images(root);
            self.selected_image = 0;
        }
    }

    /// Merge the Browser and Parameters panels into the document, then write it.
    pub fn save_config(&mut self) -> anyhow::Result<()> {
        self.browser.apply_to(&mut self.config);
        self.form.apply_to(&mut self.config);
        mosna_config::write_config(&self.config, &self.config_path)?;
        Ok(())
    }

    /// Start an analysis, after saving the configuration it will read.
    pub fn start(&mut self, step: Step) {
        if self.run.is_some() {
            return;
        }
        let Some(working_dir) = self.working_dir().map(Path::to_path_buf) else {
            self.notice = Some("Please choose a working directory first.".into());
            return;
        };
        if let Err(error) = self.save_config() {
            self.notice = Some(format!("Failed to save config:\n{error}"));
            return;
        }

        let arguments = step.arguments(&self.config_path, &working_dir);
        let mut command = std::process::Command::new(analysis_binary());
        command
            .args(&arguments)
            .current_dir(&working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        match command.spawn() {
            Ok(mut child) => {
                let (sender, output) = std::sync::mpsc::channel();

                // stdout carries the progress protocol, stderr the failure
                // message; both are shown in the log, so both are drained —
                // leaving one unread would eventually block the child.
                for stream in [
                    child.stdout.take().map(StreamKind::Out),
                    child.stderr.take().map(StreamKind::Err),
                ]
                .into_iter()
                .flatten()
                {
                    let sender = sender.clone();
                    std::thread::spawn(move || stream.pump(sender));
                }

                self.log.clear();
                self.status = format!("Running: {}", step.label());
                self.progress = None;
                self.active_step = Some(step);
                self.last_run_failed = false;
                self.run = Some(Run {
                    step,
                    child,
                    output,
                    started: Instant::now(),
                });
            }
            Err(error) => {
                self.notice = Some(format!(
                    "Could not start `{}`:\n{error}\n\nIs the `mosna` binary next to this one, \
                     or on your PATH?",
                    analysis_binary().display()
                ));
            }
        }
    }

    /// Ask the running analysis to stop.
    pub fn stop(&mut self) {
        if let Some(run) = &mut self.run {
            let _ = run.child.kill();
            self.status = "Stopping process...".to_string();
        }
    }

    /// Drain the running analysis's output and notice when it finishes.
    pub fn poll_run(&mut self) {
        let Some(run) = &mut self.run else { return };

        while let Ok(line) = run.output.try_recv() {
            match parse_output_line(&line) {
                OutputLine::Info(message) => self.status = message,
                OutputLine::Progress {
                    current,
                    total,
                    description,
                } => {
                    if !description.is_empty() {
                        self.status = description;
                    }
                    self.progress = Some((current, total));
                }
                OutputLine::Plain => {}
            }
            self.log.push((classify(&line), line));
        }

        let finished = match run.child.try_wait() {
            Ok(Some(status)) => Some(status.success()),
            Ok(None) => None,
            Err(_) => Some(false),
        };

        let Some(success) = finished else { return };

        let step = run.step;
        let elapsed = run.started.elapsed().as_secs_f64();
        self.run = None;
        self.last_run_failed = !success;

        let duration = format_duration(elapsed);
        if success {
            self.status = format!("✅ {} completed in {duration}", step.label());
            self.progress = Some((1, 1));
            self.refresh_images();
            self.viewer_tab = ViewerTab::Images;
        } else {
            self.status = format!("❌ {} failed in {duration}", step.label());
            self.progress = Some((0, 1));
            // The last line that is neither progress nor routine info is the
            // one that says what actually went wrong.
            let reason = self
                .log
                .iter()
                .rev()
                .map(|(_, line)| line.as_str())
                .find(|line| {
                    !line.contains("[QT_PROGRESS]")
                        && !line.contains("[QT_INFO]")
                        && !line.trim().is_empty()
                })
                .unwrap_or("Unknown error.")
                .to_string();
            self.notice = Some(format!("{}\n\n{reason}", step.label()));
        }
    }
}

/// Where to find the analysis binary the interface launches.
///
/// Resolved by `mosna-paths`, which the installer also uses to decide where to
/// put it — so a relocated or user-local install is found without any special
/// case here.
fn analysis_binary() -> PathBuf {
    mosna_paths::binary::resolve_analysis(&mosna_paths::Environment::detect())
}

/// One of the child's output streams.
enum StreamKind {
    Out(std::process::ChildStdout),
    Err(std::process::ChildStderr),
}

impl StreamKind {
    fn pump(self, sender: std::sync::mpsc::Sender<String>) {
        use std::io::{BufRead, BufReader};
        let reader: Box<dyn BufRead> = match self {
            StreamKind::Out(stream) => Box::new(BufReader::new(stream)),
            StreamKind::Err(stream) => Box::new(BufReader::new(stream)),
        };
        for line in reader.lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    }
}

impl eframe::App for MosnaApp {
    /// What the window is cleared to before anything is drawn on it.
    ///
    /// eframe's default is a near-black, which the panels used to cover
    /// invisibly. Against a silver interface it shows — at start-up, and along
    /// the edge while the window is being resized — so the page colour is
    /// stated here instead.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::BACKGROUND.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        theme::apply(&ctx);
        self.poll_run();

        // A run only repaints when output arrives, so ask for frames while one
        // is in flight; otherwise the progress bar would freeze.
        if self.run.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }

        // Modals are windows, which still take the context; the panels take the
        // root `Ui` and carve it up in order — top bar, then the two sides,
        // then whatever is left goes to the viewer.
        panels::modals::show(self, &ctx);
        panels::top_bar::show(self, ui);
        panels::browser::show(self, ui);
        panels::parameters::show(self, ui);
        panels::viewer::show(self, ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> MosnaApp {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("configuration.yaml");
        std::fs::write(
            &path,
            "Tysserand:\n  Nodes directory: /data\n  Patient column name: patient\n  \
             Extension: parquet\n  Min neighbors: 3\n",
        )
        .unwrap();
        let mut app = MosnaApp::new(path);
        // Keep the directory alive for the lifetime of the test.
        app.browser.working_dir = Some(dir.keep());
        app
    }

    #[test]
    fn a_fresh_application_asks_for_a_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("configuration.yaml");
        std::fs::write(&path, "Tysserand:\n  Min neighbors: 3\n").unwrap();

        let app = MosnaApp::new(path);
        assert!(app.needs_working_dir);
        assert_eq!(app.status, "Ready.");
    }

    #[test]
    fn a_missing_configuration_becomes_a_notice_not_a_panic() {
        let app = MosnaApp::new(PathBuf::from("/nonexistent/configuration.yaml"));
        assert!(app.notice.is_some());
        // The interface still opens, so the user can pick another file.
        assert!(app.form.sections.is_empty());
    }

    #[test]
    fn choosing_a_working_directory_clears_the_prompt() {
        let mut app = app();
        let dir = tempfile::tempdir().unwrap();
        app.set_working_dir(dir.path().to_path_buf());
        assert!(!app.needs_working_dir);
    }

    #[test]
    fn saving_merges_both_panels_into_the_document() {
        let mut app = app();
        app.browser.patient_column = "case".into();
        app.form
            .set_text("Tysserand", "General", "Min neighbors", "9");
        app.save_config().unwrap();

        let reloaded = mosna_config::get_config(&app.config_path).unwrap();
        assert_eq!(
            reloaded.get("Tysserand", "Patient column name"),
            Some(&serde_yaml::Value::String("case".into()))
        );
        assert_eq!(
            reloaded.get("Tysserand", "Min neighbors"),
            Some(&serde_yaml::Value::Number(9.into()))
        );
    }

    #[test]
    fn starting_without_a_working_directory_is_refused() {
        let mut app = app();
        app.browser.working_dir = None;
        app.start(Step::Tysserand);
        assert!(app.run.is_none());
        assert!(app.notice.as_deref().unwrap().contains("working directory"));
    }

    #[test]
    fn a_bad_nodes_directory_shows_in_the_status_bar() {
        let mut app = app();
        app.browser.nodes_directory = "/nonexistent/mosna".into();
        app.refresh_nodes();
        assert!(app.rows.is_empty());
        assert!(app.status.contains("/nonexistent/mosna"), "{}", app.status);
    }
}
