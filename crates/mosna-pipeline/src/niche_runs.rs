//! The catalogue of everything step 3 has computed, `Niche_Analysis/runs.json`.
//!
//! # What it is
//!
//! The three stages of a niche analysis are nested, because that is how they
//! depend on one another: a projection is a projection *of* an aggregation, and
//! a partition is a partition *of* a projection. The catalogue is shaped the
//! same way — every aggregation, and inside it every projection made from it,
//! and inside each of those every partition made from it.
//!
//! A run is then three numbers, and both the directory it writes to and the
//! files it was computed from are named after them:
//!
//! ```text
//! Niche_Analysis/
//! ├─ runs.json
//! ├─ 1-1-1/                          the first of each
//! ├─ 1-1-2/                          the same projection, clustered again
//! └─ 2-1-1/                          a second aggregation
//! temp/intermediate_files/
//! ├─ var_aggreg-1/
//! │  ├─ var_aggreg_1.parquet
//! │  └─ reduction-1/
//! │     ├─ reduction_1.parquet
//! │     ├─ clustering_1.parquet
//! │     └─ clustering_2.parquet
//! └─ var_aggreg-2/ …
//! ```
//!
//! `1-1-2` is not a label to look up: it is the path. The partition is
//! `temp/intermediate_files/var_aggreg-1/reduction-1/clustering_2.parquet`, and
//! what produced it is the projection and the aggregation it sits under.
//!
//! # Why the numbers are local
//!
//! A projection is numbered among the projections of *its* aggregation, not
//! among all projections. Two aggregations reduced with identical settings both
//! get projection 1, and they do not collide because they are in different
//! directories. That is what lets a number mean "the first thing I tried here"
//! rather than a running count of everything ever computed.
//!
//! # The one zero
//!
//! A configuration with `reducer_type: none` has no projection: its middle
//! number is `0`, and that directory holds partitions and no projection file.
//! Everywhere else the numbering starts at one, so a zero in the first or last
//! position would mean something had gone wrong.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{json, Map, Value};

use crate::error::{create_dir_all, PipelineError, Result};
use crate::report::clock::stamp;

/// The file, relative to `Niche_Analysis`.
pub const CATALOGUE: &str = "runs.json";

/// The one top-level key of the register that is not an aggregation.
///
/// Everything else at the top level is a number — an aggregation and everything
/// computed from it. Per-sample runs need a place of their own because they are
/// not aggregations: one of them *owns* several, one per sample.
pub const PER_SAMPLE_RUNS: &str = "per_sample_runs";

/// The number a stage carries when it did not run at all.
pub const ABSENT: u32 = 0;

/// How far a run got.
///
/// # Why this is recorded at all
///
/// The register used to be written only once a run had finished, which left two
/// holes. A run that died half-way had already created its numbered directory,
/// so the next run handed that number was told its settings "had already been
/// run" — an entry the register knew nothing about. And a per-sample run that
/// failed on its third sample lost the record of the two that had succeeded,
/// along with the numbers naming their caches.
///
/// A run now announces itself as [`Status::Running`] the moment it claims its
/// numbers, and says how it ended. Nothing is inferred from a directory's
/// existence any more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Claimed, and still being computed — or abandoned by a process that died.
    Running,
    Done,
    Failed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Running => "running",
            Status::Done => "done",
            Status::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "done" => Status::Done,
            "failed" => Status::Failed,
            _ => Status::Running,
        }
    }
}

/// The three numbers that name a run's directory and its cache files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunNumbers {
    pub var_aggreg: u32,
    pub reduction: u32,
    pub clustering: u32,
}

impl RunNumbers {
    /// The directory name: the three numbers, in stage order.
    pub fn directory(&self) -> String {
        format!("{}-{}-{}", self.var_aggreg, self.reduction, self.clustering)
    }
}

/// One aggregation, and everything computed from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Aggregation {
    /// `aggregated` or `per_sample`.
    pub mode: String,
    /// The settings that produced it, under the names the configuration uses.
    pub parameters: Value,
    pub id: u32,
    pub path: String,
    /// The sample this aggregation covers, or empty for the pooled cohort.
    ///
    /// Part of its identity: the per-sample mode aggregates every sample under
    /// the same settings, so without this they would all be one aggregation.
    pub sample: String,
    pub updated: String,
    pub status: Status,
    pub reductions: BTreeMap<u32, Reduction>,
}

