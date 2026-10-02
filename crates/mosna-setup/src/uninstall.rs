//! The uninstall, run on a worker thread while the window shows its progress.
//!
//! It removes what `INSTALLATION.exe` put on the machine: MOSNA itself and its
//! shortcuts, through `mosna-install`'s own uninstall; the build in the MOSNA
//! folder; then, as chosen, the prerequisites installed for it and the MOSNA
//! folder itself.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

use mosna_install::{Installer, Sources};
use mosna_paths::layout::{Layout, DISPLAY_NAME};
use mosna_paths::Environment;

use crate::install::{self, Event, NoWindow, Reporter};
use crate::prerequisites::{self, Prerequisite};

/// The log kept on disk, for when the window is gone.
pub fn log_file() -> PathBuf {
    std::env::temp_dir().join("mosna-uninstall.log")
}

/// What the user chose in the window.
#[derive(Debug, Clone)]
pub struct Choices {
    /// The MOSNA folder, when the uninstaller was found in one.
    pub root: Option<PathBuf>,
    /// Delete the MOSNA folder as a whole, not just its build.
    pub remove_folder: bool,
    pub prerequisites: Vec<Prerequisite>,
}

/// A prerequisite present on the machine, and whether the installer is known
/// to have put it there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    pub prerequisite: Prerequisite,
    pub installed_by_mosna: bool,
}

/// The prerequisites this uninstaller could remove.
///
/// Python only when it is the one the installer installs: any other may be
/// anything, and serve anything. The build tools only as the stand-alone
/// product, never a Visual Studio that happens to carry them.
pub fn found(root: Option<&Path>) -> Vec<Found> {
    let recorded = root.map(prerequisites::recorded).unwrap_or_default();
    Prerequisite::ALL
        .into_iter()
        .filter(|prerequisite| is_present(*prerequisite))
        .map(|prerequisite| Found {
            prerequisite,
            installed_by_mosna: recorded.contains(&prerequisite),
        })
        .collect()
}

fn is_present(prerequisite: Prerequisite) -> bool {
    match prerequisite {
        Prerequisite::Python => installed_python().is_some(),
        Prerequisite::Rust => rustup().is_some(),
        Prerequisite::BuildTools => build_tools_path().is_some(),
    }
}

/// Run the whole uninstall. Never panics the window: the outcome arrives as
/// `Done` or `Failed`.
pub fn run(choices: Choices, sender: Sender<Event>) {
    let mut report = Reporter::logging_to(sender, &log_file());
    let mut failures = Vec::new();

    report.step("Suppression de MOSNA et de ses raccourcis");
    let prefix = match remove_application(&mut report) {
        Ok(prefix) => prefix,
        Err(message) => {
            failures.push(message);
            PathBuf::new()
        }
    };

    if let Some(root) = &choices.root {
        let build = root.join("target");
        if build.is_dir() && !choices.remove_folder {
            report.step("Suppression de la compilation (dossier target)");
            match std::fs::remove_dir_all(&build) {
                Ok(()) => report.line(format!("supprimé : {}", build.display())),
                Err(e) => failures.push(format!(
                    "{} n'a pas pu être supprimé : {e}",
                    build.display()
                )),
            }
        }
    }

    for prerequisite in &choices.prerequisites {
        report.step(format!("Désinstallation : {}", prerequisite.label()));
        match remove_prerequisite(*prerequisite, &mut report) {
            Ok(()) => {
                if let Some(root) = &choices.root {
                    prerequisites::forget(root, *prerequisite);
                }
            }
            Err(message) => failures.push(message),
        }
    }

    if let (Some(root), true) = (&choices.root, choices.remove_folder) {
        report.step(format!("Suppression du dossier {}", root.display()));
        match std::fs::remove_dir_all(root) {
            Ok(()) => report.line(format!("supprimé : {}", root.display())),
            Err(e) => failures.push(format!(
                "{} n'a pas pu être supprimé entièrement : {e}. Fermez ce qui l'utilise \
                 (explorateur, terminal), puis supprimez-le à la main.",
                root.display()
            )),
        }
    }

    if failures.is_empty() {
        report.send(Event::Done(prefix));
    } else {
        report.send(Event::Failed(failures.join("\n")));
    }
}

// ---------------------------------------------------------------------------
// MOSNA itself
// ---------------------------------------------------------------------------

