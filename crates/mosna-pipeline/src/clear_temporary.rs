//! Port of `package/clear_temporary.py`.

use std::path::Path;

use crate::error::{PipelineError, Result};
use crate::progress::Progress;

/// Remove the `temp` directory of a working directory.
///
/// Does nothing when it is absent, which is the Python's behaviour and lets the
/// GUI's "Clear Temp Files" button be pressed at any time.
pub fn clear_temporary(working_dir: &Path, progress: &dyn Progress) -> Result<()> {
    let temp_dir = working_dir.join("temp");

    if temp_dir.is_dir() {
        std::fs::remove_dir_all(&temp_dir).map_err(|source| PipelineError::Remove {
            path: temp_dir.clone(),
            source,
        })?;
        progress.info(&format!(
            "[INFO] Temporary folder removed: {}",
            temp_dir.display()
        ));
    } else {
        progress.info(&format!(
            "[INFO] No temporary folder found at: {}",
            temp_dir.display()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::SilentProgress;

    #[test]
    fn removes_the_directory_and_its_contents() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("temp/net_dir_mosna");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("nodes_patient-1.parquet"), b"x").unwrap();

        clear_temporary(dir.path(), &SilentProgress).unwrap();
        assert!(!dir.path().join("temp").exists());
    }

    #[test]
    fn an_absent_directory_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        clear_temporary(dir.path(), &SilentProgress).unwrap();
    }

    #[test]
    fn only_the_temp_directory_is_touched() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("temp")).unwrap();
        std::fs::create_dir_all(dir.path().join("Tysserand_Network")).unwrap();
        std::fs::write(dir.path().join("Tysserand_Network/net_1.png"), b"x").unwrap();

        clear_temporary(dir.path(), &SilentProgress).unwrap();
        assert!(dir.path().join("Tysserand_Network/net_1.png").is_file());
    }
}
