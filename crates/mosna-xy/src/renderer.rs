//! Running the Python renderer.
//!
//! The analyses queue their figures and then hand the queue to
//! `python -m mosna_xy render`. The interpreter is found the same way the
//! interface finds the analysis binary — an override, then the environment the
//! installer created, then the bare name — and the sub-process's stdout is
//! inherited, so the `[QT_PROGRESS]` lines it prints reach the interface
//! through the same pipe the computation's did.

use std::path::{Path, PathBuf};

/// The module the interpreter is asked to run.
const MODULE: &str = "mosna_xy";

/// What running a command produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub success: bool,
    pub message: String,
}

/// How a command is run.
///
/// A trait, so the argument list and the failure reporting can be tested
/// without an interpreter — and so a test suite never depends on what happens
/// to be installed on the machine running it.
pub trait Run: Sync {
    fn run(&self, program: &Path, arguments: &[String]) -> std::io::Result<Outcome>;
}

/// Runs a real sub-process, letting its output through to ours.
#[derive(Debug, Default, Clone, Copy)]
pub struct Subprocess;

impl Run for Subprocess {
    fn run(&self, program: &Path, arguments: &[String]) -> std::io::Result<Outcome> {
        // Standard output is inherited: the renderer's progress lines are the
        // analysis's progress lines as far as the interface is concerned.
        // Standard error is captured, because that is what has to end up in
        // the message when something goes wrong.
        let output = std::process::Command::new(program)
            .args(arguments)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::piped())
            .output()?;

        let mut message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if message.is_empty() {
            message = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
        Ok(Outcome {
            success: output.status.success(),
            message,
        })
    }
}

/// The renderer: an interpreter, and what to ask it for.
pub struct Renderer<R: Run = Subprocess> {
    interpreter: PathBuf,
    formats: Vec<String>,
    /// Visible to the crate so the tests can assert on what was run without a
    /// real interpreter anywhere in sight.
    pub(crate) runner: R,
}

impl Renderer<Subprocess> {
    /// The interpreter this machine is set up to use.
    pub fn detect() -> Self {
        Self::with(
            mosna_paths::python::resolve(&mosna_paths::Environment::detect()),
            Subprocess,
        )
    }
}

impl<R: Run> Renderer<R> {
    /// A renderer with an explicit interpreter and runner.
    pub fn with(interpreter: impl Into<PathBuf>, runner: R) -> Self {
        Self {
            interpreter: interpreter.into(),
            formats: vec!["png".to_string(), "html".to_string()],
            runner,
        }
    }

    /// Which formats each figure is written in.
    pub fn formats(mut self, formats: &[&str]) -> Self {
        self.formats = formats.iter().map(|f| f.to_string()).collect();
        self
    }

    pub fn interpreter(&self) -> &Path {
        &self.interpreter
    }

    /// The argument list for drawing a queue.
    pub fn render_arguments(&self, queue: &Path) -> Vec<String> {
        vec![
            "-m".to_string(),
            MODULE.to_string(),
            "render".to_string(),
            queue.display().to_string(),
            "--formats".to_string(),
            self.formats.join(","),
        ]
    }

    /// Draw everything waiting in `queue`.
    pub fn render(&self, queue: &Path) -> anyhow::Result<()> {
        let outcome = self.invoke(&self.render_arguments(queue))?;
        if outcome.success {
            return Ok(());
        }
        Err(anyhow::anyhow!(
            "cannot draw the figures: {}",
            outcome.message
        ))
    }

    /// Ask the renderer to report itself, so a missing or broken installation
    /// is found before an analysis runs rather than after it.
    pub fn check(&self) -> anyhow::Result<String> {
        let arguments = vec!["-m".to_string(), MODULE.to_string(), "check".to_string()];
        let outcome = self.invoke(&arguments)?;
        if outcome.success {
            return Ok(outcome.message);
        }
        Err(anyhow::anyhow!(
            "the figure renderer is not usable: {}",
            outcome.message
        ))
    }

