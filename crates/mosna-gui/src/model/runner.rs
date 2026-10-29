//! Launching an analysis and reading its output — port of
//! `ScriptRunnerThread` and `MosnaGUI._on_output_line`.

use std::path::Path;

/// The buttons of the action bar.
///
/// Three analyses, and three things to do with the directory they filled:
/// describe it, draw the figures its runs set aside, or take the intermediates
/// back out of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Tysserand,
    Assortativity,
    NicheAnalysis,
    GenerateReport,
    PlotFigures,
    ClearTemporary,
}

impl Step {
    /// Every step, in the order the buttons appear.
    pub fn all() -> [Step; 6] {
        [
            Step::Tysserand,
            Step::Assortativity,
            Step::NicheAnalysis,
            Step::GenerateReport,
            Step::PlotFigures,
            Step::ClearTemporary,
        ]
    }

    /// The action bar, row by row.
    ///
    /// An analysis takes the full width: it is the thing the user came to do,
    /// and its caption is long. The report has a row of its own: it is read
    /// last, once the figures exist. Drawing the figures set aside and clearing
    /// share the last row because they are the same kind of act — neither
    /// computes anything, both operate on a directory that is already full —
    /// and because putting the destructive one beside a harmless one, rather
    /// than alone under buttons that all start work, is what stops it being
    /// pressed by reflex.
    ///
    /// Written here rather than in the panel so the arrangement can be stated
    /// as a test instead of discovered by looking.
    pub fn rows() -> Vec<Vec<Step>> {
        vec![
            vec![Step::Tysserand],
            vec![Step::Assortativity],
            vec![Step::NicheAnalysis],
            vec![Step::GenerateReport],
            vec![Step::PlotFigures, Step::ClearTemporary],
        ]
    }

    /// The button's caption.
    pub fn label(self) -> &'static str {
        match self {
            Step::Tysserand => "Step 1 — Tysserand",
            Step::Assortativity => "Step 2 — Assortativity",
            Step::NicheAnalysis => "Step 3 — Niche Analysis",
            Step::GenerateReport => "Generate report",
            Step::PlotFigures => "Plot all figures",
            Step::ClearTemporary => "Clear temporary data",
        }
    }

    /// What the button does, spelled out for the tooltip.
    ///
    /// The two that share a row need it most: one of them deletes, and a
    /// caption of three words is not enough to say what.
    pub fn hint(self) -> &'static str {
        match self {
            Step::Tysserand => "Reconstruct a spatial network for every sample.",
            Step::Assortativity => "Measure which cell types sit next to which.",
            Step::NicheAnalysis => "Group neighbourhoods into spatial niches.",
            Step::GenerateReport => {
                "Collect every figure in the working directory into report.html,                  next to the results."
            }
            Step::PlotFigures => {
                "Draw every figure the analyses run \"without plotting figures\" set \
                 aside, from the start. Nothing is computed again."
            }
            Step::ClearTemporary => {
                "Delete the temp folder and the intermediate networks in it.                  The figures and the tables are kept."
            }
        }
    }

    /// The `mosna` sub-command this step runs.
    pub fn sub_command(self) -> &'static str {
        match self {
            Step::Tysserand => "tysserand-network",
            Step::Assortativity => "assortativity",
            Step::NicheAnalysis => "niche-analysis",
            Step::GenerateReport => "generate-report",
            Step::PlotFigures => "render-figures",
            Step::ClearTemporary => "clear-temporary",
        }
    }

    /// Whether the step reads `configuration.yaml`.
    ///
    /// The two that do not both act on a directory that already exists:
    /// clearing matches `clear_temporary.py`, whose parser declares only
    /// `--working_dir`, and the report describes what it finds rather than what
    /// was asked for — which is what lets it be made for results copied off a
    /// cluster, whose YAML is somewhere else entirely.
    pub fn takes_config(self) -> bool {
        !matches!(
            self,
            Step::ClearTemporary | Step::GenerateReport | Step::PlotFigures
        )
    }

    /// The analysis a section of the Parameters panel configures, if any.
    ///
    /// Those three are the ones that draw figures, so they are the ones whose
    /// panel offers to run without drawing them.
    pub fn for_section(name: &str) -> Option<Step> {
        match name {
            "Tysserand" => Some(Step::Tysserand),
            "Assortativity" => Some(Step::Assortativity),
            "Niche Analysis" => Some(Step::NicheAnalysis),
            _ => None,
        }
    }

    /// The full argument list, flags included.
    pub fn arguments(self, config_path: &Path, working_dir: &Path) -> Vec<String> {
        let mut arguments = vec![self.sub_command().to_string()];
        if self.takes_config() {
            arguments.push("--file".to_string());
            arguments.push(config_path.to_string_lossy().into_owned());
        }
        arguments.push("--working_dir".to_string());
        arguments.push(working_dir.to_string_lossy().into_owned());
        arguments
    }
}

