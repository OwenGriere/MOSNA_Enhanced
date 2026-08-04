//! Finding the analysis binary the interface launches.

use std::path::PathBuf;

use crate::Environment;

/// File name of the analysis binary.
pub const ANALYSIS_FILE_NAME: &str = if cfg!(windows) { "mosna.exe" } else { "mosna" };

/// File name of the interface binary.
pub const INTERFACE_FILE_NAME: &str = if cfg!(windows) {
    "mosna-gui.exe"
} else {
    "mosna-gui"
};

/// Locate the analysis binary, in order of precedence:
///
/// 1. `MOSNA_BIN`, so a developer can point a packaged interface at a fresh
///    build without reinstalling;
/// 2. the copy beside the running executable, which is how the installer lays
///    them out and what lets the whole prefix be moved;
/// 3. the bare name, leaving the shell to search `PATH`.
pub fn resolve_analysis(environment: &Environment) -> PathBuf {
    if let Some(path) = &environment.mosna_bin {
        return path.clone();
    }

    if let Some(directory) = &environment.exe_dir {
        let beside = directory.join(ANALYSIS_FILE_NAME);
        if beside.is_file() {
            return beside;
        }
    }

    PathBuf::from(ANALYSIS_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_carry_the_platform_extension() {
        if cfg!(windows) {
            assert!(ANALYSIS_FILE_NAME.ends_with(".exe"));
            assert!(INTERFACE_FILE_NAME.ends_with(".exe"));
        } else {
            assert_eq!(ANALYSIS_FILE_NAME, "mosna");
            assert_eq!(INTERFACE_FILE_NAME, "mosna-gui");
        }
    }

    #[test]
    fn an_override_is_taken_verbatim_even_when_absent() {
        // A typo in `MOSNA_BIN` must surface as "cannot start that", not be
        // silently swapped for something else.
        let environment = Environment {
            mosna_bin: Some(PathBuf::from("/nowhere/mosna")),
            ..Default::default()
        };
        assert_eq!(
            resolve_analysis(&environment),
            PathBuf::from("/nowhere/mosna")
        );
    }

    #[test]
    fn a_bare_environment_falls_back_to_the_name() {
        assert_eq!(
            resolve_analysis(&Environment::default()),
            PathBuf::from(ANALYSIS_FILE_NAME)
        );
    }
}
