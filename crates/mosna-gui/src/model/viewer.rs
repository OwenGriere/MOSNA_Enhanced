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

/// How a figure is named in the gallery's tab strip.
///
/// # Why the file name is not enough
///
/// Every niche run writes the same three figures — `cluster_labels`,
/// `Niches_Histogram`, `Niches_Aggregated_Composition_total` — into a directory
/// named after the run. The gallery labelled each tab with the file stem alone,
/// so a working directory holding a dozen runs offered a dozen identical tabs
/// and no way to tell which run any of them belonged to. Comparing runs is the
/// reason the numbering exists, and the gallery was the one place it did not
/// appear.
///
/// The label is the path from the analysis directory down to the file, with the
/// extension dropped: `1-1-3 · cluster_labels`, and for a per-sample run
/// `2-1-1 · patient-1_chunk-8 · Niches_Histogram`.
pub fn figure_label(analysis_dir: &Path, figure: &Path) -> String {
    let stem = figure
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let Ok(relative) = figure.strip_prefix(analysis_dir) else {
        return stem;
    };
    let mut parts: Vec<String> = relative
        .parent()
        .into_iter()
        .flat_map(|parent| parent.components())
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.push(stem);
    parts.join(" · ")
}

/// Order figures the way a reader counts, not the way a byte comparison does.
///
/// `1-1-10` sorts before `1-1-2` as text, so a gallery of a dozen runs came out
/// in an order that looks like a mistake — and so did the report, and the file
/// manager. Zero-padding the numbers would fix the sort and break every
/// `niches_1-1-2` column already written into a cohort, so the ordering is
/// fixed where it belongs: at the comparison.
pub fn sort_figures(figures: &mut [PathBuf]) {
    figures.sort_by_cached_key(|path| sort_key(path));
}

/// A path as a sequence of components, each split into its runs of digits and
/// non-digits so that `10` compares as ten rather than as `"10"`.
fn sort_key(path: &Path) -> Vec<Vec<Chunk>> {
    path.components()
        .map(|component| chunks(&component.as_os_str().to_string_lossy()))
        .collect()
}

/// One stretch of a name: a number, or the text between numbers.
///
/// Numbers sort before text at the same position, which only matters for names
/// that mix the two in different ways and never arises here.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Chunk {
    Number(u64),
    Text(String),
}

fn chunks(name: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut rest = name;
    while !rest.is_empty() {
        let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if digits > 0 {
            // A number too long to hold is text: it is not a run number, and
            // saturating would make two different names compare equal.
            match rest[..digits].parse::<u64>() {
                Ok(value) => chunks.push(Chunk::Number(value)),
                Err(_) => chunks.push(Chunk::Text(rest[..digits].to_string())),
            }
            rest = &rest[digits..];
            continue;
        }
        let text = rest
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(rest.len());
        chunks.push(Chunk::Text(rest[..text].to_string()));
        rest = &rest[text..];
    }
    chunks
}

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
    sort_figures(&mut images);
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
    // By number, so `1-1-10` follows `1-1-2` rather than preceding it.
    sort_figures(&mut directories);

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

    // -----------------------------------------------------------------------
    // Telling one run's figures from another's
    // -----------------------------------------------------------------------

    /// Every run writes the same three file names into a directory of its own.
    /// The gallery labelled each tab with the file stem alone, so a working
    /// directory holding a dozen runs gave a dozen tabs called
    /// `cluster_labels` — and nothing said which run any of them came from.
    #[test]
    fn a_figure_is_labelled_by_the_run_it_came_from() {
        let root = Path::new("/w/Niche_Analysis");
        assert_eq!(
            figure_label(root, &root.join("1-1-3/cluster_labels.png")),
            "1-1-3 · cluster_labels"
        );
        assert_eq!(
            figure_label(
                root,
                &root.join("2-1-1/patient-1_chunk-8/Niches_Histogram.png")
            ),
            "2-1-1 · patient-1_chunk-8 · Niches_Histogram"
        );
    }

    /// A figure sitting directly in the analysis directory has no run to name,
    /// and keeps the plain stem it always had.
    #[test]
    fn a_figure_outside_any_run_keeps_its_own_name() {
        let root = Path::new("/w/Assortativity");
        assert_eq!(figure_label(root, &root.join("abundance.png")), "abundance");
    }

    /// A path from somewhere else entirely still yields something to click on
    /// rather than an empty tab.
    #[test]
    fn a_figure_from_elsewhere_still_has_a_label() {
        assert_eq!(
            figure_label(Path::new("/w/Niche_Analysis"), Path::new("/other/x.png")),
            "x"
        );
    }

    // -----------------------------------------------------------------------
    // Ordering
    // -----------------------------------------------------------------------

    /// `1-1-10` sorts before `1-1-2` as text, so a gallery of a dozen runs came
    /// out in an order that looks like a mistake. The numbers are read as
    /// numbers.
    #[test]
    fn runs_are_ordered_by_number_and_not_by_spelling() {
        let mut paths: Vec<PathBuf> = ["1-1-2", "1-1-10", "1-1-1", "2-1-1", "1-2-1"]
            .iter()
            .map(|run| PathBuf::from(format!("/w/Niche_Analysis/{run}/cluster_labels.png")))
            .collect();
        sort_figures(&mut paths);

        let order: Vec<String> = paths
            .iter()
            .map(|p| p.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(order, vec!["1-1-1", "1-1-2", "1-1-10", "1-2-1", "2-1-1"]);
    }

    /// Ordinary names are still ordered the way they always were.
    #[test]
    fn names_that_are_not_runs_keep_their_usual_order() {
        let mut paths: Vec<PathBuf> = ["b.png", "a.png", "c.png"]
            .iter()
            .map(|n| PathBuf::from(format!("/w/x/{n}")))
            .collect();
        sort_figures(&mut paths);
        assert_eq!(
            paths
                .iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["a.png", "b.png", "c.png"]
        );
    }
}
