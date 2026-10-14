//! The two implementations, behind one way of starting a step.
//!
//! Both take the same flags (`--file`, `--working_dir`), read the same
//! configuration and print progress in the same protocol, so a step is a
//! command line and nothing more. Each runs in its own process: a niche
//! analysis lasts minutes, and a crash in one must not take the server down.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// MOSNA_Enhanced, the `mosna` binary.
    Rust,
    /// MOSNA_GUI, `python -m package.<module>`.
    Python,
}

impl Backend {
    pub fn parse(name: &str) -> anyhow::Result<Self> {
        match name {
            "rust" => Ok(Backend::Rust),
            "python" => Ok(Backend::Python),
            other => anyhow::bail!("unknown backend `{other}` (expected rust or python)"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Rust => "rust",
            Backend::Python => "python",
        }
    }

    /// Whether two runs of `step` on the same inputs give the same result.
    ///
    /// Only MOSNA_GUI's niche step is not: its UMAP runs with
    /// `random_state=None`. Delaunay and the seeded permutations are.
    pub fn deterministic(self, step: Step) -> bool {
        !(self == Backend::Python && step == Step::Niches)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Network,
    Assortativity,
    Niches,
}

impl Step {
    pub fn parse(name: &str) -> anyhow::Result<Self> {
        match name {
            "network" => Ok(Step::Network),
            "assortativity" => Ok(Step::Assortativity),
            "niches" => Ok(Step::Niches),
            other => {
                anyhow::bail!("unknown step `{other}` (expected network, assortativity or niches)")
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Step::Network => "network",
            Step::Assortativity => "assortativity",
            Step::Niches => "niches",
        }
    }

    /// The section of `configuration.yaml` the step reads.
    pub fn section(self) -> &'static str {
        match self {
            Step::Network => mosna_config::section::TYSSERAND,
            Step::Assortativity => mosna_config::section::ASSORTATIVITY,
            Step::Niches => mosna_config::section::NICHE_ANALYSIS,
        }
    }

    fn subcommand(self) -> &'static str {
        match self {
            Step::Network => "tysserand-network",
            Step::Assortativity => "assortativity",
            Step::Niches => "niche-analysis",
        }
    }

    fn module(self) -> &'static str {
        match self {
            Step::Network => "package.tysserand_network",
            Step::Assortativity => "package.assortativity",
            Step::Niches => "package.niche_analysis",
        }
    }
}

/// A file or directory, and how it was found — reported so that a wrong pick
/// can be traced to the rule that made it.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub path: PathBuf,
    pub found_by: String,
}

impl Found {
    fn new(path: PathBuf, found_by: impl Into<String>) -> Self {
        Self {
            path,
            found_by: found_by.into(),
        }
    }
}

/// Where each implementation is.
///
/// The server ships in both repositories, and a user may have downloaded only
/// one: each backend is looked for, and may be absent. Checks and comparisons
/// need neither; only `run_step` does.
#[derive(Debug, Clone)]
pub struct Locations {
    /// The `mosna` binary of MOSNA_Enhanced.
    pub mosna_bin: Option<Found>,
    /// The MOSNA_GUI checkout, whose `package/` is run with `python -m`.
    pub gui_dir: Option<Found>,
    /// The interpreter that runs it.
    pub python: Found,
}

/// The file whose presence marks a MOSNA_GUI checkout.
const GUI_MARKER: &str = "package/tysserand_network.py";

/// What discovery reads, so a test can hand it a world of its own.
pub struct Probe<'a> {
    pub var: &'a dyn Fn(&str) -> Option<std::ffi::OsString>,
    pub exe: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
}

impl Locations {
    pub fn from_env() -> Self {
        Self::discover(&Probe {
            var: &|name| std::env::var_os(name),
            exe: std::env::current_exe().ok(),
            cwd: std::env::current_dir().ok(),
        })
    }

