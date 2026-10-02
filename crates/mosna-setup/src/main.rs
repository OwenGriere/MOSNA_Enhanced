//! Entry point of `INSTALLATION.exe`.

// A double-clicked installer has no use for a console window behind it.
#![cfg_attr(windows, windows_subsystem = "windows")]

use mosna_setup::relaunch::{self, Start};
use mosna_setup::window;

fn main() {
    // No "setup" or "install" in the copy's name: Windows takes those for
    // legacy installers when they carry no manifest.
    let Start::Run(source) = relaunch::start("mosna-assistant.exe") else {
        return;
    };
    if let Err(error) = window::show(source) {
        relaunch::report_window_error("Installation de MOSNA Enhanced", "INSTALLATION.exe", &error);
    }
}
