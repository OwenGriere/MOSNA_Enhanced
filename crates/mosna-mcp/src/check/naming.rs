//! Which files of a directory are samples, and whether both implementations
//! read the same identifiers out of their names.
//!
//! Both implementations discover samples with the same regex, so a file that
//! does not match is skipped by both — silently. Saying *why* it was skipped is
//! most of the value here: a cohort of a hundred files where three carry an `_`
//! in their identifier runs without error and analyses ninety-seven.
//!
//! Decoding is where they part. Python splits the file stem on the column names
//! wherever they occur (`find_sample_from_file.py`); Rust matches the structure
//! the encoder produces (`mosna_io::find_sample_from_file`). They agree on
//! every realistic name, and disagree — Python returning a truncated or wrong
//! identifier, or raising `IndexError` — when a column name also occurs in the
//! prefix or inside an identifier. Both decoders are run on every file, so that
//! case is caught before it attributes a sample to the wrong patient.

use std::path::{Path, PathBuf};

use regex::Regex;

use mosna_io::discovery::find_sample::build_pattern;
use mosna_io::{find_sample_from_file, SampleId};

use crate::report::{Affects, Report, Severity};

/// How samples are named in a directory.
#[derive(Debug, Clone)]
pub struct Naming {
    pub extension: String,
    pub patient_column: String,
    pub sample_column: Option<String>,
}

/// A file both implementations will read, and who it belongs to.
#[derive(Debug, Clone)]
pub struct SampleFile {
    pub path: PathBuf,
    pub id: SampleId,
}

impl SampleFile {
    pub fn name(&self) -> String {
        file_name(&self.path)
    }
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// List the sample files of `dir`, explaining the ones left out.
pub fn scan_directory(dir: &Path, naming: &Naming) -> (Vec<SampleFile>, Report) {
    let mut report = Report::default();
    let sample_column = naming.sample_column.as_deref();

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            report.error(
                "dir-unreadable",
                None,
                format!("cannot read directory {}: {error}", dir.display()),
            );
            return (Vec::new(), report);
        }
    };

    let pattern = build_pattern(&naming.extension, &naming.patient_column, sample_column);
    let regex = match Regex::new(&pattern) {
        Ok(regex) => regex,
        Err(error) => {
            report.error(
                "bad-naming",
                None,
                format!("the configured column names make an invalid file pattern: {error}"),
            );
            return (Vec::new(), report);
        }
    };

    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();

    let mut files = Vec::new();
    let mut other_files = Vec::new();
    for name in names {
        if regex.is_match(&name) {
            let path = dir.join(&name);
            match find_sample_from_file(&path, &naming.patient_column, sample_column) {
                Ok(id) => {
                    compare_with_python(&name, &id, naming, &mut report);
                    files.push(SampleFile { path, id });
                }
                Err(error) => report.error("name-undecodable", Some(&name), error.to_string()),
            }
        } else if name.starts_with("nodes_") {
            report.warning(
                "name-ignored",
                Some(&name),
                format!(
                    "skipped by both implementations: {}",
                    why_not_matched(&name, naming)
                ),
            );
        } else if !name.starts_with("edges_") {
            other_files.push(name);
        }
    }

    if !other_files.is_empty() {
        let shown: Vec<&str> = other_files.iter().take(5).map(String::as_str).collect();
        report.info(
            "other-files",
            None,
            format!(
                "{} file(s) not named `nodes_…` are ignored, e.g. {}",
                other_files.len(),
                shown.join(", ")
            ),
        );
    }

    if files.is_empty() {
        report.error(
            "no-samples",
            None,
            format!(
                "no file of {} matches `{pattern}`: nothing would be analysed",
                dir.display()
            ),
        );
    }

    (files, report)
}

/// Explain why a `nodes_…` file does not match the discovery pattern.
pub fn why_not_matched(name: &str, naming: &Naming) -> String {
    let suffix = format!(".{}", naming.extension);
    let Some(stem) = name.strip_suffix(&suffix) else {
        return format!("its extension is not `{suffix}`");
    };
    let stem = stem.strip_prefix("nodes_").unwrap_or(stem);
    let prefix = format!("{}-", naming.patient_column);
    let Some(rest) = stem.strip_prefix(&prefix) else {
        return format!("it does not start with `nodes_{prefix}`");
    };
    match &naming.sample_column {
        None if rest.contains('_') => format!(
            "`{rest}` contains `_`: either an identifier holds an underscore (forbidden), \
             or the data has two levels and `Sample column name` must be set"
        ),
        None => "its patient identifier is empty".to_string(),
        Some(sample) => {
            let separator = format!("_{sample}-");
            if !rest.contains(&separator) {
                format!(
                    "it has no `{separator}` part, which `Sample column name: {sample}` requires"
                )
            } else {
                "an identifier is empty or contains `_` (identifiers may not contain underscores)"
                    .to_string()
            }
        }
    }
}

