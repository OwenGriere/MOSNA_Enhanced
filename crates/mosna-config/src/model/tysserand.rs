//! Typed view of the `Tysserand` section.

use crate::error::Result;
use crate::model::raw::RawConfig;
use crate::section;
use crate::value::{get_int, get_opt_str, get_str};

/// Method used to draw the edges of a spatial network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgesMethod {
    Delaunay,
    Knn,
}

impl EdgesMethod {
    pub fn parse(s: &str) -> Self {
        match s {
            "knn" => EdgesMethod::Knn,
            // `delaunay` is the default in the shipped configuration, and the
            // Python `link_solitaries` falls back to it for any other value.
            _ => EdgesMethod::Delaunay,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EdgesMethod::Delaunay => "delaunay",
            EdgesMethod::Knn => "knn",
        }
    }
}

/// Parameters driving step 1, the spatial network reconstruction.
#[derive(Debug, Clone)]
pub struct TysserandConfig {
    pub nodes_directory: String,
    pub patient_column: String,
    /// `None` when the dataset has a single identifier level.
    pub sample_column: Option<String>,
    pub extension: String,
    pub x_column: String,
    pub y_column: String,
    pub phenotype_column: String,
    pub edges_method: EdgesMethod,
    pub min_neighbors: usize,
    pub cpu: usize,
}

impl TysserandConfig {
    /// Extract and type-check the section.
    pub fn from_raw(config: &RawConfig) -> Result<Self> {
        let s = config.section(section::TYSSERAND)?;
        let name = section::TYSSERAND;
        Ok(Self {
            nodes_directory: get_str(s, name, "Nodes directory")?,
            patient_column: get_str(s, name, "Patient column name")?,
            sample_column: get_opt_str(s, "Sample column name"),
            extension: get_str(s, name, "Extension")?,
            x_column: get_str(s, name, "X coordinates column")?,
            y_column: get_str(s, name, "Y coordinates column")?,
            phenotype_column: get_str(s, name, "Phenotype column")?,
            edges_method: EdgesMethod::parse(&get_str(s, name, "Edges method")?),
            min_neighbors: get_int(s, name, "Min neighbors")?.max(0) as usize,
            cpu: get_int(s, name, "CPU")?.max(1) as usize,
        })
    }
}
