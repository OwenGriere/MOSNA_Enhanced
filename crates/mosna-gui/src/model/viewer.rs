//! Collecting the figures an analysis produced — port of
//! `MosnaGUI._collect_analysis_images` and friends.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Figures of one analysis: those about the cohort, and those about a patient.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnalysisImages {
    pub global: Vec<PathBuf>,
    pub patients: BTreeMap<String, Vec<PathBuf>>,
}

/// Everything the viewer can show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnalysisImageSet {
    pub tysserand: AnalysisImages,
    pub assortativity: AnalysisImages,
    pub niches: AnalysisImages,
}

/// Image extensions the viewer displays.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg"];

/// Gather every figure under a working directory.
///
/// # One corrected path
///
/// The Python looks for the niche figures under `Niches_Analysis`, while
/// `niche_analysis.py` writes them to `Niche_Analysis` — so the viewer's Niches
/// tab is always empty. The correct directory is used here.
pub fn collect_analysis_images(working_dir: &Path) -> AnalysisImageSet {
    AnalysisImageSet {
        tysserand: collect_tysserand(&working_dir.join("Tysserand_Network")),
        assortativity: collect_assortativity(&working_dir.join("Assortativity")),
        niches: collect_niches(&working_dir.join("Niche_Analysis")),
    }
}

/// Step 1 writes one `net_{patient}-{sample}.png` per sample.
fn collect_tysserand(folder: &Path) -> AnalysisImages {
    let mut images = AnalysisImages::default();
    for path in list_images(folder) {
        match patient_of(&path, "net") {
            Some(patient) => images.patients.entry(patient).or_default().push(path),
            // A file that does not follow the convention is still worth showing.
            None => images.global.push(path),
        }
    }
    images
}

/// Step 2 writes cohort figures at the top level and one heatmap per sample
/// under `assort_files`.
fn collect_assortativity(folder: &Path) -> AnalysisImages {
    let mut images = AnalysisImages {
        global: list_images(folder),
        ..Default::default()
    };

    for sub in ["assort_files", "assort_files_without_diag"] {
        for path in list_images(&folder.join(sub)) {
            match patient_of(&path, "heatmap_zscore") {
                Some(patient) => images.patients.entry(patient).or_default().push(path),
                None => images.global.push(path),
            }
        }
    }
    images
}

/// Step 3 writes its figures inside a saving directory, itself nested under
/// `Aggregation` or `Per_sample`. The whole tree is walked, because the saving
/// directory is named by the user.
fn collect_niches(folder: &Path) -> AnalysisImages {
    let mut images = AnalysisImages::default();
    for path in walk_images(folder, 4) {
        match patient_of(&path, "niches") {
            Some(patient) => images.patients.entry(patient).or_default().push(path),
            None => images.global.push(path),
        }
    }
    images
}

/// The images directly inside `folder`, sorted by name.
fn list_images(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };

    let mut images: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_image(path))
        .collect();
    images.sort();
    images
}

/// Every image under `folder`, descending at most `depth` levels.
fn walk_images(folder: &Path, depth: usize) -> Vec<PathBuf> {
    let mut images = list_images(folder);
    if depth == 0 {
        return images;
    }

    let Ok(entries) = std::fs::read_dir(folder) else {
        return images;
    };
    let mut directories: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    directories.sort();

    for directory in directories {
        images.extend(walk_images(&directory, depth - 1));
    }
    images
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// The patient a figure belongs to, from a `{prefix}_{patient}-{sample}` name.
///
/// Port of `_extract_patient_sample`: strip the prefix, then split on `-` and
/// take the first part.
fn patient_of(path: &Path, prefix: &str) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let suffix = stem.strip_prefix(prefix)?.strip_prefix('_')?;
    let patient = suffix.split('-').next()?.trim();
    if patient.is_empty() {
        None
    } else {
        Some(patient.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: PathBuf) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn network_figures_are_grouped_by_patient() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Tysserand_Network/net_1-1.png"));
        touch(dir.path().join("Tysserand_Network/net_1-2.png"));
        touch(dir.path().join("Tysserand_Network/net_2-1.png"));

        let images = collect_analysis_images(dir.path());
        assert_eq!(images.tysserand.patients["1"].len(), 2);
        assert_eq!(images.tysserand.patients["2"].len(), 1);
    }

    #[test]
    fn a_single_level_network_figure_is_grouped_too() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Tysserand_Network/net_7.png"));

        let images = collect_analysis_images(dir.path());
        assert!(images.tysserand.patients.contains_key("7"));
    }

    #[test]
    fn assortativity_separates_cohort_and_per_sample_figures() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Assortativity/abundance.png"));
        touch(
            dir.path()
                .join("Assortativity/assort_files/heatmap_zscore_3-1.png"),
        );
        touch(
            dir.path()
                .join("Assortativity/assort_files_without_diag/heatmap_zscore_3-1.png"),
        );

        let images = collect_analysis_images(dir.path());
        assert_eq!(images.assortativity.global.len(), 1);
        assert_eq!(
            images.assortativity.patients["3"].len(),
            2,
            "both heatmap variants belong to the patient"
        );
    }

    #[test]
    fn niche_figures_are_found_under_the_directory_step_three_writes_to() {
        let dir = tempfile::tempdir().unwrap();
        touch(
            dir.path()
                .join("Niche_Analysis/Aggregation/run/Niches_Histogram.png"),
        );
        let images = collect_analysis_images(dir.path());
        assert_eq!(images.niches.global.len(), 1);
    }

    #[test]
    fn per_sample_niche_figures_are_found_too() {
        let dir = tempfile::tempdir().unwrap();
        touch(
            dir.path()
                .join("Niche_Analysis/Per_sample/run/patient-1_sample-1/Niches_Histogram.png"),
        );
        let images = collect_analysis_images(dir.path());
        assert_eq!(images.niches.global.len(), 1);
    }

    #[test]
    fn non_images_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Assortativity/net_stat.csv"));
        touch(dir.path().join("Assortativity/abundance.png"));

        let images = collect_analysis_images(dir.path());
        assert_eq!(images.assortativity.global.len(), 1);
    }

    #[test]
    fn an_empty_working_directory_yields_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let images = collect_analysis_images(dir.path());
        assert_eq!(images, AnalysisImageSet::default());
    }

    #[test]
    fn listing_is_sorted_so_the_tabs_do_not_shuffle() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["net_3-1.png", "net_1-1.png", "net_2-1.png"] {
            touch(dir.path().join("Tysserand_Network").join(name));
        }
        let images = collect_analysis_images(dir.path());
        let patients: Vec<&String> = images.tysserand.patients.keys().collect();
        assert_eq!(patients, vec!["1", "2", "3"]);
    }
}
