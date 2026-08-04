//! Typed view of the `Niche Analysis` section.

use serde_yaml::Value;

use crate::error::Result;
use crate::model::assortativity::NetworkDirectory;
use crate::model::niche_params::NicheParams;
use crate::model::raw::RawConfig;
use crate::section;
use crate::value::{
    get_bool_or, get_int_or, get_opt_str, get_str, get_string_or_list, ColumnSelector,
};

/// Whether niches are called on the pooled cohort, per sample, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingMethod {
    AggregatedNodes,
    PerSample,
    Both,
}

impl ProcessingMethod {
    pub fn parse(s: &str) -> Self {
        match s {
            "Aggregated nodes" => ProcessingMethod::AggregatedNodes,
            "Per sample" => ProcessingMethod::PerSample,
            // `niche_analysis.py` treats any other value as "run both".
            _ => ProcessingMethod::Both,
        }
    }

    pub fn with_aggregation(self) -> bool {
        matches!(
            self,
            ProcessingMethod::AggregatedNodes | ProcessingMethod::Both
        )
    }

    pub fn per_sample(self) -> bool {
        matches!(self, ProcessingMethod::PerSample | ProcessingMethod::Both)
    }
}

/// Feature extraction method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NichesMethod {
    /// Neighbors Aggregation Statistics.
    Nas,
    ScanIt,
}

impl NichesMethod {
    pub fn parse(s: &str) -> Self {
        match s {
            "SCAN-IT" => NichesMethod::ScanIt,
            _ => NichesMethod::Nas,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            NichesMethod::Nas => "NAS",
            NichesMethod::ScanIt => "SCAN-IT",
        }
    }
}

/// Parameters driving step 3, the niche analysis.
#[derive(Debug, Clone)]
pub struct NicheAnalysisConfig {
    pub network_directory: NetworkDirectory,
    pub saving_directory: String,
    pub extension: String,
    pub patient_column: String,
    pub sample_column: Option<String>,
    pub processing_method: ProcessingMethod,
    pub niches_method: NichesMethod,
    /// Column holding cell phenotypes, used to describe niche composition.
    pub phenotype_column: Option<String>,
    /// Column(s) aggregated over each neighbourhood.
    pub column_to_aggregate: ColumnSelector,
    pub plot_network: bool,
    pub x_column: Option<String>,
    pub y_column: Option<String>,
    pub cpu: usize,
    pub aggregated: NicheParams,
    pub per_sample: NicheParams,
}

impl NicheAnalysisConfig {
    /// Extract and type-check the section.
    pub fn from_raw(config: &RawConfig) -> Result<Self> {
        let s = config.section(section::NICHE_ANALYSIS)?;
        let name = section::NICHE_ANALYSIS;

        let empty = Value::Mapping(Default::default());
        let aggregated = s.get(section::AGGREGATED_NODES).unwrap_or(&empty);
        let per_sample = s.get(section::PER_SAMPLE).unwrap_or(&empty);

        Ok(Self {
            network_directory: NetworkDirectory::parse(get_opt_str(s, "Network directory")),
            saving_directory: get_str(s, name, "Saving directory")?,
            extension: get_str(s, name, "Extension")?,
            patient_column: get_str(s, name, "Patient column name")?,
            sample_column: get_opt_str(s, "Sample column name"),
            processing_method: ProcessingMethod::parse(&get_str(s, name, "Processing method")?),
            niches_method: NichesMethod::parse(&get_str(s, name, "Niches method")?),
            phenotype_column: get_opt_str(s, "Phenotype column"),
            column_to_aggregate: get_string_or_list(s, name, "Column to aggregate")?,
            plot_network: get_bool_or(s, "Plot Network", false),
            x_column: get_opt_str(s, "X coordinates column for niches"),
            y_column: get_opt_str(s, "Y coordinates column for niches"),
            cpu: get_int_or(s, "CPU", 1).max(1) as usize,
            aggregated: NicheParams::from_value(aggregated),
            per_sample: NicheParams::from_value(per_sample),
        })
    }

    /// `true` when the aggregated columns must be one-hot encoded first, which
    /// is the case exactly when a single column was selected.
    pub fn make_onehot(&self) -> bool {
        self.column_to_aggregate.is_single()
    }

    /// `true` when the network re-plot with niche labels should run.
    ///
    /// Matches `if X is not None and Y is not None and config['Plot Network']`.
    pub fn should_plot_network(&self) -> bool {
        self.plot_network && self.x_column.is_some() && self.y_column.is_some()
    }
}
