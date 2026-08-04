//! Delimited-text reading, the equivalent of `pandas.read_csv`.
//!
//! Column types are inferred the way pandas does: a column whose every
//! non-empty cell parses as an integer becomes `Int64`, otherwise as a float it
//! becomes `Float64`, otherwise it stays as text. Empty cells become nulls,
//! which read back as `NaN` through [`crate::table::Table::f64_column`] — the
//! same value `pd.read_csv` would produce.

use std::path::Path;
use std::sync::Arc;

use arrow_array::{ArrayRef, Float64Array, Int64Array, StringArray};

use crate::error::{IoError, Result};
use crate::table::Table;

/// Read a delimited file with the given field separator.
pub fn read_delimited(path: impl AsRef<Path>, delimiter: u8) -> Result<Table> {
    let path = path.as_ref();
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(false)
        .from_path(path)
        .map_err(|source| IoError::Csv {
            path: path.to_path_buf(),
            source,
        })?;

    let headers: Vec<String> = reader
        .headers()
        .map_err(|source| IoError::Csv {
            path: path.to_path_buf(),
            source,
        })?
        .iter()
        .map(str::to_string)
        .collect();

    let n_cols = headers.len();
    let mut cells: Vec<Vec<String>> = vec![Vec::new(); n_cols];
    for record in reader.records() {
        let record = record.map_err(|source| IoError::Csv {
            path: path.to_path_buf(),
            source,
        })?;
        for (idx, cell) in cells.iter_mut().enumerate() {
            cell.push(record.get(idx).unwrap_or("").to_string());
        }
    }

    let pairs: Vec<(String, ArrayRef)> = headers
        .into_iter()
        .zip(cells)
        .map(|(name, values)| (name, infer_column(&values)))
        .collect();

    Ok(Table::from_columns(pairs)?.with_origin(path))
}

/// Choose the narrowest Arrow type that holds every value in the column.
fn infer_column(values: &[String]) -> ArrayRef {
    let non_empty = || values.iter().filter(|v| !is_null_token(v));

    if non_empty().count() > 0 && non_empty().all(|v| v.trim().parse::<i64>().is_ok()) {
        let array: Int64Array = values
            .iter()
            .map(|v| {
                if is_null_token(v) {
                    None
                } else {
                    v.trim().parse::<i64>().ok()
                }
            })
            .collect();
        return Arc::new(array);
    }

    if non_empty().count() > 0 && non_empty().all(|v| v.trim().parse::<f64>().is_ok()) {
        let array: Float64Array = values
            .iter()
            .map(|v| {
                if is_null_token(v) {
                    None
                } else {
                    v.trim().parse::<f64>().ok()
                }
            })
            .collect();
        return Arc::new(array);
    }

    let array: StringArray = values
        .iter()
        .map(|v| {
            if is_null_token(v) {
                None
            } else {
                Some(v.as_str())
            }
        })
        .collect();
    Arc::new(array)
}

/// Tokens pandas treats as missing values in a delimited file.
fn is_null_token(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.is_empty()
        || matches!(
            trimmed,
            "NA" | "N/A" | "n/a" | "NaN" | "nan" | "NULL" | "null" | "None" | "#N/A"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(contents: &str, name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        (dir, path)
    }

    #[test]
    fn infers_integer_float_and_text_columns() {
        let (_dir, path) = write_temp("id,x,Cluster\n1,1.5,A\n2,2.5,B\n3,3.0,A\n", "nodes.csv");
        let table = read_delimited(&path, b',').unwrap();
        assert_eq!(table.n_rows(), 3);
        assert_eq!(table.f64_column("id").unwrap(), vec![1.0, 2.0, 3.0]);
        assert_eq!(table.f64_column("x").unwrap(), vec![1.5, 2.5, 3.0]);
        assert_eq!(table.string_column("Cluster").unwrap(), vec!["A", "B", "A"]);
    }

    #[test]
    fn reads_tab_separated_files() {
        let (_dir, path) = write_temp("a\tb\n1\t2\n", "nodes.tsv");
        let table = read_delimited(&path, b'\t').unwrap();
        assert_eq!(table.column_names(), vec!["a", "b"]);
        assert_eq!(table.f64_column("a").unwrap(), vec![1.0]);
    }

    #[test]
    fn missing_cells_become_nan() {
        let (_dir, path) = write_temp("x,y\n1.0,10.0\n,20.0\n3.0,30.0\n", "nodes.csv");
        let table = read_delimited(&path, b',').unwrap();
        let values = table.f64_column("x").unwrap();
        assert_eq!(values.len(), 3);
        assert_eq!(values[0], 1.0);
        assert!(values[1].is_nan());
        assert_eq!(values[2], 3.0);
    }

    #[test]
    fn blank_lines_are_skipped_like_pandas() {
        // `pd.read_csv` defaults to `skip_blank_lines=True`, so a wholly empty
        // line is not a row of nulls — it is not a row at all.
        let (_dir, path) = write_temp("x\n1.0\n\n3.0\n", "nodes.csv");
        let table = read_delimited(&path, b',').unwrap();
        assert_eq!(table.f64_column("x").unwrap(), vec![1.0, 3.0]);
    }

    #[test]
    fn a_column_of_only_nulls_stays_textual() {
        let (_dir, path) = write_temp("p\n\n\n", "nodes.csv");
        let table = read_delimited(&path, b',').unwrap();
        assert!(table.dropna_string_column("p").unwrap().is_empty());
    }
}
