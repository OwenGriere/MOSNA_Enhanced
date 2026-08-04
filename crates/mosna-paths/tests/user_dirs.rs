//! Tests of `~/.config/user-dirs.dirs`, written before the implementation.
//!
//! A localised desktop is called `Bureau`, `Escritorio` or `Schreibtisch`, and
//! the name lives in this file — the graphical session does not usually export
//! `XDG_DESKTOP_DIR` into the environment. Reading only the variable means the
//! launcher lands in an English `Desktop` folder the user never opens, which is
//! the same as not creating one.

use std::path::{Path, PathBuf};

use mosna_paths::user_dirs;

/// A configuration directory holding `user-dirs.dirs` with the given contents.
fn config_home_with(contents: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("user-dirs.dirs"), contents).unwrap();
    dir
}

const HOME: &str = "/home/someone";

fn desktop(config_home: &Path) -> Option<PathBuf> {
    user_dirs::desktop(config_home, Path::new(HOME))
}

// ---------------------------------------------------------------------------
// The ordinary case
// ---------------------------------------------------------------------------

/// What `xdg-user-dirs` writes on a French system.
#[test]
fn a_localised_desktop_is_found() {
    let config = config_home_with(
        r#"# This file is written by xdg-user-dirs-update
XDG_DESKTOP_DIR="$HOME/Bureau"
XDG_DOWNLOAD_DIR="$HOME/Téléchargements"
XDG_DOCUMENTS_DIR="$HOME/Documents"
"#,
    );
    assert_eq!(
        desktop(config.path()),
        Some(PathBuf::from("/home/someone/Bureau"))
    );
}

/// A desktop moved off the home directory entirely.
#[test]
fn an_absolute_path_is_taken_as_it_is() {
    let config = config_home_with("XDG_DESKTOP_DIR=\"/mnt/shared/desk\"\n");
    assert_eq!(
        desktop(config.path()),
        Some(PathBuf::from("/mnt/shared/desk"))
    );
}

/// The quotes are conventional, not required.
#[test]
fn the_quotes_are_optional() {
    let config = config_home_with("XDG_DESKTOP_DIR=$HOME/Bureau\n");
    assert_eq!(
        desktop(config.path()),
        Some(PathBuf::from("/home/someone/Bureau"))
    );
}

/// Whitespace around the assignment is tolerated, as the shell would.
#[test]
fn surrounding_whitespace_is_ignored() {
    let config = config_home_with("  XDG_DESKTOP_DIR = \"$HOME/Bureau\"  \n");
    assert_eq!(
        desktop(config.path()),
        Some(PathBuf::from("/home/someone/Bureau"))
    );
}

/// Only the desktop is of interest; the rest of the file is not our business.
#[test]
fn the_other_directories_are_ignored() {
    let config = config_home_with(
        "XDG_MUSIC_DIR=\"$HOME/Musique\"\nXDG_DESKTOP_DIR=\"$HOME/Bureau\"\nXDG_VIDEOS_DIR=\"$HOME/Vidéos\"\n",
    );
    assert_eq!(
        desktop(config.path()),
        Some(PathBuf::from("/home/someone/Bureau"))
    );
}

/// A commented-out line is not a setting.
#[test]
fn a_commented_line_is_not_read() {
    let config = config_home_with("# XDG_DESKTOP_DIR=\"$HOME/Bureau\"\n");
    assert_eq!(desktop(config.path()), None);
}

// ---------------------------------------------------------------------------
// Nothing to read
// ---------------------------------------------------------------------------

/// Most of the world has no such file; that is not an error.
#[test]
fn a_missing_file_yields_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(desktop(dir.path()), None);
}

#[test]
fn a_file_without_a_desktop_entry_yields_nothing() {
    let config = config_home_with("XDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\n");
    assert_eq!(desktop(config.path()), None);
}

/// A malformed file must not bring the installer down.
#[test]
fn a_malformed_file_is_survived() {
    for contents in [
        "XDG_DESKTOP_DIR\n",
        "XDG_DESKTOP_DIR=\n",
        "XDG_DESKTOP_DIR=\"\"\n",
        "\0\0\0",
        "",
    ] {
        let config = config_home_with(contents);
        assert_eq!(desktop(config.path()), None, "on input {contents:?}");
    }
}

/// `xdg-user-dirs` writes the home directory itself to mean "this user has no
/// desktop". Dropping a launcher into the home root would be worse than not
/// creating one.
#[test]
fn a_desktop_set_to_the_home_directory_means_there_is_none() {
    for value in ["\"$HOME\"", "\"$HOME/\"", "\"/home/someone\""] {
        let config = config_home_with(&format!("XDG_DESKTOP_DIR={value}\n"));
        assert_eq!(desktop(config.path()), None, "on value {value}");
    }
}

// ---------------------------------------------------------------------------
// Precedence
// ---------------------------------------------------------------------------

/// The environment variable, when a session does export it, still wins: it is
/// the more specific statement of the two.
#[test]
fn the_environment_variable_wins_over_the_file() {
    use mosna_paths::Environment;

    let config = config_home_with("XDG_DESKTOP_DIR=\"$HOME/Bureau\"\n");
    let environment = Environment {
        home: Some(PathBuf::from(HOME)),
        xdg_config_home: Some(config.path().to_path_buf()),
        desktop_dir: Some(PathBuf::from("/explicit")),
        ..Default::default()
    };
    assert_eq!(environment.desktop_dir(), Some(PathBuf::from("/explicit")));
}

/// Without the variable, the file decides — this is the case on the machines
/// that motivated all of the above.
#[test]
fn the_file_is_consulted_when_the_variable_is_absent() {
    use mosna_paths::Environment;

    let config = config_home_with("XDG_DESKTOP_DIR=\"$HOME/Bureau\"\n");
    let environment = Environment {
        home: Some(PathBuf::from(HOME)),
        xdg_config_home: Some(config.path().to_path_buf()),
        ..Default::default()
    };
    assert_eq!(
        environment.desktop_dir(),
        Some(PathBuf::from("/home/someone/Bureau"))
    );
}

/// And with neither, the English default: it is right far more often than it
/// is wrong, and there is nothing better to guess.
#[test]
fn without_a_variable_or_a_file_the_default_is_used() {
    use mosna_paths::Environment;

    let dir = tempfile::tempdir().unwrap();
    let environment = Environment {
        home: Some(PathBuf::from(HOME)),
        xdg_config_home: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    assert_eq!(
        environment.desktop_dir(),
        Some(PathBuf::from("/home/someone/Desktop"))
    );
}