/// Which analyses run *without plotting figures*.
///
/// One box per analysis, at the top of its panel. Ticked, the analysis writes
/// its tables and every other file as usual, and sets its figures aside instead
/// of drawing them; [`Step::PlotFigures`] draws them afterwards.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WithoutFigures {
    pub tysserand: bool,
    pub assortativity: bool,
    pub niche_analysis: bool,
}

impl WithoutFigures {
    /// The box of `step`, for an analysis; `None` for anything else.
    pub fn get_mut(&mut self, step: Step) -> Option<&mut bool> {
        match step {
            Step::Tysserand => Some(&mut self.tysserand),
            Step::Assortativity => Some(&mut self.assortativity),
            Step::NicheAnalysis => Some(&mut self.niche_analysis),
            _ => None,
        }
    }

    /// Whether `step` is to run without drawing.
    pub fn skips(mut self, step: Step) -> bool {
        self.get_mut(step).is_some_and(|ticked| *ticked)
    }

    /// The arguments `step` is started with, `--no-figures` included when its
    /// box is ticked.
    pub fn arguments(self, step: Step, config_path: &Path, working_dir: &Path) -> Vec<String> {
        let mut arguments = step.arguments(config_path, working_dir);
        if self.skips(step) {
            arguments.push("--no-figures".to_string());
        }
        arguments
    }
}

/// What a line of the process's stdout carries.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputLine {
    /// A status message for the status bar.
    Info(String),
    /// A position in the current step.
    Progress {
        current: usize,
        total: usize,
        description: String,
    },
    /// The run directory the analysis has just claimed.
    ///
    /// A run's numbers come from the register while it runs, so this is the
    /// only reliable way for a sweep to link what it launched to what it
    /// produced — see `mosna_pipeline::progress::Progress::run_directory`.
    Run(String),
    /// Anything else: shown in the log, nothing more.
    Plain,
}

/// Parse one line of the analysis process's stdout.
///
/// The protocol is the Python's, unchanged, so either interface can drive
/// either backend:
///
/// ```text
/// [QT_INFO] <message>
/// [QT_PROGRESS] current=<n> total=<n> desc=<text>
/// ```
///
/// A malformed progress line is treated as plain output rather than guessed at:
/// a wrong `total` would leave the progress bar stuck.
pub fn parse_output_line(line: &str) -> OutputLine {
    if let Some(message) = line.strip_prefix("[QT_INFO]") {
        return OutputLine::Info(message.trim().to_string());
    }

    if let Some(name) = line.strip_prefix("[QT_RUN]") {
        let name = name.trim();
        if !name.is_empty() {
            return OutputLine::Run(name.to_string());
        }
    }

    if let Some(payload) = line.strip_prefix("[QT_PROGRESS]") {
        let current = field(payload, "current=").and_then(|v| v.parse().ok());
        let total = field(payload, "total=").and_then(|v| v.parse().ok());
        if let (Some(current), Some(total)) = (current, total) {
            let description = payload
                .find("desc=")
                .map(|at| payload[at + "desc=".len()..].trim().to_string())
                .unwrap_or_default();
            return OutputLine::Progress {
                current,
                total,
                description,
            };
        }
    }

    OutputLine::Plain
}

/// The whitespace-delimited value following `name` in `payload`.
fn field<'a>(payload: &'a str, name: &str) -> Option<&'a str> {
    let at = payload.find(name)? + name.len();
    let rest = &payload[at..];
    Some(rest.split_whitespace().next().unwrap_or(rest))
}

