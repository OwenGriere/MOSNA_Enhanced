//! Port of `neighbors.py::compute_spatial_omic_features_{single,all}_network(s)`.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use mosna_io::read::get_opener::{read_table, Extension};
use mosna_io::{SampleId, Table};

use crate::error::{CoreError, Result};
use crate::nas::make_features_nas::make_features_nas;
use crate::nas::onehot::one_hot;

/// Everything the feature computation needs besides the sample to process.
#[derive(Debug, Clone)]
pub struct SofOptions {
    /// Directory holding the `nodes_*` and `edges_*` files.
    pub net_dir: PathBuf,
    pub extension: Extension,
    pub patient_column: String,
    pub sample_column: Option<String>,
    /// Column(s) aggregated over each neighbourhood.
    pub attributes_col: Vec<String>,
    /// Feature names in the output, fixed across the cohort.
    pub use_attributes: Vec<String>,
    /// One-hot encode `attributes_col[0]` before aggregating.
    pub make_onehot: bool,
    pub order: usize,
    pub stat_names: Vec<String>,
    pub var_sep: String,
    pub add_sample_info: bool,
}

impl SofOptions {
    fn sample_column(&self) -> Option<&str> {
        self.sample_column.as_deref()
    }
}

/// The aggregated feature table for one or many networks.
#[derive(Debug, Clone)]
pub struct VarAggreg {
    /// Names of the feature columns, e.g. `["cancer mean", "cancer std"]`.
    pub column_names: Vec<String>,
    /// Row-major features, `n_rows * column_names.len()`.
    pub values: Vec<f64>,
    pub n_rows: usize,
    /// Patient id of each row.
    pub patients: Vec<String>,
    /// Sample id of each row, when the dataset has two levels.
    pub samples: Vec<Option<String>>,
}

impl VarAggreg {
    pub fn n_columns(&self) -> usize {
        self.column_names.len()
    }

    pub fn row(&self, i: usize) -> &[f64] {
        let w = self.n_columns();
        &self.values[i * w..(i + 1) * w]
    }

    /// The matrix handed to the dimensionality reduction and clustering.
    ///
    /// # Faithful to a surprising Python behaviour
    ///
    /// `aggregated_niches` calls `get_clusterer(data=var_aggreg.values, ...)`,
    /// and `var_aggreg` still carries the patient and sample id columns that
    /// `compute_spatial_omic_features_*` appended. Those two columns therefore
    /// enter UMAP as ordinary numeric features, and the identifiers influence
    /// the niches that come out.
    ///
    /// That looks unintended — an id is not a measurement — but it is what the
    /// Python produces, and dropping the columns here would make every niche
    /// label disagree with a Python run on the same input. The behaviour is
    /// reproduced so results match; see `PROGRESS.log` for the flag raised
    /// about it. Removing the ids is a one-line change in this method once the
    /// intended semantics are confirmed.
    ///
    /// The ids are parsed as numbers, mirroring the implicit conversion numpy
    /// performs on the object array. A non-numeric id becomes `NaN` here, where
    /// Python raises inside `check_array`.
    pub fn clustering_matrix(&self) -> (Vec<f64>, usize) {
        let n_id_columns = 1 + usize::from(self.samples.iter().any(Option::is_some));
        let width = self.n_columns() + n_id_columns;
        let mut out = Vec::with_capacity(self.n_rows * width);

        for i in 0..self.n_rows {
            out.extend_from_slice(self.row(i));
            out.push(self.patients[i].parse::<f64>().unwrap_or(f64::NAN));
            if n_id_columns == 2 {
                out.push(
                    self.samples[i]
                        .as_deref()
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(f64::NAN),
                );
            }
        }
        (out, width)
    }

