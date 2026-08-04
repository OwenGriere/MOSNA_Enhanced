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
///
/// Step 1's figures are not among them. Its only output was a picture of the
/// network, and the Network tab draws the network itself — from the same
/// files, at any zoom, with every attribute still readable at the pointer.
/// Scanning `Tysserand_Network` to offer a flat copy of that would be offering
/// the worse of the two.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnalysisImageSet {
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
        assortativity: collect_assortativity(&working_dir.join("Assortativity")),
        niches: collect_niches(&working_dir.join("Niche_Analysis")),
    }
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

/// The URI the image loader wants for a figure on disk.
///
/// `egui_extras`' file loader takes what follows `file://` as a path, with one
/// concession to Windows: a leading slash is stripped, and anything else is
/// read as the hostname of a UNC share. So `file://C:\runs\fig.png` — which is
/// what pasting a Windows path after the scheme produces — is looked for at
/// `\\C:\runs\fig.png`, on a machine that does not exist, and the figure never
/// appears. The path is put in the shape the loader parses instead.
pub fn file_uri(path: &Path) -> String {
    file_uri_of(&path.display().to_string(), cfg!(windows))
}

/// The rule itself, with the platform as an argument so both branches can be
/// tested from either one.
fn file_uri_of(path: &str, windows: bool) -> String {
    // Only on Windows: a backslash is an ordinary character in a Unix file
    // name, and rewriting it there would break the very paths it means to fix.
    let path = if windows {
        path.replace('\\', "/")
    } else {
        path.to_string()
    };
    let separator = if path.starts_with('/') { "" } else { "/" };
    format!("file://{separator}{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Unix path is already rooted, so it needs the third slash and nothing
    /// else — the loader percent-decodes nothing, so nothing may be encoded.
    #[test]
    fn a_unix_figure_becomes_a_three_slash_uri() {
        let uri = file_uri_of("/home/user/Niche_Analysis/cluster labels.png", false);
        assert_eq!(uri, "file:///home/user/Niche_Analysis/cluster labels.png");
        assert_eq!(
            uri.strip_prefix("file://"),
            Some("/home/user/Niche_Analysis/cluster labels.png"),
            "what the loader reads back must be the path it was given"
        );
    }

    /// A Windows path is not rooted at a slash and is spelled with backslashes;
    /// both have to be fixed, or the loader goes looking for a network share.
    #[test]
    fn a_windows_figure_becomes_a_drive_letter_uri() {
        let uri = file_uri_of(r"C:\Users\owen\runs\fig.png", true);
        assert_eq!(uri, "file:///C:/Users/owen/runs/fig.png");

        // The loader's own parsing: strip the scheme, then the leading slash.
        let path = uri
            .strip_prefix("file://")
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap();
        assert_eq!(path, "C:/Users/owen/runs/fig.png");
    }

    /// A backslash is a legal character in a Unix file name.
    #[test]
    fn a_unix_name_containing_a_backslash_is_left_alone() {
        assert_eq!(
            file_uri_of(r"/data/odd\name.png", false),
            r"file:///data/odd\name.png"
        );
    }

    fn touch(path: PathBuf) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    /// Step 1's figures are the Network tab's business now, and nothing here
    /// should go looking for them — a gallery of them beside a tab that draws
    /// the same networks live is two answers to one question.
    #[test]
    fn step_ones_figures_are_not_collected() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Tysserand_Network/net_1-1.png"));
        touch(dir.path().join("Tysserand_Network/net_2-1.png"));

        let images = collect_analysis_images(dir.path());
        assert_eq!(images, AnalysisImageSet::default());
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
        for name in ["heatmap_zscore_3-1.png", "heatmap_zscore_1-1.png"] {
            touch(dir.path().join("Assortativity/assort_files").join(name));
        }
        let images = collect_analysis_images(dir.path());
        let patients: Vec<&String> = images.assortativity.patients.keys().collect();
        assert_eq!(patients, vec!["1", "3"]);
    }
}