/// One projection of one aggregation, and everything computed from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Reduction {
    pub parameters: Value,
    pub id: u32,
    /// Empty when there is no projection — see the module's note on zero.
    pub path: String,
    pub updated: String,
    pub status: Status,
    pub clusterings: BTreeMap<u32, Clustering>,
}

/// One partition of one projection.
#[derive(Debug, Clone, PartialEq)]
pub struct Clustering {
    pub parameters: Value,
    pub id: u32,
    pub path: String,
    pub updated: String,
    pub status: Status,
}

/// `Niche_Analysis/runs.json`, read and written as a whole.
///
/// The file is small — one entry per distinct attempt at each stage — and it is
/// rewritten at the end of a run rather than appended to, so a half-written
/// entry from an interrupted run cannot survive.
#[derive(Debug)]
pub struct Catalogue {
    path: PathBuf,
    aggregations: BTreeMap<u32, Aggregation>,
    per_sample_runs: BTreeMap<u32, PerSampleRun>,
}

/// One per-sample analysis: the same settings applied to every sample.
///
/// # Why this is a level of its own
///
/// Each sample of a per-sample run is aggregated, projected and partitioned on
/// its own, so each needs its own three numbers and its own cache files — and
/// each used to get its own *aggregation number*, which is all it got. One run
/// over four samples therefore wrote `niches_1-1-1` into the first nodes file,
/// `niches_2-1-1` into the second, and so on. There was no column the samples
/// shared, so the network view could not be asked to colour by that run; there
/// was no directory the run owned, so its figures sat among the aggregated
/// runs' indistinguishable from them; and there was no name to refer to the run
/// by, which a sweep over parameters needs before anything else.
///
/// The per-sample run is that name. It holds the numbers of the samples it is
/// made of, so the caches stay exactly where they were.
#[derive(Debug, Clone, PartialEq)]
pub struct PerSampleRun {
    pub id: u32,
    /// The identity of the settings, which every sample of the run shares.
    pub fingerprint: String,
    pub parameters: Value,
    pub cohort: String,
    /// Each sample, and the three numbers naming its caches.
    pub samples: BTreeMap<String, String>,
    pub status: Status,
    pub updated: String,
}

impl PerSampleRun {
    /// The directory a per-sample run owns, and the column its labels go into.
    pub fn directory(id: u32) -> String {
        format!("ps-{id}")
    }
}

impl Catalogue {
    /// Read the catalogue of `niche_dir`, or start an empty one.
    ///
    /// A file that cannot be parsed is an error rather than a fresh start:
    /// overwriting it would throw away the only record of what the numbered
    /// directories beside it hold.
    pub fn load(niche_dir: &Path) -> Result<Self> {
        let path = niche_dir.join(CATALOGUE);
        if !path.is_file() {
            return Ok(Self {
                path,
                aggregations: BTreeMap::new(),
                per_sample_runs: BTreeMap::new(),
            });
        }

        let text = std::fs::read_to_string(&path).map_err(|source| PipelineError::Read {
            path: path.clone(),
            source,
        })?;
        let parsed: Value = serde_json::from_str(&text).map_err(|e| {
            PipelineError::invalid(format!("{} is not valid JSON: {e}", path.display()))
        })?;
        let Value::Object(entries) = parsed else {
            return Err(PipelineError::invalid(format!(
                "{} should hold one entry per aggregation",
                path.display()
            )));
        };

        let mut aggregations = BTreeMap::new();
        let mut per_sample_runs = BTreeMap::new();
        for (key, value) in entries {
            if key == PER_SAMPLE_RUNS {
                if let Value::Object(runs) = value {
                    for (id, run) in runs {
                        let id = number(&id, &path)?;
                        per_sample_runs.insert(id, per_sample_run_from_json(id, &run, &path)?);
                    }
                }
                continue;
            }
            let id = number(&key, &path)?;
            aggregations.insert(id, aggregation_from_json(id, &value, &path)?);
        }
        Ok(Self {
            path,
            aggregations,
            per_sample_runs,
        })
    }