/// Remove the install, as `mosna-install --uninstall` would, and return where
/// it was.
fn remove_application(report: &mut Reporter) -> Result<PathBuf, String> {
    let mut environment = Environment::detect();
    // The installer put the desktop shortcut where the registry says the
    // desktop is, which OneDrive moves.
    if let Some(desktop) = install::desktop_folder() {
        environment.desktop_dir = Some(desktop);
    }
    let prefix = Layout::default_prefix(&environment).ok_or(
        "Le dossier d'installation de MOSNA est introuvable (LOCALAPPDATA n'est pas défini).",
    )?;
    let layout = Layout::new(&prefix);

    // Sources are only read by an install; an uninstall goes by the layout.
    let nowhere = PathBuf::new();
    let sources = Sources {
        analysis_binary: nowhere.clone(),
        interface_binary: nowhere.clone(),
        config: nowhere,
        icon: None,
        renderer: None,
    };
    let removed = Installer::new(layout, sources, environment.clone())
        .uninstall()
        .map_err(|e| {
            format!("MOSNA n'a pas pu être supprimé : {e}. Fermez MOSNA s'il est ouvert, puis relancez UNINSTALL.exe.")
        })?;
    for line in removed {
        report.line(line);
    }

    // An install made before the desktop was read from the registry put its
    // shortcut in the profile's own Desktop folder.
    if let Some(profile) = &environment.user_profile {
        let old = profile.join("Desktop").join(format!("{DISPLAY_NAME}.lnk"));
        if old.is_file() && std::fs::remove_file(&old).is_ok() {
            report.line(format!("removed {}", old.display()));
        }
    }

    // What the layout leaves: the folders above its own, when empty.
    for directory in [prefix.join("share"), prefix.clone()] {
        if directory.is_dir() && std::fs::remove_dir(&directory).is_ok() {
            report.line(format!("removed {}", directory.display()));
        }
    }
    Ok(prefix)
}

// ---------------------------------------------------------------------------
// The prerequisites
// ---------------------------------------------------------------------------

fn remove_prerequisite(prerequisite: Prerequisite, report: &mut Reporter) -> Result<(), String> {
    match prerequisite {
        Prerequisite::Python => remove_python(report),
        Prerequisite::Rust => remove_rust(report),
        Prerequisite::BuildTools => remove_build_tools(report),
    }
}

/// The Python the installer installs, for the user: winget and python.org's
/// installer both put it here.
fn installed_python() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let folder = PathBuf::from(local).join(r"Programs\Python\Python312");
    folder.join("python.exe").is_file().then_some(folder)
}

fn remove_python(report: &mut Reporter) -> Result<(), String> {
    if !install::winget_uninstall(report, install::PYTHON_PACKAGE) || installed_python().is_some() {
        // The installer that installed it also removes it.
        let installer = std::env::temp_dir().join("python-installer.exe");
        if !installer.is_file() {
            install::download(report, install::PYTHON_INSTALLER, &installer)?;
        }
        let mut command = Command::new(&installer);
        command.args(["/uninstall", "/quiet"]);
        install::stream(&mut command, report)?;
    }
    match installed_python() {
        None => Ok(()),
        Some(folder) => Err(format!(
            "Python n'a pas pu être désinstallé ({}). Désinstallez-le depuis Paramètres → Applications.",
            folder.display()
        )),
    }
}

fn rustup() -> Option<PathBuf> {
    let rustup = install::cargo_path()?.with_file_name(install::exe("rustup"));
    rustup.is_file().then_some(rustup)
}

fn remove_rust(report: &mut Reporter) -> Result<(), String> {
    let Some(rustup) = rustup() else {
        return Ok(());
    };
    let mut command = Command::new(&rustup);
    command.args(["self", "uninstall", "-y"]);
    install::stream(&mut command, report)?;
    if rustup.is_file() {
        Err(
            "Rust n'a pas pu être désinstallé. Fermez tout terminal ou éditeur qui l'utilise, \
             puis lancez « rustup self uninstall » dans un terminal."
                .into(),
        )
    } else {
        Ok(())
    }
}

/// Where the stand-alone build tools are installed, if they are.
fn build_tools_path() -> Option<PathBuf> {
    let vswhere =
        install::program_files_x86().join(r"Microsoft Visual Studio\Installer\vswhere.exe");
    let output = Command::new(vswhere)
        .args(["-products", "Microsoft.VisualStudio.Product.BuildTools"])
        .args(["-property", "installationPath"])
        .stderr(Stdio::null())
        .no_window()
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let path = text.lines().next()?.trim();
    (output.status.success() && !path.is_empty()).then(|| PathBuf::from(path))
}

fn remove_build_tools(report: &mut Reporter) -> Result<(), String> {
    report.line("Windows va demander une autorisation administrateur.");
    if !install::winget_uninstall(report, install::BUILD_TOOLS_PACKAGE)
        || build_tools_path().is_some()
    {
        if let Some(path) = build_tools_path() {
            let setup =
                install::program_files_x86().join(r"Microsoft Visual Studio\Installer\setup.exe");
            // As for installing, elevation is only asked for through the shell.
            let mut command = Command::new("cmd");
            command.raw_args(&format!(
                "/c start \"\" /wait \"{}\" uninstall --installPath \"{}\" --passive --norestart",
                setup.display(),
                path.display()
            ));
            install::stream(&mut command, report)?;
        }
    }
    if build_tools_path().is_none() {
        report.line(
            "Le « Visual Studio Installer » lui-même reste : il se désinstalle depuis \
             Paramètres → Applications s'il ne sert plus.",
        );
        Ok(())
    } else {
        Err("Les outils C++ de Microsoft n'ont pas pu être désinstallés. Désinstallez « Build Tools \
             for Visual Studio » depuis Paramètres → Applications."
            .into())
    }
}
