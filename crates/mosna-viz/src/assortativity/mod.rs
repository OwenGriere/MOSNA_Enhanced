//! Assortativity figures — port of `package/core/assortativity/`.
//!
//! All five read the same table: the columns of `net_stat.csv` and one row of
//! values per sample. [`Table`] does the parsing once so each figure works from
//! named blocks rather than re-deriving them from column suffixes.

pub mod abundance;
pub mod heatmap;
pub mod mean_std;
pub mod mixing_matrix;

/// The statistics table, indexed by what each figure needs.
pub struct Table<'a> {
    pub columns: &'a [String],
    pub rows: &'a [(String, Vec<f64>)],
}

impl<'a> Table<'a> {
    pub fn new(columns: &'a [String], rows: &'a [(String, Vec<f64>)]) -> Self {
        Self { columns, rows }
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() || self.columns.is_empty()
    }

    /// Indices and names of the abundance columns, `% <phenotype>`.
    pub fn abundance_columns(&self) -> Vec<(usize, String)> {
        self.columns
            .iter()
            .enumerate()
            .filter_map(|(index, name)| {
                name.strip_prefix("% ")
                    .map(|phenotype| (index, phenotype.to_string()))
            })
            .collect()
    }

    /// Indices and names of the z-scored phenotype pairs.
    ///
    /// `assort Z` is the network-wide coefficient, not a pair, and every figure
    /// excludes it — plotting it alongside the pairs would put a value on a
    /// different scale into the same colour map.
    pub fn pair_z_columns(&self) -> Vec<(usize, String)> {
        self.columns
            .iter()
            .enumerate()
            .filter(|(_, name)| name.ends_with(" Z") && name.as_str() != "assort Z")
            .map(|(index, name)| (index, name.clone()))
            .collect()
    }

    /// The two phenotypes a pair column names.
    ///
    /// `"A - B Z"` becomes `("A", "B")`. Splitting once from the left matches
    /// the Python's `col.split(' - ', maxsplit=1)`, so a phenotype containing
    /// a dash survives on the right-hand side.
    pub fn split_pair(name: &str) -> Option<(String, String)> {
        let stripped = name.strip_suffix(" Z")?;
        let (left, right) = stripped.split_once(" - ")?;
        Some((left.trim().to_string(), right.trim().to_string()))
    }

    /// Value at `(row, column)`, or `NaN` when out of range.
    pub fn value(&self, row: usize, column: usize) -> f64 {
        self.rows
            .get(row)
            .and_then(|(_, values)| values.get(column))
            .copied()
            .unwrap_or(f64::NAN)
    }

    /// The sample identifier of a row, as it appears in `net_stat.csv`.
    pub fn row_id(&self, row: usize) -> &str {
        self.rows.get(row).map(|(id, _)| id.as_str()).unwrap_or("")
    }

    /// The patient and sample a row's identifier names.
    ///
    /// The per-sample figures are named `heatmap_zscore_{patient}-{sample}`,
    /// which is what the interface parses to group its gallery.
    pub fn row_short_name(&self, row: usize) -> String {
        let id = self.row_id(row);
        let mut parts = id.split('_');
        let patient = parts
            .next()
            .and_then(|part| part.split_once('-').map(|(_, value)| value))
            .unwrap_or(id);
        match parts
            .next()
            .and_then(|part| part.split_once('-').map(|(_, value)| value))
        {
            Some(sample) => format!("{patient}-{sample}"),
            None => patient.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> (Vec<String>, Vec<(String, Vec<f64>)>) {
        let columns = vec![
            "# total".into(),
            "% A".into(),
            "% B".into(),
            "assort Z".into(),
            "A - A Z".into(),
            "A - B Z".into(),
            "B - B Z".into(),
        ];
        let rows = vec![
            (
                "patient-1_sample-2".to_string(),
                vec![10.0, 0.6, 0.4, 3.0, 1.0, -2.0, 0.5],
            ),
            (
                "patient-3_sample-1".to_string(),
                vec![20.0, 0.5, 0.5, 1.0, 0.2, -1.0, 0.1],
            ),
        ];
        (columns, rows)
    }

    #[test]
    fn abundance_columns_are_found_by_prefix() {
        let (columns, rows) = table();
        let table = Table::new(&columns, &rows);
        let found = table.abundance_columns();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].1, "A");
    }

    #[test]
    fn the_overall_coefficient_is_not_a_pair() {
        let (columns, rows) = table();
        let table = Table::new(&columns, &rows);
        let pairs = table.pair_z_columns();
        assert_eq!(pairs.len(), 3);
        assert!(!pairs.iter().any(|(_, name)| name == "assort Z"));
    }

    #[test]
    fn a_pair_splits_into_its_two_phenotypes() {
        assert_eq!(
            Table::split_pair("A - B Z"),
            Some(("A".to_string(), "B".to_string()))
        );
        // A phenotype containing a dash survives.
        assert_eq!(
            Table::split_pair("T-cell - B Z"),
            Some(("T-cell".to_string(), "B".to_string()))
        );
        assert_eq!(Table::split_pair("assort"), None);
    }

    #[test]
    fn the_short_name_matches_what_the_interface_parses() {
        let (columns, rows) = table();
        let table = Table::new(&columns, &rows);
        assert_eq!(table.row_short_name(0), "1-2");
        assert_eq!(table.row_short_name(1), "3-1");
    }

    #[test]
    fn a_single_level_identifier_yields_the_patient_alone() {
        let columns = vec!["% A".to_string()];
        let rows = vec![("patient-7".to_string(), vec![1.0])];
        let table = Table::new(&columns, &rows);
        assert_eq!(table.row_short_name(0), "7");
    }

    #[test]
    fn an_out_of_range_lookup_is_missing_not_a_panic() {
        let (columns, rows) = table();
        let table = Table::new(&columns, &rows);
        assert!(table.value(99, 0).is_nan());
        assert_eq!(table.row_id(99), "");
    }
}
