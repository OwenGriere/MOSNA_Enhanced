//! Building a [`Table`] from Arrow data or from plain Rust vectors.

use std::sync::Arc;

use arrow_array::{
    ArrayRef, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray, UInt32Array,
};
use arrow_schema::{ArrowError, DataType, Field, Schema};

use crate::table::Table;

impl Table {
    /// Wrap an Arrow schema and matching column arrays.
    pub fn new(schema: Arc<Schema>, columns: Vec<ArrayRef>) -> Result<Self, ArrowError> {
        let n_rows = columns.first().map(|c| c.len()).unwrap_or(0);
        if let Some(bad) = columns.iter().find(|c| c.len() != n_rows) {
            return Err(ArrowError::InvalidArgumentError(format!(
                "column length mismatch: expected {n_rows}, found {}",
                bad.len()
            )));
        }
        if schema.fields().len() != columns.len() {
            return Err(ArrowError::InvalidArgumentError(format!(
                "schema has {} fields but {} columns were given",
                schema.fields().len(),
                columns.len()
            )));
        }
        Ok(Self {
            schema,
            columns,
            n_rows,
            origin: Default::default(),
        })
    }

    /// Concatenate a sequence of record batches sharing one schema.
    pub fn from_batches(schema: Arc<Schema>, batches: &[RecordBatch]) -> Result<Self, ArrowError> {
        if batches.is_empty() {
            let columns = schema
                .fields()
                .iter()
                .map(|f| arrow_array::new_empty_array(f.data_type()))
                .collect();
            return Table::new(schema, columns);
        }
        if batches.len() == 1 {
            let batch = &batches[0];
            return Table::new(batch.schema(), batch.columns().to_vec());
        }
        let n_cols = schema.fields().len();
        let mut columns = Vec::with_capacity(n_cols);
        for idx in 0..n_cols {
            let slices: Vec<&dyn arrow_array::Array> =
                batches.iter().map(|b| b.column(idx).as_ref()).collect();
            columns.push(arrow_select::concat::concat(&slices)?);
        }
        Table::new(schema, columns)
    }

    /// Build a table from `(name, column)` pairs.
    pub fn from_columns(pairs: Vec<(String, ArrayRef)>) -> Result<Self, ArrowError> {
        let fields: Vec<Field> = pairs
            .iter()
            .map(|(name, array)| {
                Field::new(name, array.data_type().clone(), array.null_count() > 0)
            })
            .collect();
        let columns: Vec<ArrayRef> = pairs.into_iter().map(|(_, array)| array).collect();
        Table::new(Arc::new(Schema::new(fields)), columns)
    }

    /// Build a table of `f64` columns, the common case for computed results.
    pub fn from_f64_columns(pairs: Vec<(String, Vec<f64>)>) -> Result<Self, ArrowError> {
        let pairs = pairs
            .into_iter()
            .map(|(name, values)| {
                let array: ArrayRef = Arc::new(Float64Array::from(values));
                (name, array)
            })
            .collect();
        Table::from_columns(pairs)
    }

    /// Build the two-column edge table the pipelines write out.
    ///
    /// `pd.DataFrame(data=pairs, columns=['source', 'target'])` produces int64
    /// columns from a numpy integer array, so the same width is used here to
    /// keep the parquet files interchangeable.
    pub fn from_edges(pairs: &[(u32, u32)]) -> Result<Self, ArrowError> {
        let source: ArrayRef = Arc::new(Int64Array::from_iter_values(
            pairs.iter().map(|(s, _)| *s as i64),
        ));
        let target: ArrayRef = Arc::new(Int64Array::from_iter_values(
            pairs.iter().map(|(_, t)| *t as i64),
        ));
        Table::from_columns(vec![
            ("source".to_string(), source),
            ("target".to_string(), target),
        ])
    }

    /// An empty table with no columns and no rows.
    pub fn empty() -> Self {
        Self {
            schema: Arc::new(Schema::empty()),
            columns: Vec::new(),
            n_rows: 0,
            origin: Default::default(),
        }
    }

    /// Helper building a string column array.
    pub fn string_array(values: impl IntoIterator<Item = impl AsRef<str>>) -> ArrayRef {
        Arc::new(StringArray::from_iter_values(values))
    }

    /// Helper building an `f64` column array.
    pub fn f64_array(values: impl IntoIterator<Item = f64>) -> ArrayRef {
        Arc::new(Float64Array::from_iter_values(values))
    }

    /// Helper building an `i64` column array.
    pub fn i64_array(values: impl IntoIterator<Item = i64>) -> ArrayRef {
        Arc::new(Int64Array::from_iter_values(values))
    }

    /// Helper building a `u32` column array, used for cluster labels.
    pub fn u32_array(values: impl IntoIterator<Item = u32>) -> ArrayRef {
        Arc::new(UInt32Array::from_iter_values(values))
    }

    /// Helper building a boolean column array.
    pub fn bool_array(values: impl IntoIterator<Item = bool>) -> ArrayRef {
        Arc::new(BooleanArray::from_iter(values.into_iter().map(Some)))
    }

    /// The Arrow type of a column, for error messages.
    pub fn column_type(&self, name: &str) -> Option<&DataType> {
        self.column(name).map(|c| c.data_type())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_columns_infers_shape() {
        let table = Table::from_columns(vec![
            ("x".into(), Table::f64_array([1.0, 2.0, 3.0])),
            ("label".into(), Table::string_array(["a", "b", "c"])),
        ])
        .unwrap();
        assert_eq!(table.n_rows(), 3);
        assert_eq!(table.n_columns(), 2);
        assert_eq!(table.column_names(), vec!["x", "label"]);
    }

    #[test]
    fn mismatched_column_lengths_are_rejected() {
        let err = Table::from_columns(vec![
            ("x".into(), Table::f64_array([1.0, 2.0])),
            ("y".into(), Table::f64_array([1.0])),
        ])
        .unwrap_err();
        assert!(err.to_string().contains("length mismatch"));
    }

    #[test]
    fn set_column_replaces_in_place() {
        let mut table =
            Table::from_columns(vec![("x".into(), Table::f64_array([1.0, 2.0]))]).unwrap();
        table
            .set_column("niches", Table::u32_array([0, 1]))
            .unwrap();
        table
            .set_column("niches", Table::u32_array([3, 4]))
            .unwrap();
        assert_eq!(table.n_columns(), 2);
        assert_eq!(table.column_names(), vec!["x", "niches"]);
    }

    #[test]
    fn edge_tables_use_the_python_column_names() {
        let table = Table::from_edges(&[(0, 1), (1, 2)]).unwrap();
        assert_eq!(table.column_names(), vec!["source", "target"]);
        assert_eq!(table.n_rows(), 2);
    }
}
