//! What the cohort is, so a cached result can be attributed to it.
//!
//! # The mistake this exists to prevent
//!
//! A niche cache is named after the settings that produced it, and was checked
//! against the number of rows it should have. Neither says anything about the
//! *network*. So this sequence read back an answer computed from a graph that
//! no longer existed:
//!
//! ```text
//! step 1 with `Edges method: delaunay`     → a network
//! step 3                                   → features, projection, niches
//! step 1 with `Edges method: knn`          → a different network, same cells
//! step 3, settings untouched               → every stage read from the cache
//! ```
//!
//! Every neighbourhood had changed, so every aggregated feature vector had
//! changed; the niches that came back were bit-for-bit the ones from the
//! Delaunay graph, written into files that now held a k-nearest-neighbour one.
//! Nothing reported it, and nothing downstream could have detected it.
//!
//! # What goes into the fingerprint, and what it costs
//!
//! Two things decide the aggregated features: the values of the columns being
//! aggregated, and the edges over which they are aggregated.
//!
//! * **The columns are hashed exactly.** Step 3 already decodes them — it reads
//!   every nodes file to check the columns are there before anything runs — so
//!   the hash is taken during that pass and costs a walk over data already in
//!   memory. Only those columns are hashed, which is what lets the niche label
//!   columns be written back into the same files without the cohort appearing
//!   to have changed: `niches_1-1-1` is not a column anything aggregates.
//!
//! * **The edges are taken on trust, by size and modification time.** Hashing
//!   their contents would mean reading the whole edge list of the cohort before
//!   deciding whether to read it again, which is the cost the cache exists to
//!   avoid. Step 1 rewrites every edges file it produces, so any re-run moves
//!   the timestamp — which is the case this is for.
//!
//! The known gap: an edges file edited in place, by hand, so precisely that
//! neither its size nor its timestamp moves. Nothing the pipeline does produces
//! that, and a cache is allowed to trust a timestamp the way every build system
//! does.
//!
//! # Why it is a value and not a flag
//!
//! The fingerprint is part of a run's identity, not merely a reason to discard
//! a cache. A cohort that changed is a different aggregation, so it is given a
//! number of its own and writes to a directory of its own — the results
//! computed from the previous network keep theirs instead of being overwritten
//! by results that are not comparable to them.

use std::path::Path;

use rayon::prelude::*;

use mosna_io::read::get_opener::{read_table_columns, Extension};
use mosna_io::SampleId;

use crate::error::{PipelineError, Result};

/// The empty hash of FNV-1a, 64-bit.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

/// FNV-1a, spelled out so the fingerprint is reproducible.
///
/// `DefaultHasher` would do the mixing better and is not guaranteed to give the
/// same answer from one Rust release to the next; a fingerprint that changes
/// under the compiler would silently discard every cache after an upgrade.
#[derive(Debug, Clone, Copy)]
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(FNV_OFFSET)
    }
}

impl Fnv {
    pub fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    /// Absorb a value, and a separator after it.
    ///
    /// The separator is what stops `("ab", "c")` and `("a", "bc")` hashing
    /// alike — two column values that concatenate to the same bytes are not the
    /// same cohort.
    pub fn field(&mut self, bytes: &[u8]) {
        self.write(bytes);
        self.write(&[0x1f]);
    }

    pub fn u64(&mut self, value: u64) {
        self.field(&value.to_le_bytes());
    }

    pub fn f64(&mut self, value: f64) {
        // By bits, so that a value that round-trips through parquet unchanged
        // hashes unchanged — including the sign of zero and the payload of NaN.
        self.field(&value.to_bits().to_le_bytes());
    }

    pub fn finish(self) -> u64 {
        self.0
    }

    pub fn hex(self) -> String {
        format!("{:016x}", self.0)
    }
}

/// What one pass over the cohort learns about it.
#[derive(Debug, Clone)]
pub struct CohortDigest {
    /// The fingerprint — see the module note.
    pub fingerprint: String,
    /// How many cells each sample holds, in `data_index` order.
    ///
    /// Read here because the files are open anyway. It used to be asked for
    /// again, once per run, purely to know what height a cached result should
    /// have; it is also what bounds the number of clusters a partition can have.
    pub rows: Vec<usize>,
}

