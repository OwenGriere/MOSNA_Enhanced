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
use mosna_core::reduction::umap::{cluster_neighbours, umap, Metric, Scalar, UmapParams};
use mosna_io::read::get_opener::{read_table, Extension};
use mosna_io::write::write_parquet::write_parquet;
use mosna_io::{SampleId, Table};
use serde_json::json;

use crate::assortativity::resolve_network_directory;
use crate::error::{create_dir_all, PipelineError, Result};
use crate::figures::FigureSink;
use crate::niche_cache::{self, Caches};
use crate::niche_cohort;
use crate::niche_lock::RegisterLock;
use crate::niche_record::{self, Outcome, PerSampleRecord, Record, Rendering};
use crate::niche_runs::{Catalogue, PerSampleRun, RunNumbers, Status};
use crate::progress::Progress;

/// How the two processing methods are spelled in the run register.
const MODE_AGGREGATED: &str = "aggregated";
const MODE_PER_SAMPLE: &str = "per_sample";

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
    // Before anything is read, let alone written: this step writes the niche
    // labels back into the nodes files and can only write parquet.
    niche_cohort::require_writable_network(extension, &net_dir)?;
    require_network(&settings.network_directory, &net_dir)?;

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

    // Every file must carry the columns to aggregate before anything runs, and
    // the same pass says what the cohort holds — see [`crate::niche_cohort`].
    //
    // Only those columns are read. This used to decode every column of every
    // nodes file in the cohort — gigabytes, to answer a question the parquet
    // footer already holds — and did it one file at a time. `read_table_columns`
    // raises the same `MissingColumn` naming the same file, so a configuration
    // that names a column nothing has fails exactly as it did.
    let aggregate_columns = settings.column_to_aggregate.to_vec();
    let digest = niche_cohort::verify_and_digest(
        &net_dir,
        extension,
        &data_index,
        &settings.patient_column,
        sample_column,
        &aggregate_columns,
    )?;
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

    // The caches and the catalogue, both of which outlive this run: the first
    // so the next one need not recompute what this one did, the second so a
    // numbered directory can be read back into the settings behind it.
    let caches = Caches::under(working_dir);
    let niche_dir = working_dir.join("Niche_Analysis");

    let cohort = Cohort {
        fingerprint: digest.fingerprint.clone(),
        network_directory: match &settings.network_directory {
            mosna_config::model::assortativity::NetworkDirectory::Default => "Default".to_string(),
            mosna_config::model::assortativity::NetworkDirectory::Custom(path) => path.clone(),
        },
        patient_column: settings.patient_column.clone(),
        sample_column: settings.sample_column.clone(),
    };

    if settings.processing_method.with_aggregation() {
        let stems = Stems::claim(
            &settings.aggregated,
            &aggregate_columns,
            &use_attributes,
            &cohort,
            Some(digest.total_rows()),
            &niche_dir,
            MODE_AGGREGATED,
            None,
        )?;
        let save_dir = announce(&niche_dir, &stems, progress);

        // The outcome is recorded either way: a run that dies half-way is
        // marked failed rather than left claiming to be running, and the
        // directory it created is taken back down.
        let outcome = run_aggregated(
            &settings,
            config,
            &net_dir,
            extension,
            &data_index,
            &use_attributes,
            digest.total_rows(),
            &save_dir,
            &caches,
            &stems,
            &cohort,
            MODE_AGGREGATED,
            None,
            progress,
            figures,
        );
        finish(&niche_dir, &save_dir, &stems, outcome)?;
        progress.info("[INFO] Niches found for aggregated nodes");
    }

    if settings.processing_method.per_sample() {
        // Each sample claims its own numbers when its turn comes, and is
        // recorded as it finishes. They used to be handed out in one pass
        // because the register was not written until the whole loop ended — so
        // a failure on the third sample lost the record of the first two, along
        // with the numbers naming their caches.
        run_per_sample(
            &settings,
            config,
            &net_dir,
            extension,
            &data_index,
            &aggregate_columns,
            &use_attributes,
            &digest,
            &niche_dir,
            &caches,
            &cohort,
            progress,
            figures,
        )?;
        progress.info("[INFO] Niches found for each samples");
    }

    Ok(())
}

/// The three cache files a run resolves to, and the numbers that name them.
///
/// The numbers *are* the paths: `1-2-3` is
/// `temp/intermediate_files/var_aggreg-1/reduction-2/clustering_3.parquet`,
/// sitting beside the projection it came from, inside the aggregation that
/// projection came from. They also name the column its labels are written back
/// into, `niches_1-2-3`. Nothing about
/// the settings is in any of those names — they are in `runs.json`, which is
/// the one place with room to spell them out.
#[derive(Debug, Clone)]
struct Stems {
    numbers: RunNumbers,
    /// Whether a projection is computed at all; without one there is nothing to
    /// cache between the features and the partition.
    reduces: bool,
    /// Whether the run's directory already existed when it was claimed.
    repeat: bool,
    /// The identity of the run — see [`crate::niche_record`].
    fingerprint: String,
    /// The settings it actually used, for `run.json`.
    parameters: serde_json::Value,
    /// What each file must record about its own provenance, so that a stale one
    /// left behind by a deleted catalogue is recomputed rather than read.
    features_source: String,
    reduction_source: String,
    clustering_source: String,
}

