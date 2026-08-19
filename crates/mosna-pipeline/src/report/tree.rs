//! Reading the output directory.
//!
//! The report is built from what is on disk, not from what an analysis says it
//! wrote: a run that was interrupted, a folder copied from a cluster, a figure
//! deleted by hand — the report describes the directory as it actually is.

use std::path::{Path, PathBuf};

/// Extensions the gallery treats as an image.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg"];

/// One figure: the image, and the interactive chart beside it.
///
/// Either may be missing. `xy` writes both, but a directory that has been
/// tidied by hand, or produced by an older version, may hold only one — and a
/// report that refused to mention a figure because its sibling was missing
/// would be hiding the very thing the reader is looking for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Figure {
    /// The file name without its extension, which is what identifies the
    /// sample or the variant to a reader.
    pub stem: String,
    /// Relative to the working directory, or `None`.
    pub image: Option<PathBuf>,
    pub chart: Option<PathBuf>,
}

/// The figures of one directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gallery {
    /// Relative to the working directory. Empty for the root.
    pub directory: PathBuf,
    pub figures: Vec<Figure>,
}

/// A node of the directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Directory { name: String, children: Vec<Entry> },
    File { name: String, bytes: u64 },
}

/// Everything the report needs about the output directory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Output {
    /// Every directory holding figures, in the order they are shown.
    pub galleries: Vec<Gallery>,
    /// The whole listing, figures and data files alike.
    pub tree: Vec<Entry>,
    pub files: usize,
    pub bytes: u64,
}

/// Read `working_dir`, ignoring `report_name` and anything hidden.
pub fn scan(working_dir: &Path, report_name: &str) -> Output {
    let mut output = Output::default();
    output.tree = walk(working_dir, Path::new(""), report_name, &mut output);
    // The galleries come out of the walk in the order the tree was descended,
    // which is the order the report shows them in.
    output
}

/// One level: its listing, and the galleries and totals it contributes.
fn walk(absolute: &Path, relative: &Path, report_name: &str, output: &mut Output) -> Vec<Entry> {
    let Ok(entries) = std::fs::read_dir(absolute) else {
        // A directory that cannot be read is reported as empty. A report is the
        // wrong place to discover a permission error, and the rest of it is
        // still worth having.
        return Vec::new();
    };

    let mut directories: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<(String, u64)> = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Anything hidden is scratch space: the figure queue, and whatever a
        // tool or an editor left behind.
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            directories.push((name, path));
        } else if path.is_file() {
            if relative.as_os_str().is_empty() && name == report_name {
                continue;
            }
            let bytes = entry.metadata().map(|data| data.len()).unwrap_or(0);
            files.push((name, bytes));
        }
    }

    directories.sort();
    files.sort();

    // The figures of *this* directory, before descending: a gallery is listed
    // before the galleries it contains, as the tree reads.
    let figures = figures_of(&files, relative);
    if !figures.is_empty() {
        output.galleries.push(Gallery {
            directory: relative.to_path_buf(),
            figures,
        });
    }

    let mut listing: Vec<Entry> = Vec::with_capacity(directories.len() + files.len());
    for (name, path) in directories {
        let children = walk(&path, &relative.join(&name), report_name, output);
        listing.push(Entry::Directory { name, children });
    }
    for (name, bytes) in files {
        output.files += 1;
        output.bytes += bytes;
        listing.push(Entry::File { name, bytes });
    }
    listing
}

