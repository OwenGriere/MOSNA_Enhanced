//! Tests of the figure generation, written before the implementation.
//!
//! A figure cannot be asserted pixel for pixel against matplotlib, and trying
//! would pin the layout rather than the meaning. What is asserted instead:
//!
//! * the colour maps reproduce matplotlib's exactly, because a diverging map
//!   whose centre is not white makes a z-score of zero look like a signal;
//! * the normalisations place `vcenter` at the middle, for the same reason;
//! * every figure the Python writes is written, under the same name, in the
//!   same directory — that naming is what the interface scans for;
//! * the images are valid PNGs of the expected size and are not blank;
//! * the colours a figure was given actually appear in it.

use std::path::{Path, PathBuf};

use mosna_core::niches::{make_niches_composition, Normalize};
use mosna_io::SampleId;
use mosna_viz::colormap::{blues, make_cluster_cmap, rd_bu_r, tab20};
use mosna_viz::norm::{SymLogNorm, TwoSlopeNorm};
use mosna_viz::Figures;

use mosna_pipeline::FigureSink;

/// Decode a PNG and return its dimensions and pixels.
fn read_png(path: &Path) -> (u32, u32, Vec<[u8; 3]>) {
    let image = image::open(path)
        .unwrap_or_else(|e| panic!("{} is not a readable image: {e}", path.display()))
        .to_rgb8();
    let (width, height) = image.dimensions();
    let pixels = image.pixels().map(|p| [p[0], p[1], p[2]]).collect();
    (width, height, pixels)
}