/// Refuse a network directory that is not there, saying which mistake it is.
///
/// # Why the two cases are told apart
///
/// `Network directory: Default` is `temp/net_dir_mosna`, which step 1 creates
/// and `clear-temporary` removes — so a user who has just cleared the temporary
/// data, or who has not run step 1 at all, arrives here. The failure used to be
/// `failed to read .../temp/net_dir_mosna: No such file or directory (os error
/// 2)`: a path they never chose, and no hint that the answer is to press the
/// first button.
///
/// A custom directory that is absent is the opposite mistake — a path they
/// typed — and telling them to run step 1 would send them the wrong way.
fn require_network(
    configured: &mosna_config::model::assortativity::NetworkDirectory,
    net_dir: &Path,
) -> Result<()> {
    use mosna_config::model::assortativity::NetworkDirectory;

    if net_dir.is_dir() {
        return Ok(());
    }
    Err(PipelineError::invalid(match configured {
        NetworkDirectory::Default => format!(
            "there is no network to analyse at {}: run Step 1 (Tysserand) first, \
             or point `Network directory` at a directory that already holds one",
            net_dir.display()
        ),
        NetworkDirectory::Custom(_) => format!(
            "the configured `Network directory` does not exist: {}",
            net_dir.display()
        ),
    }))
}

/// What an aggregation is *of*, beside the settings that shape it.
///
/// # Why the cohort is part of a run's identity
///
/// `NicheParams::features_parameters` describes how the neighbourhoods are
/// summarised — the statistics, the columns, the order. It cannot describe what
/// they are summarised from: the network, and the way the cohort is split into
/// samples. Two runs agreeing on every setting and disagreeing on either of
/// those produce different features, and used to be given the same number, the
/// same cache file and the same directory.
///
/// Recording them makes such a run a *new* aggregation rather than a
/// re-execution of the old one. It is given its own number, so the results
/// computed from the previous cohort keep theirs instead of being overwritten
/// by results not comparable to them.
#[derive(Debug, Clone)]
pub struct Cohort {
    /// The digest of [`crate::niche_cohort::verify_and_digest`].
    pub fingerprint: String,
    /// The network directory, as configured — `Default`, or the custom path.
    pub network_directory: String,
    pub patient_column: String,
    pub sample_column: Option<String>,
}

impl Cohort {
    /// The aggregation's full identity: its settings, and what it aggregated.
    fn features_identity(
        &self,
        params: &NicheParams,
        columns: &[String],
        n_phenotypes: usize,
    ) -> serde_json::Value {
        let mut identity = params.features_parameters(columns, n_phenotypes);
        let map = identity
            .as_object_mut()
            .expect("features_parameters builds an object");
        map.insert("network_directory".into(), json!(self.network_directory));
        map.insert("patient_column".into(), json!(self.patient_column));
        map.insert("sample_column".into(), json!(self.sample_column));
        map.insert("cohort".into(), json!(self.fingerprint));
        identity
    }
}

/// The identity of a set of niche settings, before any numbers are attached.
///
/// A per-sample run needs this on its own: it has to know which run it is
/// before it starts claiming numbers for each of its samples, and those numbers
/// differ from sample to sample while the run does not.
#[derive(Debug, Clone)]
struct Identity {
    fingerprint: String,
    /// The settings, flattened, for the register to display.
    parameters: serde_json::Value,
}

impl Stems {
    /// What these settings are, independently of which sample they are applied
    /// to and of the numbers they will be given.
    fn identity(
        params: &NicheParams,
        columns: &[String],
        use_attributes: &[String],
        cohort: &Cohort,
        observations: Option<usize>,
    ) -> Identity {
        let features = cohort.features_identity(params, columns, use_attributes.len());
        let reduction = (params.reducer_type != ReducerType::None)
            .then(|| params.reduction_parameters(observations));
        let clustering = params.clustering_parameters(observations);
        Identity {
            fingerprint: niche_record::fingerprint(&features, reduction.as_ref(), &clustering),
            parameters: clustering_identity(params, &features, reduction.as_ref(), &clustering),
        }
    }

