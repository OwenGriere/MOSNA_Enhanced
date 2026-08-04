//! Port of `package/utils/find_sample.py::find_sample`.

use std::path::{Path, PathBuf};

use regex::Regex;

use crate::error::{IoError, Result};

/// List the nodes files of a directory that match the naming convention.
///
/// Reproduces the Python regexes exactly:
///
/// ```python
/// rf"^nodes_{re.escape(patient)}-[^_]+\.{re.escape(extension)}$"
/// rf"^nodes_{re.escape(patient)}-[^_]+_{re.escape(sample)}-[^_]+\.{re.escape(extension)}$"
/// ```
///
/// Note the `[^_]+` classes: an identifier containing an underscore does not
/// match, and neither does a two-level file when the run is configured as
/// single-level. That strictness is deliberate on the Python side — it is what
/// stops a `patient-1_sample-2` file being mistaken for a `patient-1` one — so
/// it is preserved rather than relaxed.
///
/// Results are sorted by file name, matching Python's `sorted(...)` over
/// `Path` objects, so downstream row order is reproducible.
pub fn find_sample(
    net_dir: impl AsRef<Path>,
    extension: &str,
    patient_column: &str,
    sample_column: Option<&str>,
) -> Result<Vec<PathBuf>> {
    let net_dir = net_dir.as_ref();
    let pattern = build_pattern(extension, patient_column, sample_column);
    let regex = Regex::new(&pattern)
        .map_err(|e| IoError::invalid(format!("invalid file pattern `{pattern}`: {e}")))?;

    let entries = std::fs::read_dir(net_dir).map_err(|source| IoError::Read {
        path: net_dir.to_path_buf(),
        source,
    })?;

    let mut matches = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| IoError::Read {
            path: net_dir.to_path_buf(),
            source,
        })?;
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if regex.is_match(name) {
            matches.push(entry.path());
        }
    }
    matches.sort();
    Ok(matches)
}

/// Build the file-name regex for a given naming scheme.
pub fn build_pattern(extension: &str, patient_column: &str, sample_column: Option<&str>) -> String {
    match sample_column {
        None => format!(
            r"^nodes_{}-[^_]+\.{}$",
            regex::escape(patient_column),
            regex::escape(extension)
        ),
        Some(sample_column) => format!(
            r"^nodes_{}-[^_]+_{}-[^_]+\.{}$",
            regex::escape(patient_column),
            regex::escape(sample_column),
            regex::escape(extension)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"").unwrap();
    }

    #[test]
    fn finds_two_level_files_in_sorted_order() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "nodes_patient-2_sample-1.parquet");
        touch(p, "nodes_patient-1_sample-1.parquet");
        touch(p, "edges_patient-1_sample-1.parquet");
        touch(p, "cell_types.npy");

        let found = find_sample(p, "parquet", "patient", Some("sample")).unwrap();
        let names: Vec<String> = found
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec![
                "nodes_patient-1_sample-1.parquet",
                "nodes_patient-2_sample-1.parquet"
            ]
        );
    }

    #[test]
    fn single_level_pattern_excludes_two_level_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "nodes_patient-1.parquet");
        touch(p, "nodes_patient-2_sample-1.parquet");

        let found = find_sample(p, "parquet", "patient", None).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0]
            .to_string_lossy()
            .ends_with("nodes_patient-1.parquet"));
    }

    #[test]
    fn the_extension_is_part_of_the_match() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "nodes_patient-1.parquet");
        touch(p, "nodes_patient-1.csv");

        assert_eq!(find_sample(p, "csv", "patient", None).unwrap().len(), 1);
        assert_eq!(find_sample(p, "parquet", "patient", None).unwrap().len(), 1);
    }

    #[test]
    fn identifiers_containing_an_underscore_do_not_match() {
        // `[^_]+` in the Python regex; documented here so a future change to
        // the pattern trips this test rather than silently altering discovery.
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "nodes_patient-A_B.parquet");

        assert!(find_sample(p, "parquet", "patient", None)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_missing_directory_is_reported() {
        let err = find_sample("/nonexistent/mosna", "parquet", "patient", None).unwrap_err();
        assert!(err.to_string().contains("/nonexistent/mosna"));
    }
}