    /// The three numbers for this run: the ones it already has, or the next
    /// free ones at whichever level it is new.
    ///
    /// `reduction` is `None` for a configuration that does not reduce, which is
    /// the only way to get a zero. Recording as it goes is what lets the
    /// per-sample mode resolve every sample in one pass: the catalogue is not
    /// written until the run ends, so without this every sample would be handed
    /// the same numbers and would overwrite the sample before it.
    pub fn resolve(
        &mut self,
        mode: &str,
        sample: &str,
        features: &Value,
        reduction: Option<&Value>,
        clustering: &Value,
    ) -> RunNumbers {
        let stamped = now();

        let existing = self
            .aggregations
            .values()
            .find(|a| &a.parameters == features && a.sample == sample)
            .map(|a| a.id);
        let var_aggreg = existing.unwrap_or_else(|| next(&self.aggregations));
        let aggregation = self
            .aggregations
            .entry(var_aggreg)
            .or_insert_with(|| Aggregation {
                mode: mode.to_string(),
                parameters: features.clone(),
                id: var_aggreg,
                path: paths::features(var_aggreg),
                sample: sample.to_string(),
                updated: stamped.clone(),
                status: Status::Running,
                reductions: BTreeMap::new(),
            });
        aggregation.updated = stamped.clone();

        // A run that does not reduce keeps its partitions under number zero, so
        // that one rule — the partition is under its projection — has no
        // exception to it.
        let (reduction_id, parameters) = match reduction {
            Some(parameters) => {
                let existing = aggregation
                    .reductions
                    .values()
                    .find(|r| &r.parameters == parameters && r.id != ABSENT)
                    .map(|r| r.id);
                (
                    existing.unwrap_or_else(|| next_above_zero(&aggregation.reductions)),
                    parameters.clone(),
                )
            }
            None => (ABSENT, json!({ "reducer_type": "none" })),
        };

        let projection = aggregation
            .reductions
            .entry(reduction_id)
            .or_insert_with(|| Reduction {
                parameters,
                id: reduction_id,
                path: if reduction_id == ABSENT {
                    String::new()
                } else {
                    paths::reduction(var_aggreg, reduction_id)
                },
                updated: stamped.clone(),
                status: Status::Running,
                clusterings: BTreeMap::new(),
            });
        projection.updated = stamped.clone();

        let existing = projection
            .clusterings
            .values()
            .find(|c| &c.parameters == clustering)
            .map(|c| c.id);
        let clustering_id = existing.unwrap_or_else(|| next(&projection.clusterings));
        let partition = projection
            .clusterings
            .entry(clustering_id)
            .or_insert_with(|| Clustering {
                parameters: clustering.clone(),
                id: clustering_id,
                path: paths::clustering(var_aggreg, reduction_id, clustering_id),
                updated: stamped.clone(),
                status: Status::Running,
            });
        partition.updated = stamped;

        RunNumbers {
            var_aggreg,
            reduction: reduction_id,
            clustering: clustering_id,
        }
    }

    /// The same, but refusing numbers whose directory belongs to someone else.
    ///
    /// # Why a number can be taken
    ///
    /// The numbers come from the register, so losing it starts the counting
    /// again — and the next run was handed `1-1-1` and wrote over the results
    /// of a run it had nothing to do with. The directories outlive the register
    /// and say what produced them, so a number that would land on a *different*
    /// run's results is passed over and the next free one is taken instead.
    ///
    /// `usable` is asked about the whole triple, because that is what names the
    /// directory. Only the last number moves: the aggregation and the
    /// projection are shared with other runs, and renumbering them would orphan
    /// their caches.
    pub fn resolve_avoiding(
        &mut self,
        mode: &str,
        sample: &str,
        features: &Value,
        reduction: Option<&Value>,
        clustering: &Value,
        usable: &dyn Fn(RunNumbers) -> bool,
    ) -> RunNumbers {
        let mut numbers = self.resolve(mode, sample, features, reduction, clustering);
        if usable(numbers) {
            return numbers;
        }

        // The directory this run would write to holds another run's results, so
        // the register's claim on it is stale — the register and the disk
        // disagree, and the disk is the one with the results on it. The entry
        // goes, and the next free partition number under the same projection is
        // tried, until one lands somewhere unclaimed.
        //
        // Dropping the rejected entry matters: left in place it said `running`,
        // so a register read back after a recycled number showed a run no
        // process was working on and no directory belonged to.
        let stamped = now();
        loop {
            let projection = self
                .aggregations
                .get_mut(&numbers.var_aggreg)
                .and_then(|a| a.reductions.get_mut(&numbers.reduction))
                .expect("resolve just created this projection");
            projection.clusterings.remove(&numbers.clustering);

            let next = next(&projection.clusterings).max(numbers.clustering + 1);
            projection.clusterings.insert(
                next,
                Clustering {
                    parameters: clustering.clone(),
                    id: next,
                    path: paths::clustering(numbers.var_aggreg, numbers.reduction, next),
                    updated: stamped.clone(),
                    status: Status::Running,
                },
            );
            numbers.clustering = next;
            if usable(numbers) {
                return numbers;
            }
        }
    }

