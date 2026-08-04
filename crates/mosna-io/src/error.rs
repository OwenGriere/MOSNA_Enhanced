//! Error type of the I/O layer.

use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, IoError>;

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("parquet error on {path}: {source}")]
    Parquet {
        path: PathBuf,
        #[source]
        source: parquet::errors::ParquetError,
    },

    #[error("arrow error: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),

    #[error("csv error on {path}: {source}")]
    Csv {
        path: PathBuf,
        #[source]
        source: csv::Error,
    },

    #[error("unsupported file extension `{0}` (expected csv, tsv or parquet)")]
    UnsupportedExtension(String),

    /// A column named in the configuration is absent from the table.
    ///
    /// Carries the same information as the Python
    /// `assert col in df.columns, f"{col} are not in {df}"`.
    #[error("column `{column}` is not present in {path}")]
    MissingColumn { path: PathBuf, column: String },

    #[error("column `{column}` in {path} has type {found}, which cannot be read as {expected}")]
    ColumnType {
        path: PathBuf,
        column: String,
        expected: &'static str,
        found: String,
    },

    #[error("{0}")]
    Invalid(String),
}

impl IoError {
    pub fn invalid(msg: impl Into<String>) -> Self {
        IoError::Invalid(msg.into())
    }
}