impl CohortDigest {
    /// How many cells the whole cohort holds.
    pub fn total_rows(&self) -> usize {
        self.rows.iter().sum()
    }
}

/// Check that every nodes file carries the columns to aggregate, and say what
/// the cohort holds.
///
/// One pass, because the check and the fingerprint want the same bytes: the
/// values of exactly those columns, in exactly those files. This used to be the
/// verification loop of [`crate::niche_analysis`] and has absorbed the
/// fingerprint rather than reading the cohort a second time to compute it.
///
/// The per-sample digests are combined in `data_index` order, so a cohort that
/// gained a sample, lost one, or had one renamed does not hash the same.
pub fn verify_and_digest(
    net_dir: &Path,
    extension: Extension,
    data_index: &[SampleId],
    patient_column: &str,
    sample_column: Option<&str>,
    columns: &[String],
) -> Result<CohortDigest> {
    let names: Vec<&str> = columns.iter().map(String::as_str).collect();

    // Parallel, and order-preserving: `par_iter().map(..).collect()` keeps the
    // input order, which is what makes the combined digest deterministic.
    let per_sample: Vec<(u64, usize)> = data_index
        .par_iter()
        .map(|id| {
            let file_name = id.nodes_file_name(patient_column, sample_column, extension.as_str());
            let path = net_dir.join(&file_name);
            let table = read_table_columns(&path, extension, &names)?;
            table.require_columns(&names)?;

            let mut hash = Fnv::default();
            hash.field(file_name.as_bytes());
            hash.u64(table.n_rows() as u64);

            for name in &names {
                hash.field(name.as_bytes());
                // Numeric columns are aggregated as they stand; a categorical
                // one is one-hot encoded first. Either way it is the values
                // that decide the feature table.
                if table.is_numeric_column(name)? {
                    for value in table.f64_column(name)? {
                        hash.f64(value);
                    }
                } else {
                    for value in table.opt_string_column(name)? {
                        // An empty cell and the string "" are different inputs
                        // to the one-hot encoding, so they hash differently.
                        match value {
                            Some(text) => hash.field(text.as_bytes()),
                            None => hash.field(b"\x00none"),
                        }
                    }
                }
            }

            // And the graph the values will be aggregated over. By stamp
            // rather than by content — see the module note.
            hash.field(b"edges");
            let edges = net_dir.join(id.edges_file_name(
                patient_column,
                sample_column,
                extension.as_str(),
            ));
            match std::fs::metadata(&edges) {
                Ok(metadata) => {
                    hash.u64(metadata.len());
                    let stamp = metadata
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|since| since.as_nanos() as u64)
                        .unwrap_or_default();
                    hash.u64(stamp);
                }
                // An absent edges file is not this function's error to raise —
                // the aggregation will say so, and far more clearly. It is
                // still part of the cohort's state, so it is hashed as absent.
                Err(_) => hash.field(b"\x00absent"),
            }

            Ok((hash.finish(), table.n_rows()))
        })
        .collect::<Result<Vec<(u64, usize)>>>()?;

    let mut combined = Fnv::default();
    combined.u64(per_sample.len() as u64);
    for (digest, _) in &per_sample {
        combined.u64(*digest);
    }
    Ok(CohortDigest {
        fingerprint: combined.hex(),
        rows: per_sample.into_iter().map(|(_, rows)| rows).collect(),
    })
}

