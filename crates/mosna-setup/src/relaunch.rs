//! Finding the MOSNA folder a program was started from — and leaving it.
//!
//! Both programs may remove the folder they sit in: the installer moves it,
//! the uninstaller can delete it. Windows will not move or delete a folder
//! while a program inside it runs, so each first starts a copy of itself from
//! the temporary folder, telling it where it came from, and exits.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::place;

/// Passed to the copy started from the temporary folder, naming the MOSNA
/// folder it came from.
const SOURCE_FLAG: &str = "--source";

/// How a program goes on once started.
pub enum Start {
    /// A copy has taken over; this one has nothing left to do.
    Relaunched,
    /// Run here, from this MOSNA folder if one was found.
    Run(Option<PathBuf>),
}

/// Find the MOSNA folder, relaunching from a copy named `copy_name` in the
/// temporary folder when running from inside it.
pub fn start(copy_name: &str) -> Start {
    // Explorer starts a program in its own folder, and Windows will not move
    // or delete a folder some process is using as its current directory.
    let _ = std::env::set_current_dir(std::env::temp_dir());

    let arguments: Vec<String> = std::env::args().collect();
    if let Some(at) = arguments.iter().position(|a| a == SOURCE_FLAG) {
        return Start::Run(arguments.get(at + 1).map(PathBuf::from));
    }
    let here = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from));
    if let Some(here) = here.as_ref().filter(|here| place::is_mosna(here)) {
        if relaunch_from_temp(here, copy_name) {
            return Start::Relaunched;
        }
    }
    Start::Run(here)
}

fn relaunch_from_temp(source: &Path, copy_name: &str) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let copy = std::env::temp_dir().join(copy_name);
    std::fs::copy(&exe, &copy).is_ok()
        && Command::new(&copy)
            .arg(SOURCE_FLAG)
            .arg(source)
            .current_dir(std::env::temp_dir())
            .spawn()
            .is_ok()
}

/// Without a console, an error returned by the window would vanish without a
/// word: the usual one is a machine whose graphics driver offers no OpenGL 2.
pub fn report_window_error(title: &str, program: &str, error: &dyn std::fmt::Display) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(format!(
            "La fenêtre n'a pas pu s'ouvrir :\n\n{error}\n\n\
             Mettez à jour le pilote de la carte graphique, puis relancez {program}."
        ))
        .show();
}
