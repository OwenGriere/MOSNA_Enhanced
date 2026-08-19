//! Tests of the installer, written before the implementation.
//!
//! An installer that half-works is worse than none: it leaves files behind, or
//! overwrites the user's settings, or produces a menu entry that launches
//! nothing. Each of those is asserted here against a real temporary prefix.

use std::path::{Path, PathBuf};

use mosna_install::{desktop, plan::Action, Installer, Sources};
use mosna_paths::layout::Layout;
use mosna_paths::Environment;

/// An environment with a temporary home, so no test can write a launcher onto
/// the machine running it. `Installer` takes this explicitly for that reason.
fn sandboxed(home: &tempfile::TempDir) -> Environment {
    Environment {
        home: Some(home.path().to_path_buf()),
        ..Default::default()
    }
}

/// A build directory with plausible artefacts to install from.
struct Build {
    _dir: tempfile::TempDir,
    sources: Sources,
}

impl Build {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let analysis = root.join("mosna");
        let interface = root.join("mosna-gui");
        let config = root.join("configuration.yaml");
        std::fs::write(&analysis, b"#!/bin/sh\nexit 0\n").unwrap();
        std::fs::write(&interface, b"#!/bin/sh\nexit 0\n").unwrap();
        std::fs::write(&config, "Tysserand:\n  CPU: 4\n").unwrap();

        Self {
            _dir: dir,
            sources: Sources {
                analysis_binary: analysis,
                interface_binary: interface,
                config,
                // The icon is optional; the tests that need one point at the
                // repository's.
                icon: None,
                renderer: None,
            },
        }
    }

    fn with_icon(mut self, icon: PathBuf) -> Self {
        self.sources.icon = Some(icon);
        self
    }
}

fn repository_icon() -> Option<PathBuf> {
    let candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/logo.ico")
        .canonicalize()
        .ok()?;
    candidate.is_file().then_some(candidate)
}

// ---------------------------------------------------------------------------
// The desktop entry
// ---------------------------------------------------------------------------

/// The entry must be a valid desktop file pointing at the installed interface.
/// A relative `Exec` would launch nothing when the menu runs it from `/`.
#[test]
fn the_desktop_entry_launches_the_installed_interface() {
    let layout = Layout::new("/opt/mosna");
    let entry = desktop::entry(&layout);

    assert!(entry.starts_with("[Desktop Entry]"), "{entry}");
    assert!(entry.contains("Type=Application"));
    assert!(entry.contains("Name=MOSNA"));
    assert!(
        entry.contains("Exec=/opt/mosna/bin/mosna-gui"),
        "the Exec line must be absolute: {entry}"
    );
    assert!(entry.contains("Icon=mosna"));
    assert!(entry.contains("Terminal=false"));
    assert!(
        entry.contains("Categories=Science;Biology;"),
        "the entry must be filed under a category: {entry}"
    );
}

/// Every line after the header is a `key=value` pair; a stray line makes the
/// whole file invalid and the entry silently disappears from the menu.
#[test]
fn the_desktop_entry_is_well_formed() {
    let entry = desktop::entry(&Layout::new("/opt/mosna"));

    for line in entry.lines().skip(1).filter(|l| !l.trim().is_empty()) {
        assert!(
            line.contains('=') && !line.starts_with('='),
            "malformed line: `{line}`"
        );
    }
    assert!(entry.ends_with('\n'), "the file must end with a newline");
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// The plan must be inspectable before anything is written, which is what makes
/// a dry run possible.
#[test]
fn the_plan_lists_every_file_it_will_write() {
    let build = Build::new();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let installer = Installer::new(
        Layout::new(prefix.path()),
        build.sources.clone(),
        sandboxed(&home),
    );

    let plan = installer.plan();
    let written: Vec<&Path> = plan.written_paths().collect();

    let layout = Layout::new(prefix.path());
    assert!(written.contains(&layout.analysis_binary().as_path()));
    assert!(written.contains(&layout.interface_binary().as_path()));
    assert!(written.contains(&layout.menu_entry().unwrap().as_path()));
    assert!(written.contains(&layout.shipped_config().as_path()));
}

/// Directories come before the files inside them, or the copy fails.
///
/// Only for what lands inside the prefix: a launcher goes to the desktop,
/// whose folder is created by the launcher step itself because it belongs to
/// the user, not to the install.
#[test]
fn directories_are_created_before_their_contents() {
    let build = Build::new();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let installer = Installer::new(
        Layout::new(prefix.path()),
        build.sources.clone(),
        sandboxed(&home),
    );

    let plan = installer.plan();
    let mut seen_directories: Vec<PathBuf> = Vec::new();

    for action in plan.actions() {
        match action {
            Action::CreateDirectory(path) => seen_directories.push(path.clone()),
            other => {
                if let Some(target) = other.target() {
                    if !target.starts_with(prefix.path()) {
                        continue;
                    }
                    let parent = target.parent().unwrap().to_path_buf();
                    assert!(
                        seen_directories.contains(&parent),
                        "{} is written before its directory exists",
                        target.display()
                    );
                }
            }
        }
    }
}

/// A missing build artefact is caught while planning, before a half-install is
/// written to disk.
#[test]
fn a_missing_artefact_is_refused_before_anything_is_written() {
    let build = Build::new();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut sources = build.sources.clone();
    sources.analysis_binary = PathBuf::from("/nonexistent/mosna");

    let installer = Installer::new(Layout::new(prefix.path()), sources, sandboxed(&home));
    let error = installer.verify_sources().unwrap_err();
    assert!(error.to_string().contains("/nonexistent/mosna"), "{error}");

    // Nothing was created.
    assert!(!prefix.path().join("bin").exists());
}

// ---------------------------------------------------------------------------
// Installing
// ---------------------------------------------------------------------------

#[test]
fn installing_puts_every_file_in_place() {
    let build = Build::new();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(prefix.path());
    let installer = Installer::new(layout.clone(), build.sources.clone(), sandboxed(&home));

    installer.install().unwrap();

    for path in layout.all_paths() {
        // The icon is only installed when a source icon was given.
        if path == layout.icon() {
            continue;
        }
        assert!(path.is_file(), "{} was not installed", path.display());
    }
}

/// A binary that is not executable is a binary that does not run.
#[cfg(unix)]
#[test]
fn installed_binaries_are_executable() {
    use std::os::unix::fs::PermissionsExt;

    let build = Build::new();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(prefix.path());
    Installer::new(layout.clone(), build.sources.clone(), sandboxed(&home))
        .install()
        .unwrap();

    for binary in [layout.analysis_binary(), layout.interface_binary()] {
        let mode = std::fs::metadata(&binary).unwrap().permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "{} is not executable (mode {mode:o})",
            binary.display()
        );
    }
}

