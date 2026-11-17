//! What a command shows while it runs: the interface's protocol, or bars.
//!
//! The interface launches these same commands and reads `[QT_INFO]`,
//! `[QT_PROGRESS]`, `[QT_RUN]` and `[QT_TIME]` lines off their stdout; it says
//! so by setting [`PROTOCOL_ENV`] to `qt`. Anywhere else the same reports are
//! shown the way `mosna sensitivity-analysis` shows its own — a heading, lines
//! above the bars for what happened, and two bars, the command and the step in
//! progress — so the two read alike in a terminal and in a batch job's log.

use std::io::IsTerminal;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use mosna_pipeline::{Progress, StdoutProgress};

/// Set to `qt` by the interface, which parses the protocol lines.
pub const PROTOCOL_ENV: &str = "MOSNA_PROGRESS";

/// Whether the process was started by the interface.
pub fn protocol_requested() -> bool {
    std::env::var(PROTOCOL_ENV).is_ok_and(|value| value.eq_ignore_ascii_case("qt"))
}

/// A progress message without the `[INFO]`-style tags meant for the interface.
pub fn clean(message: &str) -> String {
    let mut text = message.trim();
    while let Some(rest) = text.strip_prefix('[') {
        match rest.find(']') {
            Some(end) => text = rest[end + 1..].trim_start(),
            None => break,
        }
    }
    text.lines().next().unwrap_or_default().to_string()
}

/// The bars' look, shared with the sensitivity analysis.
pub fn style(template: &str) -> ProgressStyle {
    ProgressStyle::with_template(template)
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("█▓░")
}

/// The lower bar: the step in progress.
pub const STEP_TEMPLATE: &str =
    "  {spinner:.green} {bar:30.green/white} {pos:>5}/{len:5} {wide_msg}";

/// Print one line of the run's story.
///
/// Above the bars in a terminal. Anywhere else — a batch job's log file, a
/// pipe — the bars are not drawn at all, and the lines are the only record of
/// how far the command got, so they go to stdout as they are.
pub fn say(bars: &MultiProgress, interactive: bool, line: String) {
    if interactive {
        let _ = bars.println(line);
    } else {
        println!("{line}");
    }
}

/// Where a command's reports go.
pub trait Display: Progress + Send + Sync {
    /// One line of the figure renderer's stdout.
    fn renderer_line(&self, line: &str);
    /// The command is over.
    fn finish(&self, succeeded: bool);
}

/// The interface's protocol, unchanged.
pub struct Protocol;

impl Progress for Protocol {
    fn info(&self, message: &str) {
        StdoutProgress.info(message);
    }
    fn step(&self, current: usize, total: usize, description: &str) {
        StdoutProgress.step(current, total, description);
    }
    fn run_directory(&self, name: &str) {
        StdoutProgress.run_directory(name);
    }
    fn timing(&self, stage: &str, seconds: Option<f64>) {
        StdoutProgress.timing(stage, seconds);
    }
}

impl Display for Protocol {
    /// Passed on as it is: the renderer speaks the protocol already.
    fn renderer_line(&self, line: &str) {
        println!("{line}");
    }
    fn finish(&self, _succeeded: bool) {}
}

/// The step being shown on the lower bar.
struct Phase {
    /// As it was announced when the step began: the later reports append the
    /// sample just finished, which says nothing about the step as a whole.
    description: String,
    total: usize,
    last: usize,
    done: bool,
}

/// A heading, lines above two bars: the sensitivity analysis' layout.
pub struct Terminal {
    bars: MultiProgress,
    interactive: bool,
    command: ProgressBar,
    step: ProgressBar,
    phase: Mutex<Option<Phase>>,
}

impl Terminal {
    /// Print the heading and start the bars.
    ///
    /// `title` names the command, `subject` what it was given, and `details`
    /// follow on indented lines.
    pub fn start(title: &str, subject: &str, details: &[String]) -> Self {
        println!("{title} — {subject}");
        for line in details {
            println!("  {line}");
        }
        println!();

        // Bars only where someone is watching: indicatif draws nothing elsewhere.
        let interactive = std::io::stderr().is_terminal();
        let bars = MultiProgress::new();
        let command = bars.add(ProgressBar::new_spinner());
        command.set_style(style("{prefix:.bold} [{elapsed_precise}] {msg}"));
        command.set_prefix(title.to_string());
        command.enable_steady_tick(Duration::from_millis(500));
        let step = bars.add(ProgressBar::new(1));
        step.set_style(style(STEP_TEMPLATE));
        step.set_message("starting");
        step.enable_steady_tick(Duration::from_millis(120));
        Self {
            bars,
            interactive,
            command,
            step,
            phase: Mutex::new(None),
        }
    }