    /// Claim the numbers for this run, publishing the claim before any work
    /// starts.
    ///
    /// # Why the claim is published immediately
    ///
    /// The register used to be written only once a run had finished. Two runs
    /// started together therefore both read an empty register, both were handed
    /// `1-1-1`, and both wrote to that directory, that cache file and that
    /// label column: one of the two was lost, without a word. Claiming under
    /// [`RegisterLock`] and writing the register straight away is what makes a
    /// working directory safe to share — which a sweep over a grid of
    /// parameters needs before anything else.
    ///
    /// The lock is held for the claim alone, never for the computation, so runs
    /// still overlap; what they cannot do is choose the same numbers.
    #[allow(clippy::too_many_arguments)]
    fn claim(
        params: &NicheParams,
        columns: &[String],
        use_attributes: &[String],
        cohort: &Cohort,
        observations: Option<usize>,
        niche_dir: &Path,
        mode: &str,
        sample: Option<&str>,
    ) -> Result<Self> {
        let features = cohort.features_identity(params, columns, use_attributes.len());
        let reduces = params.reducer_type != ReducerType::None;
        let reduction = reduces.then(|| params.reduction_parameters(observations));
        let clustering = params.clustering_parameters(observations);
        let fingerprint = niche_record::fingerprint(&features, reduction.as_ref(), &clustering);

        let (numbers, repeat) = {
            let _guard = RegisterLock::acquire(niche_dir)?;
            let mut catalogue = Catalogue::load(niche_dir)?;
            let numbers = catalogue.resolve_avoiding(
                mode,
                sample.unwrap_or_default(),
                &features,
                reduction.as_ref(),
                &clustering,
                // A directory that already holds a *different* run keeps it.
                &|candidate: RunNumbers| {
                    !niche_record::belongs_to_another(
                        &niche_dir.join(candidate.directory()),
                        &fingerprint,
                    )
                },
            );
            catalogue.save()?;
            let repeat = niche_dir.join(numbers.directory()).is_dir();
            (numbers, repeat)
        };

        // The provenance each file records is the chain of settings behind it,
        // not its own alone. The directories already keep the stages apart; this
        // is what catches a file whose directory was reused because `runs.json`
        // had been deleted and the numbering started again.
        let features_source = compact(&json!({ "sample": sample, "features": features }));
        let reduction_source = compact(&json!({
            "sample": sample,
            "features": features,
            "reduction": reduction,
        }));
        let clustering_source = compact(&json!({
            "sample": sample,
            "features": features,
            "reduction": reduction,
            "clustering": clustering,
        }));

        Ok(Self {
            numbers,
            reduces,
            repeat,
            fingerprint,
            parameters: clustering_identity(params, &features, reduction.as_ref(), &clustering),
            features_source,
            reduction_source,
            clustering_source,
        })
    }

    /// Record how this run ended, under the lock that guards the register.
    fn complete(&self, niche_dir: &Path, status: Status) -> Result<()> {
        let _guard = RegisterLock::acquire(niche_dir)?;
        let mut catalogue = Catalogue::load(niche_dir)?;
        catalogue.mark(self.numbers, status);
        catalogue.save()
    }
}

/// The settings a run actually used, flattened into one object.
///
/// What `run.json` records, and what the interface reads to compare two runs:
/// the aggregation, the projection and the partition as they were resolved —
/// `k_cluster` already capped by `n_neighbors`, the phenotype count filled in —
/// rather than the configuration's two sub-sections, only one of which ran.
fn clustering_identity(
    params: &NicheParams,
    features: &serde_json::Value,
    reduction: Option<&serde_json::Value>,
    clustering: &serde_json::Value,
) -> serde_json::Value {
    let mut flat = serde_json::Map::new();
    for source in [Some(features), reduction, Some(clustering)].into_iter().flatten() {
        if let Some(object) = source.as_object() {
            for (key, value) in object {
                flat.insert(key.clone(), value.clone());
            }
        }
    }
    if reduction.is_none() {
        flat.insert("reducer_type".into(), json!("none"));
    }
    // Not part of any stage's identity, but part of what was asked for.
    flat.insert("metric".into(), json!(params.metric.as_str()));
    serde_json::Value::Object(flat)
}

/// A value as one line, for the parquet footers that record provenance.
fn compact(value: &serde_json::Value) -> String {
    value.to_string()
}

/// The column a run's labels are written back into, named after the run.
///
/// `niches_1-0-2` rather than `niches`: the nodes files outlive any one run,
/// and a fixed name would have each new set of settings replace the labels of
/// the one before, leaving no way to compare two runs in the network view.
///
/// A per-sample run is named `ps-1`, not by the three numbers of one of its
/// samples: every sample of such a run is aggregated separately and so carries
/// different numbers, and naming the column after them left the run with no
/// column its samples shared — nothing the network view could be asked to
/// colour by.
fn niche_column(run: &str) -> String {
    format!("niches_{run}")
}

/// Where a run writes, and what that means for what is already there.
///
/// # Why the old warning was misleading
///
/// It said "these settings were already run; its results are being
/// overwritten" whenever the directory existed — which was true of three quite
/// different situations, and wrong about two of them. It fired for a run that
/// only changed `normalize`, where nothing is overwritten and a figure is
/// added; it fired for an empty directory left behind by a run that had failed,
/// which held no results at all; and it fired when a recycled number had landed
/// on somebody else's results, where "these settings" was simply false.
///
/// The first two no longer happen — a failed run takes its directory down, and
/// a claim never lands on another run's results — so what is left is the one
/// case the message was always meant for, and it now says which it is.
fn announce(niche_dir: &Path, stems: &Stems, progress: &dyn Progress) -> PathBuf {
    let name = stems.numbers.directory();
    let save_dir = niche_dir.join(&name);
    if stems.repeat {
        progress.info(&format!(
            "[INFO] The same settings were run before as Niche_Analysis/{name}; \
             its results are being written again"
        ));
    }
    save_dir
}

