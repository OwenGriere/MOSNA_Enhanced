//! Which sample a figure is about.
//!
//! The report is read patient by patient, so every figure has to say whose it
//! is. Nothing in the file records that — the report takes no configuration and
//! cannot know which columns the run was grouped by — so it is read back out of
//! the names the analyses wrote, which are a contract the interface already
//! parses the same way.
//!
//! Two shapes carry it:
//!
//! ```text
//! net_1-8.png                     the sample is in the file name
//! heatmap_zscore_1-8.png          idem
//! Per_sample/run/patient-1_chunk-8/Niches_Histogram.png   in the directory
//! ```
//!
//! Everything else — `abundance`, the clustered heatmaps, the composition of
//! the niches — is about the cohort as a whole, and says so by matching
//! neither.

/// A patient, and the sample of that patient when there is one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Subject {
    pub patient: String,
    pub sample: Option<String>,
}

impl Subject {
    /// How the subject is written in a heading: `Patient 1`, `Patient 1 · 8`.
    pub fn label(&self) -> String {
        match &self.sample {
            Some(sample) => format!("Patient {} · {sample}", self.patient),
            None => format!("Patient {}", self.patient),
        }
    }

    /// What the search box matches against.
    pub fn search_key(&self) -> String {
        match &self.sample {
            Some(sample) => format!("{} {sample} {}-{sample}", self.patient, self.patient),
            None => self.patient.clone(),
        }
    }
}

/// File-name prefixes that are followed by a sample identifier.
const PREFIXES: [&str; 2] = ["net_", "heatmap_zscore_"];

/// The subject a file name names, if it names one.
///
/// `net_1-8` and `heatmap_zscore_1-8` are the two the analyses write. Both are
/// a known prefix followed by `{patient}` or `{patient}-{sample}`, which is the
/// same thing the interface parses to group its gallery.
pub fn from_stem(stem: &str) -> Option<Subject> {
    let rest = PREFIXES
        .iter()
        .find_map(|prefix| stem.strip_prefix(prefix))?;
    identifiers(rest)
}

/// The subject a directory name names, if it names one.
///
/// `patient-1_chunk-8`: the column names are whatever the run was configured
/// with, so what is read is the shape — `label-value`, then optionally another
/// — and not the labels themselves.
pub fn from_directory(name: &str) -> Option<Subject> {
    let mut parts = name.split('_');
    let patient = value_of(parts.next()?)?;
    let sample = match parts.next() {
        Some(part) => Some(value_of(part)?),
        None => None,
    };
    // A third part is not a shape this writes, and guessing at it would put a
    // figure under a patient it does not belong to.
    if parts.next().is_some() {
        return None;
    }
    Some(Subject { patient, sample })
}

/// The value of a `label-value` pair, when there is one on each side.
fn value_of(part: &str) -> Option<String> {
    let (label, value) = part.split_once('-')?;
    (!label.is_empty() && !value.is_empty()).then(|| value.to_string())
}

/// `1-8` or `1`, as they appear after a prefix.
fn identifiers(rest: &str) -> Option<Subject> {
    match rest.split_once('-') {
        Some((patient, sample)) => (!patient.is_empty() && !sample.is_empty()).then(|| Subject {
            patient: patient.to_string(),
            sample: Some(sample.to_string()),
        }),
        None => (!rest.is_empty()).then(|| Subject {
            patient: rest.to_string(),
            sample: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(patient: &str, sample: Option<&str>) -> Subject {
        Subject {
            patient: patient.to_string(),
            sample: sample.map(str::to_string),
        }
    }

    #[test]
    fn a_network_names_its_patient_and_sample() {
        assert_eq!(from_stem("net_1-8"), Some(subject("1", Some("8"))));
        assert_eq!(from_stem("net_6-5"), Some(subject("6", Some("5"))));
    }

    /// A cohort grouped by patient alone writes `net_1.png`: one level, and the
    /// figure still belongs to a patient.
    #[test]
    fn a_single_level_network_names_only_its_patient() {
        assert_eq!(from_stem("net_1"), Some(subject("1", None)));
    }

    #[test]
    fn a_per_sample_heatmap_names_its_sample_too() {
        assert_eq!(
            from_stem("heatmap_zscore_1-11"),
            Some(subject("1", Some("11")))
        );
    }

    /// The cohort figures: these are about every sample at once, and putting
    /// them under a patient would be a claim about the data that is false.
    #[test]
    fn a_cohort_figure_belongs_to_no_one() {
        for stem in [
            "abundance",
            "Assortativity_heatmap_with_dendrogram",
            "Assortativity_heatmap_across_patient_without_auto_paired_pheno",
            "Niches_Aggregated_Composition_total",
            "Niches_Histogram",
            "cluster_labels",
            "cluster_labels_metric-cosine",
        ] {
            assert_eq!(from_stem(stem), None, "{stem} was taken for a sample");
        }
    }

    /// Step 3 writes one directory per sample, named the way `mosna-io` names
    /// samples: `{patient column}-{value}_{sample column}-{value}`. The column
    /// names are whatever the run was configured with, so only the shape can be
    /// relied on.
    #[test]
    fn a_per_sample_directory_names_its_sample() {
        assert_eq!(
            from_directory("patient-1_chunk-8"),
            Some(subject("1", Some("8")))
        );
        assert_eq!(
            from_directory("patient-6_sample-5"),
            Some(subject("6", Some("5")))
        );
        assert_eq!(from_directory("patient-2"), Some(subject("2", None)));
    }

    /// The directories that are not samples, and must not become one.
    #[test]
    fn the_structural_directories_are_not_samples() {
        for name in [
            "Aggregation",
            "Per_sample",
            "niche_cluster",
            "assort_files",
            "assort_files_without_diag",
            "Tysserand_Network",
            "temp",
        ] {
            assert_eq!(from_directory(name), None, "{name} was taken for a sample");
        }
    }

    /// A patient identifier is not always a number — a cohort may name them
    /// `A`, or `CTRL-04`. What decides is the shape of the name, not the shape
    /// of the identifier.
    #[test]
    fn an_identifier_that_is_not_a_number_still_decodes() {
        assert_eq!(from_stem("net_A-II"), Some(subject("A", Some("II"))));
        assert_eq!(
            from_directory("patient-A_chunk-II"),
            Some(subject("A", Some("II")))
        );
    }

    /// Nothing that could be mistaken for a sample: an empty identifier is not
    /// one, and neither is a prefix on its own.
    #[test]
    fn an_empty_identifier_is_not_a_subject() {
        assert_eq!(from_stem("net_"), None);
        assert_eq!(from_stem("net_-8"), None);
        assert_eq!(from_directory("patient-"), None);
        assert_eq!(from_directory("_chunk-8"), None);
    }

    #[test]
    fn a_subject_reads_as_a_heading() {
        assert_eq!(subject("1", Some("8")).label(), "Patient 1 · 8");
        assert_eq!(subject("1", None).label(), "Patient 1");
    }

    /// The search box is typed into with whatever the reader has in mind: the
    /// patient, the sample, or the pair as it appears in the file name.
    #[test]
    fn a_subject_is_searchable_by_either_half_or_by_both() {
        let key = subject("1", Some("8")).search_key();
        assert!(key.contains('1'));
        assert!(key.contains('8'));
        assert!(key.contains("1-8"), "the pair as written in the file name");
    }
}
