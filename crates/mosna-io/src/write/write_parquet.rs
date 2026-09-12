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