    /// Record how a run ended.
    ///
    /// Silently does nothing for numbers the register does not hold: the caller
    /// is reporting on a run it claimed, and a register that has lost the entry
    /// in between is a register somebody deleted, not an error worth aborting a
    /// finished analysis over.
    pub fn mark(&mut self, numbers: RunNumbers, status: Status) {
        let stamped = now();
        let Some(aggregation) = self.aggregations.get_mut(&numbers.var_aggreg) else {
            return;
        };
        aggregation.status = status;
        aggregation.updated = stamped.clone();

        let Some(projection) = aggregation.reductions.get_mut(&numbers.reduction) else {
            return;
        };
        projection.status = status;
        projection.updated = stamped.clone();

        if let Some(partition) = projection.clusterings.get_mut(&numbers.clustering) {
            partition.status = status;
            partition.updated = stamped;
        }
    }

    /// The per-sample run these settings belong to, opening one if it is new.
    ///
    /// Matched on the fingerprint, which every sample of a run shares because it
    /// is taken over the settings and the cohort and not over the sample. So
    /// re-running the same per-sample settings lands on the same run — one
    /// directory, one label column — and changing any of them opens the next.
    pub fn per_sample_run(
        &mut self,
        fingerprint: &str,
        parameters: &Value,
        cohort: &str,
    ) -> u32 {
        let stamped = now();
        let existing = self
            .per_sample_runs
            .values()
            .find(|run| run.fingerprint == fingerprint)
            .map(|run| run.id);

        let id = existing.unwrap_or_else(|| next(&self.per_sample_runs));
        let run = self.per_sample_runs.entry(id).or_insert_with(|| PerSampleRun {
            id,
            fingerprint: fingerprint.to_string(),
            parameters: parameters.clone(),
            cohort: cohort.to_string(),
            samples: BTreeMap::new(),
            status: Status::Running,
            updated: stamped.clone(),
        });
        run.status = Status::Running;
        run.updated = stamped;
        id
    }

    /// The same, refusing an id whose directory belongs to another run.
    ///
    /// The per-sample counterpart of [`fn@Self::resolve_avoiding`]: losing the
    /// register restarts the `ps-` numbering, and `ps-1` already held another
    /// run's results, which were overwritten.
    pub fn per_sample_run_avoiding(
        &mut self,
        fingerprint: &str,
        parameters: &Value,
        cohort: &str,
        usable: &dyn Fn(u32) -> bool,
    ) -> u32 {
        let mut id = self.per_sample_run(fingerprint, parameters, cohort);
        if usable(id) {
            return id;
        }

        // Its directory is someone else's, so the register's claim on it is
        // stale. Drop it — left in place it said `running` for a run nothing
        // was working on — and take the next free number, until one lands
        // somewhere unclaimed.
        let stamped = now();
        loop {
            self.per_sample_runs.remove(&id);
            let next = next(&self.per_sample_runs).max(id + 1);
            self.per_sample_runs.insert(
                next,
                PerSampleRun {
                    id: next,
                    fingerprint: fingerprint.to_string(),
                    parameters: parameters.clone(),
                    cohort: cohort.to_string(),
                    samples: BTreeMap::new(),
                    status: Status::Running,
                    updated: stamped.clone(),
                },
            );
            if usable(next) {
                return next;
            }
            id = next;
        }
    }

