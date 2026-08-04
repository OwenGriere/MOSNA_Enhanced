//! The Browser panel's state — port of `BrowserPanel`.

use std::path::{Path, PathBuf};

use mosna_config::RawConfig;
use mosna_io::{find_sample, find_sample_from_file};
use serde_yaml::Value;

/// One row of the "Files found" table.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleRow {
    pub patient: String,
    pub sample: Option<String>,
    pub nodes_file: String,
    /// `None` when the matching edges file has not been produced yet.
    pub edges_file: Option<String>,
    pub nodes_path: PathBuf,
}

/// The values the Browser panel owns.
///
/// These five keys are deliberately edited in one place and pushed into all
/// three sections, because a mismatch between them is the most common way to
/// end up with an analysis that silently finds no samples.
#[derive(Debug, Clone, Default)]
pub struct BrowserState {
    pub nodes_directory: String,
    /// Empty when `Network directory` is `Default`.
    pub network_directory: String,
    pub network_directory_is_default: bool,
    pub patient_column: String,
    /// Empty means a single-level dataset.
    pub sample_column: String,
    pub extension: String,
    /// Set once the user has chosen a working directory.
    pub working_dir: Option<PathBuf>,
}

impl BrowserState {
    /// Seed the panel from the configuration, as `load_from_config` does.
    pub fn from_config(config: &RawConfig) -> Self {
        let text = |section: &str, key: &str| {
            config
                .get(section, key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };

        let network = config
            .get("Assortativity", "Network directory")
            .and_then(Value::as_str)
            .unwrap_or("Default")
            .to_string();
        let is_default = network.is_empty() || network == "Default";

        Self {
            nodes_directory: text("Tysserand", "Nodes directory"),
            network_directory: if is_default { String::new() } else { network },
            network_directory_is_default: is_default,
            patient_column: text("Tysserand", "Patient column name"),
            sample_column: text("Tysserand", "Sample column name"),
            extension: {
                let tysserand = text("Tysserand", "Extension");
                if tysserand.is_empty() {
                    let assortativity = text("Assortativity", "Extension");
                    if assortativity.is_empty() {
                        "parquet".to_string()
                    } else {
                        assortativity
                    }
                } else {
                    tysserand
                }
            },
            working_dir: None,
        }
    }

    /// The sample column, or `None` for a single-level dataset.
    pub fn sample_column(&self) -> Option<&str> {
        if self.sample_column.trim().is_empty() {
            None
        } else {
            Some(self.sample_column.trim())
        }
    }

    /// Where the reconstructed networks live.
    ///
    /// `Default` is the output of step 1, under the working directory.
    pub fn resolved_network_directory(&self) -> Option<PathBuf> {
        if self.network_directory_is_default {
            self.working_dir
                .as_ref()
                .map(|root| root.join("temp/net_dir_mosna"))
        } else if self.network_directory.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(self.network_directory.trim()))
        }
    }

    /// List the nodes files of the nodes directory.
    pub fn discover_nodes(&self) -> anyhow::Result<Vec<SampleRow>> {
        let directory = PathBuf::from(self.nodes_directory.trim());
        self.discover_in(&directory, &directory)
    }

    /// List the nodes files of the network directory, resolving edges too.
    pub fn discover_networks(&self) -> anyhow::Result<Vec<SampleRow>> {
        let directory = self
            .resolved_network_directory()
            .ok_or_else(|| anyhow::anyhow!("Network directory is not set"))?;
        self.discover_in(&directory, &directory)
    }

    fn discover_in(&self, nodes_dir: &Path, edges_dir: &Path) -> anyhow::Result<Vec<SampleRow>> {
        if self.patient_column.trim().is_empty() {
            anyhow::bail!("Patient column name is required to build the filename pattern.");
        }

        let files = find_sample(
            nodes_dir,
            self.extension.trim(),
            self.patient_column.trim(),
            self.sample_column(),
        )?;

        let mut rows = Vec::with_capacity(files.len());
        for path in files {
            let id =
                find_sample_from_file(&path, self.patient_column.trim(), self.sample_column())?;
            let nodes_file = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();

            // The edges file is only listed when it actually exists: before
            // step 1 has run there is none, and showing a name that is not
            // there would suggest the network is ready.
            let candidate = edges_dir.join(nodes_file.replacen("nodes_", "edges_", 1));
            let edges_file = candidate.is_file().then(|| {
                candidate
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            });

            rows.push(SampleRow {
                patient: id.patient,
                sample: id.sample,
                nodes_file,
                edges_file,
                nodes_path: path,
            });
        }
        Ok(rows)
    }

    /// Push the panel's values into all three sections.
    ///
    /// Port of `_apply_browser_values_to_config`. Writing them everywhere is
    /// what keeps the three steps agreeing on how files are named.
    pub fn apply_to(&self, config: &mut RawConfig) {
        let optional = |text: &str| -> Value {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Value::Null
            } else {
                Value::String(trimmed.to_string())
            }
        };

        config.set(
            "Tysserand",
            "Nodes directory",
            optional(&self.nodes_directory),
        );

        let network = if self.network_directory_is_default {
            Value::String("Default".into())
        } else {
            optional(&self.network_directory)
        };

        for section in ["Tysserand", "Assortativity", "Niche Analysis"] {
            config.set(
                section,
                "Patient column name",
                optional(&self.patient_column),
            );
            config.set(section, "Sample column name", optional(&self.sample_column));
            config.set(section, "Extension", optional(&self.extension));
        }
        for section in ["Assortativity", "Niche Analysis"] {
            config.set(section, "Network directory", network.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Tysserand:
  Nodes directory: /data
  Patient column name: patient
  Sample column name: sample
  Extension: parquet
Assortativity:
  Network directory: Default
  Patient column name: patient
  Sample column name: sample
  Extension: parquet
Niche Analysis:
  Network directory: Default
  Patient column name: patient
  Sample column name: sample
  Extension: parquet
";

    fn state() -> BrowserState {
        BrowserState::from_config(&RawConfig::from_yaml_str(SAMPLE).unwrap())
    }

    #[test]
    fn reads_its_values_from_the_configuration() {
        let browser = state();
        assert_eq!(browser.nodes_directory, "/data");
        assert_eq!(browser.sample_column(), Some("sample"));
        assert!(browser.network_directory_is_default);
    }

    #[test]
    fn a_custom_network_directory_is_kept() {
        let config =
            RawConfig::from_yaml_str("Assortativity:\n  Network directory: /nets\n").unwrap();
        let browser = BrowserState::from_config(&config);
        assert!(!browser.network_directory_is_default);
        assert_eq!(browser.network_directory, "/nets");
    }

    #[test]
    fn the_default_network_directory_needs_a_working_directory() {
        let mut browser = state();
        assert_eq!(browser.resolved_network_directory(), None);

        browser.working_dir = Some(PathBuf::from("/work"));
        assert_eq!(
            browser.resolved_network_directory(),
            Some(PathBuf::from("/work/temp/net_dir_mosna"))
        );
    }

    #[test]
    fn an_empty_sample_column_means_a_single_level_dataset() {
        let mut browser = state();
        browser.sample_column = "   ".into();
        assert_eq!(browser.sample_column(), None);
    }

    #[test]
    fn discovery_pairs_nodes_with_their_edges() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("nodes_patient-1_sample-1.parquet"), b"").unwrap();
        std::fs::write(dir.path().join("edges_patient-1_sample-1.parquet"), b"").unwrap();

        let mut browser = state();
        browser.nodes_directory = dir.path().to_string_lossy().into_owned();

        let rows = browser.discover_nodes().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].patient, "1");
        assert_eq!(
            rows[0].edges_file.as_deref(),
            Some("edges_patient-1_sample-1.parquet")
        );
    }

    #[test]
    fn a_missing_patient_column_is_refused() {
        let mut browser = state();
        browser.patient_column = String::new();
        let error = browser.discover_nodes().unwrap_err();
        assert!(error.to_string().contains("Patient column name"));
    }

    #[test]
    fn applying_writes_every_section() {
        let mut config = RawConfig::from_yaml_str(SAMPLE).unwrap();
        let mut browser = state();
        browser.patient_column = "case".into();
        browser.apply_to(&mut config);

        for section in ["Tysserand", "Assortativity", "Niche Analysis"] {
            assert_eq!(
                config.get(section, "Patient column name"),
                Some(&Value::String("case".into()))
            );
        }
    }

    #[test]
    fn an_empty_sample_column_is_written_as_null() {
        let mut config = RawConfig::from_yaml_str(SAMPLE).unwrap();
        let mut browser = state();
        browser.sample_column = String::new();
        browser.apply_to(&mut config);
        assert_eq!(
            config.get("Tysserand", "Sample column name"),
            Some(&Value::Null),
            "a cleared sample column means a single-level dataset, which the \
             configuration spells as null"
        );
    }
}