    /// The `(patient, sample)` pair of each row, deduplicated in first-seen
    /// order — the equivalent of `var_aggreg[[id1, id2]].drop_duplicates()`.
    pub fn unique_sample_ids(&self) -> Vec<SampleId> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for i in 0..self.n_rows {
            let id = SampleId {
                patient: self.patients[i].clone(),
                sample: self.samples[i].clone(),
            };
            if seen.insert(id.clone()) {
                out.push(id);
            }
        }
        out
    }

    /// Convert to a table for caching as `var_aggreg.parquet`.
    pub fn to_table(&self, patient_column: &str, sample_column: Option<&str>) -> Result<Table> {
        let mut pairs: Vec<(String, arrow_array_ref::ArrayRef)> = Vec::new();
        let width = self.n_columns();
        for (col, name) in self.column_names.iter().enumerate() {
            let values: Vec<f64> = (0..self.n_rows)
                .map(|row| self.values[row * width + col])
                .collect();
            pairs.push((name.clone(), Table::f64_array(values)));
        }
        pairs.push((
            patient_column.to_string(),
            Table::string_array(self.patients.iter()),
        ));
        if let Some(sample_column) = sample_column {
            pairs.push((
                sample_column.to_string(),
                Table::string_array(self.samples.iter().map(|s| s.clone().unwrap_or_default())),
            ));
        }
        Table::from_columns(pairs).map_err(|e| CoreError::shape(e.to_string()))
    }

    /// Read back a cached `var_aggreg.parquet`.
    pub fn from_table(
        table: &Table,
        patient_column: &str,
        sample_column: Option<&str>,
    ) -> Result<Self> {
        let column_names: Vec<String> = table
            .column_names()
            .into_iter()
            .filter(|name| *name != patient_column && Some(*name) != sample_column)
            .map(str::to_string)
            .collect();

        let n_rows = table.n_rows();
        let width = column_names.len();
        let mut values = vec![0.0; n_rows * width];
        for (col, name) in column_names.iter().enumerate() {
            let column = table.f64_column(name)?;
            for (row, value) in column.into_iter().enumerate() {
                values[row * width + col] = value;
            }
        }

        let patients = table.string_column(patient_column)?;
        let samples = match sample_column {
            Some(name) => table.opt_string_column(name)?,
            None => vec![None; n_rows],
        };

        Ok(Self {
            column_names,
            values,
            n_rows,
            patients,
            samples,
        })
    }
}

/// Compute the NAS features of one network.
pub fn compute_spatial_omic_features_single_network(
    options: &SofOptions,
    sample: &SampleId,
) -> Result<VarAggreg> {
    let ext = options.extension.as_str();
    let nodes_path = options.net_dir.join(sample.nodes_file_name(
        &options.patient_column,
        options.sample_column(),
        ext,
    ));
    let edges_path = options.net_dir.join(sample.edges_file_name(
        &options.patient_column,
        options.sample_column(),
        ext,
    ));

    let nodes = read_table(&nodes_path, options.extension)?;
    let edges = read_table(&edges_path, options.extension)?;
    let pairs = edges.edges()?;
    let n_obs = nodes.n_rows();

    let x = build_feature_matrix(&nodes, options, &nodes_path)?;

    let features = make_features_nas(
        &x,
        n_obs,
        &pairs,
        options.order,
        &options.use_attributes,
        &options.stat_names,
        &options.var_sep,
    );

    Ok(VarAggreg {
        column_names: features.column_names,
        values: features.values,
        n_rows: features.n_rows,
        patients: vec![sample.patient.clone(); n_obs],
        samples: vec![sample.sample.clone(); n_obs],
    })
}

/// The `n_obs * n_var` attribute matrix fed to the aggregation.
fn build_feature_matrix(nodes: &Table, options: &SofOptions, path: &Path) -> Result<Vec<f64>> {
    if options.make_onehot {
        let column = options.attributes_col.first().ok_or_else(|| {
            CoreError::invalid(
                "`attributes_col` has to be of length 1 to make dummy variables".to_string(),
            )
        })?;
        let labels = nodes.opt_string_column(column)?;
        Ok(one_hot(&labels, &options.use_attributes))
    } else {
        // Already-numeric attribute columns, selected in the fixed cohort order
        // so every sample yields the same column meaning.
        let n_var = options.use_attributes.len();
        let n_obs = nodes.n_rows();
        let mut x = vec![0.0f64; n_obs * n_var];
        for (col, name) in options.use_attributes.iter().enumerate() {
            if !nodes.has_column(name) {
                // Python fills a missing attribute with zeros via
                // `nodes[col] = 0`; the column stays present and inert.
                continue;
            }
            let values = nodes.f64_column(name)?;
            if values.len() != n_obs {
                return Err(CoreError::shape(format!(
                    "column `{name}` of {} has {} rows, expected {n_obs}",
                    path.display(),
                    values.len()
                )));
            }
            for (row, value) in values.into_iter().enumerate() {
                x[row * n_var + col] = value;
            }
        }
        Ok(x)
    }
}

