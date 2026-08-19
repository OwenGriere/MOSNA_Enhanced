//! Installing the Python figure renderer.
//!
//! MOSNA draws its figures with the `xy` charting library, which is Python.
//! The install builds a virtual environment of its own under the prefix and
//! installs the renderer into it, rather than touching whatever Python the
//! machine already has: two installs at different versions each want their
//! own, and a scientific workstation's Python is somebody's working
//! environment, not ours to write into.

use std::path::{Path, PathBuf};

use mosna_paths::layout::Layout;
use mosna_paths::python;

/// The oldest interpreter `xy` supports.
pub const MINIMUM_PYTHON: (u32, u32) = (3, 11);

/// What running a command produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub success: bool,
    pub output: String,
}

/// How a command is run, so the argument lists can be tested without a Python
/// anywhere in sight.
pub trait Run {
    fn run(&self, program: &Path, arguments: &[&str]) -> std::io::Result<Outcome>;
}

/// Runs a real sub-process and captures what it said.
#[derive(Debug, Default, Clone, Copy)]
pub struct Subprocess;

impl Run for Subprocess {
    fn run(&self, program: &Path, arguments: &[&str]) -> std::io::Result<Outcome> {
        let output = std::process::Command::new(program)
            .args(arguments)
            .output()?;
        let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let errors = String::from_utf8_lossy(&output.stderr);
        if !errors.trim().is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(errors.trim());
        }
        Ok(Outcome {
            success: output.status.success(),
            output: text,
        })
    }
}

/// Puts the renderer under a prefix.
pub struct RendererInstall {
    layout: Layout,
    /// The `python/` directory of the source tree.
    source: PathBuf,
}

impl RendererInstall {
    pub fn new(layout: Layout, source: impl Into<PathBuf>) -> Self {
        Self {
            layout,
            source: source.into(),
        }
    }

    /// Where the environment goes.
    pub fn venv_dir(&self) -> PathBuf {
        python::venv_dir(&self.layout)
    }

    /// The interpreter inside it, once it exists.
    pub fn venv_interpreter(&self) -> PathBuf {
        python::venv_interpreter(&self.layout)
    }

    /// Create the environment.
    pub fn create_arguments(&self) -> Vec<String> {
        vec![
            "-m".to_string(),
            "venv".to_string(),
            self.venv_dir().display().to_string(),
        ]
    }

    /// Install the renderer into it.
    ///
    /// `--no-input` because an installer that stops to ask a question nobody
    /// is watching for looks like a hang.
    pub fn install_arguments(&self) -> Vec<String> {
        vec![
            "-m".to_string(),
            "pip".to_string(),
            "install".to_string(),
            "--quiet".to_string(),
            "--no-input".to_string(),
            self.source.display().to_string(),
        ]
    }

