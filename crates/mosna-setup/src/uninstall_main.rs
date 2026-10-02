//! Entry point of `UNINSTALL.exe`.

#![cfg_attr(windows, windows_subsystem = "windows")]

use mosna_setup::relaunch::{self, Start};
use mosna_setup::uninstall_window;

fn main() {
    // Run from a copy, since the MOSNA folder it sits in may be deleted.
    let Start::Run(source) = relaunch::start("mosna-assistant-retrait.exe") else {
        return;
    };
    if let Err(error) = uninstall_window::show(source) {
        relaunch::report_window_error("Désinstallation de MOSNA Enhanced", "UNINSTALL.exe", &error);
    }
}