    /// Note which cache files one sample of a per-sample run resolved to.
    ///
    /// Recorded as the sample finishes rather than at the end of the cohort: a
    /// run that failed on its third sample used to lose the record of the two
    /// before it, and with it the numbers naming their caches.
    pub fn record_sample(&mut self, run: u32, sample: &str, numbers: RunNumbers) {
        if let Some(entry) = self.per_sample_runs.get_mut(&run) {
            entry
                .samples
                .insert(sample.to_string(), numbers.directory());
            entry.updated = now();
        }
    }

    /// Record how a per-sample run ended.
    pub fn mark_per_sample(&mut self, run: u32, status: Status) {
        if let Some(entry) = self.per_sample_runs.get_mut(&run) {
            entry.status = status;
            entry.updated = now();
        }
    }

    /// One per-sample run, for tests and for callers reporting on a directory.
    pub fn per_sample(&self, id: u32) -> Option<&PerSampleRun> {
        self.per_sample_runs.get(&id)
    }

    /// One aggregation, for tests and for callers reporting on a directory.
    pub fn aggregation(&self, id: u32) -> Option<&Aggregation> {
        self.aggregations.get(&id)
    }

    /// How many aggregations are recorded.
    pub fn len(&self) -> usize {
        self.aggregations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.aggregations.is_empty()
    }

    /// Write the catalogue back.
    pub fn save(&self) -> Result<()> {
        let mut root = Map::new();
        for (id, aggregation) in &self.aggregations {
            root.insert(id.to_string(), aggregation_to_json(aggregation));
        }
        if !self.per_sample_runs.is_empty() {
            let mut runs = Map::new();
            for (id, run) in &self.per_sample_runs {
                runs.insert(id.to_string(), per_sample_run_to_json(run));
            }
            root.insert(PER_SAMPLE_RUNS.to_string(), Value::Object(runs));
        }

        let text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| {
            PipelineError::invalid(format!("cannot serialise the run catalogue: {e}"))
        })?;
        if let Some(parent) = self.path.parent() {
            create_dir_all(parent)?;
        }
        std::fs::write(&self.path, text).map_err(|source| PipelineError::Write {
            path: self.path.clone(),
            source,
        })
    }
}

/// Where each cached file lives, relative to the working directory.
///
/// One rule: an aggregation owns a directory, a projection owns a directory
/// inside it, and the partitions sit beside the projection they came from.
pub mod paths {
    /// The directory of one aggregation.
    pub fn aggregation_dir(var_aggreg: u32) -> String {
        format!("temp/intermediate_files/var_aggreg-{var_aggreg}")
    }

    /// The directory of one projection, and of the partitions made from it.
    pub fn reduction_dir(var_aggreg: u32, reduction: u32) -> String {
        format!("{}/reduction-{reduction}", aggregation_dir(var_aggreg))
    }

    pub fn features(var_aggreg: u32) -> String {
        format!(
            "{}/var_aggreg_{var_aggreg}.parquet",
            aggregation_dir(var_aggreg)
        )
    }

    pub fn reduction(var_aggreg: u32, reduction: u32) -> String {
        format!(
            "{}/reduction_{reduction}.parquet",
            reduction_dir(var_aggreg, reduction)
        )
    }

    pub fn clustering(var_aggreg: u32, reduction: u32, clustering: u32) -> String {
        format!(
            "{}/clustering_{clustering}.parquet",
            reduction_dir(var_aggreg, reduction)
        )
    }
}

/// The current time, in the catalogue's spelling.
pub fn now() -> String {
    stamp(SystemTime::now())
}

/// The next free number, counting from one.
fn next<T>(entries: &BTreeMap<u32, T>) -> u32 {
    entries.keys().next_back().map_or(1, |last| last + 1)
}

/// The same, skipping the zero a run without a projection occupies.
fn next_above_zero<T>(entries: &BTreeMap<u32, T>) -> u32 {
    entries
        .keys()
        .rfind(|id| **id != ABSENT)
        .map_or(1, |last| last + 1)
}

fn number(key: &str, path: &Path) -> Result<u32> {
    key.parse().map_err(|_| {
        PipelineError::invalid(format!(
            "{} holds the key `{key}`, which is not a number",
            path.display()
        ))
    })
}

fn aggregation_to_json(entry: &Aggregation) -> Value {
    let mut reductions = Map::new();
    for (id, reduction) in &entry.reductions {
        reductions.insert(id.to_string(), reduction_to_json(reduction));
    }
    json!({
        "mode": entry.mode,
        "parameters": entry.parameters,
        "id": entry.id,
        "path": entry.path,
        "sample": entry.sample,
        "updated": entry.updated,
        "status": entry.status.as_str(),
        "reduction": Value::Object(reductions),
    })
}

