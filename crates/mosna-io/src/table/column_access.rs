//! Typed reads of a [`Table`] column.
//!
//! Parquet files in the wild store the same logical column in many physical
//! types — coordinates as `float`, `double` or `decimal`, phenotypes as `utf8`,
//! `large_utf8` or dictionary-encoded. Pandas hides that behind its own dtype
//! promotion; these accessors do the same by casting through `arrow_cast`
//! rather than requiring one exact type.

use arrow_array::cast::AsArray;
use arrow_array::{Array, Float64Array, StringArray};
use arrow_schema::DataType;

use crate::error::{IoError, Result};
use crate::table::Table;

impl Table {
    /// Read a column as `f64`, casting when needed.
    ///
    /// Nulls become `NaN`, matching how pandas surfaces a missing numeric cell.
    pub fn f64_column(&self, name: &str) -> Result<Vec<f64>> {
        let array = self.require_column(name)?;
        let casted =
            arrow_cast::cast(array, &DataType::Float64).map_err(|_| IoError::ColumnType {
                path: self.origin().to_path_buf(),
                column: name.to_string(),
                expected: "float",
                found: format!("{:?}", array.data_type()),
            })?;
        let values: &Float64Array = casted.as_primitive();
        Ok((0..values.len())
            .map(|i| {
                if values.is_null(i) {
                    f64::NAN
                } else {
                    values.value(i)
                }
            })
            .collect())
    }

    /// Read a column as `usize`, rejecting negative and non-integral values.
    ///
    /// Used for edge endpoints, where a bad value would index out of bounds.
    pub fn index_column(&self, name: &str) -> Result<Vec<usize>> {
        let values = self.f64_column(name)?;
        values
            .iter()
            .map(|&v| {
                if v.is_finite() && v >= 0.0 && v.fract() == 0.0 {
                    Ok(v as usize)
                } else {
                    Err(IoError::invalid(format!(
                        "column `{name}` in {} contains {v}, which is not a valid node index",
                        self.origin().display()
                    )))
                }
            })
            .collect()
    }

    /// Read a column as strings, casting when needed.
    ///
    /// Nulls become `None`. Numeric phenotype codes render the way Python's
    /// `str()` would (`3` for an integer, `3.0` for a float), so labels agree
    /// between the two implementations.
    pub fn opt_string_column(&self, name: &str) -> Result<Vec<Option<String>>> {
        let array = self.require_column(name)?;
        // Dictionary-encoded columns must be unpacked before the string cast.
        let flattened = match array.data_type() {
            DataType::Dictionary(_, _) => arrow_cast::cast(array, &DataType::Utf8)?,
            _ => array.clone(),
        };
        let casted =
            arrow_cast::cast(&flattened, &DataType::Utf8).map_err(|_| IoError::ColumnType {
                path: self.origin().to_path_buf(),
                column: name.to_string(),
                expected: "string",
                found: format!("{:?}", array.data_type()),
            })?;
        let values: &StringArray = casted.as_string();
        Ok((0..values.len())
            .map(|i| {
                if values.is_null(i) {
                    None
                } else {
                    Some(values.value(i).to_string())
                }
            })
            .collect())
    }

    /// Read a column as strings, replacing nulls with the empty string.
    pub fn string_column(&self, name: &str) -> Result<Vec<String>> {
        Ok(self
            .opt_string_column(name)?
            .into_iter()
            .map(Option::unwrap_or_default)
            .collect())
    }

    /// Read a column as strings, dropping nulls.
    ///
    /// Equivalent to `node[col].dropna().tolist()` in `generate_cmap`.
    pub fn dropna_string_column(&self, name: &str) -> Result<Vec<String>> {
        Ok(self
            .opt_string_column(name)?
            .into_iter()
            .flatten()
            .collect())
    }

    /// Read two coordinate columns as `(x, y)` pairs.
    pub fn coords(&self, x: &str, y: &str) -> Result<Vec<[f64; 2]>> {
        let xs = self.f64_column(x)?;
        let ys = self.f64_column(y)?;
        Ok(xs.into_iter().zip(ys).map(|(a, b)| [a, b]).collect())
    }

    /// Read the `source`/`target` columns of an edges table.
    ///
    /// The Python code asserts `sorted(edges.columns) == ["source", "target"]`;
    /// the same requirement is enforced here, with the same intent of catching
    /// a file that is not actually an edge list.
    pub fn edges(&self) -> Result<Vec<(u32, u32)>> {
        let mut names = self.column_names();
        names.sort_unstable();
        if names != ["source", "target"] {
            return Err(IoError::invalid(format!(
                "edges files must contain source and target columns only, {} has {:?}",
                self.origin().display(),
                self.column_names()
            )));
        }
        let source = self.index_column("source")?;
        let target = self.index_column("target")?;
        Ok(source
            .into_iter()
            .zip(target)
            .map(|(s, t)| (s as u32, t as u32))
            .collect())
    }

