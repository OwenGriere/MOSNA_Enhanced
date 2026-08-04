//! Installs MOSNA into a prefix, and removes it again.
//!
//! The work is split into a *plan* — a list of actions, computed without
//! touching the disk — and its application. That is what makes a dry run
//! possible, lets the tests inspect what an install would do, and keeps the
//! uninstall honest: it removes exactly the paths the layout declares, and
//! nothing else.

pub mod desktop;
pub mod icon;
pub mod plan;
pub mod shell_link;
pub mod shortcut;

use std::path::PathBuf;

use mosna_paths::layout::Layout;
use mosna_paths::Environment;

pub use plan::{Action, Plan};

/// The build artefacts to install from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sources {
    pub analysis_binary: PathBuf,
    pub interface_binary: PathBuf,
    /// The configuration shipped as a starting point.
    pub config: PathBuf,
    /// Optional: without one, no icon is installed and the desktop entry falls
    /// back to whatever the theme provides.
    pub icon: Option<PathBuf>,
}

/// Installs and uninstalls a MOSNA prefix.
pub struct Installer {
    layout: Layout,
    sources: Sources,
    /// Decides where the launchers go; detected from the process by default.
    environment: Environment,
}

impl Installer {
    /// An installer writing into `layout`, taking its launcher locations from
    /// `environment`.
    ///
    /// The environment is a parameter rather than something detected inside,
    /// deliberately: an installer that reaches for the real `HOME` on its own
    /// will happily drop a launcher onto the developer's desktop the first time
    /// a test calls `install()`. Making it explicit means a caller cannot
    /// forget which desktop it is writing to — the compiler asks.
    pub fn new(layout: Layout, sources: Sources, environment: Environment) -> Self {
        Self {
            layout,
            sources,
            environment,
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Check every artefact exists, before anything is written.
    ///
    /// Discovering a missing binary halfway through leaves a prefix that looks
    /// installed and is not.
    pub fn verify_sources(&self) -> anyhow::Result<()> {
        let required = [
            ("analysis binary", &self.sources.analysis_binary),
            ("interface binary", &self.sources.interface_binary),
            ("configuration", &self.sources.config),
        ];
        for (what, path) in required {
            if !path.is_file() {
                anyhow::bail!("cannot find the {what} at {}", path.display());
            }
        }
        if let Some(icon) = &self.sources.icon {
            if !icon.is_file() {
                anyhow::bail!("cannot find the icon at {}", icon.display());
            }
        }
        Ok(())
    }

    /// What an install would do.
    pub fn plan(&self) -> Plan {
        let mut actions = Vec::new();

        for directory in self.layout.all_directories() {
            actions.push(Action::CreateDirectory(directory));
        }

        actions.push(Action::CopyExecutable {
            from: self.sources.analysis_binary.clone(),
            to: self.layout.analysis_binary(),
        });
        actions.push(Action::CopyExecutable {
            from: self.sources.interface_binary.clone(),
            to: self.layout.interface_binary(),
        });
        actions.push(Action::CopyFile {
            from: self.sources.config.clone(),
            to: self.layout.shipped_config(),
        });
        // The application-menu entry, where the platform keeps one inside the
        // prefix.
        if let Some(entry) = self.layout.menu_entry() {
            actions.push(Action::WriteFile {
                path: entry,
                contents: desktop::entry(&self.layout),
            });
        }
        if let Some(icon) = &self.sources.icon {
            actions.push(Action::ConvertIcon {
                from: icon.clone(),
                to: self.layout.icon(),
            });
        }

        // The launchers come last: they point at files that must already exist,
        // and a Windows link refuses a target that is not there yet.
        for shortcut in self.layout.shortcuts(&self.environment) {
            actions.push(Action::WriteShortcut {
                layout: self.layout.clone(),
                kind: shortcut.kind,
                path: shortcut.path,
            });
        }

        Plan::new(actions)
    }

    /// Describe the plan without carrying it out.
    pub fn dry_run(&self) -> anyhow::Result<Vec<String>> {
        self.verify_sources()?;
        Ok(self.plan().describe())
    }

    /// Carry out the install.
    pub fn install(&self) -> anyhow::Result<Vec<String>> {
        self.verify_sources()?;
        self.plan().apply()
    }

    /// Remove every file the layout declares, then any directory left empty.
    pub fn uninstall(&self) -> anyhow::Result<Vec<String>> {
        let mut done = Vec::new();

        let launchers = self
            .layout
            .shortcuts(&self.environment)
            .into_iter()
            .map(|shortcut| shortcut.path);

        for path in self.layout.all_paths().into_iter().chain(launchers) {
            if path.is_file() {
                std::fs::remove_file(&path)
                    .map_err(|e| anyhow::anyhow!("cannot remove {}: {e}", path.display()))?;
                done.push(format!("removed {}", path.display()));
            }
        }

        // Deepest first, so a directory is only considered once its children
        // are gone. `remove_dir` refuses a non-empty directory, which is
        // exactly the guard needed: a prefix shared with other software keeps
        // its own contents.
        let mut directories = self.layout.all_directories();
        directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for directory in directories {
            if directory.is_dir() && std::fs::remove_dir(&directory).is_ok() {
                done.push(format!("removed {}", directory.display()));
            }
        }

        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(dir: &std::path::Path) -> Sources {
        let analysis = dir.join("mosna");
        let interface = dir.join("mosna-gui");
        let config = dir.join("configuration.yaml");
        for path in [&analysis, &interface, &config] {
            std::fs::write(path, b"x").unwrap();
        }
        Sources {
            analysis_binary: analysis,
            interface_binary: interface,
            config,
            icon: None,
        }
    }

    #[test]
    fn verification_names_the_missing_artefact() {
        let dir = tempfile::tempdir().unwrap();
        let mut broken = sources(dir.path());
        broken.config = PathBuf::from("/nowhere/configuration.yaml");

        let installer = Installer::new(Layout::new(dir.path()), broken, Environment::default());
        let error = installer.verify_sources().unwrap_err();
        assert!(error.to_string().contains("configuration"), "{error}");
        assert!(error.to_string().contains("/nowhere"), "{error}");
    }

    #[test]
    fn a_missing_icon_is_also_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut broken = sources(dir.path());
        broken.icon = Some(PathBuf::from("/nowhere/logo.ico"));

        let installer = Installer::new(Layout::new(dir.path()), broken, Environment::default());
        assert!(installer.verify_sources().is_err());
    }

    #[test]
    fn the_plan_skips_the_icon_when_there_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let installer = Installer::new(
            Layout::new(dir.path()),
            sources(dir.path()),
            Environment::default(),
        );
        let plan = installer.plan();
        assert!(!plan
            .actions()
            .iter()
            .any(|action| matches!(action, Action::ConvertIcon { .. })));
    }

    #[test]
    fn uninstalling_reports_what_it_removed() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = tempfile::tempdir().unwrap();
        let installer = Installer::new(
            Layout::new(prefix.path()),
            sources(dir.path()),
            Environment::default(),
        );

        installer.install().unwrap();
        let removed = installer.uninstall().unwrap();
        assert!(
            removed.iter().any(|line| line.contains("mosna-gui")),
            "{removed:?}"
        );
    }
}
