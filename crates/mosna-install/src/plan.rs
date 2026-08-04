//! The list of actions an install performs.

use std::path::{Path, PathBuf};

/// One step of an install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    CreateDirectory(PathBuf),
    /// Copy a file and make it executable.
    CopyExecutable {
        from: PathBuf,
        to: PathBuf,
    },
    CopyFile {
        from: PathBuf,
        to: PathBuf,
    },
    WriteFile {
        path: PathBuf,
        contents: String,
    },
    /// Decode an icon and re-encode it as PNG at the theme's size.
    ConvertIcon {
        from: PathBuf,
        to: PathBuf,
    },
    /// Write a launcher outside the prefix: on the desktop, or in the menu.
    WriteShortcut {
        layout: mosna_paths::layout::Layout,
        kind: mosna_paths::layout::ShortcutKind,
        path: PathBuf,
    },
}

impl Action {
    /// The path this action writes, if any.
    pub fn target(&self) -> Option<&Path> {
        match self {
            Action::CreateDirectory(_) => None,
            Action::CopyExecutable { to, .. }
            | Action::CopyFile { to, .. }
            | Action::ConvertIcon { to, .. } => Some(to),
            Action::WriteFile { path, .. } | Action::WriteShortcut { path, .. } => Some(path),
        }
    }

    /// A line describing the action, for a dry run.
    pub fn describe(&self) -> String {
        match self {
            Action::CreateDirectory(path) => format!("create directory {}", path.display()),
            Action::CopyExecutable { from, to } => {
                format!(
                    "install {} -> {} (executable)",
                    from.display(),
                    to.display()
                )
            }
            Action::CopyFile { from, to } => {
                format!("install {} -> {}", from.display(), to.display())
            }
            Action::WriteFile { path, .. } => format!("write {}", path.display()),
            Action::ConvertIcon { from, to } => {
                format!("convert icon {} -> {}", from.display(), to.display())
            }
            Action::WriteShortcut { path, .. } => format!("create launcher {}", path.display()),
        }
    }

    /// Carry the action out.
    pub fn apply(&self) -> anyhow::Result<()> {
        match self {
            Action::CreateDirectory(path) => std::fs::create_dir_all(path)
                .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", path.display())),

            Action::CopyFile { from, to } => copy(from, to),

            Action::CopyExecutable { from, to } => {
                copy(from, to)?;
                make_executable(to)
            }

            Action::WriteFile { path, contents } => std::fs::write(path, contents)
                .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display())),

            Action::ConvertIcon { from, to } => crate::icon::convert(from, to),

            Action::WriteShortcut { layout, kind, path } => {
                crate::shortcut::write(layout, *kind, path)
            }
        }
    }
}

/// Copy a file, replacing any existing one so that a reinstall is an upgrade.
fn copy(from: &Path, to: &Path) -> anyhow::Result<()> {
    // Removing first avoids "text file busy" when the destination is a running
    // binary, which happens when the interface reinstalls itself.
    if to.exists() {
        let _ = std::fs::remove_file(to);
    }
    std::fs::copy(from, to)
        .map_err(|e| anyhow::anyhow!("cannot copy {} to {}: {e}", from.display(), to.display()))?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?
        .permissions();
    // rwxr-xr-x: runnable by everyone, writable only by its owner.
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
        .map_err(|e| anyhow::anyhow!("cannot make {} executable: {e}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> anyhow::Result<()> {
    // Windows decides by extension, so a copied `.exe` is already runnable.
    Ok(())
}

/// An ordered list of actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    actions: Vec<Action>,
}

impl Plan {
    pub fn new(actions: Vec<Action>) -> Self {
        Self { actions }
    }

    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    /// Every path the plan writes.
    pub fn written_paths(&self) -> impl Iterator<Item = &Path> {
        self.actions.iter().filter_map(Action::target)
    }

    /// One description per action.
    pub fn describe(&self) -> Vec<String> {
        self.actions.iter().map(Action::describe).collect()
    }

    /// Carry out every action in order, reporting what was done.
    pub fn apply(&self) -> anyhow::Result<Vec<String>> {
        let mut done = Vec::with_capacity(self.actions.len());
        for action in &self.actions {
            action.apply()?;
            done.push(action.describe());
        }
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_a_directory_has_no_target() {
        assert_eq!(Action::CreateDirectory(PathBuf::from("/a")).target(), None);
    }

    #[test]
    fn every_writing_action_reports_its_target() {
        let actions = [
            Action::CopyFile {
                from: "/a".into(),
                to: "/b".into(),
            },
            Action::CopyExecutable {
                from: "/a".into(),
                to: "/b".into(),
            },
            Action::WriteFile {
                path: "/b".into(),
                contents: String::new(),
            },
            Action::ConvertIcon {
                from: "/a".into(),
                to: "/b".into(),
            },
        ];
        for action in actions {
            assert_eq!(action.target(), Some(Path::new("/b")));
        }
    }

    #[test]
    fn writing_a_file_creates_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mosna.desktop");
        Action::WriteFile {
            path: path.clone(),
            contents: "[Desktop Entry]\n".into(),
        }
        .apply()
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[Desktop Entry]\n");
    }

    #[test]
    fn copying_replaces_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("new");
        let to = dir.path().join("old");
        std::fs::write(&from, b"new").unwrap();
        std::fs::write(&to, b"old").unwrap();

        Action::CopyFile {
            from,
            to: to.clone(),
        }
        .apply()
        .unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn copying_an_executable_sets_the_bit() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("mosna");
        let to = dir.path().join("installed");
        std::fs::write(&from, b"#!/bin/sh\n").unwrap();

        Action::CopyExecutable {
            from,
            to: to.clone(),
        }
        .apply()
        .unwrap();

        let mode = std::fs::metadata(&to).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755, "mode is {mode:o}");
    }

    #[test]
    fn a_failing_action_names_the_path() {
        let error = Action::CopyFile {
            from: "/nowhere/a".into(),
            to: "/nowhere/b".into(),
        }
        .apply()
        .unwrap_err();
        assert!(error.to_string().contains("/nowhere/a"), "{error}");
    }

    #[test]
    fn a_plan_reports_every_action_it_ran() {
        let dir = tempfile::tempdir().unwrap();
        let plan = Plan::new(vec![
            Action::CreateDirectory(dir.path().join("bin")),
            Action::WriteFile {
                path: dir.path().join("bin/note"),
                contents: "x".into(),
            },
        ]);
        let done = plan.apply().unwrap();
        assert_eq!(done.len(), 2);
        assert!(dir.path().join("bin/note").is_file());
    }
}