/// How many distinct colours a figure uses — a blank canvas has one.
fn distinct_colours(pixels: &[[u8; 3]]) -> usize {
    let mut seen: Vec<[u8; 3]> = pixels.to_vec();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

/// Whether a colour appears in the figure, allowing for anti-aliasing.
fn contains_colour(pixels: &[[u8; 3]], colour: [u8; 3], tolerance: i32) -> bool {
    pixels
        .iter()
        .any(|p| (0..3).all(|c| (p[c] as i32 - colour[c] as i32).abs() <= tolerance))
}

// ---------------------------------------------------------------------------
// Colour maps
// ---------------------------------------------------------------------------

/// `RdBu_r` is what every z-score figure uses. Its centre must be the near-white
/// matplotlib uses; a centre with any tint makes zero look like a signal.
#[test]
fn the_diverging_map_matches_matplotlib() {
    let map = rd_bu_r();

    // The ColorBrewer end points, reversed: blue at the bottom, red at the top.
    assert_eq!(
        map.sample(0.0),
        [0x05, 0x30, 0x61],
        "the low end must be blue"
    );
    assert_eq!(
        map.sample(1.0),
        [0x67, 0x00, 0x1f],
        "the high end must be red"
    );

    let centre = map.sample(0.5);
    assert_eq!(centre, [0xf7, 0xf7, 0xf7], "the centre must be near-white");
}

/// The map is continuous: neighbouring samples cannot jump.
#[test]
fn the_diverging_map_is_continuous() {
    let map = rd_bu_r();
    let mut previous = map.sample(0.0);
    for step in 1..=200 {
        let current = map.sample(step as f64 / 200.0);
        let jump: i32 = (0..3)
            .map(|c| (current[c] as i32 - previous[c] as i32).abs())
            .max()
            .unwrap();
        assert!(jump <= 12, "a jump of {jump} at {step}/200");
        previous = current;
    }
}

#[test]
fn the_sequential_map_matches_matplotlib() {
    let map = blues();
    assert_eq!(
        map.sample(0.0),
        [0xf7, 0xfb, 0xff],
        "Blues starts near-white"
    );
    assert_eq!(map.sample(1.0), [0x08, 0x30, 0x6b], "Blues ends dark blue");
}

/// The cluster palette is what colours the niches and the phenotypes. Its exact
/// order matters: two runs of the same data must colour the same cluster the
/// same way, and it must match the Python so figures stay comparable.
#[test]
fn the_cluster_palette_matches_the_python() {
    let ten = make_cluster_cmap(10);
    assert_eq!(ten.len(), 10);
    assert_eq!(
        ten[0],
        [0x1F, 0x77, 0xB4],
        "the first colour is the tab10 blue"
    );
    assert_eq!(ten[9], [0x7F, 0x7F, 0x7F], "grey goes last");

    // Above ten, the pale companions are appended, saturated first.
    let fifteen = make_cluster_cmap(15);
    assert_eq!(fifteen[..10], ten[..]);
    assert_eq!(fifteen[10], [0xAE, 0xC7, 0xE8]);

    // Above twenty, four bright colours extend the palette.
    let twenty_four = make_cluster_cmap(24);
    assert_eq!(twenty_four[20], [0x00, 0xFF, 0xFF]);
    assert_eq!(twenty_four.len(), 24);
}

/// More clusters than the palette holds must still get a colour, by cycling —
/// which is what `clusters_cmap[i % n_colors]` does in the Python.
#[test]
fn the_cluster_palette_cycles_when_exhausted() {
    let many = make_cluster_cmap(60);
    assert_eq!(many.len(), 60);
    assert_eq!(many[0], many[24], "the palette must wrap around");
}

#[test]
fn the_abundance_palette_is_tab20() {
    let palette = tab20();
    assert_eq!(palette.len(), 20);
    assert_eq!(palette[0], [0x1f, 0x77, 0xb4]);
    assert_eq!(palette[1], [0xae, 0xc7, 0xe8]);
}

// ---------------------------------------------------------------------------
// Normalisations
// ---------------------------------------------------------------------------

/// A z-score figure is only readable if zero sits at the centre of the map.
#[test]
fn the_two_slope_norm_puts_the_centre_in_the_middle() {
    // An asymmetric range, which is the normal case for z-scores.
    let norm = TwoSlopeNorm::new(-2.0, 0.0, 8.0);

    assert_eq!(norm.normalise(-2.0), 0.0);
    assert_eq!(norm.normalise(0.0), 0.5);
    assert_eq!(norm.normalise(8.0), 1.0);

    // Each half is linear on its own scale.
    assert_eq!(norm.normalise(-1.0), 0.25);
    assert_eq!(norm.normalise(4.0), 0.75);
}

#[test]
fn the_two_slope_norm_clamps_outside_its_range() {
    let norm = TwoSlopeNorm::new(-1.0, 0.0, 1.0);
    assert_eq!(norm.normalise(-99.0), 0.0);
    assert_eq!(norm.normalise(99.0), 1.0);
}

/// A degenerate range must not divide by zero and paint the whole figure with
/// one colour or, worse, `NaN`.
#[test]
fn the_two_slope_norm_survives_a_degenerate_range() {
    let norm = TwoSlopeNorm::new(0.0, 0.0, 0.0);
    let value = norm.normalise(0.0);
    assert!(value.is_finite() && (0.0..=1.0).contains(&value));
}

/// The mean-assortativity figure uses a symmetric log scale, so that a few
/// enormous z-scores do not flatten everything else to the centre colour.
#[test]
fn the_symmetric_log_norm_is_centred_and_monotone() {
    let norm = SymLogNorm::new(1.0, -100.0, 100.0);

    assert!(
        (norm.normalise(0.0) - 0.5).abs() < 1e-12,
        "zero sits at the centre"
    );
    assert_eq!(norm.normalise(-100.0), 0.0);
    assert_eq!(norm.normalise(100.0), 1.0);

    let mut previous = -1.0;
    for step in 0..=200 {
        let value = -100.0 + step as f64;
        let normalised = norm.normalise(value);
        assert!(normalised >= previous, "not monotone at {value}");
        previous = normalised;
    }
}

/// Below the linear threshold the scale is linear, which is what keeps small
/// values from being crushed into the centre.
#[test]
fn the_symmetric_log_norm_is_linear_near_zero() {
    let norm = SymLogNorm::new(1.0, -100.0, 100.0);
    let quarter = norm.normalise(0.25) - 0.5;
    let half = norm.normalise(0.5) - 0.5;
    assert!(
        (half / quarter - 2.0).abs() < 1e-9,
        "the linear region is not linear: {quarter} then {half}"
    );
}

// ---------------------------------------------------------------------------
// The network figure
// ---------------------------------------------------------------------------

/// Coordinates, edges and phenotype labels of one sample.
type Network = (Vec<[f64; 2]>, Vec<(u32, u32)>, Vec<String>);

/// A small network with three phenotypes.
fn network_fixture() -> Network {
    let mut coords = Vec::new();
    let mut labels = Vec::new();
    for i in 0..30 {
        let angle = i as f64 * std::f64::consts::TAU / 30.0;
        coords.push([angle.cos() * 10.0, angle.sin() * 10.0]);
        labels.push(["A", "B", "C"][i % 3].to_string());
    }
    let pairs: Vec<(u32, u32)> = (0..30u32).map(|i| (i, (i + 1) % 30)).collect();
    (coords, pairs, labels)
}

/// Step 1's figure is named after the sample, because the interface groups the
/// gallery by patient using that name.
#[test]
fn the_network_figure_is_named_after_its_sample() {
    let dir = tempfile::tempdir().unwrap();
    let (coords, pairs, labels) = network_fixture();
    let figures = Figures::for_tests();

    figures
        .network(
            &SampleId::with_sample("12", "3"),
            "patient",
            Some("sample"),
            &coords,
            &pairs,
            &labels,
            dir.path(),
        )
        .unwrap();

    assert!(
        dir.path().join("net_12-3.png").is_file(),
        "expected net_12-3.png, found {:?}",
        std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_single_level_network_figure_omits_the_sample() {
    let dir = tempfile::tempdir().unwrap();
    let (coords, pairs, labels) = network_fixture();

    Figures::for_tests()
        .network(
            &SampleId::patient_only("7"),
            "patient",
            None,
            &coords,
            &pairs,
            &labels,
            dir.path(),
        )
        .unwrap();

    assert!(dir.path().join("net_7.png").is_file());
}

/// The figure must actually show the phenotypes it was given, in the palette
/// colours — a plot that drew only edges would still be a valid PNG.
#[test]
fn the_network_figure_shows_its_phenotypes() {
    let dir = tempfile::tempdir().unwrap();
    let (coords, pairs, labels) = network_fixture();

    Figures::for_tests()
        .network(
            &SampleId::patient_only("1"),
            "patient",
            None,
            &coords,
            &pairs,
            &labels,
            dir.path(),
        )
        .unwrap();

    let (width, height, pixels) = read_png(&dir.path().join("net_1.png"));
    assert!(width > 100 && height > 100, "{width}x{height} is too small");
    assert!(
        distinct_colours(&pixels) > 3,
        "the figure looks blank: {} distinct colours",
        distinct_colours(&pixels)
    );

    // Three phenotypes, so the first three palette colours must appear.
    let palette = make_cluster_cmap(3);
    for (index, colour) in palette.iter().enumerate() {
        assert!(
            contains_colour(&pixels, *colour, 40),
            "phenotype {index} ({colour:?}) is missing from the figure"
        );
    }
}

#[test]
fn an_empty_network_still_produces_a_figure() {
    let dir = tempfile::tempdir().unwrap();
    Figures::for_tests()
        .network(
            &SampleId::patient_only("1"),
            "patient",
            None,
            &[],
            &[],
            &[],
            dir.path(),
        )
        .unwrap();
    assert!(dir.path().join("net_1.png").is_file());
}

// ---------------------------------------------------------------------------
// The assortativity figures
// ---------------------------------------------------------------------------

/// A statistics table shaped like the one step 2 writes.
fn assortativity_fixture() -> (Vec<String>, Vec<(String, Vec<f64>)>) {
    let attributes = ["A", "B", "C"];
    let mut columns = vec!["# total".to_string()];
    columns.extend(attributes.iter().map(|a| format!("% {a}")));
    columns.extend(
        ["assort", "assort MEAN", "assort STD", "assort Z"]
            .iter()
            .map(|s| s.to_string()),
    );
    for suffix in [" RAW", " MEAN", " STD", " Z"] {
        for (i, a) in attributes.iter().enumerate() {
            for b in attributes.iter().skip(i) {
                columns.push(format!("{a} - {b}{suffix}"));
            }
        }
    }

    let rows: Vec<(String, Vec<f64>)> = (1..=4)
        .map(|sample| {
            let values: Vec<f64> = (0..columns.len())
                .map(|i| ((i * 7 + sample * 13) % 23) as f64 - 11.0)
                .collect();
            (format!("patient-{sample}_sample-1"), values)
        })
        .collect();

    (columns, rows)
}

/// Step 2 writes six cohort figures plus one per sample in each of two
/// sub-directories. The interface looks for exactly these names.
#[test]
fn every_assortativity_figure_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let (columns, rows) = assortativity_fixture();

    Figures::for_tests()
        .assortativity(&columns, &rows, dir.path())
        .unwrap();

    for name in [
        "abundance.png",
        "Assortativity_heatmap_with_dendrogram.png",
        "Assortativity_heatmap_with_dendrogram_without_auto_paired_pheno.png",
        "Assortativity_heatmap_across_patient.png",
        "Assortativity_heatmap_across_patient_without_auto_paired_pheno.png",
    ] {
        let path = dir.path().join(name);
        assert!(path.is_file(), "{name} was not written");
        let (_, _, pixels) = read_png(&path);
        assert!(distinct_colours(&pixels) > 3, "{name} looks blank");
    }

    // One mixing matrix per sample, in each of the two variants.
    for sub in ["assort_files", "assort_files_without_diag"] {
        let folder = dir.path().join(sub);
        assert!(folder.is_dir(), "{sub} was not created");
        let count = std::fs::read_dir(&folder).unwrap().count();
        assert_eq!(count, 4, "{sub} should hold one figure per sample");
        assert!(
            folder.join("heatmap_zscore_1-1.png").is_file(),
            "the per-sample figure is misnamed; the interface groups by that name"
        );
    }
}

/// An empty table must not panic; step 2 can legitimately produce no rows.
#[test]
fn assortativity_figures_handle_an_empty_table() {
    let dir = tempfile::tempdir().unwrap();
    Figures::for_tests()
        .assortativity(&[], &[], dir.path())
        .unwrap();
}

// ---------------------------------------------------------------------------
// The niche figures
// ---------------------------------------------------------------------------

#[test]
fn the_niche_figures_are_written_with_the_normalisation_in_the_name() {
    let dir = tempfile::tempdir().unwrap();
    let cell_types: Vec<String> = (0..60)
        .map(|i| ["A", "B", "C", "D"][i % 4].to_string())
        .collect();
    let niches: Vec<u32> = (0..60).map(|i| (i / 20) as u32).collect();
    let composition = make_niches_composition(&cell_types, &niches, Normalize::Total).unwrap();

    Figures::for_tests()
        .niche_composition(&composition, &niches, Normalize::Total, dir.path())
        .unwrap();

    assert!(dir
        .path()
        .join("Niches_Aggregated_Composition_total.png")
        .is_file());
    assert!(dir.path().join("Niches_Histogram.png").is_file());

    let (_, _, pixels) = read_png(&dir.path().join("Niches_Histogram.png"));
    assert!(distinct_colours(&pixels) > 3, "the histogram looks blank");
}

/// The composition heatmap uses the sequential map, so its colours must come
/// from `Blues` rather than the categorical palette.
#[test]
fn the_composition_heatmap_uses_the_sequential_map() {
    let dir = tempfile::tempdir().unwrap();
    // An uneven composition, so the map actually spans: a matrix whose cells
    // are all equal has no darkest shade to look for.
    let cell_types: Vec<String> = (0..40)
        .map(|i| if (18..38).contains(&i) { "B" } else { "A" }.to_string())
        .collect();
    let niches: Vec<u32> = (0..40).map(|i| (i / 20) as u32).collect();
    let composition = make_niches_composition(&cell_types, &niches, Normalize::Niche).unwrap();

    Figures::for_tests()
        .niche_composition(&composition, &niches, Normalize::Niche, dir.path())
        .unwrap();

    let path = dir.path().join("Niches_Aggregated_Composition_niche.png");
    let (_, _, pixels) = read_png(&path);
    assert!(
        contains_colour(&pixels, blues().sample(1.0), 60),
        "the darkest Blues shade is missing, so the map is not Blues"
    );
}

#[test]
fn niche_figures_handle_an_empty_composition() {
    let dir = tempfile::tempdir().unwrap();
    let composition = make_niches_composition(&[], &[], Normalize::Total).unwrap();
    Figures::for_tests()
        .niche_composition(&composition, &[], Normalize::Total, dir.path())
        .unwrap();
}

// ---------------------------------------------------------------------------
// The embedding figure
// ---------------------------------------------------------------------------

#[test]
fn the_embedding_figure_is_written_and_coloured_by_cluster() {
    let dir = tempfile::tempdir().unwrap();

    let mut embedding = Vec::new();
    let mut labels = Vec::new();
    for cluster in 0..3u32 {
        for i in 0..40 {
            let angle = i as f64 * std::f64::consts::TAU / 40.0;
            embedding.push(cluster as f64 * 20.0 + angle.cos());
            embedding.push(angle.sin());
            labels.push(cluster);
        }
    }

    Figures::for_tests()
        .embedding(&embedding, 2, &labels, dir.path())
        .unwrap();

    let written: Vec<PathBuf> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|e| e == "png").unwrap_or(false))
        .collect();
    assert_eq!(written.len(), 1, "expected one figure, got {written:?}");

    let name = written[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        name.starts_with("cluster_labels"),
        "the interface looks for this prefix, got `{name}`"
    );

    let (_, _, pixels) = read_png(&written[0]);
    for (index, colour) in make_cluster_cmap(3).iter().enumerate() {
        assert!(
            contains_colour(&pixels, *colour, 40),
            "cluster {index} ({colour:?}) is missing from the figure"
        );
    }
}

/// A one-dimensional embedding cannot be scattered in a plane; the figure must
/// be skipped rather than drawn wrong or panicking.
#[test]
fn a_one_dimensional_embedding_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    Figures::for_tests()
        .embedding(&[1.0, 2.0, 3.0], 1, &[0, 0, 1], dir.path())
        .unwrap();

    let count = std::fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(count, 0, "nothing should have been drawn");
}
