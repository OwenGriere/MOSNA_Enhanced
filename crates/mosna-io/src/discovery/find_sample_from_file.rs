//! Port of `package/utils/find_sample_from_file.py::find_sample_from_file`.

use std::path::Path;

use crate::discovery::sample_id::SampleId;
use crate::error::{IoError, Result};

/// Decode the patient and sample identifiers out of a network file name.
///
/// Faithful port of the Python string surgery:
///
/// ```python
/// if column_sample is None:
///     return file.stem.split(column_patient)[1][1:], None
/// else:
///     parts = file.stem.split(column_sample)
///     sample = parts[1][1:]
///     patient = parts[0].split(column_patient)[1][1:-1]
///     return patient, sample
/// ```
///
/// The slicing drops the `-` that follows the column name and, in the two-level
/// case, the trailing `_` that precedes the sample column. Python raises
/// `IndexError` when the name does not contain the expected markers; here that
/// becomes a named error mentioning the offending file.
///
/// # Structural matching instead of blind splitting
///
/// The Python splits the whole stem on the column names wherever they occur,
/// which is ambiguous in two ways that were found by property testing:
///
/// * a column name occurring inside the `nodes_` / `edges_` prefix — a table
///   keyed by a column called `node`, `de` or `es` — makes the split land in
///   the prefix and return a garbage identifier;
/// * an identifier that happens to spell out part of the other column's
///   separator shifts the split and truncates the id.
///
/// Both cases fail silently: a wrong patient id means a sample is attributed to
/// the wrong person, with nothing in the output to say so.
///
/// This matches the structure the encoder produces instead — the
/// `{patient_column}-` prefix, then the `_{sample_column}-` separator taken at
/// its last occurrence — so decoding is the exact inverse of
/// [`SampleId::nodes_file_name`] for every input. For column names that do not
/// collide, which is every realistic one, the two implementations agree.
pub fn find_sample_from_file(
    file: impl AsRef<Path>,
    patient_column: &str,
    sample_column: Option<&str>,
) -> Result<SampleId> {
    let file = file.as_ref();
    let full_stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| IoError::invalid(format!("cannot read file name of {}", file.display())))?;

    let stem = full_stem
        .strip_prefix("nodes_")
        .or_else(|| full_stem.strip_prefix("edges_"))
        .unwrap_or(full_stem);

    let malformed = |what: &str| {
        IoError::invalid(format!(
            "{what} not found in file name `{stem}` (expected `{patient_column}-<id>{}`)",
            sample_column
                .map(|s| format!("_{s}-<id>"))
                .unwrap_or_default()
        ))
    };

    // Match the structure the encoder produces rather than splitting on the
    // column names wherever they happen to occur.
    let rest = stem
        .strip_prefix(&format!("{patient_column}-"))
        .ok_or_else(|| malformed(patient_column))?;

    match sample_column {
        None => {
            if rest.is_empty() {
                return Err(malformed(patient_column));
            }
            Ok(SampleId::patient_only(rest))
        }
        Some(sample_column) => {
            // Split at the *last* separator: a sample id can never contain an
            // underscore (the discovery regex forbids it), so the last
            // occurrence is always the real one even if the patient id happens
            // to spell the separator out.
            let separator = format!("_{sample_column}-");
            let (patient, sample) = rest
                .rsplit_once(&separator)
                .ok_or_else(|| malformed(sample_column))?;

            if patient.is_empty() || sample.is_empty() {
                return Err(malformed("identifiers"));
            }
            Ok(SampleId::with_sample(patient, sample))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_two_level_name() {
        let id = find_sample_from_file(
            "nodes_patient-12_sample-3.parquet",
            "patient",
            Some("sample"),
        )
        .unwrap();
        assert_eq!(id, SampleId::with_sample("12", "3"));
    }

    #[test]
    fn decodes_a_single_level_name() {
        let id = find_sample_from_file("nodes_patient-12.parquet", "patient", None).unwrap();
        assert_eq!(id, SampleId::patient_only("12"));
    }

    #[test]
    fn decodes_an_edges_name_the_same_way() {
        let id = find_sample_from_file(
            "edges_patient-A1_sample-B2.parquet",
            "patient",
            Some("sample"),
        )
        .unwrap();
        assert_eq!(id, SampleId::with_sample("A1", "B2"));
    }

    /// A column name that also occurs inside the `nodes_` / `edges_` prefix
    /// used to make the Python split land in the prefix and return a garbage
    /// identifier. Stripping the prefix first fixes it.
    #[test]
    fn a_column_name_colliding_with_the_prefix_still_decodes() {
        let id = find_sample_from_file("nodes_node-42.parquet", "node", None).unwrap();
        assert_eq!(id, SampleId::patient_only("42"));

        let id = find_sample_from_file("nodes_node-42_es-7.parquet", "node", Some("es")).unwrap();
        assert_eq!(id, SampleId::with_sample("42", "7"));
    }

    #[test]
    fn round_trips_against_sample_id() {
        for id in [SampleId::with_sample("7", "2"), SampleId::patient_only("7")] {
            let sample_column = id.sample.as_ref().map(|_| "sample");
            let name = id.nodes_file_name("patient", sample_column, "parquet");
            let decoded = find_sample_from_file(&name, "patient", sample_column).unwrap();
            assert_eq!(decoded, id);
        }
    }

    #[test]
    fn a_name_without_the_markers_is_reported() {
        let err = find_sample_from_file("cell_types.parquet", "patient", None).unwrap_err();
        assert!(err.to_string().contains("cell_types"));
    }
}