/// Decode a name the way `package/utils/find_sample_from_file.py` does.
///
/// `None` is the `IndexError` Python would raise. The slicing follows Python's,
/// which never fails on a short string: `"a"[1:-1]` is `""`.
pub fn python_decode(
    stem: &str,
    patient_column: &str,
    sample_column: Option<&str>,
) -> Option<SampleId> {
    fn slice(s: &str, start: usize, drop_last: bool) -> String {
        let chars: Vec<char> = s.chars().collect();
        let end = if drop_last {
            chars.len().saturating_sub(1)
        } else {
            chars.len()
        };
        if start >= end {
            return String::new();
        }
        chars[start..end].iter().collect()
    }

    match sample_column {
        None => {
            let part = stem.split(patient_column).nth(1)?;
            Some(SampleId::patient_only(slice(part, 1, false)))
        }
        Some(sample_column) => {
            let mut parts = stem.split(sample_column);
            let head = parts.next()?;
            let sample = slice(parts.next()?, 1, false);
            let patient = slice(head.split(patient_column).nth(1)?, 1, true);
            Some(SampleId::with_sample(patient, sample))
        }
    }
}

fn compare_with_python(name: &str, rust: &SampleId, naming: &Naming, report: &mut Report) {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    match python_decode(
        stem,
        &naming.patient_column,
        naming.sample_column.as_deref(),
    ) {
        None => report.push(
            Severity::Error,
            "name-python-crash",
            Affects::Python,
            Some(name),
            "MOSNA_GUI would raise IndexError decoding this name: a column name occurs \
             where the Python split does not expect it",
        ),
        Some(python) if &python != rust => report.push(
            Severity::Error,
            "name-python-divergence",
            Affects::Python,
            Some(name),
            format!(
                "Rust reads {} but MOSNA_GUI reads {}: a column name also occurs in the \
                 prefix or in an identifier, and Python splits on every occurrence",
                describe(rust),
                describe(&python)
            ),
        ),
        Some(_) => {}
    }
}

fn describe(id: &SampleId) -> String {
    match &id.sample {
        Some(sample) => format!("patient `{}` sample `{sample}`", id.patient),
        None => format!("patient `{}`", id.patient),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naming(sample: Option<&str>) -> Naming {
        Naming {
            extension: "parquet".into(),
            patient_column: "patient".into(),
            sample_column: sample.map(str::to_string),
        }
    }

    #[test]
    fn python_decoding_agrees_on_ordinary_names() {
        let id = python_decode("nodes_patient-12_sample-3", "patient", Some("sample")).unwrap();
        assert_eq!(id, SampleId::with_sample("12", "3"));
        let id = python_decode("nodes_patient-12", "patient", None).unwrap();
        assert_eq!(id, SampleId::patient_only("12"));
    }

    /// `es` occurs in `nodes_`: Python's split lands in the prefix.
    #[test]
    fn python_decoding_goes_wrong_when_a_column_name_hides_in_the_prefix() {
        let python = python_decode("nodes_es-42", "es", None).unwrap();
        assert_ne!(python, SampleId::patient_only("42"));
    }

    #[test]
    fn a_divergence_is_reported_against_python() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("nodes_es-42.parquet"), b"").unwrap();
        let naming = Naming {
            extension: "parquet".into(),
            patient_column: "es".into(),
            sample_column: None,
        };
        let (files, report) = scan_directory(dir.path(), &naming);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, SampleId::patient_only("42"));
        assert!(report
            .findings
            .iter()
            .any(|f| f.code == "name-python-divergence" && f.affects == Affects::Python));
    }

    #[test]
    fn skipped_files_are_explained() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "nodes_patient-1_sample-1.parquet",
            "nodes_patient-2_sample-A_B.parquet",
            "nodes_patient-3.parquet",
            "nodes_patient-4_sample-1.csv",
            "edges_patient-1_sample-1.parquet",
        ] {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        let (files, report) = scan_directory(dir.path(), &naming(Some("sample")));
        assert_eq!(files.len(), 1);
        let ignored: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.code == "name-ignored")
            .collect();
        assert_eq!(ignored.len(), 3);
        assert!(ignored.iter().any(|f| f.message.contains("extension")));
        assert!(ignored.iter().any(|f| f.message.contains("_sample-")));
    }

    #[test]
    fn a_two_level_file_in_a_one_level_run_says_how_to_fix_it() {
        let reason = why_not_matched("nodes_patient-1_sample-2.parquet", &naming(None));
        assert!(reason.contains("Sample column name"));
    }

    #[test]
    fn an_empty_directory_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (_, report) = scan_directory(dir.path(), &naming(None));
        assert!(!report.is_ok());
    }
}
