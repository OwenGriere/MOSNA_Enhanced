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

/// Which screen the window is showing.
///
/// The sensitivity analysis replaces the main layout rather than opening a
/// window of its own: choosing a grid is done *instead of* the main interface,
/// with the whole width, and it ends by sending the user back to the figures
/// its runs produced. The arrow in its top-left corner is the way back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Main,
    Sensitivity,
}

/// The settings a sweep is about to be built on, and what is wrong with them.
#[derive(Debug, Clone)]
pub struct SweepGate {
    /// The General settings, as `(name, value)`.
    pub summary: Vec<(String, crate::model::sweep::SettingValue)>,
    /// What is missing; empty when the section is complete.
    pub problems: Vec<String>,
}

impl SweepGate {
    /// Whether the sensitivity screen can be entered.
    pub fn is_ready(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Where a sweep's results go, under `Niche_Analysis`.
///
/// Its own directory because a sweep of two hundred runs would otherwise bury
/// the handful started by hand. Only the results move: the register and the
/// intermediate files stay put, so a swept run shares its cache with a
/// hand-started one and their numbers cannot collide.
pub const SENSITIVITY_DIR: &str = "sensitivity_analysis";

/// The comparison table a finished sweep leaves beside its runs.
pub const SENSITIVITY_CSV: &str = "sensitivity.csv";

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
    /// The directory the analysis now running has claimed, once it says so.
    pub last_run_directory: Option<String>,
    pub run: Option<Run>,

    /// The manual, and where the reader is in it.
    pub documentation: Documentation,
    pub manual: ManualState,

    /// Whether the side panels have been folded away to a band.
    ///
    /// The viewer between them takes whatever they leave, so folding one is
    /// how the Network tab gets a screen's width to draw in. The Viewer itself
    /// does not fold: it is what the window is for.
    pub browser_folded: bool,
    pub parameters_folded: bool,

    /// Which screen the window is showing.
    pub screen: Screen,
    /// The settings a sweep would run on, shown for confirmation before the
    /// sensitivity screen opens.
    ///
    /// Always shown, even when everything is filled in: every run of a grid
    /// shares these, so one of them wrong is a grid that fails from end to end
    /// — and that is a mistake worth one glance before minutes of work.
    pub sweep_gate: Option<SweepGate>,
    /// The grid the sensitivity screen is editing.
    pub sweep_form: crate::model::sweep::SweepForm,
    /// The sweep under way, if any.
    pub sweep: Option<crate::model::sweep::Sweep>,
    /// Whether the comparison of a finished sweep is being drawn.
    ///
    /// It is one more sub-process, started once the last run lands, and the
    /// screen shows its figures when it ends.
    pub comparing: bool,

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
        let sweep_form = crate::model::sweep::SweepForm::from_config(&config);

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
            last_run_directory: None,
            run: None,
            documentation: Documentation::build(),
            manual: ManualState::default(),
            browser_folded: false,
            parameters_folded: false,
            screen: Screen::Main,
            sweep_gate: None,
            sweep_form,
            sweep: None,
            comparing: false,
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
        self.log.clear();
        if let Err(error) = self.spawn(step, &arguments, &working_dir) {
            self.notice = Some(error);
        }
    }