/// Pair the images and the charts of one directory by their stem.
fn figures_of(files: &[(String, u64)], relative: &Path) -> Vec<Figure> {
    let mut figures: Vec<Figure> = Vec::new();

    for (name, _) in files {
        let path = Path::new(name);
        let Some(extension) = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
        else {
            continue;
        };
        let is_image = IMAGE_EXTENSIONS.contains(&extension.as_str());
        let is_chart = extension == "html";
        if !is_image && !is_chart {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let full = relative.join(name);

        match figures.iter_mut().find(|figure| figure.stem == stem) {
            Some(figure) => {
                if is_image {
                    figure.image = Some(full);
                } else {
                    figure.chart = Some(full);
                }
            }
            None => figures.push(Figure {
                stem: stem.to_string(),
                image: is_image.then(|| full.clone()),
                chart: is_chart.then_some(full),
            }),
        }
    }

    figures.sort_by(|a, b| a.stem.cmp(&b.stem));
    figures
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: PathBuf, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn scanned(dir: &Path) -> Output {
        scan(dir, "report.html")
    }

    #[test]
    fn an_empty_directory_yields_an_empty_report() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(scanned(dir.path()), Output::default());
    }

    /// The sections of the report are the directories of the output, which is
    /// what makes a reader able to find a figure again on disk.
    #[test]
    fn figures_are_grouped_by_the_directory_they_are_in() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Assortativity/abundance.png"), b"x");
        touch(
            dir.path()
                .join("Assortativity/assort_files/heatmap_zscore_1-8.png"),
            b"x",
        );
        touch(dir.path().join("Tysserand_Network/net_1-8.png"), b"x");

        let output = scanned(dir.path());
        let directories: Vec<&Path> = output
            .galleries
            .iter()
            .map(|gallery| gallery.directory.as_path())
            .collect();

        assert_eq!(
            directories,
            vec![
                Path::new("Assortativity"),
                Path::new("Assortativity/assort_files"),
                Path::new("Tysserand_Network"),
            ],
            "the galleries are not in the order of the tree"
        );
    }

    /// A PNG and the chart beside it are two views of one figure. Listing them
    /// as two entries would double the report and say nothing twice.
    #[test]
    fn an_image_and_the_chart_beside_it_are_one_figure() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Assortativity/abundance.png"), b"x");
        touch(dir.path().join("Assortativity/abundance.html"), b"x");

        let output = scanned(dir.path());
        assert_eq!(output.galleries.len(), 1);
        assert_eq!(output.galleries[0].figures.len(), 1);

        let figure = &output.galleries[0].figures[0];
        assert_eq!(figure.stem, "abundance");
        assert_eq!(
            figure.image.as_deref(),
            Some(Path::new("Assortativity/abundance.png"))
        );
        assert_eq!(
            figure.chart.as_deref(),
            Some(Path::new("Assortativity/abundance.html"))
        );
    }

    #[test]
    fn a_figure_with_only_an_image_is_still_a_figure() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Tysserand_Network/net_1.png"), b"x");

        let figure = &scanned(dir.path()).galleries[0].figures[0];
        assert!(figure.image.is_some());
        assert_eq!(figure.chart, None);
    }

    /// And the other way round: the interactive chart is the more useful of the
    /// two, so a directory holding only charts is a gallery.
    #[test]
    fn a_figure_with_only_a_chart_is_still_a_figure() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Tysserand_Network/net_1.html"), b"x");

        let output = scanned(dir.path());
        assert_eq!(output.galleries.len(), 1);
        let figure = &output.galleries[0].figures[0];
        assert_eq!(figure.image, None);
        assert!(figure.chart.is_some());
    }

    /// Regenerating a report must not make the report grow a section about
    /// itself.
    #[test]
    fn the_report_does_not_describe_itself() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("report.html"), b"<html>");
        touch(dir.path().join("Tysserand_Network/net_1.png"), b"x");

        let output = scanned(dir.path());
        assert_eq!(
            output.galleries.len(),
            1,
            "the report was taken for a figure"
        );
        assert_eq!(output.files, 1, "the report counted itself");
    }

    /// The figure queue is scratch space with a dot in front of its name, and
    /// so is anything else a tool leaves behind.
    #[test]
    fn hidden_directories_are_left_out() {
        let dir = tempfile::tempdir().unwrap();
        touch(
            dir.path().join(".mosna-figures/00000-network/figure.json"),
            b"{}",
        );
        touch(dir.path().join("Tysserand_Network/net_1.png"), b"x");

        let output = scanned(dir.path());
        assert_eq!(output.files, 1);
        assert_eq!(output.tree.len(), 1);
    }

    /// The listing is the whole directory, not only its figures: `net_stat.csv`
    /// and the intermediate networks are what someone reading the report will
    /// want to find next.
    #[test]
    fn the_listing_carries_every_file_with_its_size() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Assortativity/net_stat.csv"), b"12345");
        touch(
            dir.path()
                .join("temp/net_dir_mosna/nodes_patient-1.parquet"),
            b"xx",
        );

        let output = scanned(dir.path());
        assert_eq!(output.files, 2);
        assert_eq!(output.bytes, 7);
        assert!(output.galleries.is_empty(), "a table is not a figure");

        let names: Vec<&str> = output
            .tree
            .iter()
            .map(|entry| match entry {
                Entry::Directory { name, .. } => name.as_str(),
                Entry::File { name, .. } => name.as_str(),
            })
            .collect();
        assert_eq!(names, vec!["Assortativity", "temp"]);
    }

    /// Nested, so the report shows the shape of the directory rather than a
    /// flat list of paths.
    #[test]
    fn the_listing_is_nested() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("a/b/c.txt"), b"x");

        let output = scanned(dir.path());
        let Entry::Directory { name, children } = &output.tree[0] else {
            panic!("expected a directory");
        };
        assert_eq!(name, "a");
        let Entry::Directory { name, children } = &children[0] else {
            panic!("expected a nested directory");
        };
        assert_eq!(name, "b");
        assert_eq!(
            children[0],
            Entry::File {
                name: "c.txt".to_string(),
                bytes: 1
            }
        );
    }

    /// Relative, so the report and the figures can be copied elsewhere
    /// together and still work.
    #[test]
    fn figure_paths_are_relative_to_the_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        touch(
            dir.path()
                .join("Niche_Analysis/Aggregation/run/Niches_Histogram.png"),
            b"x",
        );

        let figure = &scanned(dir.path()).galleries[0].figures[0];
        assert_eq!(
            figure.image.as_deref(),
            Some(Path::new(
                "Niche_Analysis/Aggregation/run/Niches_Histogram.png"
            )),
            "an absolute path would break the moment the folder moved"
        );
    }

    #[test]
    fn figures_inside_a_directory_are_sorted_by_name() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["net_2-1.png", "net_1-1.png", "net_10-1.png"] {
            touch(dir.path().join("Tysserand_Network").join(name), b"x");
        }
        let output = scanned(dir.path());
        let stems: Vec<&str> = output.galleries[0]
            .figures
            .iter()
            .map(|figure| figure.stem.as_str())
            .collect();
        assert_eq!(stems, vec!["net_1-1", "net_10-1", "net_2-1"]);
    }

    /// A directory that cannot be read is reported as empty rather than
    /// panicking: a report is the wrong place to discover a permission error.
    #[test]
    fn an_unreadable_directory_is_survived() {
        assert_eq!(
            scan(Path::new("/nowhere/at/all"), "report.html"),
            Output::default()
        );
    }
}