/// Record how a run ended, and leave nothing behind if it failed.
///
/// A run creates its directory before it computes anything. One that died
/// half-way used to leave that directory empty and unrecorded, so the next run
/// handed the same number was told its settings had already been run — a
/// warning about results that did not exist.
fn finish(
    niche_dir: &Path,
    save_dir: &Path,
    stems: &Stems,
    outcome: Result<()>,
) -> Result<()> {
    match outcome {
        Ok(()) => {
            stems.complete(niche_dir, Status::Done)?;
            Ok(())
        }
        Err(error) => {
            stems.complete(niche_dir, Status::Failed)?;
            // Only if it is empty: a run that failed after writing some of its
            // figures leaves them, because deleting a user's results to tidy up
            // after ourselves is the worse mistake.
            if save_dir.is_dir()
                && std::fs::read_dir(save_dir).is_ok_and(|mut entries| entries.next().is_none())
            {
                let _ = std::fs::remove_dir(save_dir);
            }
            Err(error)
        }
    }
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
    expected_rows: usize,
    save_dir: &Path,
    caches: &Caches,
    stems: &Stems,
    cohort: &Cohort,
    mode: &str,
    sample: Option<&str>,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    create_dir_all(save_dir)?;
    caches.prepare(&stems.numbers)?;
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
        caches,
        stems,
        expected_rows,
        progress,
    )?;
    progress.step(1, 3, "[PROCESS] Niches Analysis");

    progress.info("[PROCESS] Reduction and Clustering of Spatial Niches");
    let (input, labels, components) =
        cached_reduce_and_cluster(&var_aggreg, params, caches, stems, progress)?;
    progress.step(2, 3, "[PROCESS] Niches Analysis");

    draw_clusters(&input, &labels, save_dir, progress, figures)?;
    save_embedding(
        &input,
        &var_aggreg,
        save_dir,
        &settings.patient_column,
        sample_column,
        progress,
    )?;

    // Writing the labels back is what lets the network be re-plotted coloured
    // by niche, and what keeps the composition aligned with the cells.
    //
    // The Python has this line commented out in the aggregated path
    // (`#cell_types = merge_niche_pheno(...)`) while still asking
    // `generate_cmap(net_dir, 'niches', ...)` for the re-plot a few lines
    // later — which cannot work, because nothing ever creates that column. The
    // write is restored here; it is the only way the `Plot Network` option can
    // function. The column carries the run's numbers, so a second run adds a
    // layer beside the first instead of overwriting it.
    {
        // Held for the whole rewrite: every nodes file is read, given this
        // run's column, and written back, and a second run doing the same at
        // the same moment would lose one of the two columns.
        let _nodes = RegisterLock::nodes(net_dir)?;
        merge_niche_pheno(
            net_dir,
            data_index,
            &settings.patient_column,
            sample_column,
            extension,
            &niche_column(&stems.numbers.directory()),
            &labels,
        )?;
    }

    plot_networks(
        settings,
        net_dir,
        extension,
        data_index,
        &labels,
        save_dir,
        progress,
        figures,
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

    // `parameters.json` is the configuration one would feed back to `mosna` to
    // run this again; `run.json` is what this run actually was.
    save_config(save_dir, config.section(section::NICHE_ANALYSIS)?)?;
    record_of(
        stems,
        cohort,
        settings,
        params,
        mode,
        sample,
        Outcome::of(&labels, components),
    )
    .write(save_dir)?;
    progress.step(3, 3, "[PROCESS] Niches Analysis");
    Ok(())
}

/// What a run directory records about itself — see [`crate::niche_record`].
#[allow(clippy::too_many_arguments)]
fn record_of(
    stems: &Stems,
    cohort: &Cohort,
    settings: &NicheAnalysisConfig,
    params: &NicheParams,
    mode: &str,
    sample: Option<&str>,
    result: Outcome,
) -> Record {
    Record {
        numbers: stems.numbers,
        run: stems.numbers.directory(),
        fingerprint: stems.fingerprint.clone(),
        mode: mode.to_string(),
        sample: sample.map(str::to_string),
        cohort: cohort.fingerprint.clone(),
        network_directory: cohort.network_directory.clone(),
        patient_column: cohort.patient_column.clone(),
        sample_column: cohort.sample_column.clone(),
        parameters: stems.parameters.clone(),
        render: Rendering {
            normalize: params.normalize.as_str().to_string(),
            phenotype_column: settings.phenotype_column.clone(),
        },
        result,
        status: Status::Done,
    }
}

