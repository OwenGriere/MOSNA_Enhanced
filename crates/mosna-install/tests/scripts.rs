//! Tests of the installer script, and of how the Windows installer fits beside it.
//!
//! The scripts are the first thing a user runs and the one part of the project
//! no compiler checks. What can be checked is that they are there, that they
//! hand over to the tested installer rather than reimplementing it, and — the
//! failure that actually happens — that the paths they pass still exist.

use std::path::{Path, PathBuf};

/// The project root, from this crate's manifest.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn script(name: &str) -> String {
    let path = root().join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

// ---------------------------------------------------------------------------
// Both platforms have an installer
// ---------------------------------------------------------------------------

#[test]
fn each_platform_has_an_installer() {
    for name in ["install.sh", "INSTALLATION.exe", "UNINSTALL.exe"] {
        assert!(root().join(name).is_file(), "{name} is missing");
    }
}

/// A shell script that is not executable has to be run as `bash install.sh`,
/// which is not what the manual tells the reader to type.
#[cfg(unix)]
#[test]
fn the_shell_script_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(root().join("install.sh"))
        .unwrap()
        .permissions()
        .mode();
    assert!(mode & 0o111 != 0, "install.sh is not executable: {mode:o}");
}

// ---------------------------------------------------------------------------
// It hands over to the tested installer
// ---------------------------------------------------------------------------

/// The script may not copy files itself: every decision belongs in
/// `mosna-install`, which has tests.
#[test]
fn the_script_delegates_to_the_installer() {
    {
        let name = "install.sh";
        let text = script(name);
        assert!(
            text.contains("mosna-install"),
            "{name} does not call the installer"
        );
        assert!(
            text.contains("--build-dir") && text.contains("--config"),
            "{name} does not tell the installer where the artefacts are"
        );
    }
}

/// It builds the two binaries the install needs.
#[test]
fn the_script_builds_what_it_installs() {
    {
        let name = "install.sh";
        let text = script(name);
        assert!(
            text.contains("--bin mosna "),
            "{name} misses the analysis binary"
        );
        assert!(text.contains("mosna-gui"), "{name} misses the interface");
        assert!(text.contains("--release"), "{name} builds a debug binary");
    }
}

/// Every option the manual mentions has to exist in the script it belongs to.
#[test]
fn the_documented_options_are_the_ones_the_script_accepts() {
    let shell = script("install.sh");
    for option in ["--prefix", "--dry-run", "--uninstall"] {
        assert!(
            shell.contains(option),
            "install.sh does not accept {option}"
        );
    }
}

/// `--uninstall` must work after a failed build, so the script may not build
/// first unconditionally.
#[test]
fn uninstalling_does_not_require_a_build() {
    {
        let name = "install.sh";
        let text = script(name).to_lowercase();
        let builds = text.find("cargo build").expect("no build at all");
        let guards = text.find("uninstall");
        assert!(
            guards.is_some_and(|position| position < builds),
            "{name} builds before checking whether it was asked to uninstall"
        );
    }
}

/// It explains how to get Rust rather than failing with `command not found`.
#[test]
fn a_missing_toolchain_is_explained() {
    {
        let name = "install.sh";
        assert!(
            script(name).contains("rustup"),
            "{name} does not say how to install the toolchain"
        );
    }
}

// ---------------------------------------------------------------------------
// The paths they pass
// ---------------------------------------------------------------------------

/// The artefacts the script points at must exist in the tree.
///
/// This is the test that catches a move: a script quietly passing a path that
/// no longer exists fails only on a user's machine, at install time.
#[test]
fn the_paths_the_scripts_pass_exist() {
    for relative in ["CONFIG/configuration.yaml", "assets/logo.ico"] {
        assert!(
            root().join(relative).is_file(),
            "{relative} is missing from the project"
        );
    }

    {
        let name = "install.sh";
        let text = script(name);
        assert!(
            text.contains("CONFIG/configuration.yaml"),
            "{name} does not ship the configuration"
        );
        assert!(
            !text.contains("../CONFIG"),
            "{name} still reaches outside the project for its configuration"
        );
    }
}

// ---------------------------------------------------------------------------
// Build prerequisites beyond Rust
// ---------------------------------------------------------------------------

