//! One editable configuration key — port of `ParametersPanel._get_widget` and
//! `ParametersPanel.parse_value`.

use serde_yaml::Value;

/// Keys that pick a single column of the selected nodes file.
pub const COLUMN_KEYS: &[&str] = &[
    "X coordinates column",
    "Y coordinates column",
    "Phenotype column",
    "X coordinates column for niches",
    "Y coordinates column for niches",
];

/// Keys that pick one or several columns.
pub const MULTI_COLUMN_KEYS: &[&str] = &["Column to aggregate"];

/// Keys the Browser panel owns; they never appear in the Parameters panel.
pub const BROWSER_KEYS: &[&str] = &[
    "Nodes directory",
    "Network directory",
    "Patient column name",
    "Sample column name",
    "Extension",
];

/// The placeholder shown by a picker with nothing chosen.
pub const NO_SELECTION: &str = "— select column —";

/// Fixed option lists, as hard-coded in `ParametersPanel.FIXED_OPTIONS`.
pub fn fixed_options(key: &str) -> Option<&'static [&'static str]> {
    Some(match key {
        "Niches method" => &["NAS", "SCAN-IT"],
        "Processing method" => &["Aggregated nodes", "Per sample"],
        "reducer_type" => &["umap"],
        "clusterer_type" => &["leiden", "ecg", "spectral", "gmm", "hdbscan"],
        "order" => &["1", "2"],
        "metric" => &["manhattan", "euclidean", "cosine"],
        "Edges method" => &["delaunay", "knn"],
        "stat_funcs" => &["np.mean,np.std", "np.mean"],
        "normalize" => &["total", "niche", "obs", "clr", "niche&obs", "all"],
        _ => return None,
    })
}

/// Explanations shown when hovering a parameter, from
/// `ParametersPanel.PARAM_TOOLTIPS`.
pub fn tooltip(key: &str) -> Option<&'static str> {
    Some(match key {
        "order" => "Neighborhood order for NAS aggregation.\n1 = direct neighbors only, 2 = includes 2nd-degree neighbors.",
        "stat_funcs" => "Statistical functions applied to neighbor features (e.g. np.mean, np.std).",
        "stat_names" => "Names associated with stat_funcs, used to label output columns.",
        "clusterer_type" => "Clustering algorithm used to define niches: gmm, leiden, hdbscan, spectral, ecg.",
        "metric" => "Distance metric for comparing observations: euclidean, manhattan or cosine.",
        "normalize" => "Normalization applied to niche features before the model:\ntotal, niche, obs, clr, niche&obs, all.",
        "reducer_type" => "Dimensionality reduction applied before clustering (currently: umap).",
        "n_neighbors" => "Number of neighbors used to build the local graph structure (UMAP / KNN).",
        "min_dist" => "UMAP parameter: how tightly points can cluster in reduced space.\nSmaller = tighter groups.",
        "dim_clust" => "Number of dimensions kept after reduction, used for clustering.",
        "k_cluster" => "Neighbors used during the clustering graph construction step.",
        "n_clusters" => "Number of clusters to produce (gmm, spectral).",
        "resolution" => "Leiden granularity. Lower → fewer clusters, higher → more clusters.",
        "min_cluster_size" => "HDBSCAN minimum cluster size. Smaller allows rarer clusters.",
        "Number of shuffle" => "Number of randomizations to build the null distribution for assortativity.",
        "Edges method" => "Delaunay: triangulation-based. KNN: k nearest neighbours.",
        "Min neighbors" => "Minimum number of neighbors for KNN edge generation.",
        "CPU" => "Number of CPU cores for parallel processing.",
        _ => return None,
    })
}

/// The kind of editor a key gets.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    /// A free-text box.
    Text { text: String },
    /// A drop-down over a fixed list.
    Choice {
        options: Vec<String>,
        selected: usize,
    },
    /// A drop-down over the columns of the selected nodes file.
    ColumnPicker {
        columns: Vec<String>,
        /// `None` means nothing is selected.
        selected: Option<String>,
    },
    /// A menu allowing several columns at once.
    MultiColumnPicker {
        columns: Vec<String>,
        selected: Vec<String>,
    },
    /// `index` or a chosen column.
    IndexPicker {
        columns: Vec<String>,
        /// `None` means the positional index.
        custom: Option<String>,
    },
    /// A path with a browse button.
    DirectoryPath { path: String },
}

/// One key of the configuration, with its editor state.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub key: String,
    pub kind: FieldKind,
    pub tooltip: Option<&'static str>,
    /// Cleared when the chosen algorithm makes the parameter irrelevant.
    pub enabled: bool,
}

