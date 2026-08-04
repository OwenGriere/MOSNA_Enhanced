//! Tests of the desktop and menu shortcuts, written before the implementation.
//!
//! The Python `setup.sh` finishes by putting a launcher on the desktop. An
//! install that skips that step technically succeeds and practically fails:
//! the user has no way to start the application.

use std::path::{Path, PathBuf};

use mosna_install::{shortcut, Installer, Sources};
use mosna_paths::layout::{Layout, Platform, ShortcutKind};
use mosna_paths::Environment;

/// Plausible build artefacts to install from.
fn sources(dir: &Path, platform: Platform) -> Sources {
    let suffix = platform.executable_suffix();
    let analysis = dir.join(format!("mosna{suffix}"));
    let interface = dir.join(format!("mosna-gui{suffix}"));
    let config = dir.join("configuration.yaml");
    for path in [&analysis, &interface, &config] {
        std::fs::write(path, b"x").unwrap();
    }
    Sources {
        analysis_binary: analysis,
        interface_binary: interface,
        config,
        icon: None,
    }
}

fn unix_environment(home: &Path) -> Environment {
    Environment {
        home: Some(home.to_path_buf()),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// The desktop entry written to the desktop
// ---------------------------------------------------------------------------

/// The launcher lands on the desktop, pointing at the installed interface.
#[test]
fn a_launcher_is_placed_on_the_desktop() {
    let build = tempfile::tempdir().unwrap();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("Desktop")).unwrap();

    let layout = Layout::for_platform(prefix.path(), Platform::Unix);
    let installer = Installer::new(
        layout.clone(),
        sources(build.path(), Platform::Unix),
        unix_environment(home.path()),
    );

    installer.install().unwrap();

    let launcher = home.path().join("Desktop/mosna.desktop");
    assert!(launcher.is_file(), "no launcher on the desktop");

    let contents = std::fs::read_to_string(&launcher).unwrap();
    assert!(contents.starts_with("[Desktop Entry]"));
    assert!(
        contents.contains(&format!("Exec={}", layout.interface_binary().display())),
        "the launcher does not point at the installed interface:\n{contents}"
    );
}

/// A desktop file that is not executable is refused by GNOME and KDE with
/// "untrusted application launcher"; the user then cannot start it by
/// double-clicking, which is the whole point of the shortcut.
#[cfg(unix)]
#[test]
fn the_desktop_launcher_is_executable() {
    use std::os::unix::fs::PermissionsExt;

    let build = tempfile::tempdir().unwrap();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("Desktop")).unwrap();

    Installer::new(
        Layout::for_platform(prefix.path(), Platform::Unix),
        sources(build.path(), Platform::Unix),
        unix_environment(home.path()),
    )
    .install()
    .unwrap();

    let mode = std::fs::metadata(home.path().join("Desktop/mosna.desktop"))
        .unwrap()
        .permissions()
        .mode();
    assert!(mode & 0o111 != 0, "mode is {mode:o}");
}

/// The desktop folder may not exist yet on a fresh account.
#[test]
fn a_missing_desktop_folder_is_created() {
    let build = tempfile::tempdir().unwrap();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();

    Installer::new(
        Layout::for_platform(prefix.path(), Platform::Unix),
        sources(build.path(), Platform::Unix),
        unix_environment(home.path()),
    )
    .install()
    .unwrap();

    assert!(home.path().join("Desktop/mosna.desktop").is_file());
}

/// Without a home directory there is nowhere to put a launcher. The install
/// must still finish: the binaries are what matter.
#[test]
fn an_install_without_a_home_still_succeeds() {
    let build = tempfile::tempdir().unwrap();
    let prefix = tempfile::tempdir().unwrap();
    let layout = Layout::for_platform(prefix.path(), Platform::Unix);

    Installer::new(
        layout.clone(),
        sources(build.path(), Platform::Unix),
        Environment::default(),
    )
    .install()
    .unwrap();

    assert!(layout.interface_binary().is_file());
}

/// Uninstalling takes the launcher away too, or the desktop keeps an icon that
/// launches nothing.
#[test]
fn uninstalling_removes_the_launcher() {
    let build = tempfile::tempdir().unwrap();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();

    let installer = Installer::new(
        Layout::for_platform(prefix.path(), Platform::Unix),
        sources(build.path(), Platform::Unix),
        unix_environment(home.path()),
    );

    installer.install().unwrap();
    assert!(home.path().join("Desktop/mosna.desktop").is_file());

    installer.uninstall().unwrap();
    assert!(!home.path().join("Desktop/mosna.desktop").exists());
}

