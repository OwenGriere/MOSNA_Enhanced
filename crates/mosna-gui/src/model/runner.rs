//! Launching an analysis and reading its output — port of
//! `ScriptRunnerThread` and `MosnaGUI._on_output_line`.

use std::path::Path;

/// The buttons of the action bar.
///
/// Three analyses, and two things to do with the directory they filled:
/// describe it, or take the intermediates back out of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Tysserand,
    Assortativity,
    NicheAnalysis,
    GenerateReport,
    ClearTemporary,
}

impl Step {
    /// Every step, in the order the buttons appear.
    pub fn all() -> [Step; 5] {
        [
            Step::Tysserand,
            Step::Assortativity,
            Step::NicheAnalysis,
            Step::GenerateReport,
            Step::ClearTemporary,
        ]
    }

    /// The action bar, row by row.
    ///
    /// An analysis takes the full width: it is the thing the user came to do,
    /// and its caption is long. The last two share a row because they are the
    /// same kind of act — neither computes anything, both operate on a
    /// directory that is already full — and because putting the destructive one
    /// beside a harmless one, rather than alone under three buttons that all
    /// start work, is what stops it being pressed by reflex.
    ///
    /// Written here rather than in the panel so the arrangement can be stated
    /// as a test instead of discovered by looking.
    pub fn rows() -> Vec<Vec<Step>> {
        vec![
            vec![Step::Tysserand],
            vec![Step::Assortativity],
            vec![Step::NicheAnalysis],
            vec![Step::GenerateReport, Step::ClearTemporary],
        ]
    }

    /// The button's caption.
    pub fn label(self) -> &'static str {
        match self {
            Step::Tysserand => "Step 1 — Tysserand",
            Step::Assortativity => "Step 2 — Assortativity",
            Step::NicheAnalysis => "Step 3 — Niche Analysis",
            Step::GenerateReport => "Generate report",
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
        !matches!(self, Step::ClearTemporary | Step::GenerateReport)
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

    /// The action bar: the three analyses each on their own line, and the two
    /// operations on a finished directory sharing the last one.
    #[test]
    fn the_last_row_holds_the_two_operations_on_a_finished_directory() {
        let rows = Step::rows();

        assert_eq!(rows.len(), 4, "the bar is not four rows: {rows:?}");
        for row in &rows[..3] {
            assert_eq!(row.len(), 1, "an analysis shares its row");
        }
        assert_eq!(
            rows[3],
            vec![Step::GenerateReport, Step::ClearTemporary],
            "the report is not on the left of the clear"
        );
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
}
