//! Step 2 — port of `package/assortativity.py`.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use mosna_config::model::assortativity::NetworkDirectory;
use mosna_config::validate::assert_params::{assert_params, Analysis};
use mosna_config::{section, AssortativityConfig, RawConfig};
use mosna_core::assortativity::sample_assort_mixmat;
use mosna_core::niches::find_all_phenotypes;
use mosna_io::read::get_opener::{read_table, Extension};
use mosna_io::write::write_csv::write_csv;
use mosna_io::{make_data_index, Table};

use crate::error::{create_dir_all, PipelineError, Result};
use crate::figures::FigureSink;
use crate::progress::Progress;

/// Compute z-scored assortativity and mixing matrices for every sample.
///
/// Writes `Assortativity/net_stat.csv`, one row per sample.
pub fn assortativity(
    config: &RawConfig,
    working_dir: &Path,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    assert_params(
        Analysis::Assortativity,
        config.section(section::ASSORTATIVITY)?,
    )?;
    let settings = AssortativityConfig::from_raw(config)?;

    let (net_dir, extension) = resolve_network_directory(
        &settings.network_directory,
        &settings.extension,
        working_dir,
    )?;

    let saving_folder = working_dir.join("Assortativity");
    create_dir_all(&saving_folder)?;

    let sample_column = settings.sample_column.as_deref();
    let data_index = make_data_index(
        &net_dir,
        &settings.patient_column,
        sample_column,
        extension.as_str(),
    )?;
    if data_index.is_empty() {
        return Err(PipelineError::NoSamples {
            path: net_dir,
            pattern: format!("{}-*", settings.patient_column),
        });
    }

    // The attribute vocabulary must be the cohort's, not each sample's:
    // otherwise the mixing matrices would have different shapes and could not
    // share a column layout in `net_stat.csv`.
    let attributes = find_all_phenotypes(
        &net_dir,
        &data_index,
        &settings.patient_column,
        sample_column,
        extension,
        &settings.phenotype_column,
    )?;
    let attribute_index: std::collections::HashMap<&str, u32> = attributes
        .iter()
        .enumerate()
        .map(|(i, a)| (a.as_str(), i as u32))
        .collect();

    progress.info("[PROCESS] Compute Assortativity");
    let n_shuffle = settings.effective_shuffles();
    let total = data_index.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    progress.step(0, total, "[PROCESS] Compute Assortativity");

    let mut rows: Vec<(usize, mosna_core::assortativity::SampleStats)> = data_index
        .par_iter()
        .enumerate()
        .map(|(position, id)| {
            let stem = id.str_group(&settings.patient_column, sample_column);
            let nodes = read_table(
                net_dir.join(format!("nodes_{stem}.{}", extension.as_str())),
                extension,
            )?;
            let edges = read_table(
                net_dir.join(format!("edges_{stem}.{}", extension.as_str())),
                extension,
            )?;

            let assignments: Vec<Option<u32>> = nodes
                .opt_string_column(&settings.phenotype_column)?
                .into_iter()
                .map(|label| label.and_then(|l| attribute_index.get(l.as_str()).copied()))
                .collect();

            let stats =
                sample_assort_mixmat(&assignments, &edges.edges()?, &attributes, &stem, n_shuffle);

            let finished = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            progress.step(finished, total, "[PROCESS] Compute Assortativity");
            Ok((position, stats))
        })
        .collect::<Result<Vec<_>>>()?;

    // Restore the deterministic sample order, which parallel completion loses.
    rows.sort_by_key(|(position, _)| *position);
    let rows: Vec<mosna_core::assortativity::SampleStats> =
        rows.into_iter().map(|(_, stats)| stats).collect();

    // A diagnostic run is a timing probe. The Python computes it and writes
    // nothing, so the GUI can extrapolate the cost of the full run from the
    // twenty shuffles it did.
    if settings.randomization_diagnostic {
        progress.info("[INFO] Randomization diagnostic complete, no results written");
        return Ok(());
    }

    let table = build_table(&rows)?;
    let path = saving_folder.join("net_stat.csv");
    write_csv(&table, &path)?;
    progress.info(&format!(
        "[INFO] Assortativity table saved in {}",
        saving_folder.display()
    ));

    let columns = rows
        .first()
        .map(|stats| stats.column_names.clone())
        .unwrap_or_default();
    let figure_rows: Vec<(String, Vec<f64>)> = rows
        .iter()
        .map(|stats| (stats.id.clone(), stats.values.clone()))
        .collect();
    figures.assortativity(&columns, &figure_rows, &saving_folder)?;

    Ok(())
}

/// Where the networks live, and in what format.
///
/// `Default` means the output of step 1, which is always parquet whatever the
/// input format was.
pub(crate) fn resolve_network_directory(
    directory: &NetworkDirectory,
    configured_extension: &str,
    working_dir: &Path,
) -> Result<(PathBuf, Extension)> {
    match directory {
        NetworkDirectory::Default => {
            Ok((working_dir.join("temp/net_dir_mosna"), Extension::Parquet))
        }
        NetworkDirectory::Custom(path) => Ok((
            working_dir.join(path),
            Extension::parse(configured_extension)?,
        )),
    }
}

/// Assemble the rows into the `net_stat.csv` table, `id` first.
fn build_table(rows: &[mosna_core::assortativity::SampleStats]) -> Result<Table> {
    let Some(first) = rows.first() else {
        return Ok(Table::empty());
    };

    let mut columns = Vec::with_capacity(first.column_names.len() + 1);
    columns.push((
        "id".to_string(),
        Table::string_array(rows.iter().map(|r| r.id.as_str())),
    ));

    for (index, name) in first.column_names.iter().enumerate() {
        let values: Vec<f64> = rows.iter().map(|r| r.values[index]).collect();
        columns.push((name.clone(), Table::f64_array(values)));
    }

    Table::from_columns(columns).map_err(|e| PipelineError::invalid(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_directory_is_the_step_one_output() {
        let (path, extension) =
            resolve_network_directory(&NetworkDirectory::Default, "csv", Path::new("/work"))
                .unwrap();
        assert_eq!(path, Path::new("/work/temp/net_dir_mosna"));
        assert_eq!(
            extension,
            Extension::Parquet,
            "step 1 always writes parquet, whatever the input was"
        );
    }

    #[test]
    fn a_custom_directory_keeps_the_configured_extension() {
        let (path, extension) = resolve_network_directory(
            &NetworkDirectory::Custom("nets".into()),
            "csv",
            Path::new("/work"),
        )
        .unwrap();
        assert_eq!(path, Path::new("/work/nets"));
        assert_eq!(extension, Extension::Csv);
    }

    #[test]
    fn an_absolute_custom_directory_replaces_the_working_directory() {
        let (path, _) = resolve_network_directory(
            &NetworkDirectory::Custom("/data/nets".into()),
            "parquet",
            Path::new("/work"),
        )
        .unwrap();
        assert_eq!(path, Path::new("/data/nets"));
    }

    #[test]
    fn an_empty_row_set_builds_an_empty_table() {
        assert_eq!(build_table(&[]).unwrap().n_rows(), 0);
    }
}