/// A shortcut belonging to something else in the same folder is left alone.
#[test]
fn uninstalling_leaves_other_launchers_alone() {
    let build = tempfile::tempdir().unwrap();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();

    let installer = Installer::new(
        Layout::for_platform(prefix.path(), Platform::Unix),
        sources(build.path(), Platform::Unix),
        unix_environment(home.path()),
    );
    installer.install().unwrap();

    let stranger = home.path().join("Desktop/other-app.desktop");
    std::fs::write(&stranger, b"[Desktop Entry]\n").unwrap();

    installer.uninstall().unwrap();
    assert!(stranger.is_file(), "another launcher was removed");
}

// ---------------------------------------------------------------------------
// The Windows shell link
// ---------------------------------------------------------------------------

/// A `.lnk` must carry the shell link signature, or Explorer refuses it.
///
/// The file is produced on any platform — the installer is cross-compiled and
/// tested from Linux — so its structure can be checked here even though it can
/// only be *used* on Windows.
#[test]
fn a_windows_link_carries_the_shell_link_header() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("mosna-gui.exe");
    std::fs::write(&target, b"MZ").unwrap();
    let link = dir.path().join("MOSNA.lnk");

    shortcut::write_windows_link(&target, None, &link).unwrap();

    let bytes = std::fs::read(&link).unwrap();
    assert!(bytes.len() > 76, "a shell link header is 76 bytes");

    // ShellLinkHeader: HeaderSize = 0x4C, then the shell link class id.
    assert_eq!(&bytes[0..4], &[0x4C, 0x00, 0x00, 0x00], "wrong HeaderSize");
    assert_eq!(
        &bytes[4..20],
        &[
            0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x46
        ],
        "wrong LinkCLSID"
    );
}

/// The target path has to survive into the file, or the shortcut opens nothing.
#[test]
fn a_windows_link_records_its_target() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("mosna-gui.exe");
    std::fs::write(&target, b"MZ").unwrap();
    let link = dir.path().join("MOSNA.lnk");

    shortcut::write_windows_link(&target, None, &link).unwrap();

    let bytes = std::fs::read(&link).unwrap();
    let needle = target.to_string_lossy().into_owned();
    let ascii = bytes.windows(needle.len()).any(|w| w == needle.as_bytes());
    let utf16: Vec<u8> = needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let wide = bytes.windows(utf16.len()).any(|w| w == utf16);

    assert!(ascii || wide, "the target path is not in the link");
}

/// A missing target is refused rather than producing a link to nowhere.
#[test]
fn a_windows_link_to_a_missing_target_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let error = shortcut::write_windows_link(
        Path::new("/nonexistent/mosna-gui.exe"),
        None,
        &dir.path().join("MOSNA.lnk"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("mosna-gui.exe"), "{error}");
}

/// Both Windows shortcuts are planned, and neither sits inside the prefix.
#[test]
fn a_windows_install_plans_both_shortcuts() {
    let profile = tempfile::tempdir().unwrap();
    let environment = Environment {
        user_profile: Some(profile.path().to_path_buf()),
        app_data: Some(profile.path().join("AppData/Roaming")),
        ..Default::default()
    };

    let layout = Layout::for_platform("C:/Programs/MOSNA", Platform::Windows);
    let shortcuts = layout.shortcuts(&environment);

    assert_eq!(shortcuts.len(), 2);
    for shortcut in &shortcuts {
        assert_eq!(shortcut.kind, ShortcutKind::WindowsLink);
        assert!(
            !shortcut.path.starts_with("C:/Programs/MOSNA"),
            "{} should live in the user profile",
            shortcut.path.display()
        );
    }
}

/// A dry run must not create a shortcut either.
#[test]
fn a_dry_run_creates_no_launcher() {
    let build = tempfile::tempdir().unwrap();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();

    let report = Installer::new(
        Layout::for_platform(prefix.path(), Platform::Unix),
        sources(build.path(), Platform::Unix),
        unix_environment(home.path()),
    )
    .dry_run()
    .unwrap();

    assert!(
        report.iter().any(|line| line.contains("Desktop")),
        "the dry run should mention the launcher: {report:?}"
    );
    assert!(!home.path().join("Desktop/mosna.desktop").exists());
}

/// The whole plan, shortcuts included, is inspectable before anything happens.
#[test]
fn the_plan_includes_the_launcher() {
    let build = tempfile::tempdir().unwrap();
    let prefix = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();

    let installer = Installer::new(
        Layout::for_platform(prefix.path(), Platform::Unix),
        sources(build.path(), Platform::Unix),
        unix_environment(home.path()),
    );

    let written: Vec<PathBuf> = installer
        .plan()
        .written_paths()
        .map(Path::to_path_buf)
        .collect();
    assert!(written.contains(&home.path().join("Desktop/mosna.desktop")));
}
