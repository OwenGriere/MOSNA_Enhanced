//! The identifier pair naming one network.

/// A patient identifier and, for two-level datasets, a sample identifier.
///
/// Plays the role of the `data_info` list Python passes around
/// (`[patient]` or `[patient, sample]`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SampleId {
    pub patient: String,
    pub sample: Option<String>,
}

impl SampleId {
    /// A single-level identifier.
    pub fn patient_only(patient: impl Into<String>) -> Self {
        Self {
            patient: patient.into(),
            sample: None,
        }
    }

    /// A two-level identifier.
    pub fn with_sample(patient: impl Into<String>, sample: impl Into<String>) -> Self {
        Self {
            patient: patient.into(),
            sample: Some(sample.into()),
        }
    }

    /// The `str_group` fragment used to build file names.
    ///
    /// Ports the Python
    /// ```python
    /// if len(data_info) == 1:
    ///     str_group = f'{id_level_1}-{data_info[0]}'
    /// elif len(data_info) == 2:
    ///     str_group = f'{id_level_1}-{data_info[0]}_{id_level_2}-{data_info[1]}'
    /// ```
    pub fn str_group(&self, patient_column: &str, sample_column: Option<&str>) -> String {
        match (&self.sample, sample_column) {
            (Some(sample), Some(sample_col)) => {
                format!("{patient_column}-{}_{sample_col}-{sample}", self.patient)
            }
            _ => format!("{patient_column}-{}", self.patient),
        }
    }

    /// The nodes file name for this identifier.
    pub fn nodes_file_name(
        &self,
        patient_column: &str,
        sample_column: Option<&str>,
        extension: &str,
    ) -> String {
        format!(
            "nodes_{}.{extension}",
            self.str_group(patient_column, sample_column)
        )
    }

    /// The edges file name for this identifier.
    pub fn edges_file_name(
        &self,
        patient_column: &str,
        sample_column: Option<&str>,
        extension: &str,
    ) -> String {
        format!(
            "edges_{}.{extension}",
            self.str_group(patient_column, sample_column)
        )
    }

    /// The label used in figure titles and in the `id` column of `net_stat.csv`.
    pub fn label(&self, patient_column: &str, sample_column: Option<&str>) -> String {
        self.str_group(patient_column, sample_column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_level_names_match_python() {
        let id = SampleId::with_sample("12", "3");
        assert_eq!(
            id.str_group("patient", Some("sample")),
            "patient-12_sample-3"
        );
        assert_eq!(
            id.nodes_file_name("patient", Some("sample"), "parquet"),
            "nodes_patient-12_sample-3.parquet"
        );
        assert_eq!(
            id.edges_file_name("patient", Some("sample"), "parquet"),
            "edges_patient-12_sample-3.parquet"
        );
    }

    #[test]
    fn single_level_names_omit_the_sample() {
        let id = SampleId::patient_only("12");
        assert_eq!(id.str_group("patient", None), "patient-12");
        assert_eq!(
            id.nodes_file_name("patient", None, "csv"),
            "nodes_patient-12.csv"
        );
    }

    #[test]
    fn a_sample_is_ignored_when_no_sample_column_is_configured() {
        // A single-level run must not start emitting two-level names just
        // because an id happens to carry a sample.
        let id = SampleId::with_sample("12", "3");
        assert_eq!(id.str_group("patient", None), "patient-12");
    }
}
