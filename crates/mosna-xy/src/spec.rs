//! What Rust hands the Python renderer.
//!
//! A specification is *everything* the figure needs and nothing the renderer
//! has to work out for itself: the values, the labels, the colours, the title,
//! the file it is written to. The scientific decisions — which colour map, how
//! a z-score is normalised, in what order the leaves of a dendrogram fall —
//! stay in Rust, where they are already pinned by tests. Python composes an
//! `xy` chart out of what it is given and exports it.
//!
//! # Why a file and not a pipe
//!
//! A queued specification is an artefact that survives the process that wrote
//! it: a figure that came out wrong can be re-rendered from the exact input
//! that produced it, without re-running the analysis that took an hour. The
//! queue is deleted once the renderer has succeeded.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Map, Value};

/// A large array, written beside its specification rather than inside it.
///
/// A hundred thousand cells' coordinates are two hundred thousand doubles.
/// Spelled out as JSON text that is some four megabytes to write and to parse,
/// against 1.6 to read with `numpy.fromfile`. Small arrays stay inline, where
/// they are readable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

/// One figure, ready to be drawn.
#[derive(Debug, Clone)]
pub struct Spec {
    kind: String,
    stem: String,
    save_dir: PathBuf,
    body: Map<String, Value>,
    blobs: Vec<Blob>,
}

impl Spec {
    /// A specification of `kind`, to be written as `{stem}.png` and
    /// `{stem}.html` inside `save_dir`.
    pub fn new(kind: &str, stem: impl Into<String>, save_dir: &Path) -> Self {
        Self {
            kind: kind.to_string(),
            stem: stem.into(),
            save_dir: save_dir.to_path_buf(),
            body: Map::new(),
            blobs: Vec::new(),
        }
    }

    /// The kind of figure, which is what the renderer dispatches on.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The file stem the outputs are named after.
    pub fn stem(&self) -> &str {
        &self.stem
    }

    /// Add a field to the body.
    pub fn set(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.body.insert(key.to_string(), value.into());
        self
    }

    /// Add an array of doubles as a binary blob, referenced by name.
    ///
    /// `shape` is the array's shape as `numpy` will read it, so a list of
    /// points is `[n, 2]` and a flat series is `[n]`.
    pub fn set_f64_blob(self, key: &str, values: &[f64], shape: &[usize]) -> Self {
        let bytes = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.set_blob(key, bytes, "f64", shape)
    }

    /// Add an array of unsigned 32-bit integers as a binary blob.
    pub fn set_u32_blob(self, key: &str, values: &[u32], shape: &[usize]) -> Self {
        let bytes = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.set_blob(key, bytes, "u32", shape)
    }

    fn set_blob(mut self, key: &str, bytes: Vec<u8>, dtype: &str, shape: &[usize]) -> Self {
        let file_name = format!("{key}.bin");
        self.body.insert(
            key.to_string(),
            serde_json::json!({
                "__blob__": file_name,
                "dtype": dtype,
                "shape": shape,
            }),
        );
        self.blobs.push(Blob { file_name, bytes });
        self
    }

