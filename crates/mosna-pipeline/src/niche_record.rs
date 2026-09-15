//! `Niche_Analysis/<run>/run.json` — what one run directory says about itself.
//!
//! # What it replaces
//!
//! Every run directory used to hold a `parameters.json` that was the whole
//! `Niche Analysis` section of the configuration, written unchanged. An
//! aggregated run and a per-sample run came out byte for byte identical: both
//! carried both sub-sections, neither said which one had been used, which
//! sample it covered, which numbers it had been given, or what cohort it had
//! been computed from. It was the file a user opens to ask "where did this
//! figure come from", and it did not answer.
//!
//! `run.json` answers it. `parameters.json` is still written beside it,
//! unchanged, because it is the configuration one would feed back to `mosna` to
//! run this again.
//!
//! # Why the fingerprint is in it
//!
//! A run's numbers come from the register, so losing the register starts the
//! numbering again and the next run is handed `1-1-1`. It used to overwrite
//! whatever was there. The fingerprint recorded here is what a claiming run
//! compares against: a directory whose run is not this run is left alone, and
//! another number is taken. The numbers are an alias; this is the identity.
//!
//! # Renderings
//!
//! `normalize` and `Phenotype column` choose which composition figures are
//! drawn. They change no part of the partition, so they must not make a new
//! run — but the directory then accumulates figures from several settings, and
//! a `parameters.json` naming only the last one made the earlier figures look
//! like they came from settings that never produced them. Each is appended to
//! `renderings` instead.

use std::path::Path;

use serde_json::{json, Map, Value};

use crate::error::{PipelineError, Result};
use crate::niche_cohort::Fnv;
use crate::niche_runs::{now, RunNumbers, Status};

/// The file, inside a run's directory.
pub const RECORD: &str = "run.json";

/// What decides which composition figures a run draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendering {
    pub normalize: String,
    pub phenotype_column: Option<String>,
}

impl Rendering {
    fn to_json(&self, stamped: &str) -> Value {
        json!({
            "normalize": self.normalize,
            "phenotype_column": self.phenotype_column,
            "updated": stamped,
        })
    }

    fn matches(&self, value: &Value) -> bool {
        value.get("normalize").and_then(Value::as_str) == Some(self.normalize.as_str())
            && value.get("phenotype_column").and_then(Value::as_str)
                == self.phenotype_column.as_deref()
    }
}

/// What a run found, as opposed to what it was asked for.
///
/// # Why this is on disk
///
/// The number of niches is the one figure every run of a sweep is compared on,
/// and it was nowhere: to learn it you had to open the run's label column and
/// count the distinct values. A comparison table over two hundred runs cannot
/// be built that way, and it is free to record — the partition is in memory
/// when the run ends.
///
/// `graph_components` is there for the same reason and answers a different
/// question. Leiden cannot merge two cells that no path connects, so on a graph
/// with 418 components no `resolution` will ever find fewer than 418 niches. A
/// sweep that does not know this reads as a parameter that barely does
/// anything; one that does knows to change `n_neighbors` instead. It is absent
/// for a clusterer that partitions a matrix rather than a graph.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub niches: usize,
    /// How many cells fell in each niche, by label.
    pub sizes: Vec<usize>,
    pub graph_components: Option<usize>,
}

impl Outcome {
    /// Read a partition back as what it is: a count and a size per niche.
    pub fn of(labels: &[u32], graph_components: Option<usize>) -> Self {
        let mut counts: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
        for label in labels {
            *counts.entry(*label).or_default() += 1;
        }
        Self {
            niches: counts.len(),
            sizes: counts.into_values().collect(),
            graph_components,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "niches": self.niches,
            "sizes": self.sizes,
            "graph_components": self.graph_components,
        })
    }
}

