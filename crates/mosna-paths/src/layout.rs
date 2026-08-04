//! Where an install puts each file, on each platform.

use std::path::{Path, PathBuf};

use crate::config_file::{APPLICATION_DIR, FILE_NAME};
use crate::Environment;

/// The name the desktop entry and the icon are known by.
pub const DESKTOP_ID: &str = "mosna";

/// The name a shortcut carries in a menu or on a desktop.
pub const DISPLAY_NAME: &str = "MOSNA";

/// Size of the installed icon, in pixels.
///
/// `hicolor` wants the size in the path, and 256 is what the shipped icon
/// carries at its largest.
pub const ICON_SIZE: u32 = 256;

/// Which conventions an install follows.
///
/// Carried explicitly rather than read from `cfg!` at each use, so both
/// layouts can be exercised from either platform's test suite — a Windows path
/// bug found only on Windows is a bug found too late.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Unix,
    Windows,
}

impl Platform {
    /// The platform this binary was built for.
    pub fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Unix
        }
    }

    /// Executable suffix.
    pub fn executable_suffix(self) -> &'static str {
        match self {
            Platform::Unix => "",
            Platform::Windows => ".exe",
        }
    }
}

/// What kind of file a shortcut is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKind {
    /// A freedesktop `.desktop` entry.
    DesktopEntry,
    /// A Windows shell link.
    WindowsLink,
}

/// A launcher placed outside the prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub path: PathBuf,
    pub kind: ShortcutKind,
}

/// The files an install writes.
///
/// Everything in [`Layout::all_paths`] lives inside the prefix, which is what
/// lets an uninstall remove exactly what was added. Shortcuts are deliberately
/// outside it and are listed separately by [`Layout::shortcuts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    prefix: PathBuf,
    platform: Platform,
}

impl Layout {
    /// A layout for the platform this binary was built for.
    pub fn new(prefix: impl AsRef<Path>) -> Self {
        Self::for_platform(prefix, Platform::current())
    }

    /// A layout for a named platform.
    pub fn for_platform(prefix: impl AsRef<Path>, platform: Platform) -> Self {
        Self {
            prefix: prefix.as_ref().to_path_buf(),
            platform,
        }
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn prefix(&self) -> &Path {
        &self.prefix
    }

    /// Where to install when the user says nothing.
    ///
    /// Neither default needs administrator rights: requiring `sudo` to try an
    /// application is a poor trade, and a user install is trivially reversible.
    pub fn default_prefix(environment: &Environment) -> Option<PathBuf> {
        Self::default_prefix_for(environment, Platform::current())
    }

    /// The default prefix of a named platform.
    pub fn default_prefix_for(environment: &Environment, platform: Platform) -> Option<PathBuf> {
        match platform {
            Platform::Unix => environment.home_dir().map(|home| home.join(".local")),
            Platform::Windows => environment
                .local_app_data
                .clone()
                .or_else(|| {
                    environment
                        .home_dir()
                        .map(|home| home.join("AppData/Local"))
                })
                .map(|local| local.join("Programs").join(DISPLAY_NAME)),
        }
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.prefix.join("bin")
    }

    /// The analysis binary, which the interface launches as a sub-process.
    pub fn analysis_binary(&self) -> PathBuf {
        self.bin_dir()
            .join(format!("mosna{}", self.platform.executable_suffix()))
    }

    /// The interface binary, which the shortcuts point at.
    pub fn interface_binary(&self) -> PathBuf {
        self.bin_dir()
            .join(format!("mosna-gui{}", self.platform.executable_suffix()))
    }

    /// The application's own data directory.
    pub fn share_dir(&self) -> PathBuf {
        self.prefix.join("share").join(APPLICATION_DIR)
    }

    /// The configuration shipped with the install.
    ///
    /// A first run copies this into the user's configuration directory; it is
    /// never edited in place, so a reinstall cannot lose the user's settings.
    pub fn shipped_config(&self) -> PathBuf {
        self.share_dir().join(FILE_NAME)
    }

    /// The icon.
    ///
    /// A PNG in the `hicolor` theme on Unix, where that is what desktop
    /// environments read; an `.ico` beside the data on Windows, because the
    /// shell reads nothing else for a shortcut.
    pub fn icon(&self) -> PathBuf {
        match self.platform {
            Platform::Unix => self
                .prefix
                .join("share/icons/hicolor")
                .join(format!("{ICON_SIZE}x{ICON_SIZE}"))
                .join("apps")
                .join(format!("{DESKTOP_ID}.png")),
            Platform::Windows => self.share_dir().join(format!("{DESKTOP_ID}.ico")),
        }
    }

    /// The application-menu entry, when it belongs inside the prefix.
    ///
    /// Only on Unix: the Windows Start Menu lives in the user's profile, so
    /// there it is a shortcut like any other.
    pub fn menu_entry(&self) -> Option<PathBuf> {
        match self.platform {
            Platform::Unix => Some(
                self.prefix
                    .join("share/applications")
                    .join(format!("{DESKTOP_ID}.desktop")),
            ),
            Platform::Windows => None,
        }
    }

    /// Launchers placed outside the prefix.
    ///
    /// The Python `setup.sh` puts one on the desktop, which is where a user
    /// actually looks; the same is done here, plus the Start Menu on Windows.
    /// An environment that names no home yields none rather than failing — the
    /// install itself still succeeds.
    pub fn shortcuts(&self, environment: &Environment) -> Vec<Shortcut> {
        let mut shortcuts = Vec::new();

        match self.platform {
            Platform::Unix => {
                if let Some(desktop) = environment.desktop_dir() {
                    shortcuts.push(Shortcut {
                        path: desktop.join(format!("{DESKTOP_ID}.desktop")),
                        kind: ShortcutKind::DesktopEntry,
                    });
                }
            }
            Platform::Windows => {
                let file = format!("{DISPLAY_NAME}.lnk");
                for directory in [environment.desktop_dir(), environment.start_menu_dir()]
                    .into_iter()
                    .flatten()
                {
                    shortcuts.push(Shortcut {
                        path: directory.join(&file),
                        kind: ShortcutKind::WindowsLink,
                    });
                }
            }
        }

        shortcuts
    }

    /// Every file an install creates inside the prefix.
    pub fn all_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![
            self.analysis_binary(),
            self.interface_binary(),
            self.shipped_config(),
            self.icon(),
        ];
        paths.extend(self.menu_entry());
        paths
    }

