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

    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build();
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
