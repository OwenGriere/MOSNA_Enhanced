//! How a network is written to, and read from, its two files.
//!
//! A network is a pair of tables sharing a sample's name:
//!
//! - `nodes_{sample}` — one row per cell, as given, plus a `degree` column: the
//!   number of neighbours of each cell.
//! - `edges_{sample}` — a single `indices` column: the neighbours of cell 0,
//!   then those of cell 1, and so on, each run sorted. The degrees say where
//!   one cell's run ends and the next begins.
//!
//! That is the compressed sparse row form of [`Csr`], split so that each file
//! stays an ordinary table: the offsets are the running sum of `degree`.
//!
//! The previous format — an edges table with one `source`, `target` row per
//! edge, as MOSNA's Python implementation writes it — is still read, so the
//! networks computed before, and those from the Python side, keep working.
//! Nothing writes it any more.

use std::path::Path;

use crate::error::{IoError, Result};
use crate::network::csr::{Csr, Pair};
use crate::read::get_opener::{read_table, Extension};
use crate::table::Table;

/// The nodes file's column holding each cell's number of neighbours.
pub const DEGREE_COLUMN: &str = "degree";

/// The edges file's single column of neighbours.
pub const INDICES_COLUMN: &str = "indices";

/// The two layouts an edges file can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgesFormat {
    /// One `indices` column, read with the nodes file's `degree`.
    Csr,
    /// One `source`, `target` row per edge: the format before this one.
    SourceTarget,
}

/// Which layout `edges` has, or why it is neither.
pub fn edges_format(edges: &Table) -> Result<EdgesFormat> {
    let mut names = edges.column_names();
    names.sort_unstable();
    match names.as_slice() {
        [INDICES_COLUMN] => Ok(EdgesFormat::Csr),
        ["source", "target"] => Ok(EdgesFormat::SourceTarget),
        _ => Err(IoError::invalid(format!(
            "{} is not an edges file: it should have a single `{INDICES_COLUMN}` column, \
             or the two columns `source` and `target` of the previous format, and it has {:?}",
            edges.origin().display(),
            edges.column_names()
        ))),
    }
}

impl Table {
    /// The edges file of `network`: its `indices` column, as 64-bit integers
    /// like every index column pandas writes.
    pub fn csr_edges(network: &Csr) -> Result<Table> {
        let indices = Table::i64_array(network.indices().iter().map(|&v| v as i64));
        Ok(Table::from_columns(vec![(
            INDICES_COLUMN.to_string(),
            indices,
        )])?)
    }

    /// This nodes table with the `degree` column of `network` set — appended,
    /// or replaced in place if the table already has one.
    pub fn with_degrees(mut self, network: &Csr) -> Result<Table> {
        if self.n_rows() != network.n_nodes() {
            return Err(IoError::invalid(format!(
                "{} has {} cell(s) but its network has {} node(s)",
                self.origin().display(),
                self.n_rows(),
                network.n_nodes()
            )));
        }
        let degrees = Table::i64_array(network.degrees().into_iter().map(i64::from));
        self.set_column(DEGREE_COLUMN, degrees)?;
        Ok(self)
    }

    /// An edges table in the previous format, one `source`, `target` row per
    /// pair. Only for tests and for checking that such files are still read.
    pub fn source_target_edges(pairs: &[Pair]) -> Result<Table> {
        let source = Table::i64_array(pairs.iter().map(|&(s, _)| s as i64));
        let target = Table::i64_array(pairs.iter().map(|&(_, t)| t as i64));
        Ok(Table::from_columns(vec![
            ("source".to_string(), source),
            ("target".to_string(), target),
        ])?)
    }
}

/// The network the two tables describe, whichever format the edges are in.
pub fn read_network(nodes: &Table, edges: &Table) -> Result<Csr> {
    let network = match edges_format(edges)? {
        EdgesFormat::Csr => {
            let degrees = u32_column(nodes, DEGREE_COLUMN)?;
            let indices = u32_column(edges, INDICES_COLUMN)?;
            Csr::from_degrees(&degrees, indices)
        }
        EdgesFormat::SourceTarget => {
            let source = u32_column(edges, "source")?;
            let target = u32_column(edges, "target")?;
            let pairs: Vec<Pair> = source.into_iter().zip(target).collect();
            Csr::from_pairs(&pairs, nodes.n_rows())
        }
    };
    network.map_err(|source| IoError::Network {
        nodes: nodes.origin().to_path_buf(),
        edges: edges.origin().to_path_buf(),
        source,
    })
}

