//! Reading `~/.config/user-dirs.dirs`, where a localised desktop is named.
//!
//! `xdg-user-dirs` writes this file at first login and the desktop session
//! reads it, but most sessions do **not** export `XDG_DESKTOP_DIR` into the
//! environment — the `xdg-user-dir` command reads the file each time instead.
//! An installer that trusts the variable alone therefore drops its launcher
//! into an English `Desktop` folder that, on a French or Spanish account, does
//! not exist and is never opened.
//!
//! The file is shell syntax, but only a fragment of it is ever used:
//!
//! ```text
//! # This file is written by xdg-user-dirs-update
//! XDG_DESKTOP_DIR="$HOME/Bureau"
//! XDG_DOWNLOAD_DIR="$HOME/Téléchargements"
//! ```
//!
//! So it is parsed as that fragment rather than interpreted: a real shell would
//! be both far more work and far more surprising.

use std::path::{Path, PathBuf};

/// Name of the file, inside the configuration directory.
const FILE_NAME: &str = "user-dirs.dirs";

/// The desktop directory declared in `config_home/user-dirs.dirs`, if any.
///
/// `home` expands the `$HOME` the file is written in terms of.
///
/// Returns `None` when the file is absent, says nothing about the desktop, or
/// names the home directory itself — which is how `xdg-user-dirs` spells "this
/// account has no desktop".
pub fn desktop(config_home: &Path, home: &Path) -> Option<PathBuf> {
    let contents = std::fs::read_to_string(config_home.join(FILE_NAME)).ok()?;
    let value = value_of("XDG_DESKTOP_DIR", &contents)?;
    let path = expand_home(&value, home)?;

    // A desktop equal to the home directory means there is none; writing a
    // launcher into the home root would be worse than writing none at all.
    if path == home {
        return None;
    }
    Some(path)
}

/// The value assigned to `key`, from the last line that assigns it.
fn value_of(key: &str, contents: &str) -> Option<String> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (name, value) = line.split_once('=')?;
            if name.trim() != key {
                return None;
            }
            // The quotes are conventional, not required.
            let value = value.trim().trim_matches('"').trim_matches('\'').trim();
            (!value.is_empty()).then(|| value.to_string())
        })
        .next_back()
}

/// Resolve a value written in terms of `$HOME`.
fn expand_home(value: &str, home: &Path) -> Option<PathBuf> {
    let path = match value.strip_prefix("$HOME") {
        Some(rest) => {
            let rest = rest.trim_start_matches('/');
            if rest.is_empty() {
                home.to_path_buf()
            } else {
                home.join(rest)
            }
        }
        // Anything that is not anchored on $HOME must be absolute; a relative
        // path here has no directory to be relative to.
        None if value.starts_with('/') => PathBuf::from(value),
        None => return None,
    };
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_assignment_wins() {
        let contents = "XDG_DESKTOP_DIR=\"$HOME/a\"\nXDG_DESKTOP_DIR=\"$HOME/b\"\n";
        assert_eq!(
            value_of("XDG_DESKTOP_DIR", contents).as_deref(),
            Some("$HOME/b")
        );
    }

    #[test]
    fn a_relative_value_is_refused() {
        assert_eq!(expand_home("Bureau", Path::new("/home/someone")), None);
    }

    #[test]
    fn a_key_that_merely_starts_the_same_is_not_matched() {
        let contents = "XDG_DESKTOP_DIRECTORY=\"$HOME/nope\"\n";
        assert_eq!(value_of("XDG_DESKTOP_DIR", contents), None);
    }
}