fn reduction_to_json(entry: &Reduction) -> Value {
    let mut clusterings = Map::new();
    for (id, clustering) in &entry.clusterings {
        clusterings.insert(id.to_string(), clustering_to_json(clustering));
    }
    json!({
        "parameters": entry.parameters,
        "id": entry.id,
        "path": entry.path,
        "updated": entry.updated,
        "status": entry.status.as_str(),
        "clustering": Value::Object(clusterings),
    })
}

fn per_sample_run_to_json(entry: &PerSampleRun) -> Value {
    let mut samples = Map::new();
    for (sample, numbers) in &entry.samples {
        samples.insert(sample.clone(), json!(numbers));
    }
    json!({
        "id": entry.id,
        "fingerprint": entry.fingerprint,
        "parameters": entry.parameters,
        "cohort": entry.cohort,
        "path": PerSampleRun::directory(entry.id),
        "updated": entry.updated,
        "status": entry.status.as_str(),
        "samples": Value::Object(samples),
    })
}

fn per_sample_run_from_json(id: u32, value: &Value, _path: &Path) -> Result<PerSampleRun> {
    let mut samples = BTreeMap::new();
    if let Some(Value::Object(entries)) = value.get("samples") {
        for (sample, numbers) in entries {
            samples.insert(
                sample.clone(),
                numbers.as_str().unwrap_or_default().to_string(),
            );
        }
    }
    Ok(PerSampleRun {
        id,
        fingerprint: text(value, "fingerprint"),
        parameters: value.get("parameters").cloned().unwrap_or(Value::Null),
        cohort: text(value, "cohort"),
        samples,
        status: Status::parse(&text(value, "status")),
        updated: text(value, "updated"),
    })
}

fn clustering_to_json(entry: &Clustering) -> Value {
    json!({
        "parameters": entry.parameters,
        "id": entry.id,
        "path": entry.path,
        "updated": entry.updated,
        "status": entry.status.as_str(),
    })
}

/// Read a field that should be a string, or an empty one.
///
/// A catalogue from another version should cost a re-run, never a crash: a
/// missing field reads as empty, which makes the entry match no settings and so
/// simply hands out a new number.
fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn aggregation_from_json(id: u32, value: &Value, path: &Path) -> Result<Aggregation> {
    let mut reductions = BTreeMap::new();
    if let Some(Value::Object(entries)) = value.get("reduction") {
        for (key, reduction) in entries {
            let reduction_id = number(key, path)?;
            reductions.insert(
                reduction_id,
                reduction_from_json(reduction_id, reduction, path)?,
            );
        }
    }
    Ok(Aggregation {
        mode: text(value, "mode"),
        parameters: value.get("parameters").cloned().unwrap_or(Value::Null),
        id,
        path: text(value, "path"),
        sample: text(value, "sample"),
        updated: text(value, "updated"),
        status: Status::parse(&text(value, "status")),
        reductions,
    })
}