    /// Every directory an install needs inside the prefix.
    pub fn all_directories(&self) -> Vec<PathBuf> {
        let mut directories: Vec<PathBuf> = self
            .all_paths()
            .iter()
            .filter_map(|path| path.parent().map(PathBuf::from))
            .collect();
        directories.sort();
        directories.dedup();
        directories
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_executable_suffix_follows_the_platform() {
        assert_eq!(Platform::Unix.executable_suffix(), "");
        assert_eq!(Platform::Windows.executable_suffix(), ".exe");
    }

    #[test]
    fn the_directories_cover_every_file() {
        for platform in [Platform::Unix, Platform::Windows] {
            let layout = Layout::for_platform("/opt/mosna", platform);
            let directories = layout.all_directories();
            for path in layout.all_paths() {
                let parent = path.parent().unwrap().to_path_buf();
                assert!(
                    directories.contains(&parent),
                    "{platform:?}: {} is missing",
                    parent.display()
                );
            }
        }
    }

    #[test]
    fn the_windows_layout_has_no_menu_entry_in_the_prefix() {
        let layout = Layout::for_platform("C:/Programs/MOSNA", Platform::Windows);
        assert!(!layout
            .all_paths()
            .iter()
            .any(|path| path.extension().map(|e| e == "desktop").unwrap_or(false)));
    }

    #[test]
    fn a_relocated_desktop_is_honoured() {
        let environment = Environment {
            desktop_dir: Some(PathBuf::from("/home/someone/Bureau")),
            home: Some(PathBuf::from("/home/someone")),
            ..Default::default()
        };
        let shortcuts = Layout::for_platform("/opt", Platform::Unix).shortcuts(&environment);
        assert_eq!(
            shortcuts[0].path,
            PathBuf::from("/home/someone/Bureau/mosna.desktop")
        );
    }

    #[test]
    fn windows_falls_back_to_the_profile_when_appdata_is_unset() {
        let environment = Environment {
            user_profile: Some(PathBuf::from("C:/Users/someone")),
            ..Default::default()
        };
        let shortcuts =
            Layout::for_platform("C:/Programs/MOSNA", Platform::Windows).shortcuts(&environment);
        assert_eq!(shortcuts.len(), 2);
    }

    #[test]
    fn an_empty_environment_yields_no_shortcut_on_either_platform() {
        for platform in [Platform::Unix, Platform::Windows] {
            assert!(Layout::for_platform("/opt", platform)
                .shortcuts(&Environment::default())
                .is_empty());
        }
    }
}