/// Compute the NAS features of every network and stack them.
///
/// The Python runs one `joblib` worker per sample. This uses a rayon parallel
/// map over the same unit of work, so the per-sample cost is unchanged and the
/// results are concatenated in the deterministic order of `data_index` rather
/// than in completion order.
pub fn compute_spatial_omic_features_all_networks(
    options: &SofOptions,
    data_index: &[SampleId],
    progress: &(dyn Fn(usize, usize) + Sync),
) -> Result<VarAggreg> {
    if data_index.is_empty() {
        return Err(CoreError::invalid(format!(
            "no network files found in {}",
            options.net_dir.display()
        )));
    }

    let done = std::sync::atomic::AtomicUsize::new(0);
    let total = data_index.len();

    let per_sample: Vec<VarAggreg> = data_index
        .par_iter()
        .map(|sample| {
            let result = compute_spatial_omic_features_single_network(options, sample);
            let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            progress(n, total);
            result
        })
        .collect::<Result<Vec<_>>>()?;

    let column_names = per_sample[0].column_names.clone();
    for (sample, aggreg) in data_index.iter().zip(&per_sample) {
        if aggreg.column_names != column_names {
            return Err(CoreError::shape(format!(
                "sample {} produced {} feature columns, expected {}",
                sample.patient,
                aggreg.column_names.len(),
                column_names.len()
            )));
        }
    }

    let n_rows = per_sample.iter().map(|a| a.n_rows).sum();
    let mut values = Vec::with_capacity(n_rows * column_names.len());
    let mut patients = Vec::with_capacity(n_rows);
    let mut samples = Vec::with_capacity(n_rows);
    for aggreg in per_sample {
        values.extend(aggreg.values);
        patients.extend(aggreg.patients);
        samples.extend(aggreg.samples);
    }

    Ok(VarAggreg {
        column_names,
        values,
        n_rows,
        patients,
        samples,
    })
}

/// Re-export so the `to_table` signature does not leak an Arrow dependency
/// name into this module's public surface.
mod arrow_array_ref {
    pub use arrow_array::ArrayRef;
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosna_io::write::write_parquet::write_parquet;

    fn options(dir: &Path) -> SofOptions {
        SofOptions {
            net_dir: dir.to_path_buf(),
            extension: Extension::Parquet,
            patient_column: "patient".into(),
            sample_column: Some("sample".into()),
            attributes_col: vec!["Cluster".into()],
            use_attributes: vec!["A".into(), "B".into()],
            make_onehot: true,
            order: 1,
            stat_names: vec!["mean".into(), "std".into()],
            var_sep: " ".into(),
            add_sample_info: true,
        }
    }

    /// Write a three-cell path network whose phenotypes are A, B, A.
    fn write_network(dir: &Path, patient: &str, sample: &str) {
        let nodes = Table::from_columns(vec![
            ("X".into(), Table::f64_array([0.0, 1.0, 2.0])),
            ("Y".into(), Table::f64_array([0.0, 0.0, 0.0])),
            ("Cluster".into(), Table::string_array(["A", "B", "A"])),
        ])
        .unwrap();
        let edges = Table::from_edges(&[(0, 1), (1, 2)]).unwrap();
        write_parquet(
            &nodes,
            dir.join(format!("nodes_patient-{patient}_sample-{sample}.parquet")),
        )
        .unwrap();
        write_parquet(
            &edges,
            dir.join(format!("edges_patient-{patient}_sample-{sample}.parquet")),
        )
        .unwrap();
    }