    /// Whether an interpreter is new enough, from what `--version` printed.
    ///
    /// Anything unparseable is treated as too old: the alternative is
    /// installing against an interpreter whose version nobody could establish,
    /// and failing later with an error about a language feature.
    pub fn is_new_enough(reported: &str) -> bool {
        let digits: Vec<u32> = reported
            .split_whitespace()
            .find(|word| word.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .map(|version| {
                version
                    .split('.')
                    .filter_map(|part| part.parse().ok())
                    .collect()
            })
            .unwrap_or_default();

        match digits.as_slice() {
            [major, minor, ..] => (*major, *minor) >= MINIMUM_PYTHON,
            _ => false,
        }
    }

    /// Build the environment and install the renderer into it.
    pub fn install(&self, interpreter: &Path, runner: &impl Run) -> anyhow::Result<Vec<String>> {
        let version = runner
            .run(interpreter, &["--version"])
            .map_err(|error| self.no_interpreter(interpreter, &error))?;

        if !version.success || !Self::is_new_enough(&version.output) {
            anyhow::bail!(
                "the figure renderer needs Python {}.{} or newer; {} reports \"{}\".\n\
                 Install a newer Python, or point MOSNA_PYTHON at one.",
                MINIMUM_PYTHON.0,
                MINIMUM_PYTHON.1,
                interpreter.display(),
                version.output.trim()
            );
        }

        let create = self.create_arguments();
        let arguments: Vec<&str> = create.iter().map(String::as_str).collect();
        let created = runner
            .run(interpreter, &arguments)
            .map_err(|error| self.no_interpreter(interpreter, &error))?;
        if !created.success {
            anyhow::bail!(
                "cannot create the Python environment at {}: {}",
                self.venv_dir().display(),
                created.output
            );
        }

        let install = self.install_arguments();
        let arguments: Vec<&str> = install.iter().map(String::as_str).collect();
        let installed = runner
            .run(&self.venv_interpreter(), &arguments)
            .map_err(|error| self.no_interpreter(&self.venv_interpreter(), &error))?;
        if !installed.success {
            anyhow::bail!(
                "cannot install the figure renderer from {}: {}",
                self.source.display(),
                installed.output
            );
        }

        Ok(vec![format!(
            "installed the figure renderer into {}",
            self.venv_dir().display()
        )])
    }

    fn no_interpreter(&self, interpreter: &Path, error: &std::io::Error) -> anyhow::Error {
        anyhow::anyhow!(
            "cannot run {} ({error}).\n\
             MOSNA draws its figures with the Python package `xy`, so it needs a \
             Python {}.{} or newer to install it into.",
            interpreter.display(),
            MINIMUM_PYTHON.0,
            MINIMUM_PYTHON.1
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Scripted {
        outcomes: RefCell<Vec<Outcome>>,
        calls: RefCell<Vec<(PathBuf, Vec<String>)>>,
    }

    impl Scripted {
        fn new(outcomes: Vec<Outcome>) -> Self {
            Self {
                outcomes: RefCell::new(outcomes),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn succeeding() -> Self {
            Self::new(vec![
                Outcome {
                    success: true,
                    output: "Python 3.13.1".to_string(),
                },
                Outcome {
                    success: true,
                    output: String::new(),
                },
                Outcome {
                    success: true,
                    output: String::new(),
                },
            ])
        }
    }

    impl Run for Scripted {
        fn run(&self, program: &Path, arguments: &[&str]) -> std::io::Result<Outcome> {
            self.calls.borrow_mut().push((
                program.to_path_buf(),
                arguments.iter().map(|a| a.to_string()).collect(),
            ));
            let mut outcomes = self.outcomes.borrow_mut();
            if outcomes.is_empty() {
                return Ok(Outcome {
                    success: true,
                    output: String::new(),
                });
            }
            Ok(outcomes.remove(0))
        }
    }

    fn installer() -> RendererInstall {
        RendererInstall::new(Layout::new("/opt/mosna"), "/src/python")
    }

    #[test]
    fn the_environment_goes_under_the_prefix_with_everything_else() {
        assert_eq!(
            installer().venv_dir(),
            Path::new("/opt/mosna/share/mosna/venv")
        );
    }

    /// The renderer is installed *by the environment's own interpreter*, not
    /// by the one that created it: that is what puts the package inside the
    /// environment instead of in the user's Python.
    #[test]
    fn the_renderer_is_installed_by_the_interpreter_of_its_own_environment() {
        let installer = installer();
        let runner = Scripted::succeeding();

        installer
            .install(Path::new("/usr/bin/python3"), &runner)
            .unwrap();

        let calls = runner.calls.borrow();
        assert_eq!(calls[0].0, Path::new("/usr/bin/python3"));
        assert_eq!(calls[0].1, vec!["--version"]);
        assert_eq!(calls[1].0, Path::new("/usr/bin/python3"));
        assert!(calls[1].1.contains(&"venv".to_string()));
        assert_eq!(calls[2].0, installer.venv_interpreter());
        assert!(calls[2].1.contains(&"pip".to_string()));
        assert!(calls[2].1.contains(&"/src/python".to_string()));
    }

    #[test]
    fn a_python_too_old_is_named_along_with_what_is_needed() {
        let runner = Scripted::new(vec![Outcome {
            success: true,
            output: "Python 3.9.18".to_string(),
        }]);

        let error = installer()
            .install(Path::new("/usr/bin/python3"), &runner)
            .unwrap_err()
            .to_string();

        assert!(error.contains("3.11"), "{error}");
        assert!(error.contains("3.9.18"), "{error}");
        assert_eq!(
            runner.calls.borrow().len(),
            1,
            "nothing was built on an interpreter that cannot run it"
        );
    }

    #[test]
    fn the_version_is_read_from_what_python_prints() {
        assert!(RendererInstall::is_new_enough("Python 3.11.0"));
        assert!(RendererInstall::is_new_enough("Python 3.13.1"));
        assert!(RendererInstall::is_new_enough("Python 4.0"));
        assert!(!RendererInstall::is_new_enough("Python 3.10.14"));
        assert!(!RendererInstall::is_new_enough("Python 2.7.18"));
    }

    /// An interpreter whose version cannot be established is refused rather
    /// than assumed good: the alternative fails later, with an error about a
    /// language feature nobody would connect to this.
    #[test]
    fn an_unreadable_version_is_treated_as_too_old() {
        assert!(!RendererInstall::is_new_enough(""));
        assert!(!RendererInstall::is_new_enough("Python"));
        assert!(!RendererInstall::is_new_enough("something else entirely"));
    }

    #[test]
    fn a_missing_interpreter_says_what_it_was_for() {
        struct Absent;
        impl Run for Absent {
            fn run(&self, _: &Path, _: &[&str]) -> std::io::Result<Outcome> {
                Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no"))
            }
        }

        let error = installer()
            .install(Path::new("/nowhere/python3"), &Absent)
            .unwrap_err()
            .to_string();

        assert!(error.contains("/nowhere/python3"), "{error}");
        assert!(error.contains("xy"), "{error}");
    }

    #[test]
    fn a_failed_installation_reports_what_pip_said() {
        let runner = Scripted::new(vec![
            Outcome {
                success: true,
                output: "Python 3.13.1".to_string(),
            },
            Outcome {
                success: true,
                output: String::new(),
            },
            Outcome {
                success: false,
                output: "ERROR: No matching distribution found for xy==0.0.6".to_string(),
            },
        ]);

        let error = installer()
            .install(Path::new("/usr/bin/python3"), &runner)
            .unwrap_err()
            .to_string();

        assert!(error.contains("No matching distribution"), "{error}");
    }
}
