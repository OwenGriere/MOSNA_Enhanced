//! Parquet writing, the equivalent of `DataFrame.to_parquet`.

use std::fs::File;
use std::path::Path;

use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::error::{IoError, Result};
use crate::table::Table;

/// Write `table` to `path`, creating parent directories as needed.
///
/// Snappy compression matches the default of pyarrow, which is what
/// `to_parquet` uses, so files stay the same size and remain readable by the
/// Python implementation without any conversion step.
pub fn write_parquet(table: &Table, path: impl AsRef<Path>) -> Result<()> {
    write_parquet_with_metadata(table, path, &[])
}

/// The same, but never leaving a half-written file where the old one was.
///
/// # Why this exists
///
/// A parquet file is written from front to back and is unreadable until its
/// footer lands, so anything that reads it while it is being written gets
/// `ParquetError("External: end of file")`. That is not hypothetical for the
/// nodes files: step 3 writes its niche labels back into them, and a second
/// analysis — or the interface's network view — may be reading the very same
/// file at that moment.
///
/// Writing beside the target and renaming over it closes the window. `rename`
/// is atomic within a filesystem, so a reader sees either the whole previous
/// file or the whole new one, never the seam between them.
///
/// This does not make concurrent *writers* safe: two runs that both read a file
/// and both rename their own version over it still lose one of the two changes.
/// That is what the caller's lock is for.
pub fn write_parquet_atomic(table: &Table, path: impl AsRef<Path>) -> Result<()> {
    write_parquet_atomic_with_metadata(table, path, &[])
}

/// The same, with key/value pairs recorded in the file's footer.
///
/// This is how the niche caches are written. They are shared: every run that
/// only re-clusters reads the aggregation and the projection of every other, so
/// one run can be writing the file another is reading — and a parquet is
/// unreadable until its footer lands.
pub fn write_parquet_atomic_with_metadata(
    table: &Table,
    path: impl AsRef<Path>,
    metadata: &[(String, String)],
) -> Result<()> {
    let path = path.as_ref();
    // Beside the target, so the rename stays within one filesystem — a
    // temporary directory elsewhere would turn it into a copy, which is not
    // atomic. The process id keeps two writers from sharing a scratch file.
    // The process id and a counter keep two writers — in one process or in
    // several — from sharing a scratch file and truncating each other's.
    static SCRATCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let ticket = SCRATCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch = path.with_extension(format!(
        "parquet.{}-{ticket}.tmp",
        std::process::id()
    ));

    write_parquet_with_metadata(table, &scratch, metadata)?;
    match std::fs::rename(&scratch, path) {
        Ok(()) => Ok(()),
        Err(source) => {
            let _ = std::fs::remove_file(&scratch);
            Err(IoError::Write {
                path: path.to_path_buf(),
                source,
            })
        }
    }
}

/// The same, with key/value pairs recorded in the file's footer.
///
/// # What this is for
///
/// A cached result is only reusable if it came from the inputs the reader is
/// about to use it for, and a file name cannot always say so: a name short
/// enough to read is a name that leaves something out. The footer can carry
/// what the name does not, and reading it back costs a seek — the reader
/// already parses the footer to find the row groups.
///
/// Arrow keeps its own entry in this map; the pairs given here are added
/// alongside it and do not disturb any other reader, which is the point of
/// putting them there rather than in a file of their own.
pub fn write_parquet_with_metadata(
    table: &Table,
    path: impl AsRef<Path>,
    metadata: &[(String, String)],
) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| IoError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }

    let batch = table.to_record_batch()?;
    let file = File::create(path).map_err(|source| IoError::Write {
        path: path.to_path_buf(),
        source,
    })?;

    let mut builder = WriterProperties::builder().set_compression(Compression::SNAPPY);
    if !metadata.is_empty() {
        builder = builder.set_key_value_metadata(Some(
            metadata
                .iter()
                .map(|(key, value)| parquet::file::metadata::KeyValue {
                    key: key.clone(),
                    value: Some(value.clone()),
                })
                .collect(),
        ));
    }
    let props = builder.build();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props)).map_err(|source| {
        IoError::Parquet {
            path: path.to_path_buf(),
            source,
        }
    })?;
    writer.write(&batch).map_err(|source| IoError::Parquet {
        path: path.to_path_buf(),
        source,
    })?;
    writer.close().map_err(|source| IoError::Parquet {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}
