//! A minimal columnar table over Arrow arrays.

pub mod column_access;
pub mod constructors;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::RecordBatch;
use arrow_schema::{Field, Schema};

/// A named set of equal-length Arrow columns.
///
/// The Python code passes `pandas.DataFrame` values around; this plays the same
/// role, restricted to what the pipelines actually do with them: look columns
/// up by name, read them as `f64` or as strings, append a column, and write the
/// whole thing back out.
#[derive(Debug, Clone)]
pub struct Table {
    schema: Arc<Schema>,
    columns: Vec<arrow_array::ArrayRef>,
    n_rows: usize,
    /// Where this table came from, used to build precise error messages.
    origin: PathBuf,
}

impl Table {
    /// Number of rows.
    pub fn n_rows(&self) -> usize {
        self.n_rows
    }

    /// Number of columns.
    pub fn n_columns(&self) -> usize {
        self.columns.len()
    }

    /// `true` when the table has no rows.
    pub fn is_empty(&self) -> bool {
        self.n_rows == 0
    }

    /// Column names, in file order.
    pub fn column_names(&self) -> Vec<&str> {
        self.schema
            .fields()
            .iter()
            .map(|f| f.name().as_str())
            .collect()
    }

    /// `true` when a column with that name exists.
    pub fn has_column(&self, name: &str) -> bool {
        self.schema.column_with_name(name).is_some()
    }

    /// The Arrow schema.
    pub fn schema(&self) -> &Arc<Schema> {
        &self.schema
    }

    /// The raw columns.
    pub fn columns(&self) -> &[arrow_array::ArrayRef] {
        &self.columns
    }

    /// The path this table was read from, for diagnostics.
    pub fn origin(&self) -> &Path {
        &self.origin
    }

    /// Record where this table came from.
    pub fn with_origin(mut self, origin: impl Into<PathBuf>) -> Self {
        self.origin = origin.into();
        self
    }

    /// Borrow a column by name.
    pub fn column(&self, name: &str) -> Option<&arrow_array::ArrayRef> {
        self.schema
            .column_with_name(name)
            .map(|(idx, _)| &self.columns[idx])
    }

    /// Convert to an Arrow `RecordBatch` for writing.
    pub fn to_record_batch(&self) -> Result<RecordBatch, arrow_schema::ArrowError> {
        if self.columns.is_empty() {
            return RecordBatch::try_new_with_options(
                self.schema.clone(),
                vec![],
                &arrow_array::RecordBatchOptions::new().with_row_count(Some(self.n_rows)),
            );
        }
        RecordBatch::try_new(self.schema.clone(), self.columns.clone())
    }

    /// Append a column, replacing any existing one with the same name.
    ///
    /// Replacing in place (rather than appending a duplicate) is what
    /// `df["niches"] = ...` does in pandas, and `merge_niche_pheno` relies on
    /// re-running being idempotent.
    pub fn set_column(
        &mut self,
        name: &str,
        array: arrow_array::ArrayRef,
    ) -> Result<(), arrow_schema::ArrowError> {
        if array.len() != self.n_rows && self.n_columns() > 0 {
            return Err(arrow_schema::ArrowError::InvalidArgumentError(format!(
                "column `{name}` has {} rows, table has {}",
                array.len(),
                self.n_rows
            )));
        }
        let field = Field::new(name, array.data_type().clone(), array.null_count() > 0);
        match self.schema.column_with_name(name) {
            Some((idx, _)) => {
                let mut fields: Vec<Arc<Field>> = self.schema.fields().iter().cloned().collect();
                fields[idx] = Arc::new(field);
                self.schema = Arc::new(Schema::new_with_metadata(
                    fields,
                    self.schema.metadata().clone(),
                ));
                self.columns[idx] = array;
            }
            None => {
                let mut fields: Vec<Arc<Field>> = self.schema.fields().iter().cloned().collect();
                fields.push(Arc::new(field));
                self.schema = Arc::new(Schema::new_with_metadata(
                    fields,
                    self.schema.metadata().clone(),
                ));
                self.n_rows = array.len();
                self.columns.push(array);
            }
        }
        Ok(())
    }
}