/// Render a duration — port of `MosnaGUI._format_duration`.
pub fn format_duration(seconds: f64) -> String {
    let total = seconds.round() as i64;
    let (hours, remainder) = (total / 3600, total % 3600);
    let (minutes, secs) = (remainder / 60, remainder % 60);

    if hours > 0 {
        format!("{hours} h {minutes} min {secs} s")
    } else if minutes > 0 {
        format!("{minutes} min {secs} s")
    } else {
        format!("{seconds:.2} s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clear_step_takes_no_configuration() {
        let arguments = Step::ClearTemporary.arguments(Path::new("/cfg.yaml"), Path::new("/work"));
        assert_eq!(arguments, vec!["clear-temporary", "--working_dir", "/work"]);
    }

    /// The report describes a directory, not a configuration — which is what
    /// lets it be made for results copied off a cluster, whose YAML is
    /// somewhere else entirely.
    #[test]
    fn the_report_step_takes_no_configuration_either() {
        let arguments = Step::GenerateReport.arguments(Path::new("/cfg.yaml"), Path::new("/work"));
        assert_eq!(arguments, vec!["generate-report", "--working_dir", "/work"]);
    }

    /// The action bar: the three analyses each on their own line, the report
    /// on its own, and drawing the figures set aside beside the clear.
    #[test]
    fn the_last_row_holds_the_two_operations_on_a_finished_directory() {
        let rows = Step::rows();

        assert_eq!(rows.len(), 5, "the bar is not five rows: {rows:?}");
        for row in &rows[..4] {
            assert_eq!(row.len(), 1, "an analysis or the report shares its row");
        }
        assert_eq!(rows[3], vec![Step::GenerateReport]);
        assert_eq!(
            rows[4],
            vec![Step::PlotFigures, Step::ClearTemporary],
            "plotting is not on the left of the clear"
        );
    }

    /// Drawing what was set aside acts on a directory, like the report.
    #[test]
    fn plotting_all_figures_takes_no_configuration() {
        let arguments = Step::PlotFigures.arguments(Path::new("/cfg.yaml"), Path::new("/work"));
        assert_eq!(arguments, vec!["render-figures", "--working_dir", "/work"]);
    }

    /// A ticked box adds the flag to its own analysis, and to nothing else.
    #[test]
    fn a_ticked_box_runs_its_analysis_without_figures() {
        let (config, work) = (Path::new("/cfg.yaml"), Path::new("/work"));
        let mut boxes = WithoutFigures::default();
        for step in Step::all() {
            assert!(!boxes
                .arguments(step, config, work)
                .contains(&"--no-figures".to_string()));
        }

        *boxes.get_mut(Step::Assortativity).unwrap() = true;
        for step in Step::all() {
            let flagged = boxes
                .arguments(step, config, work)
                .contains(&"--no-figures".to_string());
            assert_eq!(flagged, step == Step::Assortativity, "{step:?}");
        }
    }

    /// One box per analysis panel, and none on the others.
    #[test]
    fn each_analysis_panel_has_its_box() {
        assert_eq!(Step::for_section("Tysserand"), Some(Step::Tysserand));
        assert_eq!(
            Step::for_section("Assortativity"),
            Some(Step::Assortativity)
        );
        assert_eq!(
            Step::for_section("Niche Analysis"),
            Some(Step::NicheAnalysis)
        );
        assert_eq!(Step::for_section("General"), None);
        for step in [
            Step::GenerateReport,
            Step::PlotFigures,
            Step::ClearTemporary,
        ] {
            assert!(WithoutFigures::default().get_mut(step).is_none());
        }
    }

    /// Every step is reachable, and none is offered twice — the rows are the
    /// only thing the panel draws from.
    #[test]
    fn every_step_appears_in_the_rows_exactly_once() {
        let mut drawn: Vec<Step> = Step::rows().into_iter().flatten().collect();
        let mut expected = Step::all().to_vec();

        drawn.sort_by_key(|step| step.sub_command());
        expected.sort_by_key(|step| step.sub_command());
        assert_eq!(drawn, expected);
    }

    /// The captions of the two buttons that share a row say what each does to
    /// the directory: one writes a file, the other deletes several.
    #[test]
    fn the_shared_row_says_which_button_does_what() {
        assert_eq!(Step::GenerateReport.label(), "Generate report");
        assert_eq!(Step::ClearTemporary.label(), "Clear temporary data");
    }

    /// The sub-command is the contract with the analysis binary; a typo here is
    /// a button that reports "unrecognised subcommand" when it is pressed.
    #[test]
    fn the_report_runs_the_sub_command_the_binary_declares() {
        assert_eq!(Step::GenerateReport.sub_command(), "generate-report");
    }

    #[test]
    fn an_analysis_step_passes_both_flags() {
        let arguments = Step::Tysserand.arguments(Path::new("/cfg.yaml"), Path::new("/work"));
        assert_eq!(
            arguments,
            vec![
                "tysserand-network",
                "--file",
                "/cfg.yaml",
                "--working_dir",
                "/work"
            ]
        );
    }

    #[test]
    fn progress_lines_carry_their_description() {
        let parsed = parse_output_line("[QT_PROGRESS] current=7 total=9 desc=[PROCESS] Doing it");
        assert_eq!(
            parsed,
            OutputLine::Progress {
                current: 7,
                total: 9,
                description: "[PROCESS] Doing it".to_string(),
            }
        );
    }

    #[test]
    fn a_progress_line_without_a_description_still_parses() {
        assert_eq!(
            parse_output_line("[QT_PROGRESS] current=1 total=2"),
            OutputLine::Progress {
                current: 1,
                total: 2,
                description: String::new(),
            }
        );
    }

    #[test]
    fn a_malformed_progress_line_is_not_guessed_at() {
        assert_eq!(
            parse_output_line("[QT_PROGRESS] current=x total=2"),
            OutputLine::Plain
        );
        assert_eq!(parse_output_line("[QT_PROGRESS]"), OutputLine::Plain);
    }

    #[test]
    fn info_lines_are_trimmed() {
        assert_eq!(
            parse_output_line("[QT_INFO]   spaced   "),
            OutputLine::Info("spaced".to_string())
        );
    }

    #[test]
    fn durations_match_the_python_format() {
        assert_eq!(format_duration(0.5), "0.50 s");
        assert_eq!(format_duration(59.0), "59.00 s");
        assert_eq!(format_duration(60.0), "1 min 0 s");
        assert_eq!(format_duration(3600.0), "1 h 0 min 0 s");
    }

    #[test]
    fn the_buttons_appear_in_workflow_order() {
        let labels: Vec<&str> = Step::all().iter().map(|s| s.label()).collect();
        assert_eq!(labels[0], "Step 1 — Tysserand");
        assert_eq!(labels[2], "Step 3 — Niche Analysis");
        assert_eq!(
            *labels.last().unwrap(),
            "Clear temporary data",
            "clearing is still the last thing offered"
        );
    }

    /// A sweep links the configuration it launched to the results it produced
    /// through this line, and nothing else: the numbers are handed out by the
    /// register while the run is going, so nothing outside knows them before.
    #[test]
    fn a_run_line_carries_its_directory() {
        assert_eq!(
            parse_output_line("[QT_RUN] 1-1-3"),
            OutputLine::Run("1-1-3".to_string())
        );
        assert_eq!(
            parse_output_line("[QT_RUN]  ps-2  "),
            OutputLine::Run("ps-2".to_string())
        );
    }

    /// An empty one names nothing and is not guessed at.
    #[test]
    fn a_run_line_without_a_directory_is_plain_output() {
        assert_eq!(parse_output_line("[QT_RUN]"), OutputLine::Plain);
        assert_eq!(parse_output_line("[QT_RUN]   "), OutputLine::Plain);
    }

    /// And it is not mistaken for the lines that share its shape.
    #[test]
    fn a_run_line_is_not_an_info_line() {
        assert!(matches!(
            parse_output_line("[QT_INFO] 1-1-3"),
            OutputLine::Info(_)
        ));
    }
}
