//! Parquet reading, the equivalent of `pandas.read_parquet`.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow_schema::Schema;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ProjectionMask;

use crate::error::{IoError, Result};
use crate::table::Table;

/// Read every column of a parquet file.
pub fn read_parquet(path: impl AsRef<Path>) -> Result<Table> {
    let path = path.as_ref();
    let builder = open(path)?;
    let schema = builder.schema().clone();
    collect(builder, schema, path)
}

/// Read only `columns`, pushing the projection into the parquet reader.
///
/// Missing columns are reported rather than silently skipped, so a typo in the
/// configuration surfaces here instead of as an empty result downstream.
pub fn read_parquet_columns(path: impl AsRef<Path>, columns: &[&str]) -> Result<Table> {
    let path = path.as_ref();
    let builder = open(path)?;
    let file_schema = builder.schema().clone();

    let mut indices = Vec::with_capacity(columns.len());
    for name in columns {
        let (idx, _) =
            file_schema
                .column_with_name(name)
                .ok_or_else(|| IoError::MissingColumn {
                    path: path.to_path_buf(),
                    column: (*name).to_string(),
                })?;
        indices.push(idx);
    }

    let mask = ProjectionMask::roots(builder.parquet_schema(), indices.iter().copied());
    let projected = Arc::new(file_schema.project(&indices)?);
    let builder = builder.with_projection(mask);
    collect(builder, projected, path)
}

fn open(path: &Path) -> Result<ParquetRecordBatchReaderBuilder<File>> {
    let file = File::open(path).map_err(|source| IoError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    ParquetRecordBatchReaderBuilder::try_new(file).map_err(|source| IoError::Parquet {
        path: path.to_path_buf(),
        source,
    })
}

fn collect(
    builder: ParquetRecordBatchReaderBuilder<File>,
    schema: Arc<Schema>,
    path: &Path,
) -> Result<Table> {
    let reader = builder.build().map_err(|source| IoError::Parquet {
        path: path.to_path_buf(),
        source,
    })?;
    let mut batches = Vec::new();
    for batch in reader {
        batches.push(batch.map_err(IoError::Arrow)?);
    }
    Ok(Table::from_batches(schema, &batches)?.with_origin(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write::write_parquet::write_parquet;

    #[test]
    fn round_trips_a_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodes.parquet");

        let table = Table::from_columns(vec![
            ("X_position".into(), Table::f64_array([1.0, 2.5, 4.0])),
            ("Y_position".into(), Table::f64_array([0.0, 1.5, 3.0])),
            ("Cluster".into(), Table::string_array(["A", "B", "A"])),
        ])
        .unwrap();
        write_parquet(&table, &path).unwrap();

        let loaded = read_parquet(&path).unwrap();
        assert_eq!(loaded.n_rows(), 3);
        assert_eq!(
            loaded.column_names(),
            vec!["X_position", "Y_position", "Cluster"]
        );
        assert_eq!(
            loaded.f64_column("X_position").unwrap(),
            vec![1.0, 2.5, 4.0]
        );
        assert_eq!(
            loaded.string_column("Cluster").unwrap(),
            vec!["A", "B", "A"]
        );
    }

    #[test]
    fn projection_reads_a_single_column() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodes.parquet");
        let table = Table::from_columns(vec![
            ("x".into(), Table::f64_array([1.0, 2.0])),
            ("Cluster".into(), Table::string_array(["A", "B"])),
        ])
        .unwrap();
        write_parquet(&table, &path).unwrap();

        let projected = read_parquet_columns(&path, &["Cluster"]).unwrap();
        assert_eq!(projected.column_names(), vec!["Cluster"]);
        assert_eq!(projected.n_rows(), 2);
    }

    #[test]
    fn projecting_a_missing_column_names_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodes.parquet");
        let table = Table::from_columns(vec![("x".into(), Table::f64_array([1.0]))]).unwrap();
        write_parquet(&table, &path).unwrap();

        let err = read_parquet_columns(&path, &["Cluster"]).unwrap_err();
        assert!(err.to_string().contains("Cluster"));
    }
}
