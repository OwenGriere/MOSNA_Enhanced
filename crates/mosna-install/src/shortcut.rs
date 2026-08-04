//! Launchers placed outside the prefix: on the desktop, and in the menu.

use std::path::Path;

use mosna_paths::layout::{Layout, ShortcutKind};

/// Write a launcher of the right kind for the platform.
pub fn write(layout: &Layout, kind: ShortcutKind, path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", parent.display()))?;
    }

    match kind {
        ShortcutKind::DesktopEntry => write_desktop_entry(layout, path),
        ShortcutKind::WindowsLink => {
            write_windows_link(&layout.interface_binary(), Some(&layout.icon()), path)
        }
    }
}

/// Write a freedesktop launcher and make it executable.
///
/// GNOME and KDE refuse a desktop file on the desktop that is not executable —
/// they show "untrusted application launcher" and will not start it. A
/// launcher the user cannot double-click is no launcher at all.
fn write_desktop_entry(layout: &Layout, path: &Path) -> anyhow::Result<()> {
    std::fs::write(path, crate::desktop::entry(layout))
        .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display()))?;
    make_executable(path)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
        .map_err(|e| anyhow::anyhow!("cannot make {} executable: {e}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

/// Write a Windows shell link pointing at `target`.
///
/// Produced without any COM call, so the installer cross-compiles and its
/// output can be checked from Linux — the structure is asserted in the tests
/// even though the link can only be *used* on Windows.
pub fn write_windows_link(target: &Path, icon: Option<&Path>, path: &Path) -> anyhow::Result<()> {
    // A link to a missing target is a shortcut that opens nothing, and Windows
    // gives no useful error when it is clicked. Refuse it here instead.
    if !target.is_file() {
        anyhow::bail!("cannot link to {}: it does not exist", target.display());
    }

    let mut link = crate::shell_link::ShellLink::new(target.to_string_lossy())
        .with_name(mosna_paths::layout::DISPLAY_NAME);

    if let Some(directory) = target.parent() {
        link = link.with_working_dir(directory.to_string_lossy());
    }
    // A missing icon is not worth refusing a working shortcut over.
    if let Some(icon) = icon.filter(|candidate| candidate.is_file()) {
        link = link.with_icon(icon.to_string_lossy());
    }

    link.write(path)
        .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosna_paths::layout::Platform;

    #[test]
    fn a_desktop_entry_is_written_and_made_executable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Desktop/mosna.desktop");
        let layout = Layout::for_platform("/opt/mosna", Platform::Unix);

        write(&layout, ShortcutKind::DesktopEntry, &path).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("Exec=/opt/mosna/bin/mosna-gui"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0);
        }
    }

    #[test]
    fn the_parent_directory_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deeply/nested/mosna.desktop");
        let layout = Layout::for_platform("/opt/mosna", Platform::Unix);
        write(&layout, ShortcutKind::DesktopEntry, &path).unwrap();
        assert!(path.is_file());
    }

    #[test]
    fn a_link_names_the_missing_target() {
        let dir = tempfile::tempdir().unwrap();
        let error = write_windows_link(
            Path::new("/nowhere/mosna-gui.exe"),
            None,
            &dir.path().join("MOSNA.lnk"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("/nowhere/mosna-gui.exe"));
    }

    #[test]
    fn a_link_is_written_for_an_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("mosna-gui.exe");
        std::fs::write(&target, b"MZ").unwrap();
        let path = dir.path().join("MOSNA.lnk");

        write_windows_link(&target, None, &path).unwrap();
        assert!(path.is_file());
        assert!(std::fs::metadata(&path).unwrap().len() > 76);
    }

    #[test]
    fn a_missing_icon_is_skipped_rather_than_refused() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("mosna-gui.exe");
        std::fs::write(&target, b"MZ").unwrap();

        // An install without an icon still deserves a working shortcut.
        write_windows_link(
            &target,
            Some(Path::new("/nowhere/mosna.ico")),
            &dir.path().join("MOSNA.lnk"),
        )
        .unwrap();
        assert!(dir.path().join("MOSNA.lnk").is_file());
    }
}