    /// Find both implementations, most explicit source first:
    ///
    /// * `mosna`: `MOSNA_BIN`; next to this server (an install puts them side
    ///   by side, and so does `cargo build`); `target/release/mosna` in a
    ///   MOSNA_Enhanced checkout above or beside the server or the current
    ///   directory; `PATH`.
    /// * MOSNA_GUI: `MOSNA_GUI_DIR`; a directory holding `package/` above or
    ///   beside the server or the current directory.
    /// * Python: `MOSNA_GUI_PYTHON`; `MOSNA_PYTHON`, which MOSNA_GUI itself
    ///   reads; the checkout's `.venv`; `python3` on `PATH`.
    ///
    /// "Beside" means a sibling named `MOSNA_Enhanced` or `MOSNA_GUI`: the two
    /// checkouts usually sit in the same folder.
    pub fn discover(probe: &Probe) -> Self {
        let roots: Vec<PathBuf> = probe
            .exe
            .iter()
            .chain(probe.cwd.iter())
            .flat_map(|start| start.ancestors().map(Path::to_path_buf))
            .collect();

        let mosna_name = binary_name("mosna");
        let mosna_bin = (probe.var)("MOSNA_BIN")
            .map(|v| Found::new(PathBuf::from(v), "MOSNA_BIN"))
            .or_else(|| {
                let sibling = probe.exe.as_ref()?.parent()?.join(&mosna_name);
                sibling
                    .is_file()
                    .then(|| Found::new(sibling, "next to mosna-mcp"))
            })
            .or_else(|| {
                roots.iter().find_map(|root| {
                    [root.clone(), root.join("MOSNA_Enhanced")]
                        .into_iter()
                        .map(|dir| dir.join("target/release").join(&mosna_name))
                        .find(|path| path.is_file())
                        .map(|path| Found::new(path, "MOSNA_Enhanced checkout"))
                })
            })
            .or_else(|| on_path(probe, &mosna_name).map(|path| Found::new(path, "PATH")));

        let gui_dir = (probe.var)("MOSNA_GUI_DIR")
            .map(|v| Found::new(PathBuf::from(v), "MOSNA_GUI_DIR"))
            .or_else(|| {
                roots.iter().find_map(|root| {
                    [root.clone(), root.join("MOSNA_GUI")]
                        .into_iter()
                        .find(|dir| dir.join(GUI_MARKER).is_file())
                        .map(|dir| Found::new(dir, "MOSNA_GUI checkout"))
                })
            });

        let python_name = if cfg!(windows) {
            "python.exe"
        } else {
            "python3"
        };
        let venv_python = |dir: &Path| {
            let bin = if cfg!(windows) { "Scripts" } else { "bin" };
            dir.join(".venv").join(bin).join(python_name)
        };
        let python = (probe.var)("MOSNA_GUI_PYTHON")
            .map(|v| Found::new(PathBuf::from(v), "MOSNA_GUI_PYTHON"))
            .or_else(|| {
                (probe.var)("MOSNA_PYTHON").map(|v| Found::new(PathBuf::from(v), "MOSNA_PYTHON"))
            })
            .or_else(|| {
                let venv = venv_python(&gui_dir.as_ref()?.path);
                venv.is_file()
                    .then(|| Found::new(venv, "the checkout's .venv"))
            })
            .unwrap_or_else(|| {
                Found::new(
                    on_path(probe, python_name).unwrap_or_else(|| PathBuf::from(python_name)),
                    "PATH",
                )
            });

        Self {
            mosna_bin,
            gui_dir,
            python,
        }
    }

    /// What was found, for the `backends` tool.
    pub fn describe(&self) -> serde_json::Value {
        use serde_json::json;
        let found = |f: &Option<Found>| match f {
            Some(f) => json!({ "path": f.path, "found_by": f.found_by }),
            None => serde_json::Value::Null,
        };
        json!({
            "rust": {
                "available": self.mosna_bin.is_some(),
                "binary": found(&self.mosna_bin),
                "if_missing": "build MOSNA_Enhanced (cargo build --release -p mosna-cli) or \
                               install it, then set MOSNA_BIN to its `mosna` binary",
            },
            "python": {
                "available": self.gui_dir.is_some(),
                "project": found(&self.gui_dir),
                "interpreter": { "path": self.python.path, "found_by": self.python.found_by },
                "if_missing": "download MOSNA_GUI and set MOSNA_GUI_DIR to it; set \
                               MOSNA_GUI_PYTHON to the interpreter of its environment \
                               (e.g. the `mosna-GUI` conda env) if `python3` is not it",
            },
            "note": "the checks and comparisons work without either; only run_step needs one",
        })
    }

