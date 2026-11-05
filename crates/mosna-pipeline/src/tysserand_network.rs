//! Step 1 — port of `package/tysserand_network.py`.

use std::path::Path;

use rayon::prelude::*;

use mosna_config::model::tysserand::EdgesMethod;
use mosna_config::validate::assert_params::{assert_params, Analysis};
use mosna_config::{section, RawConfig, TysserandConfig};
use mosna_core::geometry::{
    build_delaunay, link_solitaries, DelaunayTrim, LinkMethod, NodeAdaptive, TrimDist,
};
use mosna_io::read::get_opener::{read_table, Extension};
use mosna_io::write::write_parquet::write_parquet;
use mosna_io::{find_sample, find_sample_from_file, Table};

use crate::error::{create_dir_all, PipelineError, Result};
use crate::figures::FigureSink;
use crate::progress::Progress;
use crate::verif_cpu::verif_cpu;

/// Reconstruct a spatial network for every sample.
///
/// Reads the nodes files under `Nodes directory`, builds the network with
/// Delaunay triangulation and long-edge trimming, reconnects under-connected
/// cells, and writes a `nodes_*` / `edges_*` pair per sample into
/// `temp/net_dir_mosna` — the directory steps 2 and 3 read by default.
pub fn tysserand_network(
    config: &RawConfig,
    working_dir: &Path,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    assert_params(Analysis::Tysserand, config.section(section::TYSSERAND)?)?;
    let settings = TysserandConfig::from_raw(config)?;
    progress.info("[INFO] Parameters are read correctly");

    let extension = Extension::parse(&settings.extension)?;
    let temp_folder = working_dir.join("temp/net_dir_mosna");
    let saving_folder = working_dir.join("Tysserand_Network");
    create_dir_all(&temp_folder)?;
    create_dir_all(&saving_folder)?;

    // An absolute `Nodes directory` replaces the working directory, which is
    // what `pathlib`'s `/` does on the Python side too.
    let nodes_dir = working_dir.join(&settings.nodes_directory);
    let sample_column = settings.sample_column.as_deref();
    let files = find_sample(
        &nodes_dir,
        &settings.extension,
        &settings.patient_column,
        sample_column,
    )?;

    if files.is_empty() {
        return Err(PipelineError::NoSamples {
            path: nodes_dir,
            pattern: format!("{}-*", settings.patient_column),
        });
    }

    // Every file is checked before any work starts, so a typo in a column name
    // fails immediately rather than after half the cohort has been processed.
    let total = files.len();
    progress.step(0, total, "[PROCESS] Verification of all file");
    for (index, file) in files.iter().enumerate() {
        let table = read_table(file, extension)?;
        table.require_columns(&[
            &settings.x_column,
            &settings.y_column,
            &settings.phenotype_column,
        ])?;
        progress.step(index + 1, total, "[PROCESS] Verification of all file");
    }
    progress.info("[INFO] Files are well builded");

    let workers = verif_cpu(settings.cpu, total);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|e| PipelineError::invalid(format!("cannot start the worker pool: {e}")))?;

    // The per-node rule of tysserand 0.6.0, with the three settings the
    // configuration may override; the global threshold stays off, as it is in
    // `ty.build_delaunay(coords)`.
    let trim = DelaunayTrim {
        node_adaptive: Some(NodeAdaptive {
            n_edges: settings.trim_number_of_edges,
            trim_dist_ratio: settings.trim_distance_ratio,
            min_dist: settings.trim_min_distance,
        }),
        distance: TrimDist::None,
    };

    let done = std::sync::atomic::AtomicUsize::new(0);
    progress.step(0, total, "[MULTI PROCESS] Processing file");

    pool.install(|| {
        files.par_iter().try_for_each(|file| -> Result<()> {
            let sample = find_sample_from_file(file, &settings.patient_column, sample_column)?;
            let nodes = read_table(file, extension)?;

            let coords = nodes.coords(&settings.x_column, &settings.y_column)?;
            let pairs = build_delaunay(&coords, trim)?;
            let method = match settings.edges_method {
                EdgesMethod::Delaunay => LinkMethod::Delaunay,
                EdgesMethod::Knn => LinkMethod::Knn,
            };
            let pairs = link_solitaries(&coords, &pairs, method, settings.min_neighbors)?;

            let labels = nodes.string_column(&settings.phenotype_column)?;
            figures.network(
                &sample,
                &settings.patient_column,
                sample_column,
                &coords,
                &pairs,
                &labels,
                &saving_folder,
            )?;

            // The network files are always parquet: that is what the later
            // steps read from `temp/net_dir_mosna`, whatever the input format.
            let stem = sample.str_group(&settings.patient_column, sample_column);
            write_parquet(&nodes, temp_folder.join(format!("nodes_{stem}.parquet")))?;
            write_parquet(
                &Table::from_edges(&pairs)?,
                temp_folder.join(format!("edges_{stem}.parquet")),
            )?;

            let finished = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            progress.step(
                finished,
                total,
                &format!("[MULTI PROCESS] Processing file - {stem} DONE"),
            );
            Ok(())
        })
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use mosna_config::model::tysserand::{
        DEFAULT_TRIM_DISTANCE_RATIO, DEFAULT_TRIM_MIN_DISTANCE, DEFAULT_TRIM_NUMBER_OF_EDGES,
    };
    use mosna_core::geometry::NodeAdaptive;

    /// The configuration's fallbacks and the geometry's defaults are written in
    /// two crates; a configuration without the trimming keys must still build
    /// exactly the tysserand 0.6.0 network.
    #[test]
    fn the_configuration_defaults_are_the_geometry_defaults() {
        assert_eq!(
            NodeAdaptive {
                n_edges: DEFAULT_TRIM_NUMBER_OF_EDGES,
                trim_dist_ratio: DEFAULT_TRIM_DISTANCE_RATIO,
                min_dist: DEFAULT_TRIM_MIN_DISTANCE,
            },
            NodeAdaptive::default()
        );
    }
}
