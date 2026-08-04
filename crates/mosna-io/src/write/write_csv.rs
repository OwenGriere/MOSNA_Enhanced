//! CSV writing, the equivalent of `DataFrame.to_csv`.

use std::path::Path;

use crate::error::{IoError, Result};
use crate::table::Table;

/// Write `table` to `path` as CSV with a header row.
///
/// Formatting follows `to_csv`'s conventions so the file is byte-comparable
/// with one produced by pandas:
///
/// * a `NaN` is written as an empty field, not as the text `NaN`;
/// * `inf` and `-inf` keep those spellings;
/// * floats use the shortest representation that round-trips, which is what
///   `repr()` gives in Python 3 and what `{}` gives in Rust.
pub fn write_csv(table: &Table, path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| IoError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }

    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_path(path)
        .map_err(|source| IoError::Csv {
            path: path.to_path_buf(),
            source,
        })?;

    let names = table.column_names();
    writer.write_record(&names).map_err(|source| IoError::Csv {
        path: path.to_path_buf(),
        source,
    })?;

    // Render every column once, then emit row-wise: casting per cell would
    // re-dispatch on the Arrow type for each of the (rows x columns) values.
    let rendered: Vec<Vec<String>> = names
        .iter()
        .map(|name| render_column(table, name))
        .collect::<Result<_>>()?;

    let mut row = Vec::with_capacity(names.len());
    for r in 0..table.n_rows() {
        row.clear();
        for column in &rendered {
            row.push(column[r].as_str());
        }
        writer.write_record(&row).map_err(|source| IoError::Csv {
            path: path.to_path_buf(),
            source,
        })?;
    }

    writer.flush().map_err(|source| IoError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Render one column to the text form `to_csv` would produce.
fn render_column(table: &Table, name: &str) -> Result<Vec<String>> {
    use arrow_schema::DataType;

    let array = table.require_column(name)?;
    match array.data_type() {
        DataType::Float16 | DataType::Float32 | DataType::Float64 => Ok(table
            .f64_column(name)?
            .into_iter()
            .map(format_float)
            .collect()),
        _ => Ok(table
            .opt_string_column(name)?
            .into_iter()
            .map(Option::unwrap_or_default)
            .collect()),
    }
}

/// Format a float the way pandas writes it into a CSV cell.
fn format_float(value: f64) -> String {
    if value.is_nan() {
        String::new()
    } else if value.is_infinite() {
        if value.is_sign_negative() {
            "-inf".into()
        } else {
            "inf".into()
        }
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_becomes_an_empty_field() {
        assert_eq!(format_float(f64::NAN), "");
        assert_eq!(format_float(f64::INFINITY), "inf");
        assert_eq!(format_float(f64::NEG_INFINITY), "-inf");
        assert_eq!(format_float(1.5), "1.5");
        assert_eq!(format_float(2.0), "2");
    }

    #[test]
    fn writes_header_then_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("net_stat.csv");
        let table = Table::from_columns(vec![
            ("id".into(), Table::string_array(["patient-1_sample-2"])),
            ("assort".into(), Table::f64_array([0.25])),
        ])
        .unwrap();
        write_csv(&table, &path).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, "id,assort\npatient-1_sample-2,0.25\n");
    }

    #[test]
    fn round_trips_through_the_csv_reader() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.csv");
        let table = Table::from_columns(vec![
            ("a".into(), Table::f64_array([1.5, 2.5])),
            ("b".into(), Table::string_array(["x", "y"])),
        ])
        .unwrap();
        write_csv(&table, &path).unwrap();

        let loaded = crate::read::read_csv::read_delimited(&path, b',').unwrap();
        assert_eq!(loaded.f64_column("a").unwrap(), vec![1.5, 2.5]);
        assert_eq!(loaded.string_column("b").unwrap(), vec!["x", "y"]);
    }
}
