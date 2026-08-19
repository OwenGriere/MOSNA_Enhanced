//! Where MOSNA looks for its configuration, its binaries and its data.
//!
//! Run from a checkout, everything sits next to the sources. Installed, it is
//! spread across `bin/`, `share/` and the user's configuration directory. This
//! crate is the single place that knows the difference, so the interface, the
//! command line tool and the installer cannot drift apart on it.
//!
//! Every lookup takes an [`Environment`] rather than reading the process
//! environment directly, which is what makes the precedence rules testable
//! without touching the developer's own machine.

pub mod binary;
pub mod config_file;
pub mod layout;
pub mod python;
pub mod user_dirs;

use std::path::PathBuf;

/// The pieces of the outside world path resolution depends on.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    /// Directory holding the running executable.
    pub exe_dir: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
    /// `MOSNA_CONFIG`: an explicit configuration file.
    pub mosna_config: Option<PathBuf>,
    /// `MOSNA_BIN`: an explicit analysis binary.
    pub mosna_bin: Option<PathBuf>,
    /// `MOSNA_PYTHON`: an explicit interpreter for the figure renderer.
    pub mosna_python: Option<PathBuf>,
    pub current_dir: Option<PathBuf>,

    /// `XDG_DESKTOP_DIR`, when the desktop has been relocated or localised.
    pub desktop_dir: Option<PathBuf>,
    /// `USERPROFILE`, the Windows equivalent of `HOME`.
    pub user_profile: Option<PathBuf>,
    /// `APPDATA`: roaming data, where the Start Menu lives.
    pub app_data: Option<PathBuf>,
    /// `LOCALAPPDATA`: machine-local data, where a user install belongs.
    pub local_app_data: Option<PathBuf>,
}

impl Environment {
    /// Read the real environment.
    pub fn detect() -> Self {
        let variable = |name: &str| std::env::var_os(name).map(PathBuf::from);

        Self {
            exe_dir: std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(PathBuf::from)),
            home: variable("HOME"),
            xdg_config_home: variable("XDG_CONFIG_HOME"),
            xdg_data_home: variable("XDG_DATA_HOME"),
            mosna_config: variable("MOSNA_CONFIG"),
            mosna_bin: variable("MOSNA_BIN"),
            mosna_python: variable("MOSNA_PYTHON"),
            current_dir: std::env::current_dir().ok(),
            desktop_dir: variable("XDG_DESKTOP_DIR"),
            user_profile: variable("USERPROFILE"),
            app_data: variable("APPDATA"),
            local_app_data: variable("LOCALAPPDATA"),
        }
    }

    /// The user's home, whichever variable names it on this platform.
    pub fn home_dir(&self) -> Option<&PathBuf> {
        self.home.as_ref().or(self.user_profile.as_ref())
    }

    /// The desktop folder, where a launcher goes so the user can find it.
    ///
    /// A localised desktop is called `Bureau` or `Escritorio`, and dropping a
    /// launcher into an English `Desktop` that nobody opens is the same as not
    /// creating one. Three sources, most specific first:
    ///
    /// 1. `XDG_DESKTOP_DIR`, when the session exports it;
    /// 2. `~/.config/user-dirs.dirs`, which is where the name actually lives —
    ///    most sessions do not export the variable at all;
    /// 3. `~/Desktop`, which is right far more often than it is wrong.
    pub fn desktop_dir(&self) -> Option<PathBuf> {
        if let Some(explicit) = &self.desktop_dir {
            return Some(explicit.clone());
        }

        let home = self.home_dir()?.clone();
        self.config_home()
            .and_then(|config_home| user_dirs::desktop(&config_home, &home))
            .or_else(|| Some(home.join("Desktop")))
    }

    /// The Start Menu's programs folder, on Windows.
    pub fn start_menu_dir(&self) -> Option<PathBuf> {
        self.app_data
            .clone()
            .or_else(|| self.home_dir().map(|home| home.join("AppData/Roaming")))
            .map(|roaming| roaming.join("Microsoft/Windows/Start Menu/Programs"))
    }

    /// The XDG configuration directory, falling back to `~/.config`.
    pub fn config_home(&self) -> Option<PathBuf> {
        self.xdg_config_home
            .clone()
            .or_else(|| self.home.as_ref().map(|home| home.join(".config")))
    }

    /// The XDG data directory, falling back to `~/.local/share`.
    pub fn data_home(&self) -> Option<PathBuf> {
        self.xdg_data_home
            .clone()
            .or_else(|| self.home.as_ref().map(|home| home.join(".local/share")))
    }

    /// The install prefix the running executable belongs to, i.e. the parent of
    /// its `bin` directory.
    pub fn prefix(&self) -> Option<PathBuf> {
        self.exe_dir
            .as_ref()
            .and_then(|dir| dir.parent())
            .map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_config_home_falls_back_to_the_home_directory() {
        let mut environment = Environment {
            home: Some(PathBuf::from("/home/someone")),
            ..Default::default()
        };
        assert_eq!(
            environment.config_home(),
            Some(PathBuf::from("/home/someone/.config"))
        );

        environment.xdg_config_home = Some(PathBuf::from("/elsewhere"));
        assert_eq!(environment.config_home(), Some(PathBuf::from("/elsewhere")));
    }

    #[test]
    fn the_data_home_falls_back_to_the_home_directory() {
        let environment = Environment {
            home: Some(PathBuf::from("/home/someone")),
            ..Default::default()
        };
        assert_eq!(
            environment.data_home(),
            Some(PathBuf::from("/home/someone/.local/share"))
        );
    }

    #[test]
    fn the_prefix_is_the_parent_of_the_bin_directory() {
        let environment = Environment {
            exe_dir: Some(PathBuf::from("/opt/mosna/bin")),
            ..Default::default()
        };
        assert_eq!(environment.prefix(), Some(PathBuf::from("/opt/mosna")));
    }

    #[test]
    fn an_empty_environment_resolves_to_nothing() {
        let environment = Environment::default();
        assert_eq!(environment.config_home(), None);
        assert_eq!(environment.prefix(), None);
    }

    #[test]
    fn detecting_the_real_environment_does_not_panic() {
        let environment = Environment::detect();
        // The executable always exists while a test runs.
        assert!(environment
            .exe_dir
            .as_deref()
            .map(Path::is_dir)
            .unwrap_or(false));
    }
}
