//! Port of `package/utils/read_extension.py::get_opener`.

use std::path::Path;

use crate::error::{IoError, Result};
use crate::read::{read_csv, read_parquet};
use crate::table::Table;

/// The three table formats the application supports.
///
/// Mirrors the `if extension == "csv" / "parquet" / "tsv"` dispatch of
/// `get_opener`, with the difference that an unknown extension is reported as
/// an error instead of returning `None` and failing later with
/// `TypeError: 'NoneType' object is not callable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extension {
    Csv,
    Tsv,
    Parquet,
}

impl Extension {
    /// Parse an extension string as written in the configuration.
    pub fn parse(extension: &str) -> Result<Self> {
        match extension.trim().trim_start_matches('.') {
            "csv" => Ok(Extension::Csv),
            "tsv" => Ok(Extension::Tsv),
            "parquet" => Ok(Extension::Parquet),
            other => Err(IoError::UnsupportedExtension(other.to_string())),
        }
    }

    /// The extension as it appears in file names, without a dot.
    pub fn as_str(self) -> &'static str {
        match self {
            Extension::Csv => "csv",
            Extension::Tsv => "tsv",
            Extension::Parquet => "parquet",
        }
    }

    /// Field separator for the delimited formats.
    fn delimiter(self) -> u8 {
        match self {
            Extension::Tsv => b'\t',
            _ => b',',
        }
    }
}

/// Read a whole table, dispatching on the configured extension.
pub fn read_table(path: impl AsRef<Path>, extension: Extension) -> Result<Table> {
    match extension {
        Extension::Parquet => read_parquet::read_parquet(path),
        Extension::Csv | Extension::Tsv => read_csv::read_delimited(path, extension.delimiter()),
    }
}

/// Read only the named columns.
///
/// For parquet this pushes the projection into the reader, so unrelated columns
/// are never decoded — the equivalent of `pd.read_parquet(path, columns=[...])`
/// used by `find_all_pheno` and `merge_niche_pheno`. Delimited formats have to
/// be parsed in full either way.
pub fn read_table_columns(
    path: impl AsRef<Path>,
    extension: Extension,
    columns: &[&str],
) -> Result<Table> {
    match extension {
        Extension::Parquet => read_parquet::read_parquet_columns(path, columns),
        Extension::Csv | Extension::Tsv => read_csv::read_delimited(path, extension.delimiter()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions_parse() {
        assert_eq!(Extension::parse("csv").unwrap(), Extension::Csv);
        assert_eq!(Extension::parse("parquet").unwrap(), Extension::Parquet);
        assert_eq!(Extension::parse(".tsv").unwrap(), Extension::Tsv);
    }

    #[test]
    fn unknown_extension_is_an_error_not_a_silent_none() {
        let err = Extension::parse("xlsx").unwrap_err();
        assert!(err.to_string().contains("xlsx"));
    }
}