    fn say(&self, line: String) {
        say(&self.bars, self.interactive, line);
    }
}

impl Progress for Terminal {
    fn info(&self, message: &str) {
        let message = clean(message);
        if message.is_empty() {
            return;
        }
        self.command.set_message(message.clone());
        self.say(format!("▶ {message}"));
    }

    fn step(&self, current: usize, total: usize, description: &str) {
        let description = clean(description);
        let Ok(mut phase) = self.phase.lock() else {
            return;
        };
        let begins = match phase.as_ref() {
            None => true,
            Some(phase) => phase.total != total || current < phase.last || phase.done,
        };
        if begins {
            *phase = Some(Phase {
                description: description.clone(),
                total,
                last: current,
                done: false,
            });
            self.step.reset();
        }
        self.step.set_length(total.max(1) as u64);
        self.step.set_position(current as u64);
        self.step.set_message(description);

        if let Some(phase) = phase.as_mut() {
            phase.last = current;
            if total > 0 && current >= total && !phase.done {
                phase.done = true;
                self.say(format!("  ✓ {} — {total}/{total}", phase.description));
            }
        }
    }

    fn run_directory(&self, name: &str) {
        self.say(format!("▶ run {name}"));
    }

    // The computing times are given back as one table at the end.
}

impl Display for Terminal {
    /// The renderer's protocol lines, turned into the bars and lines above.
    fn renderer_line(&self, line: &str) {
        if let Some(message) = line.strip_prefix("[QT_INFO]") {
            self.info(message);
        } else if let Some(payload) = line.strip_prefix("[QT_PROGRESS]") {
            match parse_progress(payload) {
                Some((current, total, description)) => self.step(current, total, description),
                None => self.say(line.to_string()),
            }
        } else if !line.trim().is_empty() {
            self.say(line.to_string());
        }
    }

    fn finish(&self, succeeded: bool) {
        self.step.finish_and_clear();
        if succeeded {
            self.command.finish_with_message("done");
        } else {
            self.command.abandon_with_message("failed");
        }
    }
}

/// `current=3 total=12 desc=[MULTI PROCESS] Processing file`.
fn parse_progress(payload: &str) -> Option<(usize, usize, &str)> {
    let payload = payload.trim_start();
    let rest = payload.strip_prefix("current=")?;
    let (current, rest) = rest.split_once(' ')?;
    let rest = rest.trim_start().strip_prefix("total=")?;
    let (total, description) = match rest.split_once(' ') {
        Some((total, rest)) => (total, rest.trim_start().strip_prefix("desc=").unwrap_or("")),
        None => (rest, ""),
    };
    Some((
        current.parse().ok()?,
        total.trim().parse().ok()?,
        description,
    ))
}

/// The display for this process: the protocol under the interface, the
/// heading and bars anywhere else.
pub fn display(title: &str, subject: &str, details: &[String]) -> Arc<dyn Display> {
    if protocol_requested() {
        Arc::new(Protocol)
    } else {
        Arc::new(Terminal::start(title, subject, details))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interfaces_tags_are_taken_off_the_messages() {
        assert_eq!(clean("[MULTI PROCESS] Processing file"), "Processing file");
        assert_eq!(
            clean("[INFO] Parameters are read correctly\n"),
            "Parameters are read correctly"
        );
        assert_eq!(clean("plain"), "plain");
    }

    #[test]
    fn the_renderers_progress_lines_are_read() {
        assert_eq!(
            parse_progress(" current=1 total=3 desc=[PROCESS] Drawing figures"),
            Some((1, 3, "[PROCESS] Drawing figures"))
        );
        assert_eq!(parse_progress(" current=1 total=3"), Some((1, 3, "")));
        assert_eq!(parse_progress(" current=x total=3"), None);
        assert_eq!(parse_progress(""), None);
    }
}
