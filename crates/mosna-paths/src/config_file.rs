//! Finding `configuration.yaml`.

use std::path::{Path, PathBuf};

use crate::Environment;

/// Name of the configuration file, wherever it lives.
pub const FILE_NAME: &str = "configuration.yaml";

/// Sub-directory of the configuration and data directories that belongs to us.
pub const APPLICATION_DIR: &str = "mosna";

/// Where the repository keeps its configuration.
pub const REPOSITORY_DIR: &str = "CONFIG";

/// Locate the configuration to use, in order of precedence:
///
/// 1. an explicit path, from the command line;
/// 2. `MOSNA_CONFIG`;
/// 3. the user's own copy, under the XDG configuration directory;
/// 4. the copy the installer laid down beside the binaries;
/// 5. `CONFIG/configuration.yaml` relative to the current directory, which is
///    the repository layout.
///
/// When nothing exists yet the user's path is returned anyway, so a first run
/// has somewhere to write and the caller can report a missing file rather than
/// having to guess where one should go.
pub fn resolve(explicit: Option<&Path>, environment: &Environment) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Some(path) = &environment.mosna_config {
        return path.clone();
    }

    let candidates = [
        user_path(environment),
        installed_path(environment),
        repository_path(environment),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.is_file() {
            return candidate.clone();
        }
    }

    // Nothing on disk: point at where the user's copy belongs.
    user_path(environment)
        .or_else(|| repository_path(environment))
        .unwrap_or_else(|| PathBuf::from(REPOSITORY_DIR).join(FILE_NAME))
}

/// The user's own copy, which the interface edits.
///
/// Kept separate from the installed copy so that saving never writes into a
/// system directory, and so a reinstall cannot overwrite the user's settings.
pub fn user_path(environment: &Environment) -> Option<PathBuf> {
    environment
        .config_home()
        .map(|home| home.join(APPLICATION_DIR).join(FILE_NAME))
}

/// The copy the installer placed under the prefix.
pub fn installed_path(environment: &Environment) -> Option<PathBuf> {
    environment
        .prefix()
        .map(|prefix| prefix.join("share").join(APPLICATION_DIR).join(FILE_NAME))
}

/// The repository's own configuration, relative to the current directory.
pub fn repository_path(environment: &Environment) -> Option<PathBuf> {
    environment
        .current_dir
        .as_ref()
        .map(|dir| dir.join(REPOSITORY_DIR).join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(home: &Path) -> Environment {
        Environment {
            home: Some(home.to_path_buf()),
            ..Default::default()
        }
    }

    #[test]
    fn the_user_path_lives_under_the_config_home() {
        let environment = environment(Path::new("/home/someone"));
        assert_eq!(
            user_path(&environment),
            Some(PathBuf::from(
                "/home/someone/.config/mosna/configuration.yaml"
            ))
        );
    }

    #[test]
    fn the_installed_path_lives_under_the_prefix() {
        let environment = Environment {
            exe_dir: Some(PathBuf::from("/usr/local/bin")),
            ..Default::default()
        };
        assert_eq!(
            installed_path(&environment),
            Some(PathBuf::from("/usr/local/share/mosna/configuration.yaml"))
        );
    }

    #[test]
    fn an_explicit_path_is_returned_even_when_absent() {
        // The caller reports "no such file" with the path the user gave, which
        // is more useful than silently falling back to another configuration.
        let explicit = Path::new("/nowhere/mine.yaml");
        assert_eq!(
            resolve(Some(explicit), &Environment::default()),
            explicit.to_path_buf()
        );
    }

    #[test]
    fn a_bare_environment_still_yields_a_path() {
        let resolved = resolve(None, &Environment::default());
        assert_eq!(resolved, PathBuf::from("CONFIG/configuration.yaml"));
    }
}
