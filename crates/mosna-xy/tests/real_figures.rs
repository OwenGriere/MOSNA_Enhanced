//! The figures, drawn for real.
//!
//! Everything else in this crate checks what Rust *asks* for. This checks what
//! comes back: that the renderer is there, that every file the interface looks
//! for is written under the name it looks for, and that the images are figures
//! rather than blank canvases.
//!
//! These are the guarantees the `plotters` implementation used to carry in
//! `mosna-viz/tests/figures.rs`. The engine changed; the guarantees did not.
//!
//! # When the renderer is missing
//!
//! The tests say so and stop. A checkout that has not run
//! `pip install -e python` yet should not fail its whole suite for it — but CI
//! installs the renderer, so nothing here is quietly skipped where it counts.

use std::path::Path;

use mosna_core::colormap::make_cluster_cmap;
use mosna_core::niches::{NicheComposition, Normalize};
use mosna_io::SampleId;
use mosna_pipeline::FigureSink;
use mosna_xy::renderer::Renderer;
use mosna_xy::Figures;

/// A renderer, or a printed reason there is none.
fn renderer_is_available() -> bool {
    match Renderer::detect().check() {
        Ok(reported) => {
            println!("renderer: {}", reported.replace('\n', ", "));
            true
        }
        Err(error) => {
            eprintln!("skipping: no figure renderer ({error})");
            false
        }
    }
}