    /// Whether a column holds numbers rather than labels.
    ///
    /// The storage type answers this, not the values: a `Utf8` column casts to
    /// float without error and comes back as a column of `NaN`, so "did the
    /// cast succeed" answers a different question than it appears to. A
    /// dictionary of strings is labels however it is encoded.
    pub fn is_numeric_column(&self, name: &str) -> Result<bool> {
        let data_type = self.require_column(name)?.data_type();
        Ok(match data_type {
            DataType::Dictionary(_, value) => value.is_numeric(),
            other => other.is_numeric(),
        })
    }

    /// Borrow a column, or fail with the name of the file that lacks it.
    pub fn require_column(&self, name: &str) -> Result<&arrow_array::ArrayRef> {
        self.column(name).ok_or_else(|| IoError::MissingColumn {
            path: self.origin().to_path_buf(),
            column: name.to_string(),
        })
    }

    /// Check that every listed column is present.
    ///
    /// Ports `assert_file_for_tysserand` and `assert_net_niches`.
    pub fn require_columns(&self, names: &[&str]) -> Result<()> {
        for name in names {
            self.require_column(name)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn integer_columns_read_as_floats() {
        let table = Table::from_columns(vec![("x".into(), Table::i64_array([1, 2, 3]))]).unwrap();
        assert_eq!(table.f64_column("x").unwrap(), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn numeric_columns_read_as_strings_like_python() {
        let table = Table::from_columns(vec![
            ("i".into(), Table::i64_array([3])),
            ("f".into(), Table::f64_array([3.0])),
        ])
        .unwrap();
        assert_eq!(table.string_column("i").unwrap(), vec!["3"]);
        assert_eq!(table.string_column("f").unwrap(), vec!["3.0"]);
    }

    #[test]
    fn nulls_are_dropped_by_dropna() {
        let array: arrow_array::ArrayRef =
            Arc::new(StringArray::from(vec![Some("a"), None, Some("b")]));
        let table = Table::from_columns(vec![("p".into(), array)]).unwrap();
        assert_eq!(table.dropna_string_column("p").unwrap(), vec!["a", "b"]);
        assert_eq!(table.opt_string_column("p").unwrap().len(), 3);
    }

    /// The reason this exists: the cast cannot be used to tell the two apart.
    #[test]
    fn numbers_and_labels_are_told_apart_by_their_type() {
        let table = Table::from_columns(vec![
            ("CD8".into(), Table::f64_array([0.5])),
            ("count".into(), Table::i64_array([3])),
            ("phenotype".into(), Table::string_array(["cancer"])),
        ])
        .unwrap();

        assert!(table.is_numeric_column("CD8").unwrap());
        assert!(table.is_numeric_column("count").unwrap());
        assert!(!table.is_numeric_column("phenotype").unwrap());

        // The trap: casting a label column to float succeeds, and lies.
        assert!(table.f64_column("phenotype").unwrap()[0].is_nan());
    }

    #[test]
    fn a_dictionary_of_labels_is_not_numeric() {
        let values = StringArray::from(vec!["a", "b", "a"]);
        let keys = arrow_array::Int32Array::from(vec![0, 1, 0]);
        let dictionary: arrow_array::ArrayRef =
            Arc::new(arrow_array::DictionaryArray::try_new(keys, Arc::new(values)).unwrap());
        let table = Table::from_columns(vec![("phenotype".into(), dictionary)]).unwrap();

        assert!(!table.is_numeric_column("phenotype").unwrap());
        assert_eq!(table.string_column("phenotype").unwrap()[1], "b");
    }

    #[test]
    fn missing_column_names_the_file() {
        let table = Table::from_columns(vec![("x".into(), Table::f64_array([1.0]))])
            .unwrap()
            .with_origin("/data/nodes_patient-1.parquet");
        let err = table.require_column("Cluster").unwrap_err();
        assert!(err.to_string().contains("Cluster"));
        assert!(err.to_string().contains("nodes_patient-1.parquet"));
    }

    #[test]
    fn edges_require_exactly_source_and_target() {
        let good = Table::from_edges(&[(0, 1), (2, 3)]).unwrap();
        assert_eq!(good.edges().unwrap(), vec![(0, 1), (2, 3)]);

        let bad = Table::from_columns(vec![
            ("source".into(), Table::i64_array([0])),
            ("target".into(), Table::i64_array([1])),
            ("weight".into(), Table::f64_array([1.0])),
        ])
        .unwrap();
        assert!(bad.edges().is_err());
    }

    #[test]
    fn negative_edge_indices_are_rejected() {
        let table = Table::from_columns(vec![
            ("source".into(), Table::i64_array([-1])),
            ("target".into(), Table::i64_array([1])),
        ])
        .unwrap();
        assert!(table.edges().is_err());
    }
}
