//! Step 3 — port of `package/niche_analysis.py`.

use std::path::{Path, PathBuf};

use mosna_config::model::niche_params::{
    ClustererType, Metric as ConfigMetric, NicheParams, ReducerType,
};
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
    let (input, labels) = reduce_and_cluster(&var_aggreg, params)?;
    progress.step(2, 3, "[PROCESS] Niches Analysis");

    draw_clusters(&input, &labels, save_dir, progress, figures)?;

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
        let (input, labels) = reduce_and_cluster(&var_aggreg, params)?;

        draw_clusters(&input, &labels, &save_dir, progress, figures)?;
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

/// What the clusterer is handed, whatever the reduction did.
///
/// The clusterers all read a flat row-major matrix and take its width as an
/// argument, so the width has to travel with the values rather than be
/// re-derived at each call site from `dim_clust` — which is the reduced
/// dimension, and is simply wrong when there was no reduction.
#[derive(Debug)]
struct ClusterInput {
    values: Vec<f64>,
    width: usize,
    /// Whether these are coordinates in a low-dimensional space, and so
    /// something that can be scattered in a plane.
    reduced: bool,
}

/// Reduce the features, or hand them over unchanged.
///
/// `reducer_type: none` is not a degenerate UMAP: the aggregated matrix goes to
/// the clusterer exactly as it is, which is what UMAP would have consumed.
/// Turning the reduction off therefore changes what is clustered and nothing
/// else.
fn project(var_aggreg: &VarAggreg, params: &NicheParams) -> Result<ClusterInput> {
    let (matrix, width) = var_aggreg.clustering_matrix();
    require_finite(&matrix, width)?;

    match params.reducer_type {
        ReducerType::None => Ok(ClusterInput {
            values: matrix,
            width,
            reduced: false,
        }),
        ReducerType::Umap => {
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
            Ok(ClusterInput {
                values: umap(&matrix, var_aggreg.n_rows, width, &umap_params)?,
                width: params.dim_clust,
                reduced: true,
            })
        }
    }
}

/// Refuse a matrix with a hole in it.
///
/// The aggregated features are means and standard deviations over a
/// neighbourhood, so they are finite whenever the input columns are. A `NaN`
/// here therefore means a `NaN` came in from the nodes file — an empty cell in a
/// numeric column of `Column to aggregate`. UMAP would swallow it and return an
/// embedding of `NaN`s; without a reducer it would reach the clusterer
/// directly. Both are refused, naming the cell so the offending column can be
/// found.
fn require_finite(values: &[f64], width: usize) -> Result<()> {
    let Some(position) = values.iter().position(|value| !value.is_finite()) else {
        return Ok(());
    };
    Err(PipelineError::invalid(format!(
        "the clustering input is not a number at row {}, column {} of {width}: \
         a column of `Column to aggregate` holds a value that is not numeric",
        position / width,
        position % width,
    )))
}

/// The k-nearest-neighbour graph as a list of undirected edges, each once.
///
/// The neighbour lists are directed: `j` appearing among `i`'s neighbours does
/// not stop `i` from appearing among `j`'s, and in a k-NN graph that mutual
/// case is the rule rather than the exception. Emitting one edge per (node,
/// neighbour) pair therefore hands Leiden the mutual edges twice, and
/// `leiden::Graph` sums the weights it is given — so those pairs would carry
/// weight 2 while one-sided pairs carry 1, and the modularity being optimised
/// would not be the modularity of this graph.
///
/// The reference does the same deduplication at the same point:
/// `tysserand.pairs_from_knn` ends with `remove_duplicate_pairs(pairs)`.
fn undirected_knn_edges(indices: &[Vec<usize>]) -> Vec<(usize, usize, f64)> {
    let mut edges: Vec<(usize, usize)> = indices
        .iter()
        .enumerate()
        .flat_map(|(i, neighbours)| {
            neighbours
                .iter()
                // Canonical orientation, so `(i, j)` and `(j, i)` collapse.
                .map(move |&j| (i.min(j), i.max(j)))
                // A self-loop carries no information about community structure.
                .filter(|(a, b)| a != b)
        })
        .collect();
    edges.sort_unstable();
    edges.dedup();
    edges.into_iter().map(|(a, b)| (a, b, 1.0)).collect()
}

