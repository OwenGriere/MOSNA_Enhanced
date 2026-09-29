//! Entry point of `INSTALLATION.exe`.

// A double-clicked installer has no use for a console window behind it.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::Command;

use mosna_setup::{place, window};

/// Passed to the copy of this program started from the temporary folder,
/// naming the MOSNA folder it came from.
const SOURCE_FLAG: &str = "--source";

fn main() {
    // Explorer starts a program in its own folder, and Windows will not move
    // or delete a folder some process is using as its current directory.
    let _ = std::env::set_current_dir(std::env::temp_dir());

    let arguments: Vec<String> = std::env::args().collect();
    let source = match arguments.iter().position(|a| a == SOURCE_FLAG) {
        Some(at) => arguments.get(at + 1).map(PathBuf::from),
        None => {
            let here = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(PathBuf::from));
            // Windows will not move or delete a folder while a program inside
            // it runs, and moving this folder is the first thing done. So run
            // from a copy in the temporary folder instead.
            if let Some(here) = here.as_ref().filter(|here| place::is_mosna(here)) {
                if relaunch_from_temp(here) {
                    return;
                }
            }
            here
        }
    };
    // Without a console, an error returned here would vanish without a word:
    // the usual one is a machine whose graphics driver offers no OpenGL 2.
    if let Err(error) = window::show(source) {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Installation de MOSNA Enhanced")
            .set_description(format!(
                "La fenêtre d'installation n'a pas pu s'ouvrir :\n\n{error}\n\n\
                 Mettez à jour le pilote de la carte graphique, puis relancez INSTALLATION.exe."
            ))
            .show();
    }
}

fn relaunch_from_temp(source: &std::path::Path) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    // No "setup" or "install" in the name: Windows takes those for legacy
    // installers when they carry no manifest.
    let copy = std::env::temp_dir().join("mosna-assistant.exe");
    std::fs::copy(&exe, &copy).is_ok()
        && Command::new(&copy)
            .arg(SOURCE_FLAG)
            .arg(source)
            .current_dir(std::env::temp_dir())
            .spawn()
            .is_ok()
}