/// Niches called independently for each sample.
#[allow(clippy::too_many_arguments)]
fn run_per_sample(
    settings: &NicheAnalysisConfig,
    config: &RawConfig,
    net_dir: &Path,
    extension: Extension,
    data_index: &[SampleId],
    aggregate_columns: &[String],
    use_attributes: &[String],
    digest: &niche_cohort::CohortDigest,
    save_root: &Path,
    caches: &Caches,
    cohort: &Cohort,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    let params = &settings.per_sample;
    let total = data_index.len();
    progress.step(0, total, "[PROCESS] Niches Analysis per sample");

    // The run these samples belong to, claimed once. Its identity is the
    // settings and the cohort — which every sample shares — so re-running the
    // same per-sample settings lands on the same run, one directory and one
    // label column, rather than on as many runs as there are samples.
    // No single cohort height: the samples differ, and each clamps its own
    // cluster count. The per-sample caches carry the exact identity.
    let identity = Stems::identity(params, aggregate_columns, use_attributes, cohort, None);
    let run = {
        let _guard = RegisterLock::acquire(save_root)?;
        let mut catalogue = Catalogue::load(save_root)?;
        let id = catalogue.per_sample_run_avoiding(
            &identity.fingerprint,
            &identity.parameters,
            &cohort.fingerprint,
            // A `ps-` directory holding another run's results keeps them.
            &|candidate: u32| {
                !niche_record::belongs_to_another(
                    &save_root.join(PerSampleRun::directory(candidate)),
                    &identity.fingerprint,
                )
            },
        );
        catalogue.save()?;
        id
    };
    let run_name = PerSampleRun::directory(run);
    let run_dir = save_root.join(&run_name);
    let repeat = run_dir.is_dir();

    // Written before the first sample starts, so the directory is spoken for
    // from the moment it is claimed rather than once something has finished in
    // it — which is what a concurrent claim compares against.
    let mut record = per_sample_record(
        &run_name,
        &identity,
        cohort,
        settings,
        params,
        Vec::new(),
        Status::Running,
    );
    record.write(&run_dir)?;

    if repeat {
        progress.info(&format!(
            "[INFO] The same settings were run before as Niche_Analysis/{run_name}; \
             its results are being written again"
        ));
    }

    // The loop is run as one fallible expression so that however it ends — a
    // sample failing, the cohort finishing — the run's own status is recorded
    // before the error is propagated. Returning straight out of the loop left a
    // failed run saying `running` for ever, which a batch resuming from the
    // register would read as work still in progress.
    let mut done: Vec<String> = Vec::new();
    let outcome = run_every_sample(
        settings,
        config,
        net_dir,
        extension,
        data_index,
        aggregate_columns,
        use_attributes,
        digest,
        save_root,
        &run_dir,
        &run_name,
        run,
        caches,
        cohort,
        &mut done,
        progress,
        figures,
    );

    let status = if outcome.is_ok() {
        Status::Done
    } else {
        Status::Failed
    };
    {
        let _guard = RegisterLock::acquire(save_root)?;
        let mut catalogue = Catalogue::load(save_root)?;
        catalogue.mark_per_sample(run, status);
        catalogue.save()?;
    }
    record.samples = done;
    record.status = status;
    record.write(&run_dir)?;

    outcome
}

/// Every sample of a per-sample run, in turn.
///
/// Split out so that its caller has one fallible expression to record the
/// outcome of; `done` collects the samples that finished, so a run that failed
/// part-way still says which ones it covered.
#[allow(clippy::too_many_arguments)]
fn run_every_sample(
    settings: &NicheAnalysisConfig,
    config: &RawConfig,
    net_dir: &Path,
    extension: Extension,
    data_index: &[SampleId],
    aggregate_columns: &[String],
    use_attributes: &[String],
    digest: &niche_cohort::CohortDigest,
    save_root: &Path,
    run_dir: &Path,
    run_name: &str,
    run: u32,
    caches: &Caches,
    cohort: &Cohort,
    done: &mut Vec<String>,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    let params = &settings.per_sample;
    let sample_column = settings.sample_column.as_deref();
    let total = data_index.len();

    for (position, id) in data_index.iter().enumerate() {
        // Each sample is still its own attempt at every stage, so each has its
        // own three numbers and its own cache files — and claims them when its
        // turn comes, so a failure part-way through the cohort does not lose the
        // record of the samples already done. What they now share is the run
        // they belong to.
        let sample = id.str_group(&settings.patient_column, sample_column);
        let rows = digest.rows[position];
        let sample_stems = &Stems::claim(
            params,
            aggregate_columns,
            use_attributes,
            cohort,
            Some(rows),
            save_root,
            MODE_PER_SAMPLE,
            Some(&sample),
        )?;
        let save_dir = run_dir.join(&sample);
        let outcome = run_one_sample(
            settings,
            config,
            net_dir,
            extension,
            id,
            use_attributes,
            rows,
            &save_dir,
            caches,
            sample_stems,
            cohort,
            &sample,
            run_name,
            progress,
            figures,
        );
        // `finish` records the sample's own outcome and propagates a failure,
        // so anything past this point is a sample that completed.
        finish(save_root, &save_dir, sample_stems, outcome)?;
        done.push(sample.clone());
        {
            let _guard = RegisterLock::acquire(save_root)?;
            let mut catalogue = Catalogue::load(save_root)?;
            catalogue.record_sample(run, &sample, sample_stems.numbers);
            catalogue.save()?;
        }
        progress.step(position + 1, total, "[PROCESS] Niches Analysis per sample");
    }

    Ok(())
}

/// What a per-sample run's own directory records — see [`crate::niche_record`].
fn per_sample_record(
    run_name: &str,
    identity: &Identity,
    cohort: &Cohort,
    settings: &NicheAnalysisConfig,
    params: &NicheParams,
    samples: Vec<String>,
    status: Status,
) -> PerSampleRecord {
    PerSampleRecord {
        run: run_name.to_string(),
        fingerprint: identity.fingerprint.clone(),
        cohort: cohort.fingerprint.clone(),
        network_directory: cohort.network_directory.clone(),
        patient_column: cohort.patient_column.clone(),
        sample_column: cohort.sample_column.clone(),
        parameters: identity.parameters.clone(),
        render: Rendering {
            normalize: params.normalize.as_str().to_string(),
            phenotype_column: settings.phenotype_column.clone(),
        },
        samples,
        status,
    }
}