/// Partition the rows into niches.
fn cluster(input: &ClusterInput, n_rows: usize, params: &NicheParams) -> Result<Vec<u32>> {
    let (values, width) = (input.values.as_slice(), input.width);

    let labels = match params.clusterer_type {
        ClustererType::Gmm => {
            let gmm = gaussian_mixture(
                values,
                n_rows,
                width,
                &GmmParams {
                    n_clusters: params.n_clusters,
                    ..GmmParams::default()
                },
            )?;
            gmm.labels
        }
        ClustererType::Leiden => {
            let k = params.effective_k_cluster();
            let graph = knn_graph(values, n_rows, width, k, Metric::Euclidean);
            let edges = undirected_knn_edges(&graph.indices);
            leiden(n_rows, &edges, params.resolution, 0)
        }
        ClustererType::Spectral => spectral_clustering(
            values,
            n_rows,
            width,
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

    Ok(labels)
}

/// Scatter the clusters in the projection, when there is one.
///
/// The figure places every cell at its coordinates in the reduced space. The
/// unreduced features have no such space: their first two columns are two
/// phenotypes, not two axes, and a scatter of them would look like a projection
/// while meaning something else entirely. It is skipped, and said so, rather
/// than drawn wrong.
fn draw_clusters(
    input: &ClusterInput,
    labels: &[u32],
    save_dir: &Path,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    if !input.reduced {
        progress.info("[INFO] No reduction: the cluster projection is not drawn");
        return Ok(());
    }
    figures.embedding(&input.values, input.width, labels, save_dir)
}

/// Reduce the features and partition them into niches.
fn reduce_and_cluster(
    var_aggreg: &VarAggreg,
    params: &NicheParams,
) -> Result<(ClusterInput, Vec<u32>)> {
    let input = project(var_aggreg, params)?;
    let labels = cluster(&input, var_aggreg.n_rows, params)?;
    Ok((input, labels))
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

    fn params(yaml: &str) -> NicheParams {
        NicheParams::from_value(&serde_yaml::from_str(yaml).unwrap())
    }

    /// Four cells, three features, numeric patient ids and no sample level.
    fn features(patients: &[&str]) -> VarAggreg {
        let n_rows = patients.len();
        VarAggreg {
            column_names: vec!["A mean".into(), "A std".into(), "B mean".into()],
            values: (0..n_rows * 3).map(|i| i as f64 * 0.5).collect(),
            n_rows,
            patients: patients.iter().map(|p| p.to_string()).collect(),
            samples: vec![None; n_rows],
        }
    }

    /// The same table with a hole punched in one feature, as an empty cell in a
    /// numeric `Column to aggregate` would produce.
    fn features_with_a_hole(patients: &[&str], position: usize) -> VarAggreg {
        let mut aggreg = features(patients);
        aggreg.values[position] = f64::NAN;
        aggreg
    }

    // -----------------------------------------------------------------------
    // What the clusterer is handed
    // -----------------------------------------------------------------------

    /// Without a reducer the clusterer receives the aggregated features
    /// themselves — the very matrix UMAP would otherwise have consumed, so that
    /// turning the reduction off changes what is clustered and nothing else.
    #[test]
    fn without_a_reducer_the_clusterer_receives_the_feature_matrix_itself() {
        let var_aggreg = features(&["1", "1", "2", "2"]);
        let (expected, expected_width) = var_aggreg.clustering_matrix();

        let input = project(&var_aggreg, &params("reducer_type: none\n")).unwrap();

        assert_eq!(input.width, expected_width, "one column per feature and id");
        assert_eq!(input.values, expected);
    }

    /// The clusterers read the matrix row by row, `width` values at a time: a
    /// length that is not a whole number of rows would silently shift every row
    /// after the first.
    #[test]
    fn the_clustering_input_is_rectangular() {
        let var_aggreg = features(&["1", "1", "2", "2"]);
        for yaml in [
            "reducer_type: none\n",
            "reducer_type: umap\ndim_clust: 2\nn_neighbors: 2\n",
        ] {
            let input = project(&var_aggreg, &params(yaml)).unwrap();
            assert_eq!(
                input.values.len(),
                var_aggreg.n_rows * input.width,
                "`{yaml}` produced a ragged matrix"
            );
        }
    }

    /// With a reducer the width is the reduced dimension, not the feature
    /// count: the figures and the clusterers both size their rows from it.
    #[test]
    fn with_a_reducer_the_clusterer_receives_one_column_per_reduced_dimension() {
        let var_aggreg = features(&["1", "1", "2", "2"]);
        let input = project(
            &var_aggreg,
            &params("reducer_type: umap\ndim_clust: 2\nn_neighbors: 2\n"),
        )
        .unwrap();

        assert_eq!(input.width, 2);
        assert!(
            input.reduced,
            "a UMAP projection can be scattered in a plane"
        );
    }

    /// The raw features are not a projection, so nothing may try to draw them
    /// as one — the first two columns of a feature table are two phenotypes,
    /// not two axes.
    #[test]
    fn the_unreduced_features_are_not_a_projection() {
        let input = project(&features(&["1", "2"]), &params("reducer_type: none\n")).unwrap();
        assert!(!input.reduced);
    }

    /// A `NaN` in the features is refused rather than clustered. Reduction used
    /// to bury it; without a reducer the hole would go straight into the
    /// clusterer and come back as niches nobody could explain.
    #[test]
    fn a_clustering_input_that_is_not_all_numbers_is_refused() {
        // Row 1, column 2 of a three-column table: value index 5.
        let err = project(
            &features_with_a_hole(&["1", "1", "2", "2"], 5),
            &params("reducer_type: none\n"),
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(message.contains("row 1"), "{message}");
        assert!(message.contains("column 2"), "{message}");
        assert!(message.contains("Column to aggregate"), "{message}");
    }

    /// And the same hole is refused when a reducer is asked for: it was never
    /// UMAP's to absorb.
    #[test]
    fn a_hole_in_the_features_is_refused_with_a_reducer_too() {
        assert!(project(
            &features_with_a_hole(&["1", "1", "2", "2"], 5),
            &params("reducer_type: umap\ndim_clust: 2\nn_neighbors: 2\n"),
        )
        .is_err());
    }

    /// A patient id that is not a number is no longer a problem: it never
    /// reaches the matrix. It used to abort the whole analysis.
    #[test]
    fn a_non_numeric_patient_id_is_no_longer_refused() {
        let input = project(
            &features(&["P01", "P01", "barcode-7", "barcode-7"]),
            &params("reducer_type: none\n"),
        )
        .expect("an identifier is metadata, not a variable");
        assert_eq!(input.width, 3, "one column per feature");
    }

    /// The whole point: no reduction still yields one niche label per cell.
    #[test]
    fn without_a_reducer_every_cell_still_gets_a_niche() {
        let var_aggreg = features(&["1", "1", "2", "2"]);
        let (_, labels) = reduce_and_cluster(
            &var_aggreg,
            &params("reducer_type: none\nclusterer_type: gmm\nn_clusters: 2\n"),
        )
        .unwrap();
        assert_eq!(labels.len(), var_aggreg.n_rows);
    }

    /// A mutually-neighbouring pair must reach Leiden once, not twice.
    ///
    /// The neighbour lists are directed; summing them without canonicalising
    /// gave those pairs twice the weight of one-sided pairs, which is not the
    /// graph the reference optimises.
    #[test]
    fn a_mutual_neighbour_pair_becomes_one_edge() {
        // 0 and 1 list each other; 2 lists 0 but 0 does not list 2.
        let indices = vec![vec![1usize], vec![0usize], vec![0usize]];
        let edges = undirected_knn_edges(&indices);

        assert_eq!(edges, vec![(0, 1, 1.0), (0, 2, 1.0)]);
    }

    /// Every edge carries the same weight, and none is a self-loop.
    #[test]
    fn the_knn_edges_are_unweighted_and_loop_free() {
        let indices = vec![vec![1, 2], vec![0, 2], vec![0, 1], vec![3]];
        let edges = undirected_knn_edges(&indices);

        assert!(edges.iter().all(|&(_, _, w)| w == 1.0));
        assert!(
            edges.iter().all(|&(a, b, _)| a != b),
            "a self-loop survived"
        );
        assert!(
            edges.windows(2).all(|w| w[0].0 <= w[1].0),
            "not deduplicable"
        );
        assert_eq!(edges.len(), 3, "three distinct pairs among four nodes");
    }

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
