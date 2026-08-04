//! Launching an analysis and reading its output — port of
//! `ScriptRunnerThread` and `MosnaGUI._on_output_line`.

use std::path::Path;

/// The four buttons of the action bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Tysserand,
    Assortativity,
    NicheAnalysis,
    ClearTemporary,
}

impl Step {
    /// Every step, in the order the buttons appear.
    pub fn all() -> [Step; 4] {
        [
            Step::Tysserand,
            Step::Assortativity,
            Step::NicheAnalysis,
            Step::ClearTemporary,
        ]
    }

    /// The button's caption.
    pub fn label(self) -> &'static str {
        match self {
            Step::Tysserand => "Step 1 — Tysserand",
            Step::Assortativity => "Step 2 — Assortativity",
            Step::NicheAnalysis => "Step 3 — Niche Analysis",
            Step::ClearTemporary => "Clear Temp Files",
        }
    }

    /// The `mosna` sub-command this step runs.
    pub fn sub_command(self) -> &'static str {
        match self {
            Step::Tysserand => "tysserand-network",
            Step::Assortativity => "assortativity",
            Step::NicheAnalysis => "niche-analysis",
            Step::ClearTemporary => "clear-temporary",
        }
    }

    /// The full argument list, flags included.
    ///
    /// Clearing the temporary files takes no configuration, matching
    /// `clear_temporary.py`, whose parser declares only `--working_dir`.
    pub fn arguments(self, config_path: &Path, working_dir: &Path) -> Vec<String> {
        let mut arguments = vec![self.sub_command().to_string()];
        if self != Step::ClearTemporary {
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
        assert_eq!(labels[3], "Clear Temp Files");
    }
}
