//! Tests of the platform-aware install layout, written before the
//! implementation.
//!
//! An install has to land in the right place on each platform and leave a
//! shortcut where the user will actually look for it — on the desktop, as the
//! Python `setup.sh` does. Getting a path wrong here means an application that
//! installs successfully and cannot be found.

use std::path::{Path, PathBuf};

use mosna_paths::layout::{Layout, Platform, ShortcutKind};
use mosna_paths::Environment;

/// A Linux-shaped environment.
fn unix_environment(home: &Path) -> Environment {
    Environment {
        home: Some(home.to_path_buf()),
        ..Default::default()
    }
}

/// A Windows-shaped environment.
fn windows_environment(profile: &Path) -> Environment {
    Environment {
        home: Some(profile.to_path_buf()),
        user_profile: Some(profile.to_path_buf()),
        app_data: Some(profile.join("AppData/Roaming")),
        local_app_data: Some(profile.join("AppData/Local")),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Where each platform installs
// ---------------------------------------------------------------------------

/// A Unix install follows the filesystem hierarchy standard.
#[test]
fn the_unix_layout_follows_the_hierarchy_standard() {
    let layout = Layout::for_platform("/opt/mosna", Platform::Unix);

    assert_eq!(layout.analysis_binary(), Path::new("/opt/mosna/bin/mosna"));
    assert_eq!(
        layout.interface_binary(),
        Path::new("/opt/mosna/bin/mosna-gui")
    );
    assert_eq!(
        layout.shipped_config(),
        Path::new("/opt/mosna/share/mosna/configuration.yaml")
    );
    assert_eq!(
        layout.icon(),
        Path::new("/opt/mosna/share/icons/hicolor/256x256/apps/mosna.png")
    );
}

/// A Windows install keeps the executables' extension and its icon as an
/// `.ico`, which is the only format the shell reads for a shortcut.
#[test]
fn the_windows_layout_uses_windows_conventions() {
    let layout = Layout::for_platform("C:/Programs/MOSNA", Platform::Windows);

    assert!(
        layout
            .analysis_binary()
            .to_string_lossy()
            .ends_with("mosna.exe"),
        "got {:?}",
        layout.analysis_binary()
    );
    assert!(layout
        .interface_binary()
        .to_string_lossy()
        .ends_with("mosna-gui.exe"));
    assert!(
        layout.icon().extension().unwrap() == "ico",
        "the Windows shell cannot use a PNG for a shortcut, got {:?}",
        layout.icon()
    );
}

/// A menu entry only belongs inside the prefix on Unix; on Windows the Start
/// Menu lives in the user's profile, so it is a shortcut like any other.
#[test]
fn the_menu_entry_is_inside_the_prefix_only_on_unix() {
    let unix = Layout::for_platform("/opt/mosna", Platform::Unix);
    assert_eq!(
        unix.menu_entry(),
        Some(PathBuf::from("/opt/mosna/share/applications/mosna.desktop"))
    );

    let windows = Layout::for_platform("C:/Programs/MOSNA", Platform::Windows);
    assert_eq!(
        windows.menu_entry(),
        None,
        "the Start Menu is not under the prefix"
    );
}

/// Everything `all_paths` lists must sit inside the prefix, whatever the
/// platform: that is what lets an uninstall clean up without guessing.
#[test]
fn every_installed_path_is_inside_the_prefix() {
    for platform in [Platform::Unix, Platform::Windows] {
        let prefix = Path::new("/opt/mosna");
        let layout = Layout::for_platform(prefix, platform);
        for path in layout.all_paths() {
            assert!(
                path.starts_with(prefix),
                "{platform:?}: {} escapes the prefix",
                path.display()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Shortcuts
// ---------------------------------------------------------------------------

/// The Python `setup.sh` puts a launcher on the desktop. So does this.
#[test]
fn a_desktop_shortcut_is_created_on_unix() {
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::for_platform("/opt/mosna", Platform::Unix);

    let shortcuts = layout.shortcuts(&unix_environment(home.path()));
    assert_eq!(shortcuts.len(), 1, "expected one desktop shortcut");

    let shortcut = &shortcuts[0];
    assert_eq!(shortcut.kind, ShortcutKind::DesktopEntry);
    assert_eq!(shortcut.path, home.path().join("Desktop/mosna.desktop"));
}

/// `XDG_DESKTOP_DIR` moves the desktop; a localised install would otherwise
/// drop the launcher into a folder the user never opens.
#[test]
fn the_desktop_directory_can_be_relocated() {
    let home = tempfile::tempdir().unwrap();
    let mut environment = unix_environment(home.path());
    environment.desktop_dir = Some(home.path().join("Bureau"));

    let layout = Layout::for_platform("/opt/mosna", Platform::Unix);
    let shortcuts = layout.shortcuts(&environment);
    assert_eq!(shortcuts[0].path, home.path().join("Bureau/mosna.desktop"));
}

/// Windows gets two: the desktop and the Start Menu.
#[test]
fn windows_gets_a_desktop_and_a_start_menu_shortcut() {
    let profile = tempfile::tempdir().unwrap();
    let layout = Layout::for_platform("C:/Programs/MOSNA", Platform::Windows);

    let shortcuts = layout.shortcuts(&windows_environment(profile.path()));
    assert_eq!(shortcuts.len(), 2, "got {shortcuts:?}");

    assert!(shortcuts
        .iter()
        .all(|s| s.kind == ShortcutKind::WindowsLink));
    assert!(
        shortcuts
            .iter()
            .any(|s| s.path == profile.path().join("Desktop/MOSNA.lnk")),
        "no desktop shortcut in {shortcuts:?}"
    );
    assert!(
        shortcuts.iter().any(|s| s.path
            == profile
                .path()
                .join("AppData/Roaming/Microsoft/Windows/Start Menu/Programs/MOSNA.lnk")),
        "no Start Menu shortcut in {shortcuts:?}"
    );
}

/// Without a home directory there is nowhere to put a shortcut; the install
/// must go ahead regardless rather than refusing.
#[test]
fn an_environment_without_a_home_yields_no_shortcut() {
    let layout = Layout::for_platform("/opt/mosna", Platform::Unix);
    assert!(layout.shortcuts(&Environment::default()).is_empty());
}

/// A shortcut is outside the prefix by construction, so it must never appear in
/// `all_paths` — an uninstall handles the two lists separately.
#[test]
fn shortcuts_are_not_part_of_the_prefix() {
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::for_platform("/opt/mosna", Platform::Unix);
    let inside = layout.all_paths();

    for shortcut in layout.shortcuts(&unix_environment(home.path())) {
        assert!(
            !inside.contains(&shortcut.path),
            "{} is listed twice",
            shortcut.path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Default prefixes
// ---------------------------------------------------------------------------

/// A user install needs no administrator rights on either platform.
#[test]
fn the_default_prefix_needs_no_administrator_rights() {
    let home = tempfile::tempdir().unwrap();

    let unix = Layout::default_prefix_for(&unix_environment(home.path()), Platform::Unix);
    assert_eq!(unix, Some(home.path().join(".local")));

    let windows = Layout::default_prefix_for(&windows_environment(home.path()), Platform::Windows);
    assert_eq!(
        windows,
        Some(home.path().join("AppData/Local/Programs/MOSNA")),
        "a Windows user install belongs under LOCALAPPDATA"
    );
}

/// The platform detected at build time is the one used by default, so a caller
/// that says nothing gets the right layout.
#[test]
fn the_current_platform_matches_the_build_target() {
    let expected = if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Unix
    };
    assert_eq!(Platform::current(), expected);
    assert_eq!(Layout::new("/opt/mosna").platform(), expected);
}