/// Installing twice must not fail, and must leave the same result: an upgrade
/// is just another install.
#[test]
fn installing_is_idempotent() {
    let build = Build::new();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(prefix.path());
    let installer = Installer::new(layout.clone(), build.sources.clone(), sandboxed(&home));

    installer.install().unwrap();
    installer.install().unwrap();

    assert!(layout.analysis_binary().is_file());
    assert!(layout.menu_entry().unwrap().is_file());
}

/// The user's own configuration lives elsewhere and must never be touched, so
/// an upgrade cannot lose their settings.
///
/// Everything an install writes is either inside the prefix or a launcher —
/// and a launcher is a `.desktop` or a `.lnk`, never a configuration.
#[test]
fn installing_never_writes_to_the_user_configuration() {
    let build = Build::new();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(prefix.path());

    let environment = sandboxed(&home);
    let installer = Installer::new(layout.clone(), build.sources.clone(), environment.clone());

    let launchers: Vec<_> = layout
        .shortcuts(&environment)
        .into_iter()
        .map(|shortcut| shortcut.path)
        .collect();

    for action in installer.plan().actions() {
        if let Some(target) = action.target() {
            assert!(
                target.starts_with(prefix.path()) || launchers.contains(&target.to_path_buf()),
                "{} is neither in the prefix nor a launcher",
                target.display()
            );
        }
    }

    // And no configuration of the user's is ever a target.
    let user_config = mosna_paths::config_file::user_path(&environment).unwrap();
    assert!(installer
        .plan()
        .written_paths()
        .all(|path| path != user_config));
}

/// The icon is converted to PNG, because desktop environments will not render
/// a Windows icon file from the `hicolor` theme.
#[test]
fn the_icon_is_converted_to_png() {
    let Some(source) = repository_icon() else {
        eprintln!("skipping: assets/logo.ico not found");
        return;
    };

    let build = Build::new().with_icon(source);
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(prefix.path());
    Installer::new(layout.clone(), build.sources.clone(), sandboxed(&home))
        .install()
        .unwrap();

    let icon = layout.icon();
    assert!(icon.is_file(), "the icon was not installed");

    let bytes = std::fs::read(&icon).unwrap();
    assert_eq!(
        &bytes[..8],
        b"\x89PNG\r\n\x1a\n",
        "the installed icon is not a PNG"
    );
}

// ---------------------------------------------------------------------------
// Uninstalling
// ---------------------------------------------------------------------------

/// An uninstall removes exactly what the install added.
#[test]
fn uninstalling_removes_what_was_installed() {
    let build = Build::new();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(prefix.path());
    let installer = Installer::new(layout.clone(), build.sources.clone(), sandboxed(&home));

    installer.install().unwrap();
    installer.uninstall().unwrap();

    for path in layout.all_paths() {
        assert!(!path.exists(), "{} survived the uninstall", path.display());
    }
}

/// A prefix shared with other software must come out intact: only our own
/// files go.
#[test]
fn uninstalling_leaves_other_software_alone() {
    let build = Build::new();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(prefix.path());

    Installer::new(layout.clone(), build.sources.clone(), sandboxed(&home))
        .install()
        .unwrap();

    let stranger = layout.bin_dir().join("someone-elses-tool");
    std::fs::write(&stranger, b"").unwrap();

    Installer::new(layout.clone(), build.sources.clone(), sandboxed(&home))
        .uninstall()
        .unwrap();

    assert!(
        stranger.is_file(),
        "the uninstall removed a file it did not install"
    );
}

/// Uninstalling something that was never installed is not an error; it is what
/// a user does after a failed install.
#[test]
fn uninstalling_an_absent_install_is_not_an_error() {
    let build = Build::new();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    Installer::new(
        Layout::new(prefix.path()),
        build.sources.clone(),
        sandboxed(&home),
    )
    .uninstall()
    .unwrap();
}

/// A dry run reports what it would do and touches nothing.
#[test]
fn a_dry_run_writes_nothing() {
    let build = Build::new();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(prefix.path());

    let report = Installer::new(layout.clone(), build.sources.clone(), sandboxed(&home))
        .dry_run()
        .unwrap();

    assert!(!report.is_empty(), "a dry run must describe its actions");
    assert!(
        !layout.bin_dir().exists(),
        "a dry run created {}",
        layout.bin_dir().display()
    );
}