    /// Start `mosna` with `arguments` and take over its output.
    ///
    /// Shared by the action bar and by the sweep, which launches the same
    /// sub-command with a configuration of its own: one place that knows how a
    /// run is started is one place where the streams cannot be left unread.
    fn spawn(
        &mut self,
        step: Step,
        arguments: &[String],
        working_dir: &Path,
    ) -> Result<(), String> {
        let mut command = std::process::Command::new(analysis_binary());
        command
            .args(arguments)
            .current_dir(working_dir)
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

                self.status = format!("Running: {}", step.label());
                self.progress = None;
                self.active_step = Some(step);
                self.last_run_failed = false;
                self.last_run_directory = None;
                self.run = Some(Run {
                    step,
                    child,
                    output,
                    started: Instant::now(),
                });
                Ok(())
            }
            Err(error) => Err(format!(
                "Could not start `{}`:\n{error}\n\nIs the `mosna` binary next to this one, \
                 or on your PATH?",
                analysis_binary().display()
            )),
        }
    }

    /// Ask the running analysis to stop.
    pub fn stop(&mut self) {
        if let Some(run) = &mut self.run {
            let _ = run.child.kill();
            self.status = "Stopping process...".to_string();
        }
    }

    // -----------------------------------------------------------------------
    // The sweep
    // -----------------------------------------------------------------------

    /// Start the grid the sensitivity screen is holding.
    ///
    /// The runs are launched one at a time, as sub-processes, exactly the way a
    /// single analysis is. Sequential rather than parallel because a run
    /// already uses every core — the samples of a cohort are aggregated in
    /// parallel, and so is the projection — so running two at once would divide
    /// the same machine between them, and because the order the grid is walked
    /// in is what lets each run read the stage above it from the cache. A
    /// sub-process is also what makes the sweep interruptible: a run that was
    /// given a parameter beyond the cohort can take longer than anyone will
    /// wait for, and there is no way to cancel one from inside.
    pub fn start_sweep(&mut self) {
        let plan = self.sweep_form.plan();
        if plan.is_empty() || self.working_dir().is_none() || self.run.is_some() {
            return;
        }
        if let Err(error) = self.sweep_base_config() {
            self.notice = Some(format!("Failed to save config:\n{error}"));
            return;
        }
        self.sweep = Some(crate::model::sweep::Sweep::new(&plan));
        self.comparing = false;
        self.log.clear();
        self.advance_sweep();
    }

    /// The configuration a sweep varies: the one the panels are showing.
    ///
    /// The Browser and the Parameters panels edit their own state and write it
    /// into the configuration only when an analysis is started. The sweep
    /// started without doing that, so it varied whatever had been loaded at
    /// start-up — and against a configuration whose `Column to aggregate` was
    /// still empty, every run of the grid failed validation in the same
    /// instant.
    pub fn sweep_base_config(&mut self) -> anyhow::Result<&RawConfig> {
        // The Browser owns the cohort — the network directory, the columns, the
        // extension — and the General tab owns the rest of what every run of a
        // grid shares. The sub-sections are deliberately left alone: they are
        // what the sweep varies, and writing the Parameters panel back over
        // them would make whatever it happened to be showing the starting point
        // of a grid the user had already described on the other screen.
        self.browser.apply_to(&mut self.config);
        self.form
            .apply_tab_to(&mut self.config, "Niche Analysis", "General");
        mosna_config::write_config(&self.config, &self.config_path)?;
        Ok(&self.config)
    }

    /// Save, check the General settings, and ask before opening the screen.
    ///
    /// The button used to open the screen directly. The configuration the sweep
    /// varies is the one the panels are showing, and nothing had written it
    /// back yet — so a grid could be chosen, started, and fail on every one of
    /// its runs for a column that was never filled in.
    pub fn open_sweep_gate(&mut self) {
        let problems = self.sweep_general_problems();
        self.sweep_gate = Some(SweepGate {
            summary: crate::model::sweep::general_summary(&self.config),
            problems,
        });
    }

    /// What the Sensitivity button does.
    ///
    /// # Why it is never greyed out
    ///
    /// The other buttons of the action bar start work, so they are disabled
    /// while work is in flight. This one does not: it opens a screen. Greying
    /// it while a sweep ran meant a user who stepped back to the main interface
    /// to look at a figure could not get back to the sweep they had started,
    /// and had to watch it from a screen that does not show it.
    ///
    /// A sweep already under way goes straight back, with no confirmation:
    /// there is nothing to confirm, the grid is already running.
    pub fn open_sensitivity(&mut self) {
        if self.sweep.as_ref().is_some_and(|sweep| sweep.is_running()) {
            self.screen = Screen::Sensitivity;
            return;
        }
        self.open_sweep_gate();
    }

    /// Enter the sensitivity screen, if the gate allows it.
    pub fn enter_sweep_screen(&mut self) {
        if self.sweep_gate.as_ref().is_some_and(SweepGate::is_ready) {
            self.screen = Screen::Sensitivity;
            self.sweep_gate = None;
        }
    }

    /// What is missing from the General settings a sweep would run on.
    pub fn sweep_general_problems(&mut self) -> Vec<String> {
        if let Err(error) = self.sweep_base_config() {
            return vec![format!("the configuration could not be saved: {error}")];
        }
        crate::model::sweep::general_problems(&self.config)
    }

    /// Stop the sweep, and the run it has in flight.
    ///
    /// # Why the run in flight is killed
    ///
    /// Letting it finish was the careful choice: a killed run leaves an entry
    /// the register still calls `running` and a directory with part of its
    /// figures in it. But a niche analysis on a real cohort is minutes, so
    /// "stop" that does nothing for minutes is indistinguishable from a button
    /// that does not work — which is exactly how it was reported.
    ///
    /// Nothing is corrupted by it. The register records what happened, the
    /// intermediate files are written atomically and are either complete or
    /// absent, and the next run over the same settings recomputes what is
    /// missing.
    pub fn stop_sweep(&mut self) {
        if let Some(sweep) = &mut self.sweep {
            sweep.stopping = true;
        }
        if let Some(run) = &mut self.run {
            let _ = run.child.kill();
        }
        self.status = "Stopping the sweep…".to_string();
    }

    /// Launch the next run of the sweep, or finish it.
    fn advance_sweep(&mut self) {
        let Some(sweep) = &self.sweep else { return };
        let Some(next) = sweep.next_pending() else {
            // Nothing left: write the table and let the screen say so.
            if self.sweep.as_ref().is_some_and(|s| s.is_over()) {
                self.write_sensitivity_table();
            }
            return;
        };

        let combination = sweep.rows[next].combination.clone();
        let sub_section = crate::model::sweep::swept_subsection(&self.config);
        let normalize = self.sweep_form.normalize.clone();
        let configuration = crate::model::sweep::apply_with(
            &self.config,
            &combination,
            sub_section,
            Some(&normalize),
        );

        match self.spawn_sweep_run(&configuration) {
            Ok(()) => {
                if let Some(sweep) = &mut self.sweep {
                    sweep.begin(next);
                }
            }
            // The sweep stops, but keeps what it has: dropping it threw away
            // every finished row, the reason the last one failed and the
            // comparison table, all for a run that merely could not start.
            Err(error) => {
                if let Some(sweep) = &mut self.sweep {
                    sweep.stopping = true;
                    if sweep.failure.is_none() {
                        sweep.failure = Some(error.clone());
                    }
                }
                self.notice = Some(error);
            }
        }
    }

    /// Write one combination's configuration and start `mosna` on it.
    ///
    /// The configuration goes to a file of its own rather than over the main
    /// `configuration.yaml`: the sweep varies a handful of keys and must not
    /// leave the interface's own settings changed when it ends.
    fn spawn_sweep_run(&mut self, configuration: &RawConfig) -> Result<(), String> {
        let Some(working_dir) = self.working_dir().map(Path::to_path_buf) else {
            return Err("Choose a working directory first.".to_string());
        };

        let path = working_dir.join("temp").join("sweep-configuration.yaml");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Could not create {}:\n{e}", parent.display()))?;
        }
        let text = configuration
            .to_yaml_string()
            .map_err(|e| format!("Could not build the configuration:\n{e}"))?;
        std::fs::write(&path, text)
            .map_err(|e| format!("Could not write {}:\n{e}", path.display()))?;

        // PNG only: a run's interactive figures are 1.8 MB of the 4.9 it
        // writes, and they are for inspecting one run rather than comparing two
        // hundred. They can be redrawn for a run worth keeping.
        let arguments = vec![
            "niche-analysis".to_string(),
            "--file".to_string(),
            path.to_string_lossy().into_owned(),
            "--working_dir".to_string(),
            working_dir.to_string_lossy().into_owned(),
            "--figure-formats".to_string(),
            "png".to_string(),
            "--results-in".to_string(),
            SENSITIVITY_DIR.to_string(),
        ];
        self.spawn(Step::NicheAnalysis, &arguments, &working_dir)
    }

    /// The comparison table, written where the runs are.
    ///
    /// The deliverable of a sensitivity analysis: the individual figures answer
    /// "what did run 1-1-7 look like", and this answers "what did varying the
    /// resolution do", which is the question that was asked.
    fn write_sensitivity_table(&mut self) {
        let (Some(sweep), Some(working_dir)) = (&self.sweep, self.working_dir()) else {
            return;
        };
        let columns = self.sweep_form.plan().columns();
        let text = crate::model::sweep::to_csv(&columns, &sweep.rows);
        let path = working_dir
            .join("Niche_Analysis")
            .join(SENSITIVITY_DIR)
            .join(SENSITIVITY_CSV);

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, text) {
            Ok(()) => self.status = format!("Sweep finished — {}", path.display()),
            Err(error) => {
                self.notice = Some(format!("Could not write {}:\n{error}", path.display()))
            }
        }

        self.compare_sweep_runs();
    }

    /// Compare the runs a sweep produced, and draw the three figures.
    ///
    /// # Why this is one more sub-process rather than work done here
    ///
    /// Every measure is between *pairs* of runs, so none of them exists until
    /// the sweep does — and drawing them needs the renderer, which is already
    /// on the other side of the sub-process boundary. Running `compare-sweep`
    /// the way every other step is run keeps the interface from linking the
    /// pipeline and the plotting crate for the sake of one screen, and means
    /// the same comparison can be asked for from a terminal on a sweep from
    /// last week.
    fn compare_sweep_runs(&mut self) {
        let Some(working_dir) = self.working_dir().map(Path::to_path_buf) else {
            return;
        };
        let finished = self
            .sweep
            .as_ref()
            .map(|sweep| sweep.rows.iter().filter(|row| row.run.is_some()).count())
            .unwrap_or(0);
        if finished < 2 {
            return;
        }

        let arguments = vec![
            "compare-sweep".to_string(),
            "--file".to_string(),
            self.config_path.to_string_lossy().into_owned(),
            "--working_dir".to_string(),
            working_dir.to_string_lossy().into_owned(),
            "--results-in".to_string(),
            SENSITIVITY_DIR.to_string(),
        ];
        // The three are the answer the sweep was run for, so unlike the
        // per-run figures they are worth their interactive copy.
        self.comparing = true;
        if let Err(error) = self.spawn(Step::NicheAnalysis, &arguments, &working_dir) {
            self.comparing = false;
            self.status = format!("The runs could not be compared: {error}");
        }
    }

    /// Record the run that has just ended and start the next one.
    fn finish_sweep_run(&mut self, success: bool) {
        let run = self.last_run_directory.clone();
        let record = run.as_deref().and_then(|name| self.run_record(name));
        let reason = (!success).then(|| self.failure_reason());

        if let Some(sweep) = &mut self.sweep {
            sweep.end_with(success, run, record.as_ref(), reason);
            self.status = sweep.caption();
            self.progress = Some((sweep.finished(), sweep.total()));
        }

        // The figures a sweep produces are the ones the viewer shows, so it is
        // refreshed as the sweep goes rather than only at the end.
        self.refresh_images();
        self.advance_sweep();

        if self.sweep.as_ref().is_some_and(|sweep| sweep.is_over()) {
            self.write_sensitivity_table();
        }
    }

    /// The last line of the log that says something.
    ///
    /// The progress and status lines are the protocol; what is left is either
    /// the failure or nothing, and the last of it is the closest to the cause.
    fn failure_reason(&self) -> String {
        self.log
            .iter()
            .rev()
            .map(|(_, line)| line.as_str())
            .find(|line| {
                !line.contains("[QT_PROGRESS]")
                    && !line.contains("[QT_INFO]")
                    && !line.contains("[QT_RUN]")
                    && !line.trim().is_empty()
            })
            .unwrap_or("Unknown error.")
            .to_string()
    }

    /// What a finished run recorded about itself, if it can be read.
    ///
    /// A swept run writes into `Niche_Analysis/sensitivity_analysis/`, so that
    /// is where its record is. Reading the top level regardless left every row
    /// of the comparison table empty while the runs themselves had succeeded.
    fn run_record(&self, run: &str) -> Option<serde_json::Value> {
        let niche_dir = self.working_dir()?.join("Niche_Analysis");
        let path = if self.sweep.is_some() {
            niche_dir.join(SENSITIVITY_DIR).join(run).join("run.json")
        } else {
            niche_dir.join(run).join("run.json")
        };
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
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
                // The run directory, as soon as the analysis has claimed it:
                // a sweep links the configuration it launched to the results it
                // produced through this and nothing else.
                OutputLine::Run(name) => self.last_run_directory = Some(name),
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

        // A sweep is a queue of these: the run that has just ended fills in its
        // own row, and the next one starts. It is handled before the status
        // line below, which speaks about a single run and would otherwise
        // announce the end of a sweep that is still going.
        // The comparison is one more sub-process of the sweep, but it is not
        // one of its runs: it fills no row and advances nothing.
        if self.comparing {
            self.comparing = false;
            self.status = if success {
                "Sweep finished — the comparison figures are drawn".to_string()
            } else {
                "Sweep finished — the comparison figures could not be drawn".to_string()
            };
            self.refresh_images();
            return;
        }
        if self.sweep.is_some() {
            self.finish_sweep_run(success);
            return;
        }

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
        match self.screen {
            Screen::Main => {
                panels::top_bar::show(self, ui);
                panels::browser::show(self, ui);
                panels::parameters::show(self, ui);
                panels::viewer::show(self, ui);
            }
            Screen::Sensitivity => panels::sensitivity::show(self, ui),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> MosnaApp {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("configuration.yaml");
        // A niche section too: the sweep varies it, so a fixture without one
        // would let a test pass that says nothing.
        std::fs::write(
            &path,
            "Tysserand:\n  Nodes directory: /data\n  Patient column name: patient\n  \
             Extension: parquet\n  Min neighbors: 3\n\
             Niche Analysis:\n  Processing method: Aggregated nodes\n  \
             Niches method: NAS\n  Extension: parquet\n  \
             Patient column name: patient\n  Network directory: Default\n  \
             Column to aggregate: Cluster\n  Phenotype column: Cluster\n  \
             Aggregated nodes:\n    reducer_type: umap\n    clusterer_type: gmm\n    \
             metric: euclidean\n    n_clusters: 6\n    n_neighbors: 20\n    \
             dim_clust: 2\n    min_dist: 0.0\n    resolution: 0.05\n    \
             k_cluster: 20\n    order: '1'\n    normalize: total\n",
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

    // -----------------------------------------------------------------------
    // The sensitivity screen
    // -----------------------------------------------------------------------

    /// The screen replaces the main layout and the arrow brings it back: one
    /// gesture in each direction, and no window to find or raise.
    #[test]
    fn the_sensitivity_screen_is_a_place_the_window_goes_and_comes_back_from() {
        let mut app = app();
        assert_eq!(app.screen, Screen::Main);

        app.screen = Screen::Sensitivity;
        assert_eq!(app.screen, Screen::Sensitivity);
        app.screen = Screen::Main;
        assert_eq!(app.screen, Screen::Main);
    }

    /// A grid with nothing in it starts nothing, rather than launching a sweep
    /// of no runs and reporting it finished.
    #[test]
    fn an_empty_grid_starts_nothing() {
        let mut app = app();
        app.start_sweep();
        assert!(app.sweep.is_none());
    }

    /// And a grid that is ready still refuses without somewhere to write.
    #[test]
    fn a_sweep_needs_a_working_directory() {
        let mut app = app();
        app.browser.working_dir = None;
        arm(&mut app);

        app.start_sweep();
        assert!(app.sweep.is_none());
    }

    /// Stopping is a request, not a kill: the run in flight is left to finish,
    /// because ending it half-way would leave a directory the register calls
    /// `running` and half a set of figures.
    #[test]
    fn stopping_a_sweep_lets_the_run_in_flight_finish() {
        let mut app = app();
        arm(&mut app);
        app.sweep = Some(crate::model::sweep::Sweep::new(&app.sweep_form.plan()));
        app.sweep.as_mut().unwrap().begin(0);

        app.stop_sweep();
        let sweep = app.sweep.as_ref().unwrap();
        assert!(sweep.stopping);
        assert_eq!(
            sweep.current,
            Some(0),
            "the run in flight was not abandoned"
        );
        assert!(
            sweep.next_pending().is_none(),
            "no further run is handed out"
        );
    }

    /// The table a finished sweep leaves behind, beside the runs it produced.
    #[test]
    fn a_finished_sweep_writes_its_comparison_table() {
        let mut app = app();
        arm(&mut app);

        let mut sweep = crate::model::sweep::Sweep::new(&app.sweep_form.plan());
        let total = sweep.total();
        assert!(total >= 2, "the fixture should sweep more than one point");
        for index in 0..total {
            sweep.begin(index);
            sweep.end(
                true,
                Some(format!("1-1-{}", index + 1)),
                Some(&serde_json::json!({
                    "result": { "niches": 6, "sizes": [10, 20], "graph_components": null }
                })),
            );
        }
        app.sweep = Some(sweep);
        app.write_sensitivity_table();

        let path = app
            .working_dir()
            .unwrap()
            .join("Niche_Analysis")
            .join(SENSITIVITY_DIR)
            .join(SENSITIVITY_CSV);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let lines: Vec<&str> = text.lines().collect();

        assert!(lines[0].starts_with("run,status,"), "{}", lines[0]);
        assert!(lines[0].ends_with(",niches,largest_niche,graph_components"));
        assert_eq!(lines.len(), total + 1, "one line per run, plus the header");
        assert!(lines[1].starts_with("1-1-1,done,"), "{}", lines[1]);
    }

    /// A grid ready to run: two cluster counts under gmm.
    fn arm(app: &mut MosnaApp) {
        let gmm = app
            .sweep_form
            .clusterers
            .iter_mut()
            .find(|c| c.clusterer == "gmm")
            .unwrap();
        gmm.selected = true;
        // Whole numbers are ticked from a menu: two cluster counts, two runs.
        let axis = gmm.axes.iter_mut().find(|a| a.key == "n_clusters").unwrap();
        axis.from = "4".into();
        axis.to = "6".into();
        axis.refresh_candidates();
        for (name, picked) in &mut axis.choices {
            *picked = matches!(name.as_str(), "4" | "6");
        }
        assert!(app.sweep_form.is_runnable());
        assert_eq!(app.sweep_form.plan().len(), 2);
    }

    /// The sweep varies the configuration the panels are showing, not the one
    /// that happened to be on disk when the window opened.
    ///
    /// The single analysis buttons save before they start; the sweep did not,
    /// so it swept whatever had been loaded at start-up. Against a
    /// configuration whose `Column to aggregate` was still empty — which is how
    /// the shipped one ships — every run of the grid failed validation in the
    /// same instant, and eighty-four of them failed before the user could read
    /// the first.
    #[test]
    fn a_sweep_varies_the_configuration_the_panels_are_showing() {
        let mut app = app();
        app.form
            .set_text("Niche Analysis", "General", "Niches method", "SCAN-IT");

        let config = app.sweep_base_config().unwrap().clone();
        assert_eq!(
            config
                .get("Niche Analysis", "Niches method")
                .and_then(|v| v.as_str()),
            Some("SCAN-IT"),
            "the sweep would have varied a configuration the user never saw"
        );
    }

    /// And only that part: the sub-sections are what the grid varies, so
    /// writing the Parameters panel back over them would make whatever it
    /// happened to be showing the starting point of the sweep.
    #[test]
    fn a_sweep_does_not_write_back_the_parameters_it_varies() {
        let mut app = app();
        app.form
            .set_text("Niche Analysis", "Aggregated nodes", "metric", "manhattan");

        let config = app.sweep_base_config().unwrap().clone();
        assert_eq!(
            config
                .get("Niche Analysis", "Aggregated nodes")
                .and_then(|sub| sub.get("metric"))
                .and_then(|v| v.as_str()),
            Some("euclidean"),
            "a parameter the sweep varies was written back over"
        );
    }

    /// A run of a sweep that fails says why. The single-run path reports the
    /// reason in a dialog; the sweep returned before reaching it, so a grid
    /// could fail from end to end reporting only a count.
    #[test]
    fn a_failed_sweep_run_keeps_the_reason_it_failed() {
        let mut app = app();
        arm(&mut app);
        app.sweep = Some(crate::model::sweep::Sweep::new(&app.sweep_form.plan()));
        app.sweep.as_mut().unwrap().begin(0);

        app.log.push((
            LogKind::Plain,
            "[QT_INFO] Verification and Convertion of the files".to_string(),
        ));
        app.log.push((
            LogKind::Error,
            "Column to aggregate parameter must be str or list".to_string(),
        ));

        app.finish_sweep_run(false);

        let sweep = app.sweep.as_ref().unwrap();
        assert_eq!(sweep.failed(), 1);
        assert_eq!(
            sweep.failure.as_deref(),
            Some("Column to aggregate parameter must be str or list"),
            "the reason was dropped"
        );
        assert!(
            sweep.caption().contains("Column to aggregate"),
            "the caption does not say why: {}",
            sweep.caption()
        );
    }

    /// Only the first reason is kept: a grid failing for one cause would
    /// otherwise overwrite it eighty-four times with the same words.
    #[test]
    fn the_first_reason_is_the_one_kept() {
        let mut app = app();
        arm(&mut app);
        app.sweep = Some(crate::model::sweep::Sweep::new(&app.sweep_form.plan()));

        app.sweep.as_mut().unwrap().begin(0);
        app.log
            .push((LogKind::Error, "the first reason".to_string()));
        app.finish_sweep_run(false);

        app.log.push((LogKind::Error, "a later reason".to_string()));
        app.sweep.as_mut().unwrap().begin(1);
        app.finish_sweep_run(false);

        assert_eq!(
            app.sweep.as_ref().unwrap().failure.as_deref(),
            Some("the first reason")
        );
    }

    /// The button saves, checks, and asks — it does not open the screen.
    #[test]
    fn the_button_asks_before_it_opens_the_screen() {
        let mut app = app();
        app.open_sweep_gate();

        assert_eq!(app.screen, Screen::Main, "the screen opened unasked");
        let gate = app.sweep_gate.as_ref().expect("a confirmation");
        assert!(gate.is_ready(), "{:?}", gate.problems);
        assert!(
            gate.summary.iter().any(|(name, value)| {
                name == "Column to aggregate" && value.caption() == "Cluster"
            }),
            "{:?}",
            gate.summary
        );

        app.enter_sweep_screen();
        assert_eq!(app.screen, Screen::Sensitivity);
        assert!(app.sweep_gate.is_none());
    }

    /// And it saves on the way, so what the screen varies is what the panels
    /// are showing.
    #[test]
    fn the_button_writes_the_panels_back_before_it_asks() {
        let mut app = app();
        app.form.set_text(
            "Niche Analysis",
            "General",
            "Processing method",
            "Per sample",
        );
        app.open_sweep_gate();

        assert_eq!(
            app.config
                .get("Niche Analysis", "Processing method")
                .and_then(|v| v.as_str()),
            Some("Per sample")
        );
    }

    /// An incomplete General section keeps the screen shut and says what is
    /// missing — the alternative was eighty-four runs failing for it.
    #[test]
    fn an_incomplete_general_section_keeps_the_screen_shut() {
        // The shipped configuration ships this way: a column nobody has picked
        // yet. It is what made eighty-four runs fail in the same instant.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("configuration.yaml");
        std::fs::write(
            &path,
            "Niche Analysis:\n  Processing method: Aggregated nodes\n  \
             Niches method: NAS\n  Extension: parquet\n  \
             Patient column name: patient\n  Column to aggregate: null\n  \
             Phenotype column: null\n",
        )
        .unwrap();
        let mut app = MosnaApp::new(path);
        app.browser.working_dir = Some(dir.keep());

        app.open_sweep_gate();

        let gate = app.sweep_gate.as_ref().unwrap();
        assert!(!gate.is_ready());
        assert!(
            gate.problems
                .iter()
                .any(|p| p.contains("Column to aggregate")),
            "{:?}",
            gate.problems
        );

        app.enter_sweep_screen();
        assert_eq!(app.screen, Screen::Main, "the screen opened anyway");
    }

    /// Stop kills the run in flight: a niche analysis is minutes, and a stop
    /// that does nothing for minutes is a stop that does not work.
    #[test]
    fn stopping_a_sweep_ends_it_rather_than_waiting() {
        let mut app = app();
        arm(&mut app);
        app.sweep = Some(crate::model::sweep::Sweep::new(&app.sweep_form.plan()));
        app.sweep.as_mut().unwrap().begin(0);

        app.stop_sweep();
        assert!(app.sweep.as_ref().unwrap().stopping);

        // Once the killed run is reaped, the sweep is over and the screen
        // offers to start another.
        app.finish_sweep_run(false);
        let sweep = app.sweep.as_ref().unwrap();
        assert!(!sweep.is_running());
        assert!(sweep.caption().contains("stopped"), "{}", sweep.caption());
    }

    /// A swept run's record is where the sweep writes it. Reading the top level
    /// regardless left every row of the comparison table empty while the runs
    /// themselves had succeeded.
    #[test]
    fn a_swept_runs_record_is_read_from_the_sweeps_own_directory() {
        let mut app = app();
        arm(&mut app);
        app.sweep = Some(crate::model::sweep::Sweep::new(&app.sweep_form.plan()));

        let run = app
            .working_dir()
            .unwrap()
            .join("Niche_Analysis")
            .join(SENSITIVITY_DIR)
            .join("1-1-1");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(
            run.join("run.json"),
            r#"{"result":{"niches":7,"sizes":[3,4],"graph_components":null}}"#,
        )
        .unwrap();

        app.sweep.as_mut().unwrap().begin(0);
        app.last_run_directory = Some("1-1-1".to_string());
        app.finish_sweep_run(true);

        let row = &app.sweep.as_ref().unwrap().rows[0];
        assert_eq!(row.niches, Some(7), "the record was not found");
        assert_eq!(row.largest, Some(4));
    }

    /// The Sensitivity button opens a screen rather than starting work, so it
    /// stays available while a sweep runs — otherwise a user who stepped back
    /// to the main interface to look at a figure could not return to the sweep
    /// they had started.
    #[test]
    fn the_sensitivity_button_goes_straight_back_while_a_sweep_runs() {
        let mut app = app();
        arm(&mut app);
        app.sweep = Some(crate::model::sweep::Sweep::new(&app.sweep_form.plan()));
        app.sweep.as_mut().unwrap().begin(0);
        app.screen = Screen::Main;

        app.open_sensitivity();

        assert_eq!(app.screen, Screen::Sensitivity);
        assert!(
            app.sweep_gate.is_none(),
            "a running sweep was asked to confirm settings it is already using"
        );
    }

    /// With no sweep in flight it asks first, as it always did.
    #[test]
    fn the_sensitivity_button_asks_when_nothing_is_running() {
        let mut app = app();
        app.open_sensitivity();

        assert_eq!(app.screen, Screen::Main);
        assert!(app.sweep_gate.is_some());
    }

    /// And a sweep that has finished is not one in flight: the settings are
    /// worth confirming again before another grid is built on them.
    #[test]
    fn the_sensitivity_button_asks_again_once_a_sweep_is_over() {
        let mut app = app();
        arm(&mut app);
        let mut sweep = crate::model::sweep::Sweep::new(&app.sweep_form.plan());
        for index in 0..sweep.total() {
            sweep.begin(index);
            sweep.end(true, None, None);
        }
        app.sweep = Some(sweep);

        app.open_sensitivity();
        assert!(app.sweep_gate.is_some());
    }

    /// A cohort aggregating thirty-four phenotypes must not spell them out in
    /// the confirmation dialog: they would push the five other settings it
    /// exists to show off the screen.
    #[test]
    fn the_confirmation_abbreviates_a_long_list_of_columns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("configuration.yaml");
        let columns: String = (1..=34).map(|i| format!("    - pheno_{i}\n")).collect();
        std::fs::write(
            &path,
            format!(
                "Niche Analysis:\n  Processing method: Aggregated nodes\n  \
                 Niches method: NAS\n  Extension: parquet\n  \
                 Patient column name: patient\n  Phenotype column: Cluster\n  \
                 Column to aggregate:\n{columns}"
            ),
        )
        .unwrap();

        let mut app = MosnaApp::new(path);
        app.browser.working_dir = Some(dir.keep());
        app.open_sweep_gate();

        let gate = app.sweep_gate.as_ref().unwrap();
        let (_, value) = gate
            .summary
            .iter()
            .find(|(name, _)| name == "Column to aggregate")
            .unwrap();

        assert_eq!(value.caption(), "34 columns");
        assert!(value.is_abbreviated(), "the names are behind the count");
        assert_eq!(value.items().len(), 34, "and all of them are still there");
    }
}