/// A column of node indices or counts, each of which must fit a `u32`.
fn u32_column(table: &Table, name: &str) -> Result<Vec<u32>> {
    table
        .index_column(name)?
        .into_iter()
        .map(|value| {
            u32::try_from(value).map_err(|_| {
                IoError::invalid(format!(
                    "column `{name}` in {} holds {value}, past the largest node index",
                    table.origin().display()
                ))
            })
        })
        .collect()
}

/// Read a sample's two files: its nodes table and its network.
pub fn read_network_files(
    nodes_path: &Path,
    edges_path: &Path,
    extension: Extension,
) -> Result<(Table, Csr)> {
    let nodes = read_table(nodes_path, extension)?;
    let edges = read_table(edges_path, extension)?;
    let network = read_network(&nodes, &edges)?;
    Ok((nodes, network))
}

/// Write a sample's two files as parquet: `nodes` with its `degree` column,
/// and the `indices` of `network`.
pub fn write_network(
    nodes: &Table,
    network: &Csr,
    nodes_path: &Path,
    edges_path: &Path,
) -> Result<()> {
    let nodes = nodes.clone().with_degrees(network)?;
    crate::write_parquet(&nodes, nodes_path)?;
    crate::write_parquet(&Table::csr_edges(network)?, edges_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::IoError;
    use arrow_schema::DataType;

    fn nodes(n: usize) -> Table {
        Table::from_columns(vec![
            ("x".into(), Table::f64_array((0..n).map(|i| i as f64))),
            (
                "phenotype".into(),
                Table::string_array((0..n).map(|i| if i % 2 == 0 { "A" } else { "B" })),
            ),
        ])
        .unwrap()
        .with_origin("nodes_patient-1.parquet")
    }

    fn network() -> Csr {
        Csr::from_pairs(&[(0, 1), (0, 2), (1, 2), (2, 3)], 5).unwrap()
    }

    // -----------------------------------------------------------------------
    // Writing
    // -----------------------------------------------------------------------

    #[test]
    fn the_edges_file_is_one_column_of_neighbours() {
        let table = Table::csr_edges(&network()).unwrap();
        assert_eq!(table.column_names(), vec![INDICES_COLUMN]);
        assert_eq!(table.n_rows(), 8);
        assert_eq!(
            table.index_column(INDICES_COLUMN).unwrap(),
            vec![1, 2, 0, 2, 0, 1, 3, 2]
        );
    }

    /// pandas reads an `int64` column as `int64`; an unsigned or a 32-bit one
    /// comes back as a different dtype, and a Python reader would have to care.
    #[test]
    fn both_columns_are_64_bit_integers() {
        let edges = Table::csr_edges(&network()).unwrap();
        assert_eq!(edges.column_type(INDICES_COLUMN), Some(&DataType::Int64));

        let nodes = nodes(5).with_degrees(&network()).unwrap();
        assert_eq!(nodes.column_type(DEGREE_COLUMN), Some(&DataType::Int64));
    }

    #[test]
    fn the_degree_is_appended_after_the_cells_own_columns() {
        let table = nodes(5).with_degrees(&network()).unwrap();
        assert_eq!(table.column_names(), vec!["x", "phenotype", DEGREE_COLUMN]);
        assert_eq!(
            table.index_column(DEGREE_COLUMN).unwrap(),
            vec![2, 2, 3, 1, 0]
        );
    }

    /// A table that already has a `degree` column — a nodes file read back,
    /// or a cohort that measured one — gets this network's, in the same place.
    #[test]
    fn an_existing_degree_column_is_replaced_in_place() {
        let mut table = nodes(5);
        table
            .set_column(DEGREE_COLUMN, Table::i64_array([9, 9, 9, 9, 9]))
            .unwrap();
        table
            .set_column("after", Table::f64_array([0.0; 5]))
            .unwrap();

        let table = table.with_degrees(&network()).unwrap();
        assert_eq!(
            table.column_names(),
            vec!["x", "phenotype", DEGREE_COLUMN, "after"]
        );
        assert_eq!(
            table.index_column(DEGREE_COLUMN).unwrap(),
            vec![2, 2, 3, 1, 0]
        );
    }

    #[test]
    fn a_network_of_another_size_is_refused() {
        let error = nodes(4).with_degrees(&network()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains('4') && message.contains('5'), "{message}");
    }

    #[test]
    fn the_previous_format_keeps_its_column_names() {
        let table = Table::source_target_edges(&[(0, 1), (1, 2)]).unwrap();
        assert_eq!(table.column_names(), vec!["source", "target"]);
        assert_eq!(table.column_type("source"), Some(&DataType::Int64));
        assert_eq!(table.n_rows(), 2);
    }

    // -----------------------------------------------------------------------
    // Telling the formats apart
    // -----------------------------------------------------------------------

    #[test]
    fn both_formats_are_recognised() {
        let csr = Table::csr_edges(&network()).unwrap();
        assert_eq!(edges_format(&csr).unwrap(), EdgesFormat::Csr);

        let legacy = Table::source_target_edges(&[(0, 1)]).unwrap();
        assert_eq!(edges_format(&legacy).unwrap(), EdgesFormat::SourceTarget);
    }

    #[test]
    fn the_previous_format_is_recognised_whatever_the_column_order() {
        let table = Table::from_columns(vec![
            ("target".into(), Table::i64_array([1])),
            ("source".into(), Table::i64_array([0])),
        ])
        .unwrap();
        assert_eq!(edges_format(&table).unwrap(), EdgesFormat::SourceTarget);
    }

    #[test]
    fn a_table_of_neither_format_is_refused_naming_both() {
        let table = Table::from_columns(vec![("from".into(), Table::i64_array([0]))])
            .unwrap()
            .with_origin("edges_patient-1.parquet");
        let message = edges_format(&table).unwrap_err().to_string();
        assert!(message.contains("edges_patient-1.parquet"), "{message}");
        assert!(message.contains(INDICES_COLUMN), "{message}");
        assert!(message.contains("source"), "{message}");
    }

    // -----------------------------------------------------------------------
    // Reading
    // -----------------------------------------------------------------------

    #[test]
    fn a_network_reads_back_from_its_two_tables() {
        let nodes = nodes(5).with_degrees(&network()).unwrap();
        let edges = Table::csr_edges(&network()).unwrap();
        assert_eq!(read_network(&nodes, &edges).unwrap(), network());
    }

    #[test]
    fn the_previous_format_reads_as_the_same_network() {
        let edges = Table::source_target_edges(&network().pairs()).unwrap();
        assert_eq!(read_network(&nodes(5), &edges).unwrap(), network());
    }

    /// In the previous format the nodes file carries no degree; when it does
    /// anyway — a stale column — the edge list is what the network is.
    #[test]
    fn in_the_previous_format_a_degree_column_is_ignored() {
        let mut stale = nodes(5);
        stale
            .set_column(DEGREE_COLUMN, Table::i64_array([7, 7, 7, 7, 7]))
            .unwrap();
        let edges = Table::source_target_edges(&network().pairs()).unwrap();
        assert_eq!(read_network(&stale, &edges).unwrap(), network());
    }

    #[test]
    fn neighbours_without_degrees_are_refused_naming_the_nodes_file() {
        let edges = Table::csr_edges(&network()).unwrap();
        let error = read_network(&nodes(5), &edges).unwrap_err();
        assert!(
            matches!(&error, IoError::MissingColumn { column, .. } if column == DEGREE_COLUMN),
            "{error:?}"
        );
        assert!(
            error.to_string().contains("nodes_patient-1.parquet"),
            "{error}"
        );
    }

    /// The nodes of one run and the edges of another: the degrees no longer
    /// add up to the entries, and saying so beats aggregating over nonsense.
    #[test]
    fn files_from_two_different_networks_are_refused_naming_both() {
        let other = Csr::from_pairs(&[(0, 1)], 5).unwrap();
        let nodes = nodes(5).with_degrees(&other).unwrap();
        let edges = Table::csr_edges(&network())
            .unwrap()
            .with_origin("edges_patient-1.parquet");

        let message = read_network(&nodes, &edges).unwrap_err().to_string();
        assert!(message.contains("nodes_patient-1.parquet"), "{message}");
        assert!(message.contains("edges_patient-1.parquet"), "{message}");
        assert!(message.contains("same network"), "{message}");
    }

    #[test]
    fn an_edge_past_the_last_cell_is_refused_in_the_previous_format() {
        let edges = Table::source_target_edges(&[(0, 9)])
            .unwrap()
            .with_origin("edges_patient-1.parquet");
        let message = read_network(&nodes(5), &edges).unwrap_err().to_string();
        assert!(message.contains("0–9"), "{message}");
        assert!(message.contains("edges_patient-1.parquet"), "{message}");
    }

    /// The Python asserts `sorted(edges.columns) == ["source", "target"]`: a
    /// weight column means the file is something else, not an edge list.
    #[test]
    fn the_previous_format_with_an_extra_column_is_refused() {
        let edges = Table::from_columns(vec![
            ("source".into(), Table::i64_array([0])),
            ("target".into(), Table::i64_array([1])),
            ("weight".into(), Table::f64_array([1.0])),
        ])
        .unwrap();
        assert!(read_network(&nodes(2), &edges).is_err());
    }

    #[test]
    fn a_negative_node_index_is_refused() {
        let legacy = Table::from_columns(vec![
            ("source".into(), Table::i64_array([-1])),
            ("target".into(), Table::i64_array([1])),
        ])
        .unwrap();
        assert!(read_network(&nodes(2), &legacy).is_err());

        let mut nodes = nodes(2);
        nodes
            .set_column(DEGREE_COLUMN, Table::i64_array([1, 1]))
            .unwrap();
        let edges =
            Table::from_columns(vec![(INDICES_COLUMN.into(), Table::i64_array([1, -1]))]).unwrap();
        assert!(read_network(&nodes, &edges).is_err());
    }

    #[test]
    fn a_degree_that_is_not_a_count_is_refused() {
        let mut nodes = nodes(2);
        nodes
            .set_column(DEGREE_COLUMN, Table::f64_array([1.5, 0.5]))
            .unwrap();
        let edges = Table::csr_edges(&Csr::from_pairs(&[(0, 1)], 2).unwrap()).unwrap();
        assert!(read_network(&nodes, &edges).is_err());
    }

    #[test]
    fn an_empty_network_reads_back() {
        let empty = Csr::empty(3);
        let nodes = nodes(3).with_degrees(&empty).unwrap();
        let edges = Table::csr_edges(&empty).unwrap();
        assert_eq!(edges.n_rows(), 0);
        assert_eq!(read_network(&nodes, &edges).unwrap(), empty);
    }

    // -----------------------------------------------------------------------
    // On disk
    // -----------------------------------------------------------------------

    #[test]
    fn a_written_network_reads_back_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let nodes_path = dir.path().join("nodes_patient-1.parquet");
        let edges_path = dir.path().join("edges_patient-1.parquet");

        write_network(&nodes(5), &network(), &nodes_path, &edges_path).unwrap();
        let (table, read) =
            read_network_files(&nodes_path, &edges_path, Extension::Parquet).unwrap();

        assert_eq!(read, network());
        assert_eq!(table.column_names(), vec!["x", "phenotype", DEGREE_COLUMN]);
        assert_eq!(table.string_column("phenotype").unwrap()[1], "B");
    }

    /// A network directory given by the user may hold CSV; `degree` and
    /// `indices` are plain integer columns there too.
    #[test]
    fn a_network_in_csv_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let nodes_path = dir.path().join("nodes_patient-1.csv");
        let edges_path = dir.path().join("edges_patient-1.csv");
        crate::write_csv(&nodes(5).with_degrees(&network()).unwrap(), &nodes_path).unwrap();
        crate::write_csv(&Table::csr_edges(&network()).unwrap(), &edges_path).unwrap();

        let (_, read) = read_network_files(&nodes_path, &edges_path, Extension::Csv).unwrap();
        assert_eq!(read, network());
    }

    #[test]
    fn a_network_in_the_previous_format_reads_back_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let nodes_path = dir.path().join("nodes_patient-1.parquet");
        let edges_path = dir.path().join("edges_patient-1.parquet");
        crate::write_parquet(&nodes(5), &nodes_path).unwrap();
        crate::write_parquet(
            &Table::source_target_edges(&network().pairs()).unwrap(),
            &edges_path,
        )
        .unwrap();

        let (_, read) = read_network_files(&nodes_path, &edges_path, Extension::Parquet).unwrap();
        assert_eq!(read, network());
    }
}