fn read_png(path: &Path) -> (u32, u32, Vec<[u8; 3]>) {
    let image = image::open(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
        .to_rgb8();
    let (width, height) = image.dimensions();
    let pixels = image.pixels().map(|p| [p[0], p[1], p[2]]).collect();
    (width, height, pixels)
}

/// How many distinct colours a figure uses — a blank canvas has one.
fn distinct_colours(pixels: &[[u8; 3]]) -> usize {
    let mut seen: std::collections::HashSet<[u8; 3]> = Default::default();
    for pixel in pixels {
        seen.insert(*pixel);
    }
    seen.len()
}

/// Whether a colour appears in the figure, allowing for anti-aliasing.
fn contains_colour(pixels: &[[u8; 3]], colour: [u8; 3], tolerance: i32) -> bool {
    pixels
        .iter()
        .any(|pixel| (0..3).all(|c| (pixel[c] as i32 - colour[c] as i32).abs() <= tolerance))
}

fn assert_is_a_figure(path: &Path) {
    assert!(path.is_file(), "{} was not written", path.display());
    let (width, height, pixels) = read_png(path);
    assert!(
        width > 100 && height > 100,
        "{width}x{height} is not a figure"
    );
    assert!(distinct_colours(&pixels) > 4, "{} is blank", path.display());
}

/// A statistics table shaped like the one step 2 writes.
fn assortativity_table() -> (Vec<String>, Vec<(String, Vec<f64>)>) {
    let columns = vec![
        "# total".to_string(),
        "% A".to_string(),
        "% B".to_string(),
        "assort Z".to_string(),
        "A - A Z".to_string(),
        "A - B Z".to_string(),
        "B - B Z".to_string(),
    ];
    let rows = vec![
        (
            "patient-1_sample-1".to_string(),
            vec![100.0, 0.6, 0.4, 3.0, 4.0, -2.0, 1.5],
        ),
        (
            "patient-2_sample-1".to_string(),
            vec![120.0, 0.4, 0.6, 1.0, 2.0, -1.0, 0.5],
        ),
        (
            "patient-3_sample-1".to_string(),
            vec![90.0, 0.5, 0.5, 2.0, 3.0, -3.0, 2.5],
        ),
    ];
    (columns, rows)
}

/// Step 1's figure is named after the sample, because the interface groups the
/// gallery by parsing that name.
#[test]
fn the_network_figure_is_named_after_its_sample_and_shows_its_phenotypes() {
    if !renderer_is_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let figures = Figures::new(dir.path());

    let coords: Vec<[f64; 2]> = (0..30).map(|i| [(i % 6) as f64, (i / 6) as f64]).collect();
    let labels: Vec<String> = (0..30)
        .map(|i| ["A", "B", "C"][i % 3].to_string())
        .collect();
    let pairs: Vec<(u32, u32)> = (0..29).map(|i| (i, i + 1)).collect();

    figures
        .network(
            &SampleId {
                patient: "4".to_string(),
                sample: Some("9".to_string()),
            },
            "patient",
            Some("sample"),
            &coords,
            &pairs,
            &labels,
            dir.path(),
        )
        .unwrap();
    figures.render().unwrap();

    let path = dir.path().join("net_4-9.png");
    assert_is_a_figure(&path);

    // A figure that drew only its edges would still be a valid PNG. The
    // phenotypes have to be *there*, in the palette they were given.
    let (_, _, pixels) = read_png(&path);
    for colour in make_cluster_cmap(3) {
        assert!(
            contains_colour(&pixels, colour, 40),
            "the phenotype coloured {colour:?} is not in the figure"
        );
    }

    assert!(
        dir.path().join("net_4-9.html").is_file(),
        "the interactive chart is missing"
    );
}

#[test]
fn a_single_level_network_figure_omits_the_sample() {
    if !renderer_is_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let figures = Figures::new(dir.path());

    figures
        .network(
            &SampleId::patient_only("1"),
            "patient",
            None,
            &[[0.0, 0.0], [1.0, 1.0]],
            &[(0, 1)],
            &["A".to_string(), "B".to_string()],
            dir.path(),
        )
        .unwrap();
    figures.render().unwrap();

    assert!(dir.path().join("net_1.png").is_file());
    assert!(!dir.path().join("net_1-.png").exists());
}

/// The whole inventory of step 2, under the names and in the directories the
/// interface scans.
#[test]
fn every_assortativity_figure_is_written_where_the_gallery_looks_for_it() {
    if !renderer_is_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let figures = Figures::new(dir.path());
    let (columns, rows) = assortativity_table();

    figures.assortativity(&columns, &rows, dir.path()).unwrap();
    figures.render().unwrap();

    for name in [
        "abundance.png",
        "Assortativity_heatmap_with_dendrogram.png",
        "Assortativity_heatmap_with_dendrogram_without_auto_paired_pheno.png",
        "Assortativity_heatmap_across_patient.png",
        "Assortativity_heatmap_across_patient_without_auto_paired_pheno.png",
    ] {
        assert_is_a_figure(&dir.path().join(name));
    }

    for folder in ["assort_files", "assort_files_without_diag"] {
        for sample in ["1-1", "2-1", "3-1"] {
            assert_is_a_figure(
                &dir.path()
                    .join(folder)
                    .join(format!("heatmap_zscore_{sample}.png")),
            );
        }
    }
}

#[test]
fn assortativity_figures_survive_an_empty_table() {
    if !renderer_is_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let figures = Figures::new(dir.path());

    figures.assortativity(&[], &[], dir.path()).unwrap();
    figures.render().expect("an empty table is not a failure");
}

#[test]
fn the_niche_figures_carry_the_normalisation_in_their_name() {
    if !renderer_is_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let figures = Figures::new(dir.path());

    let composition = NicheComposition {
        phenotypes: vec!["A".to_string(), "B".to_string(), "C".to_string()],
        niches: vec![0, 1, 2],
        counts: vec![1.0, 5.0, 3.0, 2.0, 9.0, 4.0, 7.0, 1.0, 6.0],
    };
    let niches: Vec<u32> = (0..30).map(|i| (i / 10) as u32).collect();

    figures
        .niche_composition(&composition, &niches, Normalize::Total, dir.path())
        .unwrap();
    figures.render().unwrap();

    assert_is_a_figure(&dir.path().join("Niches_Aggregated_Composition_total.png"));
    assert_is_a_figure(&dir.path().join("Niches_Histogram.png"));
}

/// The histogram's bars are the niche palette, which is what lets a bar here be
/// matched to a blob in the embedding.
#[test]
fn the_histogram_is_coloured_by_niche() {
    if !renderer_is_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let figures = Figures::new(dir.path());

    let composition = NicheComposition {
        phenotypes: vec!["A".to_string()],
        niches: vec![0, 1],
        counts: vec![1.0, 2.0],
    };
    let niches: Vec<u32> = (0..40).map(|i| (i % 4) as u32).collect();

    figures
        .niche_composition(&composition, &niches, Normalize::Total, dir.path())
        .unwrap();
    figures.render().unwrap();

    let (_, _, pixels) = read_png(&dir.path().join("Niches_Histogram.png"));
    for colour in make_cluster_cmap(4) {
        assert!(
            contains_colour(&pixels, colour, 40),
            "a niche coloured {colour:?} has no bar"
        );
    }
}

#[test]
fn niche_figures_survive_an_empty_composition() {
    if !renderer_is_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let figures = Figures::new(dir.path());
    let composition = NicheComposition {
        phenotypes: Vec::new(),
        niches: Vec::new(),
        counts: Vec::new(),
    };

    figures
        .niche_composition(&composition, &[], Normalize::Total, dir.path())
        .unwrap();
    figures
        .render()
        .expect("an empty composition is not a failure");
}

#[test]
fn the_embedding_is_written_and_coloured_by_cluster() {
    if !renderer_is_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let figures = Figures::new(dir.path());

    let mut embedding = Vec::new();
    let mut labels = Vec::new();
    for cluster in 0..3u32 {
        for step in 0..20 {
            embedding.push(cluster as f64 * 10.0 + (step % 5) as f64 * 0.2);
            embedding.push((step / 5) as f64 * 0.4);
            labels.push(cluster);
        }
    }

    figures
        .embedding(&embedding, 2, &labels, dir.path())
        .unwrap();
    figures.render().unwrap();

    let path = dir.path().join("cluster_labels.png");
    assert_is_a_figure(&path);

    let (_, _, pixels) = read_png(&path);
    for colour in make_cluster_cmap(3) {
        assert!(
            contains_colour(&pixels, colour, 40),
            "the cluster coloured {colour:?} is not in the projection"
        );
    }
}

#[test]
fn a_one_dimensional_embedding_is_skipped_rather_than_drawn_wrong() {
    if !renderer_is_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let figures = Figures::new(dir.path());

    figures
        .embedding(&[1.0, 2.0, 3.0], 1, &[0, 1, 2], dir.path())
        .unwrap();
    figures.render().unwrap();

    assert!(!dir.path().join("cluster_labels.png").exists());
}

/// The queue is scratch space. Leaving it behind would put a folder of
/// intermediate documents next to the figures the user came for.
#[test]
fn the_queue_does_not_outlive_the_figures() {
    if !renderer_is_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let figures = Figures::new(dir.path());

    figures
        .network(
            &SampleId::patient_only("1"),
            "patient",
            None,
            &[[0.0, 0.0], [1.0, 1.0]],
            &[(0, 1)],
            &["A".to_string(), "A".to_string()],
            dir.path(),
        )
        .unwrap();
    figures.render().unwrap();

    assert!(!dir.path().join(mosna_xy::spec::QUEUE_DIRECTORY).exists());
}