/// Everything a run directory records about itself.
#[derive(Debug, Clone)]
pub struct Record {
    pub numbers: RunNumbers,
    /// How the run is named — its three numbers, or `ps-1` for a per-sample
    /// run, whose samples each carry different numbers but belong to one run.
    pub run: String,
    pub fingerprint: String,
    pub mode: String,
    /// The sample a per-sample run covers; `None` for the pooled cohort.
    pub sample: Option<String>,
    pub cohort: String,
    pub network_directory: String,
    pub patient_column: String,
    pub sample_column: Option<String>,
    /// The settings of the sub-section that ran, with the derived values
    /// resolved — `k_cluster` already capped, the phenotype count filled in.
    pub parameters: Value,
    pub render: Rendering,
    pub result: Outcome,
    pub status: Status,
}

impl Record {
    /// Write the record into `save_dir`, keeping the renderings already there.
    ///
    /// A run that is re-executed with a different `normalize` adds a rendering
    /// rather than replacing the list: both sets of figures are in the
    /// directory, so both have to be accounted for.
    pub fn write(&self, save_dir: &Path) -> Result<()> {
        let path = save_dir.join(RECORD);
        let stamped = now();

        let mut renderings: Vec<Value> = read(save_dir)
            .and_then(|existing| existing.get("renderings").and_then(Value::as_array).cloned())
            .unwrap_or_default()
            .into_iter()
            // A rendering repeated is the same rendering, redrawn; it moves to
            // the end with a fresh stamp rather than appearing twice.
            .filter(|existing| !self.render.matches(existing))
            .collect();
        renderings.push(self.render.to_json(&stamped));

        let mut document = Map::new();
        document.insert("run".into(), json!(self.run));
        document.insert("numbers".into(), json!(self.numbers.directory()));
        document.insert("fingerprint".into(), json!(self.fingerprint));
        document.insert("status".into(), json!(self.status.as_str()));
        document.insert("mode".into(), json!(self.mode));
        document.insert("sample".into(), json!(self.sample));
        document.insert("cohort".into(), json!(self.cohort));
        document.insert("network_directory".into(), json!(self.network_directory));
        document.insert("patient_column".into(), json!(self.patient_column));
        document.insert("sample_column".into(), json!(self.sample_column));
        document.insert("parameters".into(), self.parameters.clone());
        document.insert("render".into(), self.render.to_json(&stamped));
        document.insert("result".into(), self.result.to_json());
        document.insert("renderings".into(), Value::Array(renderings));
        document.insert("updated".into(), json!(stamped));

        let text = serde_json::to_string_pretty(&Value::Object(document))
            .map_err(|e| PipelineError::invalid(format!("cannot serialise {RECORD}: {e}")))?;
        std::fs::write(&path, text).map_err(|source| PipelineError::Write { path, source })
    }
}

/// What a per-sample run's own directory records about itself.
///
/// A per-sample run owns `ps-1/` and writes one sub-directory per sample. Each
/// sample describes itself with a [`Record`]; this describes the run they
/// belong to — and is what a claiming run compares against, exactly as a
/// numbered run compares against the record in `1-1-1/`.
///
/// Written twice: when the run is claimed, so the directory is spoken for
/// before any sample has finished, and again at the end with the samples it
/// turned out to cover.
#[derive(Debug, Clone)]
pub struct PerSampleRecord {
    pub run: String,
    pub fingerprint: String,
    pub cohort: String,
    pub network_directory: String,
    pub patient_column: String,
    pub sample_column: Option<String>,
    pub parameters: Value,
    pub render: Rendering,
    /// The samples covered, filled in as they finish.
    pub samples: Vec<String>,
    pub status: Status,
}

impl PerSampleRecord {
    pub fn write(&self, run_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(run_dir).map_err(|source| PipelineError::Write {
            path: run_dir.to_path_buf(),
            source,
        })?;
        let path = run_dir.join(RECORD);
        let stamped = now();

        let mut document = Map::new();
        document.insert("run".into(), json!(self.run));
        document.insert("fingerprint".into(), json!(self.fingerprint));
        document.insert("status".into(), json!(self.status.as_str()));
        document.insert("mode".into(), json!("per_sample"));
        document.insert("cohort".into(), json!(self.cohort));
        document.insert("network_directory".into(), json!(self.network_directory));
        document.insert("patient_column".into(), json!(self.patient_column));
        document.insert("sample_column".into(), json!(self.sample_column));
        document.insert("parameters".into(), self.parameters.clone());
        document.insert("render".into(), self.render.to_json(&stamped));
        document.insert("samples".into(), json!(self.samples));
        document.insert("updated".into(), json!(stamped));

        let text = serde_json::to_string_pretty(&Value::Object(document))
            .map_err(|e| PipelineError::invalid(format!("cannot serialise {RECORD}: {e}")))?;
        std::fs::write(&path, text).map_err(|source| PipelineError::Write { path, source })
    }
}