/// One sample of a per-sample run, from its features to its figures.
///
/// Split out so that [`fn@finish`] has a single fallible expression to wrap:
/// the sample's outcome has to be recorded whether it succeeded or not.
#[allow(clippy::too_many_arguments)]
fn run_one_sample(
    settings: &NicheAnalysisConfig,
    config: &RawConfig,
    net_dir: &Path,
    extension: Extension,
    id: &SampleId,
    use_attributes: &[String],
    expected_rows: usize,
    save_dir: &Path,
    caches: &Caches,
    sample_stems: &Stems,
    cohort: &Cohort,
    sample: &str,
    run_name: &str,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    let params = &settings.per_sample;
    let sample_column = settings.sample_column.as_deref();
    {
        create_dir_all(save_dir)?;
        caches.prepare(&sample_stems.numbers)?;

        let single = std::slice::from_ref(id);
        let var_aggreg = load_or_compute_features(
            settings,
            net_dir,
            extension,
            single,
            use_attributes,
            params,
            caches,
            sample_stems,
            expected_rows,
            progress,
        )?;
        let (input, labels, components) =
            cached_reduce_and_cluster(&var_aggreg, params, caches, sample_stems, progress)?;

        draw_clusters(&input, &labels, save_dir, progress, figures)?;
        save_embedding(
            &input,
            &var_aggreg,
            save_dir,
            &settings.patient_column,
            sample_column,
            progress,
        )?;
        {
            let _nodes = RegisterLock::nodes(net_dir)?;
            merge_niche_pheno(
                net_dir,
                single,
                &settings.patient_column,
                sample_column,
                extension,
                &niche_column(run_name),
                &labels,
            )?;
        }

        plot_networks(
            settings,
            net_dir,
            extension,
            single,
            &labels,
            save_dir,
            progress,
            figures,
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
                        save_dir.to_path_buf()
                    };
                figures.niche_composition(&composition, &labels, normalize, &target)?;
            }
        }

        save_config(save_dir, config.section(section::NICHE_ANALYSIS)?)?;
        let mut record = record_of(
            sample_stems,
            cohort,
            settings,
            params,
            MODE_PER_SAMPLE,
            Some(sample),
            Outcome::of(&labels, components),
        );
        // Named by the run it belongs to, not by the numbers of its own caches:
        // those differ from sample to sample, and what a reader wants to know
        // is which run this directory is part of.
        record.run = run_name.to_string();
        record.write(save_dir)?;
    }

    Ok(())
}