/// Rust is not the only thing this needs.
///
/// It used to be fontconfig, which the `plotters` figure backend linked
/// against. The figures are drawn by a Python package now, so fontconfig is
/// gone from the dependency tree entirely and Python has taken its place —
/// and a machine with a perfectly good Rust toolchain and no interpreter must
/// be told so before it spends two minutes building.
#[test]
fn the_shell_script_checks_the_build_prerequisites() {
    let text = script("install.sh");
    assert!(
        text.contains("command -v cc"),
        "install.sh does not check for a C compiler"
    );
    assert!(
        text.contains("python3"),
        "install.sh does not check for a Python interpreter"
    );
}

/// And the renderer has to actually be handed to the installer, or the install
/// silently produces an application that analyses and draws nothing.
#[test]
fn the_script_installs_the_figure_renderer() {
    {
        let name = "install.sh";
        assert!(
            script(name).contains("--renderer"),
            "{name} does not pass the figure renderer to the installer"
        );
    }
}

/// And it must say what to type, per distribution family — "install Python"
/// is not an instruction anyone can follow blind.
#[test]
fn a_missing_prerequisite_names_the_command_that_installs_it() {
    let text = script("install.sh");
    for manager in ["apt", "dnf", "pacman"] {
        assert!(
            text.contains(manager),
            "install.sh gives no {manager} command"
        );
    }
}

/// Uninstalling builds nothing, so it must not demand the build tools — a user
/// cleaning up after a failed install may well be missing them.
#[test]
fn uninstalling_does_not_demand_the_build_prerequisites() {
    let text = script("install.sh");
    let checks = text
        .find("command -v cc")
        .expect("the prerequisite check is gone");
    let guard = text
        .find("skip_build")
        .expect("the uninstall guard is gone");
    assert!(
        guard < checks,
        "install.sh checks the build prerequisites before deciding whether it \
         is going to build at all"
    );
}

/// The manual and the README have to name the same prerequisites the script
/// does, or a reader prepares the wrong machine.
#[test]
fn the_readme_lists_the_same_prerequisites() {
    let readme = std::fs::read_to_string(root().join("README.md")).unwrap();
    assert!(
        readme.contains("Python"),
        "the README does not mention Python, which the figures now need"
    );
    assert!(
        readme.contains("3.11"),
        "the README does not say which Python version is needed"
    );
    assert!(
        !readme.contains("fontconfig"),
        "the README still asks for fontconfig, which nothing links against now"
    );
}

// ---------------------------------------------------------------------------
// The Windows installer, and each platform removing the other's files
// ---------------------------------------------------------------------------

/// What a Windows user double-clicks. It is committed, not built on the
/// machine: it has to be a real Windows program with a window, no console.
#[test]
fn the_windows_installers_are_windows_programs() {
    for name in ["INSTALLATION.exe", "UNINSTALL.exe"] {
        let bytes =
            std::fs::read(root().join(name)).unwrap_or_else(|_| panic!("{name} is missing"));
        assert_eq!(&bytes[..2], b"MZ", "{name} is not a Windows executable");
        let pe = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
        assert_eq!(&bytes[pe..pe + 4], b"PE\0\0");
        // IMAGE_SUBSYSTEM_WINDOWS_GUI, in the PE32+ optional header.
        let subsystem = u16::from_le_bytes(bytes[pe + 24 + 68..pe + 24 + 70].try_into().unwrap());
        assert_eq!(subsystem, 2, "{name} would open a console window");
    }
}

/// Without a manifest, Windows takes a program named INSTALLATION for a legacy
/// installer and runs it as an administrator — for a user who is not one, an
/// administrator's account, so MOSNA would land in someone else's profile.
#[test]
fn the_windows_installers_run_as_the_user() {
    for name in ["INSTALLATION.exe", "UNINSTALL.exe"] {
        let bytes = std::fs::read(root().join(name)).unwrap();
        let manifest = b"requestedExecutionLevel level=\"asInvoker\"";
        assert!(
            bytes
                .windows(manifest.len())
                .any(|window| window == manifest),
            "{name} carries no asInvoker manifest; rebuild it (see crates/mosna-setup/src/lib.rs)"
        );
    }
}

/// Neither installer deletes the other platform's files: a folder installed on
/// one system stays installable on the other.
#[test]
fn no_installer_deletes_the_other_platforms_files() {
    let text = script("install.sh");
    for name in ["INSTALLATION.exe", "UNINSTALL.exe"] {
        assert!(
            !text
                .lines()
                .any(|line| line.contains("rm ") && line.contains(name)),
            "install.sh deletes {name}"
        );
    }
    let setup = std::fs::read_to_string(root().join("crates/mosna-setup/src/install.rs")).unwrap();
    assert!(
        !setup.contains("\"install.sh\""),
        "INSTALLATION.exe deletes install.sh"
    );
}
