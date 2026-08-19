//! Finding the Python interpreter that draws the figures.

use std::path::PathBuf;

use crate::layout::{Layout, Platform};
use crate::Environment;

/// Directory name of the virtual environment, inside the shared data folder.
pub const VENV_DIR: &str = "venv";

/// Interpreter file name inside a virtual environment, per platform.
///
/// `venv` writes `bin/python3` on Unix and `Scripts\\python.exe` on Windows.
/// Neither name exists on the other platform, so this is a fact about the
/// environment being read, not about the machine reading it.
pub fn venv_interpreter_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Unix => "python3",
        Platform::Windows => "python.exe",
    }
}

/// Directory holding the interpreter inside a virtual environment.
fn venv_bin_dir(platform: Platform) -> &'static str {
    match platform {
        Platform::Unix => "bin",
        Platform::Windows => "Scripts",
    }
}

/// The virtual environment the installer creates, under the install prefix.
///
/// Under the prefix rather than in the user's home: an install is what owns
/// it, two installs at different versions each want their own, and
/// uninstalling one must not take the other's renderer with it.
pub fn venv_dir(layout: &Layout) -> PathBuf {
    layout.share_dir().join(VENV_DIR)
}

/// The interpreter of the virtual environment the installer creates.
pub fn venv_interpreter(layout: &Layout) -> PathBuf {
    venv_dir(layout)
        .join(venv_bin_dir(layout.platform()))
        .join(venv_interpreter_name(layout.platform()))
}

/// Directory name of a checkout's own virtual environment.
pub const CHECKOUT_VENV: &str = ".venv";

/// How far up from the current directory a checkout's environment is looked
/// for.
///
/// `cargo test` runs each crate's tests from that crate's own directory, which
/// is two levels below the workspace root; four leaves room for a deeper
/// layout without turning the search into a walk to the file system root.
const SEARCH_DEPTH: usize = 4;

/// The interpreter of a `.venv` at the root of the checkout being worked in.
///
/// Run from a checkout, everything sits next to the sources — the same rule
/// [`crate::config_file::repository_path`] follows. Walking up matters because
/// `cargo test` runs from the crate's directory, not the workspace's.
pub fn checkout_interpreter(environment: &Environment) -> Option<PathBuf> {
    let mut directory = environment.current_dir.clone()?;
    for _ in 0..=SEARCH_DEPTH {
        let candidate = directory
            .join(CHECKOUT_VENV)
            .join(venv_bin_dir(Platform::current()))
            .join(venv_interpreter_name(Platform::current()));
        if candidate.is_file() {
            return Some(candidate);
        }
        directory = directory.parent()?.to_path_buf();
    }
    None
}

/// Every interpreter to try, most specific first:
///
/// 1. `MOSNA_PYTHON`, so a developer can point an installed application at the
///    environment they are working in;
/// 2. the virtual environment under the running executable's prefix, which is
///    what the installer creates and what makes the prefix relocatable;
/// 3. a `.venv` at the root of the checkout, for working from the sources;
/// 4. the bare name, leaving the shell to search `PATH`.
pub fn candidates(environment: &Environment) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(explicit) = &environment.mosna_python {
        candidates.push(explicit.clone());
    }
    if let Some(prefix) = environment.prefix() {
        candidates.push(venv_interpreter(&Layout::new(prefix)));
    }
    if let Some(checkout) = checkout_interpreter(environment) {
        candidates.push(checkout);
    }
    candidates.push(PathBuf::from(FALLBACK));
    candidates
}

/// The interpreter to run.
///
/// The first candidate that is a file, and the bare name if none is — with one
/// exception, which is the same one [`crate::binary::resolve_analysis`] makes:
/// an explicit override is taken verbatim *even when it does not exist*, so a
/// typo in `MOSNA_PYTHON` surfaces as "cannot start that" rather than being
/// silently swapped for another interpreter.
pub fn resolve(environment: &Environment) -> PathBuf {
    if let Some(explicit) = &environment.mosna_python {
        return explicit.clone();
    }

    candidates(environment)
        .into_iter()
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from(FALLBACK))
}

