//! Port of `package/utils/convert_net_dir.py::convert_net_dir`.

use std::path::Path;

use rayon::prelude::*;

use crate::error::Result;
use crate::read::get_opener::{read_table, Extension};
use crate::table::Table;
use crate::write::write_parquet::write_parquet;

/// Rewrite every network file of `net_dir` as parquet into `save_dir`.
///
/// The Python version converts only the *nodes* files, because `find_sample`
/// matches the `nodes_` prefix. Its callers then read the matching `edges_`
/// files with the parquet reader, which fails when the source directory held
/// CSV. Converting the edges files too is what makes the CSV and TSV input
/// paths actually work, so this port handles both — a fix, not a behaviour
/// change for the parquet case, which never reaches this function.
pub fn convert_net_dir(
    net_dir: impl AsRef<Path>,
    save_dir: impl AsRef<Path>,
    patient_column: &str,
    sample_column: Option<&str>,
    extension: Extension,
) -> Result<usize> {
    let net_dir = net_dir.as_ref();
    let save_dir = save_dir.as_ref();
    std::fs::create_dir_all(save_dir).map_err(|source| crate::error::IoError::Write {
        path: save_dir.to_path_buf(),
        source,
    })?;

    let nodes_files = crate::discovery::find_sample::find_sample(
        net_dir,
        extension.as_str(),
        patient_column,
        sample_column,
    )?;

    // Pair each nodes file with the edges file naming the same sample.
    let mut jobs: Vec<std::path::PathBuf> = Vec::with_capacity(nodes_files.len() * 2);
    for nodes in &nodes_files {
        jobs.push(nodes.clone());
        if let Some(name) = nodes.file_name().and_then(|n| n.to_str()) {
            let edges = net_dir.join(name.replacen("nodes_", "edges_", 1));
            if edges.is_file() {
                jobs.push(edges);
            }
        }
    }

    jobs.par_iter()
        .map(|source| {
            let table: Table = read_table(source, extension)?;
            let stem = source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            write_parquet(&table, save_dir.join(format!("{stem}.parquet")))
        })
        .collect::<Result<Vec<()>>>()?;

    Ok(jobs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_nodes_and_edges_to_parquet() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");
        std::fs::create_dir_all(&src).unwrap();

        std::fs::write(
            src.join("nodes_patient-1_sample-2.csv"),
            "X_position,Y_position,Cluster\n1.0,2.0,A\n3.0,4.0,B\n",
        )
        .unwrap();
        std::fs::write(
            src.join("edges_patient-1_sample-2.csv"),
            "source,target\n0,1\n",
        )
        .unwrap();

        let converted =
            convert_net_dir(&src, &dst, "patient", Some("sample"), Extension::Csv).unwrap();
        assert_eq!(converted, 2);

        let nodes =
            crate::read::read_parquet::read_parquet(dst.join("nodes_patient-1_sample-2.parquet"))
                .unwrap();
        assert_eq!(nodes.f64_column("X_position").unwrap(), vec![1.0, 3.0]);

        let edges =
            crate::read::read_parquet::read_parquet(dst.join("edges_patient-1_sample-2.parquet"))
                .unwrap();
        assert_eq!(edges.edges().unwrap(), vec![(0, 1)]);
    }

    #[test]
    fn a_nodes_file_without_edges_still_converts() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("nodes_patient-1.csv"), "x\n1\n").unwrap();

        let converted = convert_net_dir(&src, &dst, "patient", None, Extension::Csv).unwrap();
        assert_eq!(converted, 1);
        assert!(dst.join("nodes_patient-1.parquet").is_file());
    }
}
