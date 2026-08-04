//! Tests of the two installer scripts, written before `install.ps1` exists.
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
// Both platforms have a script
// ---------------------------------------------------------------------------

#[test]
fn each_platform_has_an_installer_script() {
    for name in ["install.sh", "install.ps1"] {
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
// They hand over to the tested installer
// ---------------------------------------------------------------------------

/// Neither script may copy files itself: every decision belongs in
/// `mosna-install`, which has tests.
#[test]
fn both_scripts_delegate_to_the_installer() {
    for name in ["install.sh", "install.ps1"] {
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

/// Both build the two binaries the install needs.
#[test]
fn both_scripts_build_what_they_install() {
    for name in ["install.sh", "install.ps1"] {
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
fn the_documented_options_are_the_ones_the_scripts_accept() {
    let shell = script("install.sh");
    for option in ["--prefix", "--dry-run", "--uninstall"] {
        assert!(
            shell.contains(option),
            "install.sh does not accept {option}"
        );
    }

    let powershell = script("install.ps1");
    for option in ["-Prefix", "-DryRun", "-Uninstall"] {
        assert!(
            powershell.contains(option),
            "install.ps1 does not accept {option}"
        );
    }
}

/// `--uninstall` must work after a failed build, so neither script may build
/// first unconditionally.
#[test]
fn uninstalling_does_not_require_a_build() {
    for name in ["install.sh", "install.ps1"] {
        // Case-folded, because the two scripts spell the flag differently and
        // what matters is only which comes first.
        let text = script(name).to_lowercase();
        let builds = text.find("cargo build").expect("no build at all");
        let guards = text.find("uninstall");
        assert!(
            guards.is_some_and(|position| position < builds),
            "{name} builds before checking whether it was asked to uninstall"
        );
    }
}

/// Both explain how to get Rust rather than failing with `command not found`.
#[test]
fn a_missing_toolchain_is_explained() {
    for name in ["install.sh", "install.ps1"] {
        assert!(
            script(name).contains("rustup"),
            "{name} does not say how to install the toolchain"
        );
    }
}

// ---------------------------------------------------------------------------
// The paths they pass
// ---------------------------------------------------------------------------

/// The artefacts the scripts point at must exist in the tree.
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

    for name in ["install.sh", "install.ps1"] {
        let text = script(name);
        assert!(
            text.contains("CONFIG/configuration.yaml")
                || text.contains("CONFIG\\configuration.yaml"),
            "{name} does not ship the configuration"
        );
        assert!(
            !text.contains("../CONFIG") && !text.contains("..\\CONFIG"),
            "{name} still reaches outside the project for its configuration"
        );
    }
}

// ---------------------------------------------------------------------------
// The Windows one-liner
// ---------------------------------------------------------------------------

/// `bootstrap.ps1` is fetched and executed straight from the network, so it
/// gets its own rules: it must survive having no arguments, must not reach for
/// anything over plain HTTP, and must hand over to the installer rather than
/// growing an installation of its own.
#[test]
fn the_windows_bootstrap_exists() {
    assert!(
        root().join("bootstrap.ps1").is_file(),
        "bootstrap.ps1 is missing"
    );
}

/// Piped into `iex`, a script receives no arguments at all. A mandatory
/// parameter would make it prompt — from a pipeline, that hangs.
#[test]
fn the_bootstrap_runs_with_no_arguments() {
    let text = script("bootstrap.ps1");
    assert!(
        !text.contains("Mandatory = $true") && !text.contains("Mandatory=$true"),
        "a mandatory parameter cannot be supplied through `irm | iex`"
    );
}

/// It installs the toolchain instead of failing on a machine that has none —
/// that is the entire reason for its existence.
#[test]
fn the_bootstrap_installs_the_toolchain_itself() {
    let text = script("bootstrap.ps1");
    assert!(text.contains("rustup"), "it does not install Rust");
    assert!(
        text.contains("cargo"),
        "it does not check whether Rust is already there"
    );
}

/// It fetches the sources, by either route: `git` when it is there, the zip
/// GitHub serves when it is not.
#[test]
fn the_bootstrap_fetches_the_sources_with_or_without_git() {
    let text = script("bootstrap.ps1");
    assert!(text.contains("git clone"), "no clone path");
    assert!(
        text.contains(".zip") || text.contains("Expand-Archive"),
        "no fallback for a machine without git"
    );
}

/// Everything it downloads comes over TLS. A bootstrap fetched over plain HTTP
/// is an invitation to run someone else's code.
#[test]
fn the_bootstrap_only_downloads_over_tls() {
    let text = script("bootstrap.ps1");
    assert!(
        !text.contains("http://"),
        "bootstrap.ps1 downloads something over plain HTTP"
    );
}

/// It ends by handing over, so there is exactly one implementation of the
/// install and it is the tested one.
#[test]
fn the_bootstrap_delegates_to_the_installer() {
    let text = script("bootstrap.ps1");
    assert!(
        text.contains("install.ps1"),
        "the bootstrap does not run the installer"
    );
}

/// An existing checkout is updated or refused, never silently overwritten:
/// the directory it picks may be one the user put something else in.
#[test]
fn the_bootstrap_does_not_clobber_an_existing_directory() {
    let text = script("bootstrap.ps1");
    assert!(
        text.contains("git pull") || text.contains("Force"),
        "it neither updates an existing checkout nor guards against one"
    );
}

/// The command the README tells people to paste must name the script that
/// actually exists, on the branch that actually exists. A stale one-liner is a
/// broken install for everyone who copies it.
#[test]
fn the_readme_one_liner_matches_the_shipped_script() {
    let readme = std::fs::read_to_string(root().join("README.md")).unwrap();
    let line = readme
        .lines()
        .find(|line| line.contains("bootstrap.ps1") && line.contains("iex"))
        .expect("the README does not give the one-line Windows install");

    assert!(
        line.contains("https://"),
        "the one-liner is not over TLS: {line}"
    );

    // The branch in the URL is the one the script itself declares, read from
    // its `$Branch = '...'` assignment rather than guessed.
    let script = script("bootstrap.ps1");
    let branch = script
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix("$Branch")?.trim_start();
            let value = rest.strip_prefix('=')?.trim();
            Some(value.trim_matches('\'').trim_matches('"').to_string())
        })
        .expect("bootstrap.ps1 does not say which branch it fetches");

    assert!(
        line.contains(&branch),
        "the README fetches a different branch from the `{branch}` the script \
         clones: {line}"
    );
}

// ---------------------------------------------------------------------------
// Build prerequisites beyond Rust
// ---------------------------------------------------------------------------

/// Rust is not the only thing the build needs.
///
/// `yeslogic-fontconfig-sys` and `freetype-sys` resolve their libraries through
/// `pkg-config`, so a machine with a perfectly good Rust toolchain and no
/// fontconfig headers fails in the middle of a two-minute build with an error
/// about `PKG_CONFIG_PATH` that names nothing a user could install. Checking
/// first costs nothing and turns that into one line of instruction.
#[test]
fn the_shell_script_checks_the_build_prerequisites() {
    let text = script("install.sh");
    assert!(
        text.contains("pkg-config"),
        "install.sh does not check for pkg-config"
    );
    assert!(
        text.contains("fontconfig"),
        "install.sh does not check for the fontconfig development files"
    );
}

/// And it must say what to type, per distribution family — "install fontconfig"
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
        .find("pkg-config")
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
        readme.contains("pkg-config"),
        "the README does not mention pkg-config"
    );
    assert!(
        readme.contains("fontconfig"),
        "the README does not mention fontconfig"
    );
}