    /// Run the interpreter, turning the one failure everybody meets — no
    /// interpreter at all — into a message that names it and says what to set.
    fn invoke(&self, arguments: &[String]) -> anyhow::Result<Outcome> {
        self.runner
            .run(&self.interpreter, arguments)
            .map_err(|error| {
                anyhow::anyhow!(
                    "cannot run the Python interpreter {} ({error}).\n\
                 MOSNA draws its figures with the `xy` package. Install it, or \
                 point MOSNA_PYTHON at an interpreter that has it.",
                    self.interpreter.display()
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Records what it was asked to run, and answers as it was told to.
    struct Recording {
        outcome: Outcome,
        calls: Mutex<Vec<(PathBuf, Vec<String>)>>,
    }

    impl Recording {
        fn new(success: bool, message: &str) -> Self {
            Self {
                outcome: Outcome {
                    success,
                    message: message.to_string(),
                },
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl Run for Recording {
        fn run(&self, program: &Path, arguments: &[String]) -> std::io::Result<Outcome> {
            self.calls
                .lock()
                .unwrap()
                .push((program.to_path_buf(), arguments.to_vec()));
            Ok(self.outcome.clone())
        }
    }

    #[test]
    fn the_renderer_is_asked_for_the_queue_and_the_formats() {
        let renderer = Renderer::with("/usr/bin/python3", Recording::new(true, ""));
        let arguments = renderer.render_arguments(Path::new("/runs/.mosna-figures"));

        assert_eq!(
            arguments,
            vec![
                "-m",
                "mosna_xy",
                "render",
                "/runs/.mosna-figures",
                "--formats",
                "png,html"
            ]
        );
    }

    #[test]
    fn the_formats_can_be_narrowed() {
        let renderer =
            Renderer::with("/usr/bin/python3", Recording::new(true, "")).formats(&["png"]);
        assert!(renderer
            .render_arguments(Path::new("/q"))
            .contains(&"png".to_string()));
        assert!(!renderer
            .render_arguments(Path::new("/q"))
            .contains(&"png,html".to_string()));
    }

    #[test]
    fn drawing_runs_the_interpreter_that_was_chosen() {
        let renderer = Renderer::with(
            "/opt/mosna/share/mosna/venv/bin/python3",
            Recording::new(true, ""),
        );
        renderer.render(Path::new("/runs/.mosna-figures")).unwrap();

        let calls = renderer.runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].0,
            Path::new("/opt/mosna/share/mosna/venv/bin/python3")
        );
    }

    /// The message has to say what could not be drawn *and* what the renderer
    /// said about it: "the analysis failed" with no cause is a bug report
    /// nobody can act on.
    #[test]
    fn a_failed_render_reports_what_the_renderer_said() {
        let renderer = Renderer::with(
            "python3",
            Recording::new(false, "cannot draw 00007-network: no such colour"),
        );
        let error = renderer
            .render(Path::new("/runs/.mosna-figures"))
            .unwrap_err();
        let message = error.to_string();

        assert!(message.contains("00007-network"), "{message}");
        assert!(message.contains("figures"), "{message}");
    }

    /// A missing interpreter is the one failure everybody will hit at least
    /// once, and "No such file or directory (os error 2)" names nothing the
    /// reader can fix.
    #[test]
    fn a_missing_interpreter_says_which_one_and_what_to_do() {
        struct Absent;
        impl Run for Absent {
            fn run(&self, _: &Path, _: &[String]) -> std::io::Result<Outcome> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "not found",
                ))
            }
        }

        let renderer = Renderer::with("/nowhere/python3", Absent);
        let message = renderer.render(Path::new("/q")).unwrap_err().to_string();

        assert!(message.contains("/nowhere/python3"), "{message}");
        assert!(message.contains("MOSNA_PYTHON"), "{message}");
    }

    #[test]
    fn checking_asks_the_renderer_to_name_itself() {
        let renderer = Renderer::with("python3", Recording::new(true, "mosna-xy 0.1.0\nxy 0.0.6"));
        let reported = renderer.check().unwrap();

        assert!(reported.contains("xy 0.0.6"));
        let calls = renderer.runner.calls.lock().unwrap();
        assert_eq!(calls[0].1, vec!["-m", "mosna_xy", "check"]);
    }

    #[test]
    fn a_renderer_that_cannot_be_checked_fails_rather_than_reporting_nothing() {
        let renderer = Renderer::with("python3", Recording::new(false, "ModuleNotFoundError: xy"));
        let message = renderer.check().unwrap_err().to_string();
        assert!(message.contains("ModuleNotFoundError"), "{message}");
    }
}