/// Refuse a network directory step 3 cannot write its labels into.
///
/// # Why this is a refusal and not a conversion
///
/// The niche labels are written back into the nodes files, and
/// [`mosna_core::niches::merge_niche_pheno`] writes parquet whatever it read:
/// the network directory the pipelines produce is parquet, and every later step
/// reads it as such. Pointed at a directory of CSVs — which `Network
/// directory` and `Extension` together allow — it read `nodes_patient-1.csv`
/// and overwrote that same file with parquet bytes. The file kept its name and
/// stopped being a CSV, and if the directory was the user's own data rather
/// than a copy, the data was gone.
///
/// Converting the cohort instead would mean rewriting every file of a directory
/// the user did not ask us to touch. Refusing says what is wrong while
/// everything is still intact, and is checked before any work is done.
pub fn require_writable_network(extension: Extension, net_dir: &Path) -> Result<()> {
    if extension == Extension::Parquet {
        return Ok(());
    }
    Err(PipelineError::invalid(format!(
        "the niche analysis writes its labels back into the nodes files of {}, \
         and can only write parquet — this directory is configured as `{}`. \
         Use `Network directory: Default`, or convert the directory to parquet first",
        net_dir.display(),
        extension.as_str(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosna_io::Table;

    /// Two samples, whose nodes files carry `Cluster` and whose edges files
    /// exist, so a digest can be taken of them.
    fn cohort(dir: &Path, clusters: &[&[&str]]) -> Vec<SampleId> {
        let mut index = Vec::new();
        for (position, labels) in clusters.iter().enumerate() {
            let patient = (position + 1).to_string();
            let id = SampleId::patient_only(&patient);

            let nodes = Table::from_columns(vec![
                ("Cluster".into(), Table::string_array(labels.iter())),
                (
                    "X".into(),
                    Table::f64_array((0..labels.len()).map(|i| i as f64)),
                ),
            ])
            .unwrap();
            mosna_io::write::write_parquet::write_parquet(
                &nodes,
                dir.join(id.nodes_file_name("patient", None, "parquet")),
            )
            .unwrap();

            let edges = Table::from_edges(&[(0, 1)]).unwrap();
            mosna_io::write::write_parquet::write_parquet(
                &edges,
                dir.join(id.edges_file_name("patient", None, "parquet")),
            )
            .unwrap();

            index.push(id);
        }
        index
    }

    fn digest(dir: &Path, index: &[SampleId], columns: &[&str]) -> String {
        full(dir, index, columns).fingerprint
    }

    fn full(dir: &Path, index: &[SampleId], columns: &[&str]) -> CohortDigest {
        verify_and_digest(
            dir,
            Extension::Parquet,
            index,
            "patient",
            None,
            &columns.iter().map(|c| c.to_string()).collect::<Vec<_>>(),
        )
        .unwrap()
    }

    /// The same pass says how tall each sample is, so nothing has to read the
    /// footers again to find out.
    #[test]
    fn the_digest_reports_the_height_of_every_sample() {
        let dir = tempfile::tempdir().unwrap();
        let index = cohort(dir.path(), &[&["A", "B"], &["A", "B", "C"]]);
        let digest = full(dir.path(), &index, &["Cluster"]);

        assert_eq!(digest.rows, vec![2, 3]);
        assert_eq!(digest.total_rows(), 5);
    }

    #[test]
    fn the_same_cohort_hashes_the_same_twice() {
        let dir = tempfile::tempdir().unwrap();
        let index = cohort(dir.path(), &[&["A", "B"], &["B", "A"]]);
        assert_eq!(
            digest(dir.path(), &index, &["Cluster"]),
            digest(dir.path(), &index, &["Cluster"])
        );
    }

    /// The point of hashing the values rather than the row count: a cohort can
    /// be relabelled without changing shape, and the features change with it.
    #[test]
    fn a_relabelled_cohort_hashes_differently() {
        let dir = tempfile::tempdir().unwrap();
        let index = cohort(dir.path(), &[&["A", "B"]]);
        let before = digest(dir.path(), &index, &["Cluster"]);

        cohort(dir.path(), &[&["A", "C"]]);
        assert_ne!(before, digest(dir.path(), &index, &["Cluster"]));
    }

    /// Two samples whose values are swapped between them are not the same
    /// cohort: the aggregation stacks them in `data_index` order.
    #[test]
    fn the_order_of_the_samples_is_part_of_the_cohort() {
        let dir = tempfile::tempdir().unwrap();
        let index = cohort(dir.path(), &[&["A", "A"], &["B", "B"]]);
        let straight = digest(dir.path(), &index, &["Cluster"]);

        let reversed: Vec<SampleId> = index.iter().rev().cloned().collect();
        assert_ne!(straight, digest(dir.path(), &reversed, &["Cluster"]));
    }

    /// A cohort that gained a sample is a different cohort, even though every
    /// sample it already had is untouched.
    #[test]
    fn a_cohort_that_grew_hashes_differently() {
        let dir = tempfile::tempdir().unwrap();
        let one = cohort(dir.path(), &[&["A", "B"]]);
        let before = digest(dir.path(), &one, &["Cluster"]);

        let two = cohort(dir.path(), &[&["A", "B"], &["C", "D"]]);
        assert_ne!(before, digest(dir.path(), &two, &["Cluster"]));
    }

    /// The whole reason the edges are in the digest: the same cells, joined up
    /// differently, aggregate differently.
    #[test]
    fn a_rewritten_edge_list_hashes_differently() {
        let dir = tempfile::tempdir().unwrap();
        let index = cohort(dir.path(), &[&["A", "B", "C"]]);
        let before = digest(dir.path(), &index, &["Cluster"]);

        let edges = Table::from_edges(&[(0, 1), (1, 2), (0, 2)]).unwrap();
        mosna_io::write::write_parquet::write_parquet(
            &edges,
            dir.path().join(index[0].edges_file_name("patient", None, "parquet")),
        )
        .unwrap();

        assert_ne!(before, digest(dir.path(), &index, &["Cluster"]));
    }

    /// Only the columns being aggregated are hashed, which is what lets the
    /// niche labels be written back into the same files: a new column is not a
    /// new cohort.
    #[test]
    fn a_column_nothing_aggregates_is_not_part_of_the_cohort() {
        let dir = tempfile::tempdir().unwrap();
        let index = cohort(dir.path(), &[&["A", "B"]]);
        let before = digest(dir.path(), &index, &["Cluster"]);

        let path = dir.path().join(index[0].nodes_file_name("patient", None, "parquet"));
        let mut table = mosna_io::read::get_opener::read_table(&path, Extension::Parquet).unwrap();
        table
            .set_column("niches_1-1-1", Table::u32_array([0u32, 1]))
            .unwrap();
        mosna_io::write::write_parquet::write_parquet(&table, &path).unwrap();

        assert_eq!(
            before,
            digest(dir.path(), &index, &["Cluster"]),
            "writing a label column changed the cohort's identity"
        );
    }

    /// Aggregating a different column is a different aggregation, so the digest
    /// covers which columns were read as well as what was in them.
    #[test]
    fn hashing_a_different_column_gives_a_different_digest() {
        let dir = tempfile::tempdir().unwrap();
        let index = cohort(dir.path(), &[&["A", "B"]]);
        assert_ne!(
            digest(dir.path(), &index, &["Cluster"]),
            digest(dir.path(), &index, &["X"])
        );
    }

    /// The check half of the pass: a column nothing has is still reported, and
    /// still names the file.
    #[test]
    fn a_missing_column_is_reported_rather_than_hashed() {
        let dir = tempfile::tempdir().unwrap();
        let index = cohort(dir.path(), &[&["A", "B"]]);
        let error = verify_and_digest(
            dir.path(),
            Extension::Parquet,
            &index,
            "patient",
            None,
            &["Absent".to_string()],
        )
        .unwrap_err();
        assert!(error.to_string().contains("Absent"), "{error}");
    }

    // -----------------------------------------------------------------------
    // The network directory has to be writable
    // -----------------------------------------------------------------------

    #[test]
    fn a_parquet_network_directory_is_accepted() {
        require_writable_network(Extension::Parquet, Path::new("/net")).unwrap();
    }

    /// And a delimited one is refused, with a message that says what to do
    /// rather than only what is wrong.
    #[test]
    fn a_delimited_network_directory_is_refused_by_name() {
        for extension in [Extension::Csv, Extension::Tsv] {
            let error =
                require_writable_network(extension, Path::new("/net")).unwrap_err();
            let message = error.to_string();
            assert!(message.contains("/net"), "{message}");
            assert!(message.contains(extension.as_str()), "{message}");
            assert!(message.contains("parquet"), "{message}");
        }
    }
}
