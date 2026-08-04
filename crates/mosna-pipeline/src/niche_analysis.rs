//! Step 3 — port of `package/niche_analysis.py`.

use std::path::{Path, PathBuf};

use mosna_config::model::niche_params::{ClustererType, Metric as ConfigMetric, NicheParams};
use mosna_config::validate::assert_params::{assert_params, Analysis};
use mosna_config::{save_config, section, NicheAnalysisConfig, RawConfig};
use mosna_core::clustering::{
    gaussian_mixture, leiden, spectral_clustering, GmmParams, SpectralParams,
};
use mosna_core::nas::spatial_omic_features::{
    compute_spatial_omic_features_all_networks, SofOptions, VarAggreg,
};
use mosna_core::niches::{
    aggregate_cell_types, find_all_phenotypes, make_niches_composition, merge_niche_pheno,
    Normalize,
};
use mosna_core::reduction::umap::{knn_graph, umap, Metric, UmapParams};
use mosna_io::read::get_opener::{read_table, Extension};
use mosna_io::write::write_parquet::write_parquet;
use mosna_io::SampleId;

use crate::assortativity::resolve_network_directory;
use crate::error::{create_dir_all, PipelineError, Result};
use crate::figures::FigureSink;
use crate::progress::Progress;

/// Identify spatial niches.
///
/// Aggregates each cell's neighbourhood into a feature vector, reduces it with
/// UMAP, clusters the result, writes the niche label of every cell back into
/// the network files, and describes what each niche is made of.
pub fn niche_analysis(
    config: &RawConfig,
    working_dir: &Path,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    assert_params(
        Analysis::NicheAnalysis,
        config.section(section::NICHE_ANALYSIS)?,
    )?;
    let settings = NicheAnalysisConfig::from_raw(config)?;

    let (net_dir, extension) = resolve_network_directory(
        &settings.network_directory,
        &settings.extension,
        working_dir,
    )?;

    let sample_column = settings.sample_column.as_deref();
    let data_index = mosna_io::make_data_index(
        &net_dir,
        &settings.patient_column,
        sample_column,
        extension.as_str(),
    )?;
    if data_index.is_empty() {
        return Err(PipelineError::NoSamples {
            path: net_dir,
            pattern: format!("{}-*", settings.patient_column),
        });
    }

    // Every file must carry the columns to aggregate before anything runs.
    let aggregate_columns = settings.column_to_aggregate.to_vec();
    for id in &data_index {
        let path = net_dir.join(id.nodes_file_name(
            &settings.patient_column,
            sample_column,
            extension.as_str(),
        ));
        let table = read_table(&path, extension)?;
        let names: Vec<&str> = aggregate_columns.iter().map(String::as_str).collect();
        table.require_columns(&names)?;
    }
    progress.info("[INFO] Verification and Convertion of the files");

    // When a single categorical column is aggregated, the feature vocabulary is
    // the set of its values across the cohort; when several numeric columns are
    // given, they are the vocabulary already.
    let use_attributes = if settings.make_onehot() {
        let column = aggregate_columns
            .first()
            .expect("a single column selector is non-empty");
        find_all_phenotypes(
            &net_dir,
            &data_index,
            &settings.patient_column,
            sample_column,
            extension,
            column,
        )?
    } else {
        aggregate_columns.clone()
    };
    progress.info("[INFO] Phenotypes for all sample found");

    if settings.processing_method.with_aggregation() {
        let save_dir = working_dir
            .join("Niche_Analysis/Aggregation")
            .join(&settings.saving_directory);
        run_aggregated(
            &settings,
            config,
            &net_dir,
            extension,
            &data_index,
            &use_attributes,
            &save_dir,
            progress,
            figures,
        )?;
        progress.info("[INFO] Niches found for aggregated nodes");
    }

    if settings.processing_method.per_sample() {
        let save_root = working_dir
            .join("Niche_Analysis/Per_sample")
            .join(&settings.saving_directory);
        run_per_sample(
            &settings,
            config,
            &net_dir,
            extension,
            &data_index,
            &use_attributes,
            &save_root,
            progress,
            figures,
        )?;
        progress.info("[INFO] Niches found for each samples");
    }

    Ok(())
}