    /// The values of a double blob, for the tests that pin what was written.
    ///
    /// The blobs are the part of a specification a reader cannot check by
    /// eye, which is exactly the part worth asserting on.
    pub fn blob_values(&self, key: &str) -> Vec<f64> {
        let name = format!("{key}.bin");
        self.blobs
            .iter()
            .find(|blob| blob.file_name == name)
            .map(|blob| {
                blob.bytes
                    .chunks_exact(8)
                    .map(|chunk| f64::from_le_bytes(chunk.try_into().unwrap()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The document the renderer parses.
    ///
    /// The blob file names inside it are relative to the specification's own
    /// directory, so a queue can be moved wholesale without rewriting it.
    pub fn to_json(&self) -> Value {
        let mut document = self.body.clone();
        document.insert("kind".into(), Value::String(self.kind.clone()));
        document.insert("stem".into(), Value::String(self.stem.clone()));
        document.insert(
            "save_dir".into(),
            Value::String(self.save_dir.display().to_string()),
        );
        Value::Object(document)
    }
}

/// Where queued specifications live under a working directory.
///
/// A dot-prefixed name, and one the interface's image scan never descends
/// into: a queue that showed up in the gallery as a folder of nothing would be
/// a bug report.
pub const QUEUE_DIRECTORY: &str = ".mosna-figures";

/// The specifications waiting to be drawn.
///
/// Thread-safe: the analyses draw from inside a `rayon` loop, and two samples
/// finishing at once must not be given the same sequence number — that would
/// silently drop one of the two figures.
#[derive(Debug)]
pub struct Queue {
    directory: PathBuf,
    next: AtomicUsize,
}

impl Queue {
    /// A queue under `working_dir`, created on first use.
    pub fn new(working_dir: &Path) -> Self {
        Self {
            directory: working_dir.join(QUEUE_DIRECTORY),
            next: AtomicUsize::new(0),
        }
    }

    /// Where the specifications are written.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// How many specifications have been queued.
    pub fn len(&self) -> usize {
        self.next.load(Ordering::SeqCst)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Write one specification, and its blobs, into the queue.
    ///
    /// Returns the path of the document written.
    pub fn push(&self, spec: Spec) -> anyhow::Result<PathBuf> {
        let sequence = self.next.fetch_add(1, Ordering::SeqCst);
        let folder = self.directory.join(format!("{sequence:05}-{}", spec.kind));
        std::fs::create_dir_all(&folder)
            .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", folder.display()))?;

        for blob in &spec.blobs {
            let path = folder.join(&blob.file_name);
            std::fs::write(&path, &blob.bytes)
                .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display()))?;
        }

        let path = folder.join("figure.json");
        let document = serde_json::to_vec_pretty(&spec.to_json())?;
        std::fs::write(&path, document)
            .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display()))?;
        Ok(path)
    }

    /// Remove the queue once its figures have been drawn.
    ///
    /// Failure to clean up is not a failure of the analysis: the figures are
    /// on disk, and a leftover queue costs disk space and nothing else. It is
    /// reported by the caller, not raised.
    pub fn discard(&self) -> std::io::Result<()> {
        match std::fs::remove_dir_all(&self.directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_specification_carries_its_kind_its_stem_and_its_directory() {
        let spec = Spec::new("embedding", "cluster_labels", Path::new("/runs/niches"));
        let json = spec.to_json();
        assert_eq!(json["kind"], "embedding");
        assert_eq!(json["stem"], "cluster_labels");
        assert_eq!(json["save_dir"], "/runs/niches");
    }

    #[test]
    fn fields_reach_the_document_unchanged() {
        let spec = Spec::new("histogram", "h", Path::new("/tmp"))
            .set("title", "Niches")
            .set("counts", serde_json::json!([1, 2, 3]));
        let json = spec.to_json();
        assert_eq!(json["title"], "Niches");
        assert_eq!(json["counts"], serde_json::json!([1, 2, 3]));
    }

    /// The bytes have to be little-endian doubles in row-major order, because
    /// that is what `numpy.fromfile` reads back without being told anything.
    #[test]
    fn a_double_blob_is_little_endian_and_row_major() {
        let spec = Spec::new("network", "net_1", Path::new("/tmp")).set_f64_blob(
            "coords",
            &[1.0, 2.0, 3.0, 4.0],
            &[2, 2],
        );

        assert_eq!(spec.to_json()["coords"]["dtype"], "f64");
        assert_eq!(spec.to_json()["coords"]["shape"], serde_json::json!([2, 2]));
        assert_eq!(spec.to_json()["coords"]["__blob__"], "coords.bin");

        let blob = &spec.blobs[0];
        assert_eq!(blob.bytes.len(), 4 * 8);
        assert_eq!(&blob.bytes[..8], &1.0f64.to_le_bytes());
        assert_eq!(&blob.bytes[8..16], &2.0f64.to_le_bytes());
    }

    #[test]
    fn an_integer_blob_is_four_bytes_a_value() {
        let spec = Spec::new("network", "net_1", Path::new("/tmp")).set_u32_blob(
            "edges",
            &[0, 1],
            &[1, 2],
        );
        assert_eq!(spec.blobs[0].bytes.len(), 2 * 4);
        assert_eq!(spec.to_json()["edges"]["dtype"], "u32");
    }

    #[test]
    fn an_empty_array_is_still_a_blob_the_renderer_can_read() {
        let spec =
            Spec::new("network", "net_1", Path::new("/tmp")).set_f64_blob("coords", &[], &[0, 2]);
        assert_eq!(spec.blobs[0].bytes.len(), 0);
        assert_eq!(spec.to_json()["coords"]["shape"], serde_json::json!([0, 2]));
    }

    #[test]
    fn pushing_writes_the_document_and_its_blobs_together() {
        let dir = tempfile::tempdir().unwrap();
        let queue = Queue::new(dir.path());
        let spec =
            Spec::new("network", "net_1", dir.path()).set_f64_blob("coords", &[1.0, 2.0], &[1, 2]);

        let path = queue.push(spec).unwrap();
        assert!(path.is_file(), "{} was not written", path.display());
        assert!(path.parent().unwrap().join("coords.bin").is_file());

        let document: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(document["kind"], "network");
    }

    /// Two samples finishing at the same instant must not overwrite one
    /// another; the sequence number is what keeps them apart, and it is also
    /// what fixes the order the renderer works through.
    #[test]
    fn concurrent_pushes_each_get_their_own_folder() {
        let dir = tempfile::tempdir().unwrap();
        let queue = Queue::new(dir.path());

        let root = dir.path();
        std::thread::scope(|scope| {
            for index in 0..16 {
                let queue = &queue;
                scope.spawn(move || {
                    queue
                        .push(Spec::new("network", format!("net_{index}"), root))
                        .unwrap();
                });
            }
        });

        let folders: Vec<_> = std::fs::read_dir(queue.directory())
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(folders.len(), 16);
        assert_eq!(queue.len(), 16);
    }

    #[test]
    fn the_queue_is_named_so_the_gallery_never_shows_it() {
        let queue = Queue::new(Path::new("/runs"));
        assert_eq!(queue.directory(), Path::new("/runs/.mosna-figures"));
        assert!(QUEUE_DIRECTORY.starts_with('.'));
    }

    #[test]
    fn discarding_removes_the_queue_and_tolerates_its_absence() {
        let dir = tempfile::tempdir().unwrap();
        let queue = Queue::new(dir.path());
        queue.push(Spec::new("histogram", "h", dir.path())).unwrap();
        assert!(queue.directory().is_dir());

        queue.discard().unwrap();
        assert!(!queue.directory().exists());
        queue.discard().expect("discarding twice is not an error");
    }
}