/// The record in `save_dir`, if there is a readable one.
///
/// Every reason there might not be leads to the same place — no directory, no
/// file, or a file from a version that wrote something else — so they are not
/// told apart: the caller's question is always "is this directory already
/// somebody's?", and silence means no.
pub fn read(save_dir: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(save_dir.join(RECORD)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Whether `save_dir` holds the results of a run other than `fingerprint`.
///
/// A directory with no record is *not* another run's: it is either empty, or
/// left by a version that wrote no record, and the old contract was to reuse it
/// with a warning. Only a directory that names a different run is protected —
/// which is exactly the case a recycled number produces.
pub fn belongs_to_another(save_dir: &Path, fingerprint: &str) -> bool {
    match read(save_dir) {
        Some(record) => record
            .get("fingerprint")
            .and_then(Value::as_str)
            .is_some_and(|recorded| recorded != fingerprint),
        None => false,
    }
}

/// The identity of a run: the chain of settings behind its partition.
///
/// The same three values the cache records as its provenance, hashed so they
/// fit in a field a human can compare at a glance. Two runs share a fingerprint
/// exactly when they would compute the same labels from the same cohort.
pub fn fingerprint(features: &Value, reduction: Option<&Value>, clustering: &Value) -> String {
    let canonical = json!({
        "features": features,
        "reduction": reduction,
        "clustering": clustering,
    });
    let mut hash = Fnv::default();
    hash.field(canonical.to_string().as_bytes());
    hash.hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(a: u32, r: u32, c: u32) -> RunNumbers {
        RunNumbers {
            var_aggreg: a,
            reduction: r,
            clustering: c,
        }
    }

    fn record(fingerprint: &str, normalize: &str) -> Record {
        Record {
            numbers: numbers(1, 1, 1),
            run: "1-1-1".to_string(),
            fingerprint: fingerprint.to_string(),
            mode: "aggregated".into(),
            sample: None,
            cohort: "deadbeef".into(),
            network_directory: "Default".into(),
            patient_column: "patient".into(),
            sample_column: Some("sample".into()),
            parameters: json!({ "clusterer_type": "gmm", "n_clusters": 3 }),
            render: Rendering {
                normalize: normalize.to_string(),
                phenotype_column: Some("Cluster".into()),
            },
            result: Outcome::of(&[0, 0, 1], Some(2)),
            status: Status::Done,
        }
    }

    #[test]
    fn a_record_says_which_run_it_is_and_what_produced_it() {
        let dir = tempfile::tempdir().unwrap();
        record("abc", "total").write(dir.path()).unwrap();

        let written = read(dir.path()).unwrap();
        assert_eq!(written["run"], "1-1-1");
        assert_eq!(written["fingerprint"], "abc");
        assert_eq!(written["mode"], "aggregated");
        assert_eq!(written["sample"], Value::Null);
        assert_eq!(written["cohort"], "deadbeef");
        assert_eq!(written["parameters"]["n_clusters"], 3);
        assert_eq!(written["status"], "done");
        assert!(written["updated"].as_str().is_some());
    }

    /// The reason renderings are a list: two runs of the same settings under
    /// different normalisations leave both sets of figures in the directory.
    #[test]
    fn a_second_rendering_is_added_rather_than_replacing_the_first() {
        let dir = tempfile::tempdir().unwrap();
        record("abc", "total").write(dir.path()).unwrap();
        record("abc", "clr").write(dir.path()).unwrap();

        let written = read(dir.path()).unwrap();
        let listed: Vec<&str> = written["renderings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["normalize"].as_str().unwrap())
            .collect();
        assert_eq!(listed, vec!["total", "clr"]);
        assert_eq!(written["render"]["normalize"], "clr", "the latest is named");
    }

    /// And the same rendering twice is one rendering, redrawn.
    #[test]
    fn the_same_rendering_twice_is_recorded_once() {
        let dir = tempfile::tempdir().unwrap();
        record("abc", "total").write(dir.path()).unwrap();
        record("abc", "total").write(dir.path()).unwrap();

        assert_eq!(read(dir.path()).unwrap()["renderings"].as_array().unwrap().len(), 1);
    }

    // -----------------------------------------------------------------------
    // Whose directory is it
    // -----------------------------------------------------------------------

    #[test]
    fn a_directory_holding_this_run_is_not_another_runs() {
        let dir = tempfile::tempdir().unwrap();
        record("abc", "total").write(dir.path()).unwrap();
        assert!(!belongs_to_another(dir.path(), "abc"));
    }

    /// The case a recycled number produces: same number, different run.
    #[test]
    fn a_directory_holding_a_different_run_is_protected() {
        let dir = tempfile::tempdir().unwrap();
        record("abc", "total").write(dir.path()).unwrap();
        assert!(belongs_to_another(dir.path(), "xyz"));
    }

    /// A directory with no record is claimable — an empty one, or one left by a
    /// version that wrote none. Refusing those would renumber every run of
    /// every working directory that predates this file.
    #[test]
    fn a_directory_without_a_record_is_claimable() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!belongs_to_another(dir.path(), "abc"));

        std::fs::write(dir.path().join("parameters.json"), "{}").unwrap();
        assert!(!belongs_to_another(dir.path(), "abc"));
    }

    /// An unreadable record is treated the same way: it says nothing, so it
    /// protects nothing.
    #[test]
    fn an_unreadable_record_is_claimable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(RECORD), "{not json").unwrap();
        assert!(!belongs_to_another(dir.path(), "abc"));
    }

    // -----------------------------------------------------------------------
    // The fingerprint
    // -----------------------------------------------------------------------

    #[test]
    fn the_fingerprint_follows_every_stage() {
        let features = json!({ "order": 1 });
        let reduction = json!({ "dim_clust": 2 });
        let clustering = json!({ "n_clusters": 3 });
        let base = fingerprint(&features, Some(&reduction), &clustering);

        assert_eq!(base, fingerprint(&features, Some(&reduction), &clustering));
        assert_ne!(base, fingerprint(&json!({"order": 2}), Some(&reduction), &clustering));
        assert_ne!(
            base,
            fingerprint(&features, Some(&json!({"dim_clust": 3})), &clustering)
        );
        assert_ne!(
            base,
            fingerprint(&features, Some(&reduction), &json!({"n_clusters": 4}))
        );
        assert_ne!(
            base,
            fingerprint(&features, None, &clustering),
            "a run without a projection is not the same run"
        );
    }

    /// The count and the sizes come from the partition itself, so nothing has
    /// to open the label column to build a comparison table.
    #[test]
    fn an_outcome_counts_the_niches_and_their_sizes() {
        let outcome = Outcome::of(&[0, 0, 0, 1, 2, 2], Some(3));
        assert_eq!(outcome.niches, 3);
        assert_eq!(outcome.sizes, vec![3, 1, 2]);
        assert_eq!(outcome.graph_components, Some(3));
    }

    /// An empty partition is a run that found nothing, not a panic.
    #[test]
    fn an_empty_partition_has_no_niches() {
        let outcome = Outcome::of(&[], None);
        assert_eq!(outcome.niches, 0);
        assert!(outcome.sizes.is_empty());
        assert_eq!(outcome.graph_components, None);
    }

    /// Labels need not start at zero or be contiguous — the sizes are keyed by
    /// the label, in label order.
    #[test]
    fn the_sizes_follow_the_labels_that_are_there() {
        let outcome = Outcome::of(&[7, 2, 7, 7], None);
        assert_eq!(outcome.niches, 2);
        assert_eq!(outcome.sizes, vec![1, 3], "niche 2 first, then niche 7");
    }
}