/// Niches called once over the pooled cohort.
#[allow(clippy::too_many_arguments)]
fn run_aggregated(
    settings: &NicheAnalysisConfig,
    config: &RawConfig,
    net_dir: &Path,
    extension: Extension,
    data_index: &[SampleId],
    use_attributes: &[String],
    save_dir: &Path,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    create_dir_all(save_dir)?;
    let params = &settings.aggregated;
    let sample_column = settings.sample_column.as_deref();

    progress.info("[PROCESS] Spatial Omic Features for all networks");
    progress.step(0, 3, "[PROCESS] Niches Analysis");

    let var_aggreg = load_or_compute_features(
        settings,
        net_dir,
        extension,
        data_index,
        use_attributes,
        params,
        progress,
    )?;
    progress.step(1, 3, "[PROCESS] Niches Analysis");

    progress.info("[PROCESS] Reduction and Clustering of Spatial Niches");
    let (embedding, labels) = reduce_and_cluster(&var_aggreg, params)?;
    progress.step(2, 3, "[PROCESS] Niches Analysis");

    figures.embedding(&embedding, params.dim_clust, &labels, save_dir)?;

    // Writing the labels back is what lets the network be re-plotted coloured
    // by niche, and what keeps the composition aligned with the cells.
    //
    // The Python has this line commented out in the aggregated path
    // (`#cell_types = merge_niche_pheno(...)`) while still asking
    // `generate_cmap(net_dir, 'niches', ...)` for the re-plot a few lines
    // later — which cannot work, because nothing ever creates that column. The
    // write is restored here; it is the only way the `Plot Network` option can
    // function.
    merge_niche_pheno(
        net_dir,
        data_index,
        &settings.patient_column,
        sample_column,
        extension,
        &labels,
    )?;

    progress.info("[PROCESS] Generate Niches Composition");
    if let Some(phenotype_column) = settings.phenotype_column.as_deref() {
        let cell_types = aggregate_cell_types(
            net_dir,
            data_index,
            &settings.patient_column,
            sample_column,
            extension,
            phenotype_column,
        )?;

        for normalize in expand(params.normalize) {
            let composition = make_niches_composition(&cell_types, &labels, normalize)?;
            figures.niche_composition(&composition, &labels, normalize, save_dir)?;
        }
    }

    save_config(save_dir, config.section(section::NICHE_ANALYSIS)?)?;
    progress.step(3, 3, "[PROCESS] Niches Analysis");
    Ok(())
}

/// Niches called independently for each sample.
#[allow(clippy::too_many_arguments)]
fn run_per_sample(
    settings: &NicheAnalysisConfig,
    config: &RawConfig,
    net_dir: &Path,
    extension: Extension,
    data_index: &[SampleId],
    use_attributes: &[String],
    save_root: &Path,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    let params = &settings.per_sample;
    let sample_column = settings.sample_column.as_deref();
    let total = data_index.len();
    progress.step(0, total, "[PROCESS] Niches Analysis per sample");

    for (position, id) in data_index.iter().enumerate() {
        let stem = id.str_group(&settings.patient_column, sample_column);
        let save_dir = save_root.join(&stem);
        create_dir_all(&save_dir)?;

        let single = std::slice::from_ref(id);
        let var_aggreg =
            compute_features(settings, net_dir, extension, single, use_attributes, params)?;
        let (embedding, labels) = reduce_and_cluster(&var_aggreg, params)?;

        figures.embedding(&embedding, params.dim_clust, &labels, &save_dir)?;
        merge_niche_pheno(
            net_dir,
            single,
            &settings.patient_column,
            sample_column,
            extension,
            &labels,
        )?;

        if let Some(phenotype_column) = settings.phenotype_column.as_deref() {
            let cell_types = aggregate_cell_types(
                net_dir,
                single,
                &settings.patient_column,
                sample_column,
                extension,
                phenotype_column,
            )?;
            for normalize in expand(params.normalize) {
                let composition = make_niches_composition(&cell_types, &labels, normalize)?;
                // Each normalisation gets its own sub-directory, as the Python
                // `save_dir / f'{normalization}'` does.
                let target =
                    if params.normalize == mosna_config::model::niche_params::Normalize::All {
                        let nested = save_dir.join(normalize.as_str());
                        create_dir_all(&nested)?;
                        nested
                    } else {
                        save_dir.clone()
                    };
                figures.niche_composition(&composition, &labels, normalize, &target)?;
            }
        }

        save_config(&save_dir, config.section(section::NICHE_ANALYSIS)?)?;
        progress.step(position + 1, total, "[PROCESS] Niches Analysis per sample");
    }

    Ok(())
}

