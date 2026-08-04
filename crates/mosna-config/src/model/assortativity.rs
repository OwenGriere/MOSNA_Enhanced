//! Typed view of the `Assortativity` section.

use crate::error::Result;
use crate::model::raw::RawConfig;
use crate::section;
use crate::value::{get_bool_or, get_int, get_opt_str, get_str};

/// Where the reconstructed networks are read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkDirectory {
    /// `temp/net_dir_mosna` under the working directory, always parquet.
    Default,
    /// A user-supplied directory, read with the configured extension.
    Custom(String),
}

impl NetworkDirectory {
    pub fn parse(value: Option<String>) -> Self {
        match value.as_deref() {
            None | Some("") | Some("Default") => NetworkDirectory::Default,
            Some(path) => NetworkDirectory::Custom(path.to_string()),
        }
    }

    pub fn is_default(&self) -> bool {
        matches!(self, NetworkDirectory::Default)
    }
}

/// Parameters driving step 2, the assortativity analysis.
#[derive(Debug, Clone)]
pub struct AssortativityConfig {
    pub network_directory: NetworkDirectory,
    pub phenotype_column: String,
    pub patient_column: String,
    pub sample_column: Option<String>,
    pub extension: String,
    /// Column promoted to the node index, or `None` to keep positional order.
    pub index: Option<String>,
    pub n_shuffle: usize,
    /// When set, the run is a timing probe: it shuffles only 20 times and
    /// writes nothing, so the GUI can extrapolate the full run time.
    pub randomization_diagnostic: bool,
}

impl AssortativityConfig {
    /// Extract and type-check the section.
    pub fn from_raw(config: &RawConfig) -> Result<Self> {
        let s = config.section(section::ASSORTATIVITY)?;
        let name = section::ASSORTATIVITY;
        Ok(Self {
            network_directory: NetworkDirectory::parse(get_opt_str(s, "Network directory")),
            phenotype_column: get_str(s, name, "Phenotype column")?,
            patient_column: get_str(s, name, "Patient column name")?,
            sample_column: get_opt_str(s, "Sample column name"),
            extension: get_str(s, name, "Extension")?,
            // `Index: index` is the sentinel the GUI writes for "use the
            // positional index", so it is not a column name.
            index: get_opt_str(s, "Index").filter(|v| v != "index"),
            n_shuffle: get_int(s, name, "Number of shuffle")?.max(0) as usize,
            randomization_diagnostic: get_bool_or(s, "Randomization diagnostic", false),
        })
    }

    /// Number of shuffles actually performed, honouring the diagnostic mode.
    pub fn effective_shuffles(&self) -> usize {
        if self.randomization_diagnostic {
            20
        } else {
            self.n_shuffle
        }
    }
}