impl Field {
    /// Choose the editor for `key`, seeded from its current `value`.
    ///
    /// The branch order matters and is the Python's: a key in
    /// `MULTI_COLUMN_FIELDS` wins over `COLUMN_FIELDS`, which wins over the
    /// boolean check, which wins over `FIXED_OPTIONS`.
    pub fn for_key(key: &str, value: &Value) -> Self {
        let kind = if MULTI_COLUMN_KEYS.contains(&key) {
            FieldKind::MultiColumnPicker {
                columns: Vec::new(),
                selected: selected_columns(value),
            }
        } else if COLUMN_KEYS.contains(&key) {
            FieldKind::ColumnPicker {
                columns: Vec::new(),
                selected: value.as_str().map(str::to_string),
            }
        } else if let Value::Bool(state) = value {
            FieldKind::Choice {
                options: vec!["True".into(), "False".into()],
                selected: usize::from(!*state),
            }
        } else if let Some(options) = fixed_options(key) {
            let options: Vec<String> = options.iter().map(|o| o.to_string()).collect();
            let selected = value
                .as_str()
                .and_then(|current| options.iter().position(|o| o == current))
                .unwrap_or(0);
            FieldKind::Choice { options, selected }
        } else if key == "Index" {
            FieldKind::IndexPicker {
                columns: Vec::new(),
                // `index` is the sentinel for "use the positional index".
                custom: value.as_str().filter(|v| *v != "index").map(str::to_string),
            }
        } else if key == "Saving directory" {
            FieldKind::DirectoryPath {
                path: value.as_str().unwrap_or_default().to_string(),
            }
        } else {
            FieldKind::Text {
                text: render(value),
            }
        };

        Self {
            key: key.to_string(),
            kind,
            tooltip: tooltip(key),
            enabled: true,
        }
    }

    /// The value to write back into the configuration.
    ///
    /// Port of `parse_value`, including the order of the coercions: `order` is
    /// exempt, then null-like text, then booleans, then integers, then floats,
    /// then bracketed literals, then plain text.
    pub fn value(&self) -> Value {
        match &self.kind {
            FieldKind::Text { text } => parse_text(&self.key, text),
            FieldKind::Choice { options, selected } => {
                parse_text(&self.key, options.get(*selected).map_or("", String::as_str))
            }
            FieldKind::ColumnPicker { selected, .. } => match selected {
                Some(column) if !column.is_empty() && !column.starts_with('—') => {
                    Value::String(column.clone())
                }
                _ => Value::Null,
            },
            FieldKind::MultiColumnPicker { selected, .. } => match selected.len() {
                0 => Value::Null,
                // A single column collapses to a scalar, which is what makes
                // `make_onehot` fire on the pipeline side.
                1 => Value::String(selected[0].clone()),
                _ => Value::Sequence(selected.iter().cloned().map(Value::String).collect()),
            },
            FieldKind::IndexPicker { custom, .. } => match custom {
                Some(column) if !column.is_empty() && !column.starts_with('—') => {
                    Value::String(column.clone())
                }
                _ => Value::String("index".into()),
            },
            FieldKind::DirectoryPath { path } => {
                if path.is_empty() {
                    Value::Null
                } else {
                    Value::String(path.clone())
                }
            }
        }
    }

    /// Replace the text of a text or choice field.
    pub fn set_text(&mut self, text: &str) {
        match &mut self.kind {
            FieldKind::Text { text: current } => *current = text.to_string(),
            FieldKind::Choice { options, selected } => {
                if let Some(position) = options.iter().position(|o| o == text) {
                    *selected = position;
                }
            }
            FieldKind::DirectoryPath { path } => *path = text.to_string(),
            FieldKind::ColumnPicker { selected, .. } => {
                *selected = Some(text.to_string());
            }
            FieldKind::IndexPicker { custom, .. } => {
                *custom = Some(text.to_string());
            }
            FieldKind::MultiColumnPicker { selected, .. } => {
                *selected = vec![text.to_string()];
            }
        }
    }

    /// Offer a new column list, keeping the current choice when it survives.
    pub fn set_available_columns(&mut self, available: &[String]) {
        match &mut self.kind {
            FieldKind::ColumnPicker { columns, selected } => {
                *columns = available.to_vec();
                if let Some(current) = selected {
                    if !available.contains(current) {
                        *selected = None;
                    }
                }
            }
            FieldKind::MultiColumnPicker { columns, selected } => {
                *columns = available.to_vec();
                selected.retain(|column| available.contains(column));
            }
            FieldKind::IndexPicker { columns, custom } => {
                *columns = available.to_vec();
                if let Some(current) = custom {
                    if !available.contains(current) {
                        *custom = None;
                    }
                }
            }
            _ => {}
        }
    }

    /// Set the chosen columns of a multi-column picker.
    pub fn set_selected_columns(&mut self, chosen: &[String]) {
        if let FieldKind::MultiColumnPicker { selected, .. } = &mut self.kind {
            *selected = chosen.to_vec();
        }
    }

