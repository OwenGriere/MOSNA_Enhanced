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
    if is_run_directory(name) {
        return None;
    }
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

/// Whether `name` is one of step 3's run directories — `1-1-1`, `2-0-3`.
///
/// # Why this has to be asked first
///
/// `from_directory` reads `label-value`, which is the shape of
/// `patient-1_chunk-8`. `1-1-10` has that shape too, if one is not looking:
/// the label is `1` and the value is `1-10`. So every run directory was read as
/// a patient, and a working directory holding a dozen runs — which is the whole
/// point of the numbering — gave the report a dozen headings for patients that
/// do not exist.
///
/// A patient identifier could be a bare number, so the shape alone cannot
/// settle it; what separates them is that a run is *three* numbers joined by
/// hyphens, and a label is never a number.
///
/// A per-sample run is `ps-1`, which has the shape of a label and a value and
/// would otherwise be read as the patient `1`. Its own sub-directories are the
/// samples, and those are the subjects — `ps-1/patient-2_sample-1/…` files
/// correctly, because the report reads the directory a figure sits in.
fn is_run_directory(name: &str) -> bool {
    if let Some(id) = name.strip_prefix(PER_SAMPLE_PREFIX) {
        return !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
    }
    let mut parts = name.split('-');
    let three_numbers = (0..3).all(|_| {
        parts
            .next()
            .is_some_and(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    });
    three_numbers && parts.next().is_none()
}

/// How a per-sample run's directory is named — see
/// [`crate::niche_runs::PerSampleRun::directory`].
const PER_SAMPLE_PREFIX: &str = "ps-";

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
            // Runs are numbered, and a run directory is not a sample.
            "1",
            "2",
            "17",
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

    // -----------------------------------------------------------------------
    // A run directory is not a patient
    // -----------------------------------------------------------------------

    /// Step 3 names its directories after the three numbers of the run —
    /// `1-1-10` — or, for a per-sample run, `ps-1`. Read as a `label-value`
    /// pair, `1-1-10` yields the "patient" `1-10` and `ps-1` yields the
    /// "patient" `1`; a working directory with twelve runs in it produced
    /// twelve headings for patients that do not exist.
    #[test]
    fn a_run_directory_is_not_a_subject() {
        for run in ["1-1-1", "1-1-10", "2-0-3", "10-2-7", "ps-1", "ps-12"] {
            assert_eq!(
                from_directory(run),
                None,
                "`{run}` was taken for a patient"
            );
        }
    }

    /// And a directory that really does name a sample still does.
    #[test]
    fn a_sample_directory_is_still_a_subject() {
        assert_eq!(
            from_directory("patient-1_chunk-8"),
            Some(subject("1", Some("8")))
        );
        assert_eq!(from_directory("patient-3"), Some(subject("3", None)));
    }

    /// What separates the two is that a run is *three* numbers and a subject
    /// directory is a label and a value — and a label is never a number. Two
    /// numbers stay ambiguous, and nothing writes such a directory, so the
    /// `label-value` reading keeps them.
    #[test]
    fn the_rule_is_three_numbers_and_not_merely_hyphens() {
        assert!(from_directory("patient-1").is_some());
        assert_eq!(from_directory("1-1-1"), None);
        assert!(
            from_directory("patient-1_chunk-8").is_some(),
            "a real sample directory still reads"
        );
    }
}