    /// The command that runs `step`, not yet started.
    pub fn command(
        &self,
        backend: Backend,
        step: Step,
        config: &Path,
        working_dir: &Path,
        figure_formats: Option<&str>,
        results_in: Option<&str>,
    ) -> anyhow::Result<Command> {
        let mut command = match backend {
            Backend::Rust => {
                let Some(mosna) = &self.mosna_bin else {
                    anyhow::bail!(
                        "the rust backend needs MOSNA_Enhanced's `mosna` binary, and none was \
                         found: build or install MOSNA_Enhanced and set MOSNA_BIN (see the \
                         `backends` tool)"
                    );
                };
                let mut command = Command::new(&mosna.path);
                command.arg(step.subcommand());
                command
            }
            Backend::Python => {
                let Some(gui_dir) = &self.gui_dir else {
                    anyhow::bail!(
                        "the python backend needs a MOSNA_GUI checkout and none was found: \
                         set MOSNA_GUI_DIR to the folder holding `package/` (see the \
                         `backends` tool)"
                    );
                };
                if figure_formats.is_some() || results_in.is_some() {
                    anyhow::bail!("figure_formats and results_in exist only in the rust backend");
                }
                let mut command = Command::new(&self.python.path);
                command
                    .current_dir(&gui_dir.path)
                    .env("PYTHONUNBUFFERED", "1")
                    .args(["-m", step.module()]);
                command
            }
        };
        command
            .arg("--file")
            .arg(config)
            .arg("--working_dir")
            .arg(working_dir);
        if let Some(formats) = figure_formats {
            command.arg("--figure-formats").arg(formats);
        }
        if let Some(results_in) = results_in {
            if step != Step::Niches {
                anyhow::bail!("results_in only applies to the niches step");
            }
            command.arg("--results-in").arg(results_in);
        }
        Ok(command)
    }
}

fn on_path(probe: &Probe, name: &str) -> Option<PathBuf> {
    let path = (probe.var)("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn binary_name(stem: &str) -> String {
    format!("{stem}{}", std::env::consts::EXE_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locations() -> Locations {
        Locations {
            mosna_bin: Some(Found::new("mosna".into(), "test")),
            gui_dir: Some(Found::new("/gui".into(), "test")),
            python: Found::new("python3".into(), "test"),
        }
    }

    fn argv(command: &Command) -> Vec<String> {
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn both_backends_take_the_same_flags() {
        let l = locations();
        let rust = l
            .command(
                Backend::Rust,
                Step::Niches,
                Path::new("c.yaml"),
                Path::new("/w"),
                None,
                None,
            )
            .unwrap();
        let python = l
            .command(
                Backend::Python,
                Step::Niches,
                Path::new("c.yaml"),
                Path::new("/w"),
                None,
                None,
            )
            .unwrap();
        assert_eq!(
            argv(&rust),
            [
                "mosna",
                "niche-analysis",
                "--file",
                "c.yaml",
                "--working_dir",
                "/w"
            ]
        );
        assert_eq!(
            argv(&python),
            [
                "python3",
                "-m",
                "package.niche_analysis",
                "--file",
                "c.yaml",
                "--working_dir",
                "/w"
            ]
        );
        assert_eq!(python.get_current_dir(), Some(Path::new("/gui")));
    }

    #[test]
    fn python_without_its_checkout_is_refused_with_the_variable_to_set() {
        let l = Locations {
            gui_dir: None,
            ..locations()
        };
        let error = l
            .command(
                Backend::Python,
                Step::Network,
                Path::new("c"),
                Path::new("w"),
                None,
                None,
            )
            .unwrap_err();
        assert!(error.to_string().contains("MOSNA_GUI_DIR"));
    }

    /// The layout of a desktop holding both checkouts side by side, with the
    /// server run from either one.
    #[test]
    fn each_checkout_finds_the_other_beside_it() {
        let desk = tempfile::tempdir().unwrap();
        let enhanced = desk.path().join("MOSNA_Enhanced");
        let gui = desk.path().join("MOSNA_GUI");
        std::fs::create_dir_all(enhanced.join("target/release")).unwrap();
        std::fs::write(
            enhanced.join("target/release").join(binary_name("mosna")),
            b"",
        )
        .unwrap();
        std::fs::create_dir_all(gui.join("package")).unwrap();
        std::fs::write(gui.join(GUI_MARKER), b"").unwrap();

        for cwd in [&enhanced, &gui] {
            let found = Locations::discover(&Probe {
                var: &|_| None,
                exe: None,
                cwd: Some(cwd.clone()),
            });
            assert_eq!(found.gui_dir.unwrap().path, gui);
            assert!(found.mosna_bin.unwrap().path.starts_with(&enhanced));
        }
    }

    #[test]
    fn a_lone_checkout_reports_the_other_backend_missing() {
        let desk = tempfile::tempdir().unwrap();
        let gui = desk.path().join("MOSNA_GUI");
        std::fs::create_dir_all(gui.join("package")).unwrap();
        std::fs::write(gui.join(GUI_MARKER), b"").unwrap();

        let found = Locations::discover(&Probe {
            var: &|_| None,
            exe: None,
            cwd: Some(gui.clone()),
        });
        assert!(found.mosna_bin.is_none());
        let error = found
            .command(
                Backend::Rust,
                Step::Network,
                Path::new("c"),
                Path::new("w"),
                None,
                None,
            )
            .unwrap_err();
        assert!(error.to_string().contains("MOSNA_BIN"));
        assert_eq!(found.describe()["python"]["available"], true);
    }
}