    /// Choose a column for the index picker.
    pub fn set_custom_index(&mut self, column: &str) {
        if let FieldKind::IndexPicker { custom, .. } = &mut self.kind {
            *custom = Some(column.to_string());
        }
    }

    /// The currently chosen option of a choice field.
    pub fn choice(&self) -> Option<&str> {
        match &self.kind {
            FieldKind::Choice { options, selected } => options.get(*selected).map(String::as_str),
            _ => None,
        }
    }
}

/// The columns a `Column to aggregate` value names.
fn selected_columns(value: &Value) -> Vec<String> {
    match value {
        Value::String(one) => vec![one.clone()],
        Value::Sequence(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// Render a value for a text box.
fn render(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(state) => state.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Sequence(items) => {
            let rendered: Vec<String> = items.iter().map(render).collect();
            format!("[{}]", rendered.join(", "))
        }
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

/// Port of the coercion ladder at the end of `parse_value`.
fn parse_text(key: &str, raw: &str) -> Value {
    let text = raw.trim();

    // `assert_params` requires `order` to be a string; coercing `'1'` to the
    // integer 1 would make the configuration invalid.
    if key == "order" {
        return Value::String(text.to_string());
    }

    let lowered = text.to_ascii_lowercase();
    if matches!(lowered.as_str(), "" | "none" | "null") {
        return Value::Null;
    }
    if lowered == "true" {
        return Value::Bool(true);
    }
    if lowered == "false" {
        return Value::Bool(false);
    }
    if let Ok(integer) = text.parse::<i64>() {
        return Value::Number(integer.into());
    }
    if let Ok(float) = text.parse::<f64>() {
        return Value::Number(serde_yaml::Number::from(float));
    }
    if text.starts_with('[') || text.starts_with('{') || text.starts_with('(') {
        // `ast.literal_eval` on the Python side; YAML parses the same flow
        // sequences and mappings.
        if let Ok(parsed) = serde_yaml::from_str::<Value>(text) {
            if matches!(parsed, Value::Sequence(_) | Value::Mapping(_)) {
                return parsed;
            }
        }
    }
    Value::String(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boolean_starts_on_its_current_state() {
        let yes = Field::for_key("Plot Network", &Value::Bool(true));
        assert_eq!(yes.choice(), Some("True"));
        let no = Field::for_key("Plot Network", &Value::Bool(false));
        assert_eq!(no.choice(), Some("False"));
    }

    #[test]
    fn a_choice_starts_on_its_configured_option() {
        let field = Field::for_key("metric", &Value::String("cosine".into()));
        assert_eq!(field.choice(), Some("cosine"));
    }

    #[test]
    fn an_unknown_option_falls_back_to_the_first() {
        let field = Field::for_key("metric", &Value::String("mahalanobis".into()));
        assert_eq!(field.choice(), Some("manhattan"));
    }

    #[test]
    fn a_boolean_choice_reads_back_as_a_boolean() {
        let mut field = Field::for_key("Plot Network", &Value::Bool(true));
        assert_eq!(field.value(), Value::Bool(true));
        field.set_text("False");
        assert_eq!(field.value(), Value::Bool(false));
    }

    #[test]
    fn a_column_choice_that_disappears_is_cleared() {
        let mut field = Field::for_key("Phenotype column", &Value::String("Cluster".into()));
        field.set_available_columns(&["Type".into()]);
        assert_eq!(field.value(), Value::Null);
    }

    #[test]
    fn a_multi_column_choice_drops_columns_that_disappear() {
        let mut field = Field::for_key(
            "Column to aggregate",
            &Value::Sequence(vec![Value::String("A".into()), Value::String("B".into())]),
        );
        field.set_available_columns(&["A".into()]);
        assert_eq!(field.value(), Value::String("A".into()));
    }

    #[test]
    fn a_float_keeps_its_type() {
        let mut field = Field::for_key("min_dist", &Value::Null);
        field.set_text("0.0");
        assert!(field.value().as_f64().is_some());
        assert!(!field.value().is_i64(), "0.0 must not become the integer 0");
    }

    #[test]
    fn text_that_is_not_a_number_stays_text() {
        let mut field = Field::for_key("Saving directory key", &Value::Null);
        field.set_text("niche_cluster");
        assert_eq!(field.value(), Value::String("niche_cluster".into()));
    }

    #[test]
    fn a_sequence_renders_in_flow_style() {
        let field = Field::for_key(
            "stat_names",
            &Value::Sequence(vec![
                Value::String("mean".into()),
                Value::String("std".into()),
            ]),
        );
        match &field.kind {
            FieldKind::Text { text } => assert_eq!(text, "[mean, std]"),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn every_browser_key_is_recognised() {
        assert_eq!(BROWSER_KEYS.len(), 5);
        assert!(BROWSER_KEYS.contains(&"Extension"));
    }
}