    #[test]
    fn one_hot_features_have_the_expected_columns_and_values() {
        let dir = tempfile::tempdir().unwrap();
        write_network(dir.path(), "1", "1");

        let aggreg = compute_spatial_omic_features_single_network(
            &options(dir.path()),
            &SampleId::with_sample("1", "1"),
        )
        .unwrap();

        assert_eq!(
            aggreg.column_names,
            vec!["A mean", "B mean", "A std", "B std"]
        );
        assert_eq!(aggreg.n_rows, 3);

        // Node 0 (A) neighbours node 1 (B): its window is {A, B}, so each
        // indicator averages 0.5.
        assert!((aggreg.row(0)[0] - 0.5).abs() < 1e-12);
        assert!((aggreg.row(0)[1] - 0.5).abs() < 1e-12);
        // Node 1 (B) sees {A, B, A}: two thirds A, one third B.
        assert!((aggreg.row(1)[0] - 2.0 / 3.0).abs() < 1e-12);
        assert!((aggreg.row(1)[1] - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn every_row_carries_its_sample_identity() {
        let dir = tempfile::tempdir().unwrap();
        write_network(dir.path(), "7", "3");

        let aggreg = compute_spatial_omic_features_single_network(
            &options(dir.path()),
            &SampleId::with_sample("7", "3"),
        )
        .unwrap();
        assert!(aggreg.patients.iter().all(|p| p == "7"));
        assert!(aggreg.samples.iter().all(|s| s.as_deref() == Some("3")));
        assert_eq!(
            aggreg.unique_sample_ids(),
            vec![SampleId::with_sample("7", "3")]
        );
    }

    #[test]
    fn all_networks_stacks_in_data_index_order() {
        let dir = tempfile::tempdir().unwrap();
        write_network(dir.path(), "1", "1");
        write_network(dir.path(), "2", "1");

        let index = vec![
            SampleId::with_sample("1", "1"),
            SampleId::with_sample("2", "1"),
        ];
        let aggreg =
            compute_spatial_omic_features_all_networks(&options(dir.path()), &index, &|_, _| {})
                .unwrap();

        assert_eq!(aggreg.n_rows, 6);
        assert_eq!(&aggreg.patients[..3], &["1", "1", "1"]);
        assert_eq!(&aggreg.patients[3..], &["2", "2", "2"]);
    }

    #[test]
    fn a_phenotype_absent_from_a_sample_keeps_its_column() {
        let dir = tempfile::tempdir().unwrap();
        write_network(dir.path(), "1", "1");

        let mut opts = options(dir.path());
        // `C` never occurs in the data but is part of the cohort vocabulary.
        opts.use_attributes = vec!["A".into(), "B".into(), "C".into()];

        let aggreg =
            compute_spatial_omic_features_single_network(&opts, &SampleId::with_sample("1", "1"))
                .unwrap();
        assert_eq!(aggreg.n_columns(), 6);
        // The `C mean` column exists and is uniformly zero.
        for row in 0..aggreg.n_rows {
            assert_eq!(aggreg.row(row)[2], 0.0);
        }
    }

    #[test]
    fn the_feature_table_round_trips_through_parquet() {
        let dir = tempfile::tempdir().unwrap();
        write_network(dir.path(), "4", "2");
        let aggreg = compute_spatial_omic_features_single_network(
            &options(dir.path()),
            &SampleId::with_sample("4", "2"),
        )
        .unwrap();

        let table = aggreg.to_table("patient", Some("sample")).unwrap();
        let path = dir.path().join("var_aggreg.parquet");
        write_parquet(&table, &path).unwrap();

        let reloaded = mosna_io::read::read_parquet::read_parquet(&path).unwrap();
        let back = VarAggreg::from_table(&reloaded, "patient", Some("sample")).unwrap();

        assert_eq!(back.column_names, aggreg.column_names);
        assert_eq!(back.n_rows, aggreg.n_rows);
        assert_eq!(back.patients, aggreg.patients);
        for row in 0..aggreg.n_rows {
            for (a, b) in back.row(row).iter().zip(aggreg.row(row)) {
                assert!((a - b).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn the_clustering_matrix_appends_the_id_columns() {
        // Pins the faithful reproduction of the Python behaviour documented on
        // `clustering_matrix`: the ids are part of the clustering input.
        let aggreg = VarAggreg {
            column_names: vec!["A mean".into()],
            values: vec![0.5, 0.25],
            n_rows: 2,
            patients: vec!["1".into(), "2".into()],
            samples: vec![Some("3".into()), Some("4".into())],
        };
        let (matrix, width) = aggreg.clustering_matrix();
        assert_eq!(width, 3);
        assert_eq!(matrix, vec![0.5, 1.0, 3.0, 0.25, 2.0, 4.0]);
    }

    #[test]
    fn an_empty_data_index_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = compute_spatial_omic_features_all_networks(&options(dir.path()), &[], &|_, _| {})
            .unwrap_err();
        assert!(err.to_string().contains("no network files"));
    }
}