/// The aggregated feature table, from cache when it is already on disk.
fn load_or_compute_features(
    settings: &NicheAnalysisConfig,
    net_dir: &Path,
    extension: Extension,
    data_index: &[SampleId],
    use_attributes: &[String],
    params: &NicheParams,
    progress: &dyn Progress,
) -> Result<VarAggreg> {
    let cache: PathBuf = net_dir.join("var_aggreg.parquet");
    let sample_column = settings.sample_column.as_deref();

    if cache.is_file() {
        let table = read_table(&cache, Extension::Parquet)?;
        let cached = VarAggreg::from_table(&table, &settings.patient_column, sample_column)?;
        // A cache from a different phenotype vocabulary or a different
        // neighbourhood order would silently produce wrong niches, so it is
        // only reused when its shape still matches the configuration.
        // One block of columns per statistic; the aggregation supports at most
        // the mean and the standard deviation.
        let n_statistics = params.stat_names.len().clamp(1, 2);
        let expected_columns = use_attributes.len() * n_statistics;
        if cached.n_columns() == expected_columns {
            progress.info("[INFO] Reusing the cached aggregated features");
            return Ok(cached);
        }
        progress.info("[INFO] Cached features do not match the configuration, recomputing");
    }

    let var_aggreg = compute_features(
        settings,
        net_dir,
        extension,
        data_index,
        use_attributes,
        params,
    )?;
    let table = var_aggreg.to_table(&settings.patient_column, sample_column)?;
    write_parquet(&table, &cache)?;
    Ok(var_aggreg)
}

fn compute_features(
    settings: &NicheAnalysisConfig,
    net_dir: &Path,
    extension: Extension,
    data_index: &[SampleId],
    use_attributes: &[String],
    params: &NicheParams,
) -> Result<VarAggreg> {
    let options = SofOptions {
        net_dir: net_dir.to_path_buf(),
        extension,
        patient_column: settings.patient_column.clone(),
        sample_column: settings.sample_column.clone(),
        attributes_col: settings.column_to_aggregate.to_vec(),
        use_attributes: use_attributes.to_vec(),
        make_onehot: settings.make_onehot(),
        order: params.order,
        stat_names: params.stat_names.clone(),
        var_sep: " ".to_string(),
        add_sample_info: true,
    };
    Ok(compute_spatial_omic_features_all_networks(
        &options,
        data_index,
        &|_, _| {},
    )?)
}

/// Reduce the features and partition them into niches.
fn reduce_and_cluster(
    var_aggreg: &VarAggreg,
    params: &NicheParams,
) -> Result<(Vec<f64>, Vec<u32>)> {
    let (matrix, width) = var_aggreg.clustering_matrix();
    let n_rows = var_aggreg.n_rows;

    let umap_params = UmapParams {
        n_components: params.dim_clust,
        n_neighbors: params.n_neighbors,
        metric: match params.metric {
            ConfigMetric::Manhattan => Metric::Manhattan,
            ConfigMetric::Cosine => Metric::Cosine,
            ConfigMetric::Euclidean => Metric::Euclidean,
        },
        min_dist: params.min_dist,
        ..UmapParams::default()
    };
    let embedding = umap(&matrix, n_rows, width, &umap_params)?;

    let labels = match params.clusterer_type {
        ClustererType::Gmm => {
            let gmm = gaussian_mixture(
                &embedding,
                n_rows,
                params.dim_clust,
                &GmmParams {
                    n_clusters: params.n_clusters,
                    ..GmmParams::default()
                },
            )?;
            gmm.labels
        }
        ClustererType::Leiden => {
            let k = params.effective_k_cluster();
            let graph = knn_graph(&embedding, n_rows, params.dim_clust, k, Metric::Euclidean);
            let edges: Vec<(usize, usize, f64)> = (0..n_rows)
                .flat_map(|i| graph.indices[i].iter().map(move |&j| (i, j, 1.0)))
                .collect();
            leiden(n_rows, &edges, params.resolution, 0)
        }
        ClustererType::Spectral => spectral_clustering(
            &embedding,
            n_rows,
            params.dim_clust,
            &SpectralParams {
                n_clusters: params.n_clusters,
                ..SpectralParams::default()
            },
        )?,
        // The Python raises `RuntimeError('ecg clustering requires the cugraph
        // library')` on CPU; HDBSCAN is offered by the GUI but rejected by
        // `assert_params`, so neither is reachable from a valid configuration.
        other => {
            return Err(PipelineError::invalid(format!(
                "clusterer `{}` has no CPU implementation; use leiden, gmm or spectral",
                other.as_str()
            )))
        }
    };

    Ok((embedding, labels))
}

/// The normalisations to compute, expanding `all`.
fn expand(normalize: mosna_config::model::niche_params::Normalize) -> Vec<Normalize> {
    normalize
        .expand()
        .into_iter()
        .map(|n| Normalize::parse(n.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosna_config::model::niche_params::Normalize as ConfigNormalize;

    #[test]
    fn all_expands_to_every_normalisation() {
        let expanded = expand(ConfigNormalize::All);
        assert_eq!(expanded.len(), 5);
        assert!(expanded.contains(&Normalize::Clr));
        assert!(expanded.contains(&Normalize::NicheAndObs));
    }

    #[test]
    fn a_single_normalisation_expands_to_itself() {
        assert_eq!(expand(ConfigNormalize::Total), vec![Normalize::Total]);
    }
}