fn reduction_from_json(id: u32, value: &Value, path: &Path) -> Result<Reduction> {
    let mut clusterings = BTreeMap::new();
    if let Some(Value::Object(entries)) = value.get("clustering") {
        for (key, clustering) in entries {
            let clustering_id = number(key, path)?;
            clusterings.insert(
                clustering_id,
                Clustering {
                    parameters: clustering.get("parameters").cloned().unwrap_or(Value::Null),
                    id: clustering_id,
                    path: text(clustering, "path"),
                    updated: text(clustering, "updated"),
                    status: Status::parse(&text(clustering, "status")),
                },
            );
        }
    }
    Ok(Reduction {
        parameters: value.get("parameters").cloned().unwrap_or(Value::Null),
        id,
        path: text(value, "path"),
        updated: text(value, "updated"),
        status: Status::parse(&text(value, "status")),
        clusterings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features(order: usize) -> Value {
        json!({"stat_names": ["mean", "std"], "order": order})
    }

    fn umap(dim: usize) -> Value {
        json!({"reducer_type": "umap", "dim_clust": dim})
    }

    fn leiden(resolution: f64) -> Value {
        json!({"clusterer_type": "leiden", "resolution": resolution})
    }

    fn catalogue(dir: &Path) -> Catalogue {
        Catalogue::load(dir).unwrap()
    }

    fn triple(numbers: RunNumbers) -> (u32, u32, u32) {
        (numbers.var_aggreg, numbers.reduction, numbers.clustering)
    }

    #[test]
    fn an_absent_catalogue_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalogue = catalogue(dir.path());
        assert!(catalogue.is_empty());
        assert_eq!(
            triple(catalogue.resolve(
                "aggregated",
                "",
                &features(1),
                Some(&umap(2)),
                &leiden(0.05)
            )),
            (1, 1, 1)
        );
    }

    #[test]
    fn the_same_run_always_gets_the_same_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalogue = catalogue(dir.path());

        let first = catalogue.resolve(
            "aggregated",
            "",
            &features(1),
            Some(&umap(2)),
            &leiden(0.05),
        );
        let again = catalogue.resolve(
            "aggregated",
            "",
            &features(1),
            Some(&umap(2)),
            &leiden(0.05),
        );
        assert_eq!(first, again);
        assert_eq!(catalogue.len(), 1);
    }

    /// Changing one setting renumbers that stage and everything under it, and
    /// leaves everything above it alone.
    #[test]
    fn a_change_renumbers_only_from_where_it_happened() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalogue = catalogue(dir.path());

        catalogue.resolve(
            "aggregated",
            "",
            &features(1),
            Some(&umap(2)),
            &leiden(0.05),
        );

        // A second partition of the same projection.
        assert_eq!(
            triple(catalogue.resolve("aggregated", "", &features(1), Some(&umap(2)), &leiden(0.9))),
            (1, 1, 2)
        );
        // A second projection of the same aggregation, clustered afresh.
        assert_eq!(
            triple(catalogue.resolve(
                "aggregated",
                "",
                &features(1),
                Some(&umap(3)),
                &leiden(0.05)
            )),
            (1, 2, 1)
        );
        // A second aggregation: everything below it starts again at one.
        assert_eq!(
            triple(catalogue.resolve(
                "aggregated",
                "",
                &features(2),
                Some(&umap(2)),
                &leiden(0.05)
            )),
            (2, 1, 1)
        );
    }

    /// The numbers are local to their parent, which is the whole point of the
    /// nesting: the same projection settings under two aggregations are both
    /// number one, and do not collide because they are in different places.
    #[test]
    fn the_numbers_are_local_to_their_parent() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalogue = catalogue(dir.path());

        let first = catalogue.resolve(
            "aggregated",
            "",
            &features(1),
            Some(&umap(2)),
            &leiden(0.05),
        );
        let second = catalogue.resolve(
            "aggregated",
            "",
            &features(2),
            Some(&umap(2)),
            &leiden(0.05),
        );

        assert_eq!(triple(first), (1, 1, 1));
        assert_eq!(triple(second), (2, 1, 1));
        assert_ne!(
            catalogue.aggregation(1).unwrap().reductions[&1].path,
            catalogue.aggregation(2).unwrap().reductions[&1].path
        );
    }

    #[test]
    fn a_run_without_a_reduction_carries_the_one_zero() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalogue = catalogue(dir.path());

        let numbers = catalogue.resolve("aggregated", "", &features(1), None, &leiden(0.05));
        assert_eq!(triple(numbers), (1, 0, 1));
        assert_eq!(numbers.directory(), "1-0-1");

        // No projection file to name, and the partitions sit in its place.
        let reduction = &catalogue.aggregation(1).unwrap().reductions[&ABSENT];
        assert_eq!(reduction.path, "");
        assert_eq!(
            reduction.clusterings[&1].path,
            "temp/intermediate_files/var_aggreg-1/reduction-0/clustering_1.parquet"
        );

        // And a projection added later starts at one, not at zero plus one.
        let reduced = catalogue.resolve(
            "aggregated",
            "",
            &features(1),
            Some(&umap(2)),
            &leiden(0.05),
        );
        assert_eq!(triple(reduced), (1, 1, 1));
    }

    /// The per-sample mode resolves every sample in one pass, before the
    /// catalogue has been written. The settings are identical for all of them,
    /// so only the sample tells them apart.
    #[test]
    fn every_sample_gets_its_own_aggregation() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalogue = catalogue(dir.path());

        let given: Vec<(u32, u32, u32)> = ["patient-1", "patient-2", "patient-3"]
            .iter()
            .map(|sample| {
                triple(catalogue.resolve(
                    "per_sample",
                    sample,
                    &features(1),
                    Some(&umap(2)),
                    &leiden(0.05),
                ))
            })
            .collect();

        assert_eq!(given, vec![(1, 1, 1), (2, 1, 1), (3, 1, 1)]);
        assert_eq!(catalogue.aggregation(2).unwrap().sample, "patient-2");
    }

    #[test]
    fn the_paths_follow_the_numbers() {
        assert_eq!(
            paths::features(3),
            "temp/intermediate_files/var_aggreg-3/var_aggreg_3.parquet"
        );
        assert_eq!(
            paths::reduction(3, 2),
            "temp/intermediate_files/var_aggreg-3/reduction-2/reduction_2.parquet"
        );
        assert_eq!(
            paths::clustering(3, 2, 7),
            "temp/intermediate_files/var_aggreg-3/reduction-2/clustering_7.parquet"
        );
    }

    #[test]
    fn a_catalogue_survives_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut written = catalogue(dir.path());
        written.resolve(
            "aggregated",
            "",
            &features(1),
            Some(&umap(2)),
            &leiden(0.05),
        );
        written.resolve("aggregated", "", &features(1), Some(&umap(2)), &leiden(0.9));
        written.resolve("aggregated", "", &features(2), None, &leiden(0.05));
        written.save().unwrap();

        let read = catalogue(dir.path());
        assert_eq!(read.len(), 2);
        let first = read.aggregation(1).unwrap();
        assert_eq!(first.reductions[&1].clusterings.len(), 2);
        assert_eq!(first.reductions[&1].clusterings[&2].parameters, leiden(0.9));
        assert_eq!(read.aggregation(2).unwrap().reductions[&ABSENT].path, "");
    }

    /// The shape the file is meant to have: aggregations at the top, their
    /// projections inside them, the partitions inside those.
    #[test]
    fn the_file_nests_the_stages() {
        let dir = tempfile::tempdir().unwrap();
        let mut written = catalogue(dir.path());
        written.resolve(
            "aggregated",
            "",
            &features(1),
            Some(&umap(2)),
            &leiden(0.05),
        );
        written.save().unwrap();

        let text = std::fs::read_to_string(dir.path().join(CATALOGUE)).unwrap();
        let parsed: Value = serde_json::from_str(&text).unwrap();

        let aggregation = &parsed["1"];
        assert_eq!(aggregation["mode"], "aggregated");
        assert_eq!(aggregation["id"], 1);
        assert_eq!(
            aggregation["path"],
            "temp/intermediate_files/var_aggreg-1/var_aggreg_1.parquet"
        );
        assert_eq!(aggregation["parameters"]["order"], 1);
        assert!(aggregation["updated"].is_string());

        let reduction = &aggregation["reduction"]["1"];
        assert_eq!(
            reduction["path"],
            "temp/intermediate_files/var_aggreg-1/reduction-1/reduction_1.parquet"
        );
        assert_eq!(reduction["parameters"]["dim_clust"], 2);

        let clustering = &reduction["clustering"]["1"];
        assert_eq!(
            clustering["path"],
            "temp/intermediate_files/var_aggreg-1/reduction-1/clustering_1.parquet"
        );
        assert_eq!(clustering["parameters"]["resolution"], 0.05);
    }

    #[test]
    fn a_corrupt_catalogue_is_an_error_rather_than_a_fresh_start() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CATALOGUE), "{not json").unwrap();
        let err = Catalogue::load(dir.path()).unwrap_err();
        assert!(err.to_string().contains("not valid JSON"), "{err}");
    }

    #[test]
    fn an_entry_missing_its_fields_matches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CATALOGUE), r#"{"1": {}}"#).unwrap();
        let mut catalogue = catalogue(dir.path());
        assert_eq!(catalogue.len(), 1);
        assert_eq!(
            triple(catalogue.resolve(
                "aggregated",
                "",
                &features(1),
                Some(&umap(2)),
                &leiden(0.05)
            )),
            (2, 1, 1)
        );
    }
}