/// The aggregated feature table, from cache when it is already on disk.
#[allow(clippy::too_many_arguments)]
fn load_or_compute_features(
    settings: &NicheAnalysisConfig,
    net_dir: &Path,
    extension: Extension,
    data_index: &[SampleId],
    use_attributes: &[String],
    params: &NicheParams,
    caches: &Caches,
    stems: &Stems,
    expected_rows: usize,
    progress: &dyn Progress,
) -> Result<VarAggreg> {
    let cache: PathBuf = caches.features_path(&stems.numbers);
    let sample_column = settings.sample_column.as_deref();

    // A cache that cannot be read is a miss, not a failure — the same terms
    // `niche_cache::read_matrix` and `read_labels` are on. Propagating the error
    // ended an analysis that had done nothing wrong: the file could be mid-write
    // by another run sharing this aggregation, or left over from a version that
    // wrote something else. Either way the answer is to compute it again.
    let cached = (cache.is_file() && niche_cache::came_from(&cache, &stems.features_source))
        .then(|| read_table(&cache, Extension::Parquet).ok())
        .flatten()
        .and_then(|table| {
            VarAggreg::from_table(&table, &settings.patient_column, sample_column).ok()
        });

    if let Some(cached) = cached {
        // The name already carries the settings, so what is left to check is
        // the cohort: a working directory whose networks changed has the same
        // settings and a different answer. The width is checked too, because it
        // costs nothing and a file of the right height and the wrong shape is
        // the one mistake this would not otherwise catch.
        //
        // One block of columns per statistic. Which statistics are taken is
        // settled by `NicheParams`, which reconciles `stat_funcs` with
        // `stat_names`; counting `stat_names` here would expect a two-block
        // table from a configuration that asked for one.
        let expected_columns = use_attributes.len() * params.n_statistics();
        if cached.n_columns() == expected_columns && cached.n_rows == expected_rows {
            progress.info("[INFO] Reusing the cached aggregated features");
            return Ok(cached);
        }
        progress.info("[INFO] Cached features do not match the cohort, recomputing");
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
    niche_cache::write_table(&cache, &stems.features_source, &table)?;
    Ok(var_aggreg)
}

/// The projection and the partition, each read back when a run already made it.
///
/// The two stages are cached separately on purpose: moving `resolution` should
/// cost the clustering and not the projection, which is the expensive one.
fn cached_reduce_and_cluster(
    var_aggreg: &VarAggreg,
    params: &NicheParams,
    caches: &Caches,
    stems: &Stems,
    progress: &dyn Progress,
) -> Result<(ClusterInput, Vec<u32>, Option<usize>)> {
    let n_rows = var_aggreg.n_rows;
    let reduction_path = caches.reduction_path(&stems.numbers);

    // A projection belongs to the feature table it projected, and a partition
    // to the matrix it partitioned. Neither name says so, so each file is asked
    // what it came from before it is believed.
    let input = match stems
        .reduces
        .then(|| niche_cache::read_matrix(&reduction_path, &stems.reduction_source, n_rows))
        .flatten()
    {
        Some((values, width)) => {
            progress.info("[INFO] Reusing the cached projection");
            ClusterInput {
                values,
                width,
                reduced: true,
            }
        }
        None => {
            let input = project(var_aggreg, params)?;
            if stems.reduces {
                niche_cache::write_matrix(
                    &reduction_path,
                    &stems.reduction_source,
                    &input.values,
                    input.width,
                )?;
            }
            input
        }
    };

    let clustering_path = caches.clustering_path(&stems.numbers);
    let source = stems.clustering_source.as_str();
    let (labels, components) = match niche_cache::read_labels(&clustering_path, source, n_rows) {
        Some(labels) => {
            progress.info("[INFO] Reusing the cached partition");
            // The graph's shape travels with the partition, so a run reading it
            // back reports the same floor as the run that computed it.
            let components = niche_cache::recorded_components(&clustering_path);
            (labels, components)
        }
        None => {
            let (labels, components) = cluster(&input, n_rows, params, progress)?;
            niche_cache::write_labels_with_components(
                &clustering_path,
                source,
                &labels,
                components,
            )?;
            (labels, components)
        }
    };

    Ok((input, labels, components))
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
        stat_names: params.effective_stat_names(),
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
    require_finite(matrix, width)?;

    match params.reducer_type {
        ReducerType::None => Ok(ClusterInput {
            values: matrix.to_vec(),
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
            // Narrowed on the way in and widened on the way out: the
            // reduction runs in `umap::Scalar`, single precision like
            // umap-learn, while everything the pipeline writes to disk —
            // embedding, composition, figures — stays `f64`.
            let narrowed: Vec<Scalar> = matrix.iter().map(|&v| v as Scalar).collect();
            let embedding = umap(&narrowed, var_aggreg.n_rows, width, &umap_params)?;
            Ok(ClusterInput {
                values: embedding.into_iter().map(|v| v as f64).collect(),
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

/// The seed the clustering stage runs on.
///
/// Leiden has always been given `0` here; the neighbour search above it now
/// takes a seed too, and shares this one so that a run turns on a single
/// number. The reference seeds neither — `leidenalg` is called without a seed
/// and `knn_pairs` has nothing to seed — so there is no value to match, only a
/// value to fix.
const CLUSTER_SEED: u64 = 0;

/// Partition the rows into niches.
fn cluster(
    input: &ClusterInput,
    n_rows: usize,
    params: &NicheParams,
    progress: &dyn Progress,
) -> Result<(Vec<u32>, Option<usize>)> {
    let (values, width) = (input.values.as_slice(), input.width);
    // Only a clusterer that partitions a graph has components to report.
    let mut components: Option<usize> = None;

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
            // Single precision, like the neighbour query the reference runs
            // here. When a reduction took place these coordinates *came* from
            // single precision — umap-learn's embedding is `float32` and
            // `knn_pairs` builds its tree on it as it is — so nothing is
            // narrowed that was ever wider. When it did not, the NAS features
            // are narrowed: three significant digits where single precision
            // carries seven, and rows that were exactly equal stay exactly
            // equal, which is what the tie-break depends on.
            let narrowed: Vec<Scalar> = values.iter().map(|&v| v as Scalar).collect();
            // Exact while the cohort is small enough to afford it, approximate
            // above — the embedding's coordinates are all distinct, so the
            // exact search has nothing to group and runs at `O(n^2)`. The seed
            // is the one Leiden is given below, so the whole clustering stage
            // turns on a single number.
            let graph =
                cluster_neighbours(&narrowed, n_rows, width, k, Metric::Euclidean, CLUSTER_SEED);
            let edges = undirected_knn_edges(&graph.indices);

            // What the graph is made of, before it is partitioned.
            //
            // Leiden cannot merge two cells that no path connects, so the
            // number of connected components is a floor on the niche count
            // that no `resolution` can go below. On a cohort of 39 290 cells
            // that floor was 418: sweeping `resolution` over five orders of
            // magnitude moved the answer from 418 to 609 and never said why it
            // would not go lower. The number is cheap — one pass over edges
            // that have just been built — and it is the difference between a
            // sweep that looks broken and one that can be read.
            components = Some(connected_components(n_rows, &edges));
            if let Some(count) = components {
                progress.info(&format!(
                    "[INFO] The clustering graph has {count} connected component(s): \
                     no resolution can find fewer niches than that"
                ));
            }
            leiden(n_rows, &edges, params.resolution, CLUSTER_SEED)
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

    Ok((labels, components))
}

/// How many connected components `edges` leaves `n_rows` nodes in.
///
/// Union-find with path halving: the edges are already in memory and this is
/// one pass over them, which is nothing next to the partition that follows.
fn connected_components(n_rows: usize, edges: &[(usize, usize, f64)]) -> usize {
    let mut parent: Vec<usize> = (0..n_rows).collect();

    fn find(parent: &mut [usize], mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }

    for &(a, b, _) in edges {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[ra] = rb;
        }
    }
    (0..n_rows).filter(|&node| find(&mut parent, node) == node).count()
}

/// Draw each sample's network with its cells coloured by niche.
///
/// # Why this was missing
///
/// `Plot Network`, `X coordinates column for niches` and `Y coordinates column
/// for niches` are three settings the interface offers, the configuration
/// carries, and nothing read: `NicheAnalysisConfig::should_plot_network` was
/// called from nowhere in the repository. Turning the option on produced no
/// figure and no message.
///
/// The Python could not have drawn it either — it asked `generate_cmap(net_dir,
/// 'niches', …)` for a column its aggregated path never wrote. That column is
/// written here, so the figure this option promises can finally be drawn from
/// it.
///
/// The labels go in as text because that is what the figure treats as a
/// vocabulary: a niche is a category, and numbering them does not make the
/// distance between niche 1 and niche 7 mean anything.
#[allow(clippy::too_many_arguments)]
fn plot_networks(
    settings: &NicheAnalysisConfig,
    net_dir: &Path,
    extension: Extension,
    data_index: &[SampleId],
    labels: &[u32],
    save_dir: &Path,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    if !settings.plot_network {
        return Ok(());
    }
    let (Some(x_column), Some(y_column)) =
        (settings.x_column.as_deref(), settings.y_column.as_deref())
    else {
        // Said rather than skipped in silence: the option is on, so the user is
        // expecting a figure, and the reason they are not getting one is a
        // setting two lines below the one they turned on.
        progress.info(
            "[INFO] Plot Network is on but the niche coordinates columns are not set; \
             no network is drawn",
        );
        return Ok(());
    };

    let sample_column = settings.sample_column.as_deref();
    let mut offset = 0usize;
    for id in data_index {
        let nodes = read_table(
            net_dir.join(id.nodes_file_name(
                &settings.patient_column,
                sample_column,
                extension.as_str(),
            )),
            extension,
        )?;
        let coords = nodes.coords(x_column, y_column)?;
        let edges = read_table(
            net_dir.join(id.edges_file_name(
                &settings.patient_column,
                sample_column,
                extension.as_str(),
            )),
            extension,
        )?;
        let pairs = edges.edges()?;

        // This sample's slice of the cohort's labels, in the order the features
        // were stacked — the same split `merge_niche_pheno` makes.
        let end = offset + nodes.n_rows();
        let niches: Vec<String> = labels
            .get(offset..end)
            .ok_or_else(|| {
                PipelineError::invalid(format!(
                    "{} niche labels for a cohort of at least {end} cells",
                    labels.len()
                ))
            })?
            .iter()
            .map(|niche| niche.to_string())
            .collect();
        offset = end;

        figures.network(
            id,
            &settings.patient_column,
            sample_column,
            &coords,
            &pairs,
            &niches,
            save_dir,
        )?;
    }
    Ok(())
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

/// Write the reduced coordinates, so the reduction can be compared on its own.
///
/// The Python writes its projection to `embedding.npy`. Without the equivalent
/// here, a disagreement on the niches cannot be attributed to either of the two
/// stages that produce them: the aggregated features are identical between the
/// two implementations, so the difference lies in the reduction or in the
/// clustering, and nothing on disk separates them. With both projections
/// available, one clusterer can be run on both embeddings and one reducer on
/// both feature matrices, which does separate them.
///
/// Parquet rather than `.npy`: it is what the rest of the pipeline writes, it
/// carries its column names, and it lets the ids travel with the coordinates.
/// `embedding.npy` is a bare array whose rows only mean something next to
/// `var_aggreg`, in the same order — and the two implementations do not stack
/// the samples in the same order.
///
/// Nothing is written when the features went to the clusterer unreduced. There
/// is no projection in that case, and a file named `embedding` holding the raw
/// features would invite exactly the confusion `draw_clusters` refuses.
fn save_embedding(
    input: &ClusterInput,
    var_aggreg: &VarAggreg,
    save_dir: &Path,
    patient_column: &str,
    sample_column: Option<&str>,
    progress: &dyn Progress,
) -> Result<()> {
    if !input.reduced {
        progress.info("[INFO] No reduction: no embedding written");
        return Ok(());
    }

    let mut columns = Vec::with_capacity(input.width + 2);
    for dimension in 0..input.width {
        let values: Vec<f64> = (0..var_aggreg.n_rows)
            .map(|row| input.values[row * input.width + dimension])
            .collect();
        columns.push((format!("dim_{dimension}"), Table::f64_array(values)));
    }
    columns.push((
        patient_column.to_string(),
        Table::string_array(var_aggreg.patients.iter()),
    ));
    if let Some(sample_column) = sample_column {
        columns.push((
            sample_column.to_string(),
            Table::string_array(
                var_aggreg
                    .samples
                    .iter()
                    .map(|s| s.clone().unwrap_or_default()),
            ),
        ));
    }

    let table = Table::from_columns(columns).map_err(|e| PipelineError::invalid(e.to_string()))?;
    let path = save_dir.join("embedding.parquet");
    write_parquet(&table, &path)?;
    progress.info(&format!("[INFO] Embedding saved in {}", path.display()));
    Ok(())
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
        let settings = params("reducer_type: none\nclusterer_type: gmm\nn_clusters: 2\n");
        let input = project(&var_aggreg, &settings).unwrap();
        let (labels, _) =
            cluster(&input, var_aggreg.n_rows, &settings, &crate::SilentProgress).unwrap();
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