/// Bare interpreter name, left for the shell to find on `PATH`.
pub const FALLBACK: &str = if cfg!(windows) { "python" } else { "python3" };

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A virtual environment puts its interpreter in `bin` on Unix and in
    /// `Scripts` on Windows, and the two are not interchangeable — looking in
    /// the wrong one is how a working install reports "no Python".
    #[test]
    fn a_virtual_environment_hides_its_interpreter_somewhere_different_per_platform() {
        let unix = Layout::for_platform("/usr/local", Platform::Unix);
        assert_eq!(
            venv_interpreter(&unix),
            Path::new("/usr/local/share/mosna/venv/bin/python3")
        );

        let windows = Layout::for_platform("C:/Programs/MOSNA", Platform::Windows);
        assert_eq!(
            venv_interpreter(&windows),
            Path::new("C:/Programs/MOSNA/share/mosna/venv/Scripts/python.exe")
        );
    }

    /// An explicit override is taken verbatim and nothing else is consulted,
    /// which is what lets a developer point an installed interface at the
    /// environment they are working in.
    #[test]
    fn an_override_wins_over_everything() {
        let environment = Environment {
            mosna_python: Some(PathBuf::from("/nowhere/python")),
            exe_dir: Some(PathBuf::from("/usr/local/bin")),
            ..Default::default()
        };
        assert_eq!(candidates(&environment)[0], Path::new("/nowhere/python"));
        assert_eq!(resolve(&environment), Path::new("/nowhere/python"));
    }

    /// Installed, the interpreter sits under the same prefix as the binary
    /// that is looking for it — which is what lets the whole prefix be moved
    /// or installed twice at different versions.
    #[test]
    fn the_environment_beside_the_running_binary_comes_before_the_path() {
        let environment = Environment {
            exe_dir: Some(PathBuf::from("/opt/mosna/bin")),
            ..Default::default()
        };
        let candidates = candidates(&environment);
        let venv = venv_interpreter(&Layout::new("/opt/mosna"));
        assert_eq!(candidates[0], venv);
        assert_eq!(
            *candidates.last().unwrap(),
            Path::new(FALLBACK),
            "the last resort is always the bare name"
        );
    }

    /// With nothing installed and nothing overridden, the bare name is what
    /// is run: on a developer's machine that is the interpreter on `PATH`.
    #[test]
    fn an_empty_environment_falls_back_to_the_bare_name() {
        assert_eq!(resolve(&Environment::default()), Path::new(FALLBACK));
    }

    /// A candidate that does not exist is skipped rather than run: an install
    /// whose virtual environment was deleted must fall through to the next
    /// interpreter, not fail with "no such file".
    #[test]
    fn a_missing_candidate_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        let environment = Environment {
            exe_dir: Some(bin),
            ..Default::default()
        };
        assert_eq!(
            resolve(&environment),
            Path::new(FALLBACK),
            "nothing was installed, so nothing beside the binary can be run"
        );

        let layout = Layout::new(dir.path());
        let interpreter = venv_interpreter(&layout);
        std::fs::create_dir_all(interpreter.parent().unwrap()).unwrap();
        std::fs::write(&interpreter, b"#!/bin/sh\n").unwrap();
        assert_eq!(resolve(&environment), interpreter);
    }

    /// Working from a checkout, the environment beside the sources is the one
    /// meant — and it has to be found from a crate's own directory, because
    /// that is where `cargo test` runs.
    #[test]
    fn a_checkout_environment_is_found_from_a_crate_directory() {
        let dir = tempfile::tempdir().unwrap();
        let interpreter = dir
            .path()
            .join(CHECKOUT_VENV)
            .join(if cfg!(windows) { "Scripts" } else { "bin" })
            .join(if cfg!(windows) {
                "python.exe"
            } else {
                "python3"
            });
        std::fs::create_dir_all(interpreter.parent().unwrap()).unwrap();
        std::fs::write(&interpreter, b"#!/bin/sh\n").unwrap();

        let crate_dir = dir.path().join("crates/mosna-cli");
        std::fs::create_dir_all(&crate_dir).unwrap();

        let environment = Environment {
            current_dir: Some(crate_dir),
            ..Default::default()
        };
        assert_eq!(resolve(&environment), interpreter);
    }

    /// The walk is bounded: an interpreter twenty directories above the one
    /// being worked in belongs to something else.
    #[test]
    fn the_search_does_not_walk_to_the_root_of_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let interpreter = dir
            .path()
            .join(CHECKOUT_VENV)
            .join(if cfg!(windows) { "Scripts" } else { "bin" })
            .join(if cfg!(windows) {
                "python.exe"
            } else {
                "python3"
            });
        std::fs::create_dir_all(interpreter.parent().unwrap()).unwrap();
        std::fs::write(&interpreter, b"#!/bin/sh\n").unwrap();

        let deep = dir.path().join("a/b/c/d/e/f");
        std::fs::create_dir_all(&deep).unwrap();

        let environment = Environment {
            current_dir: Some(deep),
            ..Default::default()
        };
        assert_eq!(resolve(&environment), Path::new(FALLBACK));
    }

    #[test]
    fn the_fallback_is_the_name_that_exists_on_this_platform() {
        if cfg!(windows) {
            assert_eq!(FALLBACK, "python");
        } else {
            assert_eq!(FALLBACK, "python3");
        }
    }
}
