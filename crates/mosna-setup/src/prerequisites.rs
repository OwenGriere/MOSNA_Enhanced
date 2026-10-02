//! What the installer installed besides MOSNA, so the uninstaller removes that
//! and only that.
//!
//! The Microsoft C++ build tools, Rust and Python are installed only when they
//! are missing. One that was already there may serve other software, and is not
//! the uninstaller's to remove without asking. So each one the installer does
//! install is written down, in a file at the root of the MOSNA folder.

use std::path::Path;

/// The record, at the root of the MOSNA folder.
pub const RECORD_FILE: &str = ".mosna-prerequisites";

/// Something installed so that MOSNA could be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prerequisite {
    Python,
    Rust,
    BuildTools,
}

impl Prerequisite {
    /// In the order they are best removed: the build tools last, being the
    /// longest and the only one that asks for an administrator.
    pub const ALL: [Prerequisite; 3] = [
        Prerequisite::Python,
        Prerequisite::Rust,
        Prerequisite::BuildTools,
    ];

    /// The name in the record file.
    pub fn key(self) -> &'static str {
        match self {
            Prerequisite::Python => "python",
            Prerequisite::Rust => "rust",
            Prerequisite::BuildTools => "build-tools",
        }
    }

    /// The name shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            Prerequisite::Python => "Python 3.12",
            Prerequisite::Rust => "Rust (rustup, cargo)",
            Prerequisite::BuildTools => "Outils C++ de Microsoft (Build Tools for Visual Studio)",
        }
    }
}

/// What the record says was installed.
pub fn recorded(root: &Path) -> Vec<Prerequisite> {
    let text = std::fs::read_to_string(root.join(RECORD_FILE)).unwrap_or_default();
    Prerequisite::ALL
        .into_iter()
        .filter(|prerequisite| text.lines().any(|line| line.trim() == prerequisite.key()))
        .collect()
}

fn write(root: &Path, list: &[Prerequisite]) {
    let file = root.join(RECORD_FILE);
    if list.is_empty() {
        let _ = std::fs::remove_file(file);
        return;
    }
    let text: String = list
        .iter()
        .map(|prerequisite| format!("{}\n", prerequisite.key()))
        .collect();
    // Losing the record only means the uninstaller asks rather than proposes:
    // not worth failing an install over.
    let _ = std::fs::write(file, text);
}

/// Note that the installer installed `prerequisite`.
pub fn record(root: &Path, prerequisite: Prerequisite) {
    let mut list = recorded(root);
    if !list.contains(&prerequisite) {
        list.push(prerequisite);
        write(root, &list);
    }
}

/// Note that `prerequisite` is gone.
pub fn forget(root: &Path, prerequisite: Prerequisite) {
    let list: Vec<Prerequisite> = recorded(root)
        .into_iter()
        .filter(|recorded| *recorded != prerequisite)
        .collect();
    write(root, &list);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_prerequisite_is_read_back_once() {
        let dir = tempfile::tempdir().unwrap();
        assert!(recorded(dir.path()).is_empty());
        record(dir.path(), Prerequisite::Rust);
        record(dir.path(), Prerequisite::Rust);
        record(dir.path(), Prerequisite::BuildTools);
        assert_eq!(
            recorded(dir.path()),
            [Prerequisite::Rust, Prerequisite::BuildTools]
        );
    }

    #[test]
    fn forgetting_the_last_one_removes_the_record() {
        let dir = tempfile::tempdir().unwrap();
        record(dir.path(), Prerequisite::Python);
        forget(dir.path(), Prerequisite::Python);
        assert!(!dir.path().join(RECORD_FILE).exists());
    }

    #[test]
    fn an_unknown_line_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(RECORD_FILE), "rust\nsomething-else\n").unwrap();
        assert_eq!(recorded(dir.path()), [Prerequisite::Rust]);
    }
}
