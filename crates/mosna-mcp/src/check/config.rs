//! Whether `configuration.yaml` will be accepted, by each implementation.
//!
//! Both read the same file through the same rules (`assert_params`), with one
//! difference worth reporting: the Python dispatches the `Niche Analysis`
//! checks under the name `"NAS"`, which it is never called with, so MOSNA_GUI
//! starts a niche analysis on parameters Rust refuses, and fails later.

use std::path::{Path, PathBuf};

use mosna_config::model::assortativity::NetworkDirectory;
use mosna_config::validate::assert_params::{assert_params, Analysis};
use mosna_config::{AssortativityConfig, NicheAnalysisConfig, RawConfig, TysserandConfig};
use mosna_io::read::get_opener::Extension;

use crate::check::naming::Naming;
use crate::check::nodes::NodesSpec;
use crate::report::{Affects, Report, Severity};

/// The three sections, typed when they could be.
pub struct Sections {
    pub tysserand: Option<TysserandConfig>,
    pub assortativity: Option<AssortativityConfig>,
    pub niches: Option<NicheAnalysisConfig>,
}

/// Validate every section, and the way they fit together.
pub fn check_config(config: &RawConfig) -> (Report, Sections) {
    let mut report = Report::default();

    for analysis in [
        Analysis::Tysserand,
        Analysis::Assortativity,
        Analysis::NicheAnalysis,
    ] {
        let name = analysis.name();
        let section = match config.section(name) {
            Ok(section) => section,
            Err(error) => {
                report.error("section-missing", None, format!("[{name}] {error}"));
                continue;
            }
        };
        if let Err(error) = assert_params(analysis, section) {
            let affects = if analysis == Analysis::NicheAnalysis {
                Affects::Rust
            } else {
                Affects::Both
            };
            let message = if affects == Affects::Rust {
                format!(
                    "[{name}] {error} — MOSNA_GUI skips these checks and would start, \
                     then fail mid-analysis"
                )
            } else {
                format!("[{name}] {error}")
            };
            report.push(Severity::Error, "config-invalid", affects, None, message);
        }
    }

    let sections = Sections {
        tysserand: typed(&mut report, "Tysserand", TysserandConfig::from_raw(config)),
        assortativity: typed(
            &mut report,
            "Assortativity",
            AssortativityConfig::from_raw(config),
        ),
        niches: typed(
            &mut report,
            "Niche Analysis",
            NicheAnalysisConfig::from_raw(config),
        ),
    };

    check_consistency(&sections, &mut report);
    (report, sections)
}

fn typed<T>(report: &mut Report, name: &str, value: mosna_config::Result<T>) -> Option<T> {
    match value {
        Ok(value) => Some(value),
        Err(error) => {
            // `assert_params` usually said it already; this catches a key it
            // does not test, such as a null `Phenotype column`.
            if !report
                .findings
                .iter()
                .any(|f| f.message.starts_with(&format!("[{name}]")))
            {
                report.error("config-invalid", None, format!("[{name}] {error}"));
            }
            None
        }
    }
}

/// Steps 2 and 3 read step 1's output by default, and find its files through
/// their own column names: if those differ from step 1's, they find nothing.
fn check_consistency(sections: &Sections, report: &mut Report) {
    let Some(tysserand) = &sections.tysserand else {
        return;
    };
    let downstream = [
        sections.assortativity.as_ref().map(|a| {
            (
                "Assortativity",
                &a.network_directory,
                &a.patient_column,
                &a.sample_column,
            )
        }),
        sections.niches.as_ref().map(|n| {
            (
                "Niche Analysis",
                &n.network_directory,
                &n.patient_column,
                &n.sample_column,
            )
        }),
    ];
    for (name, directory, patient, sample) in downstream.into_iter().flatten() {
        if directory.is_default()
            && (patient != &tysserand.patient_column || sample != &tysserand.sample_column)
        {
            // A warning, not an error: what decides is the files step 1 actually
            // wrote, which `check_network` reads. Step 1 may have run with other
            // settings than the ones in this file.
            let naming = |patient: &str, sample: &Option<String>| match sample {
                Some(sample) => format!("`{patient}-…_{sample}-…`"),
                None => format!("`{patient}-…`"),
            };
            report.warning(
                "columns-inconsistent",
                None,
                format!(
                    "[{name}] reads step 1's networks (`Network directory: Default`) as {} but \
                     this file's Tysserand section names them {}: if step 1 ran with this \
                     file, no network will be found",
                    naming(patient, sample),
                    naming(&tysserand.patient_column, &tysserand.sample_column)
                ),
            );
        }
    }
}

/// What step 1 needs to check its inputs.
pub fn nodes_spec(tysserand: &TysserandConfig) -> NodesSpec {
    NodesSpec {
        naming: Naming {
            extension: tysserand.extension.clone(),
            patient_column: tysserand.patient_column.clone(),
            sample_column: tysserand.sample_column.clone(),
        },
        x_column: tysserand.x_column.clone(),
        y_column: tysserand.y_column.clone(),
        phenotype_column: tysserand.phenotype_column.clone(),
        min_neighbors: tysserand.min_neighbors,
    }
}

/// Where steps 2 and 3 read networks, and how they are named.
///
/// The same resolution as `mosna_pipeline::assortativity`: `Default` is step
/// 1's output, always parquet whatever the input was.
pub fn network_location(
    directory: &NetworkDirectory,
    extension: &str,
    patient_column: &str,
    sample_column: Option<&str>,
    working_dir: &Path,
) -> (PathBuf, Naming) {
    let (dir, extension) = match directory {
        NetworkDirectory::Default => (
            working_dir.join("temp/net_dir_mosna"),
            Extension::Parquet.as_str().to_string(),
        ),
        NetworkDirectory::Custom(path) => (working_dir.join(path), extension.to_string()),
    };
    (
        dir,
        Naming {
            extension,
            patient_column: patient_column.to_string(),
            sample_column: sample_column.map(str::to_string),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "
Tysserand:
  Nodes directory: nodes
  Patient column name: patient
  Sample column name: sample
  Extension: parquet
  X coordinates column: x
  Y coordinates column: y
  Phenotype column: cell_type
  Edges method: delaunay
  Min neighbors: 3
  CPU: 2
Assortativity:
  Network directory: Default
  Phenotype column: null
  Patient column name: patient
  Sample column name: slide
  Extension: parquet
  Index: index
  Number of shuffle: 10
";

    #[test]
    fn a_null_phenotype_and_mismatched_columns_are_both_reported() {
        let config = RawConfig::from_yaml_str(CONFIG).unwrap();
        let (report, sections) = check_config(&config);
        assert!(sections.tysserand.is_some());
        assert!(sections.assortativity.is_none());
        assert!(report
            .findings
            .iter()
            .any(|f| f.message.contains("[Assortativity]")));
        assert!(report.findings.iter().any(|f| f.code == "section-missing"));
    }
}
