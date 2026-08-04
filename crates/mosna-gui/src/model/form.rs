//! The Parameters panel's layout — port of `ParametersPanel._add_section_tab`,
//! `_add_niche_general_tab` and `_add_niche_subsection_tab`.

use mosna_config::RawConfig;
use serde_yaml::Value;

use crate::model::field::{Field, BROWSER_KEYS};

/// Keys of the niche General tab, split into the two boxes the Python builds.
const NICHE_GENERAL_KEYS: &[&str] = &[
    "Network directory",
    "Saving directory",
    "Extension",
    "Patient column name",
    "Sample column name",
    "Processing method",
    "Niches method",
    "Phenotype column",
    "Column to aggregate",
];
const NICHE_REPLOT_KEYS: &[&str] = &[
    "Plot Network",
    "X coordinates column for niches",
    "Y coordinates column for niches",
    "CPU",
];

/// Keys of a niche sub-section, split into its three boxes.
const NICHE_REDUCTION_KEYS: &[&str] = &[
    "reducer_type",
    "dim_clust",
    "n_neighbors",
    "metric",
    "min_dist",
];
const NICHE_CLUSTERING_KEYS: &[&str] = &[
    "clusterer_type",
    "k_cluster",
    "resolution",
    "n_clusters",
    "min_cluster_size",
];
const NICHE_NORM_KEYS: &[&str] = &["normalize"];

/// Sections shown first, in workflow order.
const PREFERRED_ORDER: &[&str] = &["Tysserand", "Assortativity", "Niche Analysis"];

/// A titled box of fields.
#[derive(Debug, Clone)]
pub struct Group {
    pub title: String,
    pub fields: Vec<Field>,
}

/// An inner tab of a section.
#[derive(Debug, Clone)]
pub struct Tab {
    pub name: String,
    pub groups: Vec<Group>,
}

/// A top-level tab, one per configuration section.
#[derive(Debug, Clone)]
pub struct Section {
    pub name: String,
    pub tabs: Vec<Tab>,
}

/// The whole Parameters panel.
#[derive(Debug, Clone, Default)]
pub struct Form {
    pub sections: Vec<Section>,
}

impl Form {
    /// Build the panel from a configuration document.
    ///
    /// Sections keep the file's order except that the three analyses are hoisted
    /// to the front, and the keys the Browser panel owns are left out.
    pub fn from_config(config: &RawConfig) -> Self {
        let mut names = Vec::new();
        for preferred in PREFERRED_ORDER {
            if config.section(preferred).is_ok() {
                names.push((*preferred).to_string());
            }
        }
        for name in config.section_names() {
            if !names.contains(&name) {
                names.push(name);
            }
        }

        let sections = names
            .into_iter()
            .filter_map(|name| {
                let body = config.section(&name).ok()?;
                Some(build_section(&name, body))
            })
            .collect();

        let mut form = Self { sections };
        form.refresh_enablement();
        form
    }

    /// Borrow a field by its position in the layout.
    pub fn field(&self, section: &str, tab: &str, key: &str) -> Option<&Field> {
        self.sections
            .iter()
            .find(|s| s.name == section)?
            .tabs
            .iter()
            .find(|t| t.name == tab)?
            .groups
            .iter()
            .flat_map(|g| g.fields.iter())
            .find(|f| f.key == key)
    }

    /// Borrow a field mutably.
    pub fn field_mut(&mut self, section: &str, tab: &str, key: &str) -> Option<&mut Field> {
        self.sections
            .iter_mut()
            .find(|s| s.name == section)?
            .tabs
            .iter_mut()
            .find(|t| t.name == tab)?
            .groups
            .iter_mut()
            .flat_map(|g| g.fields.iter_mut())
            .find(|f| f.key == key)
    }

    /// Set the text of a field, then re-apply the conditional enabling.
    pub fn set_text(&mut self, section: &str, tab: &str, key: &str, text: &str) {
        if let Some(field) = self.field_mut(section, tab, key) {
            field.set_text(text);
        }
        self.refresh_enablement();
    }

    /// Choose the clustering algorithm of a niche sub-section.
    pub fn set_clusterer(&mut self, section: &str, tab: &str, algorithm: &str) {
        self.set_text(section, tab, "clusterer_type", algorithm);
    }

    /// Choose the dimensionality reduction of a niche sub-section, `none`
    /// included.
    pub fn set_reducer(&mut self, section: &str, tab: &str, reducer: &str) {
        self.set_text(section, tab, "reducer_type", reducer);
    }

    /// Re-apply the conditional enabling after the panel changed a field in
    /// place.
    ///
    /// The panel edits the field it drew directly, so nothing here ran; the two
    /// pickers that decide what the rest of the tab is for have to say so.
    pub fn refresh(&mut self) {
        self.refresh_enablement();
    }

    /// Whether a field is currently editable.
    pub fn is_enabled(&self, section: &str, tab: &str, key: &str) -> bool {
        self.field(section, tab, key)
            .map(|field| field.enabled)
            .unwrap_or(false)
    }

    /// Offer the columns of the selected nodes file to every column picker.
    pub fn set_available_columns(&mut self, columns: &[String]) {
        for section in &mut self.sections {
            for tab in &mut section.tabs {
                for group in &mut tab.groups {
                    for field in &mut group.fields {
                        field.set_available_columns(columns);
                    }
                }
            }
        }
    }

    /// Write every field back into `config`, leaving other keys untouched.
    pub fn apply_to(&self, config: &mut RawConfig) {
        for section in &self.sections {
            for tab in &section.tabs {
                for group in &tab.groups {
                    for field in &group.fields {
                        let value = field.value();
                        if tab.name == "General" {
                            config.set(&section.name, &field.key, value);
                        } else {
                            set_nested(config, &section.name, &tab.name, &field.key, value);
                        }
                    }
                }
            }
        }
    }

    /// Grey out the parameters the chosen algorithms do not use.
    ///
    /// Port of `update_clustering_fields`: `resolution` belongs to leiden,
    /// `n_clusters` to gmm and spectral, `min_cluster_size` to hdbscan. Leaving
    /// them all editable would invite the user to set something with no effect.
    ///
    /// The reduction picker does the same for its own box. `dim_clust`,
    /// `min_dist` and `metric` are read by the reducer alone, so
    /// `reducer_type: none` retires them. `n_neighbors` is not: it also caps
    /// leiden's graph degree through `effective_k_cluster`, so it stays
    /// editable with no reducer at all.
    ///
    /// Each rule is neutral when its own picker is absent, so a tab carrying
    /// only one of the two keeps everything the other governs.
    fn refresh_enablement(&mut self) {
        for section in &mut self.sections {
            for tab in &mut section.tabs {
                let chosen = |key: &str| {
                    tab.groups
                        .iter()
                        .flat_map(|g| g.fields.iter())
                        .find(|f| f.key == key)
                        .and_then(|f| f.choice())
                        .map(str::to_string)
                };
                let algorithm = chosen("clusterer_type");
                let reducer = chosen("reducer_type");
                if algorithm.is_none() && reducer.is_none() {
                    continue;
                }

                let is = |picked: &Option<String>, wanted: &[&str]| {
                    picked
                        .as_deref()
                        .map(|value| wanted.contains(&value))
                        .unwrap_or(true)
                };
                let reduces = !matches!(reducer.as_deref(), Some("none"));

                for group in &mut tab.groups {
                    for field in &mut group.fields {
                        field.enabled = match field.key.as_str() {
                            "resolution" => is(&algorithm, &["leiden"]),
                            "n_clusters" => is(&algorithm, &["gmm", "spectral"]),
                            "min_cluster_size" => is(&algorithm, &["hdbscan"]),
                            "dim_clust" | "min_dist" | "metric" => reduces,
                            _ => true,
                        };
                    }
                }
            }
        }
    }
}

/// Set `config[section][tab][key]`, creating the sub-section if needed.
fn set_nested(config: &mut RawConfig, section: &str, tab: &str, key: &str, value: Value) {
    let node = config.section_mut(section);
    let Value::Mapping(map) = node else { return };

    let entry = map
        .entry(Value::String(tab.to_string()))
        .or_insert_with(|| Value::Mapping(Default::default()));
    if let Value::Mapping(inner) = entry {
        inner.insert(Value::String(key.to_string()), value);
    }
}

/// Build one top-level tab.
fn build_section(name: &str, body: &Value) -> Section {
    let Value::Mapping(map) = body else {
        return Section {
            name: name.to_string(),
            tabs: Vec::new(),
        };
    };

    // Scalar keys go to the General tab; nested mappings become their own tab.
    let mut general: Vec<(String, Value)> = Vec::new();
    let mut sub_sections: Vec<(String, Value)> = Vec::new();
    for (key, value) in map {
        let Some(key) = key.as_str() else { continue };
        if matches!(value, Value::Mapping(_)) {
            sub_sections.push((key.to_string(), value.clone()));
        } else if !BROWSER_KEYS.contains(&key) {
            general.push((key.to_string(), value.clone()));
        }
    }

    let is_niche = name == "Niche Analysis";
    let mut tabs = Vec::new();

    if !general.is_empty() {
        tabs.push(Tab {
            name: "General".to_string(),
            groups: if is_niche {
                grouped(
                    &general,
                    &[
                        ("Niche General Parameters", NICHE_GENERAL_KEYS),
                        ("Replot Network with Niche Labels", NICHE_REPLOT_KEYS),
                    ],
                )
            } else {
                vec![Group {
                    title: "General".to_string(),
                    fields: general.iter().map(|(k, v)| Field::for_key(k, v)).collect(),
                }]
            },
        });
    }

    for (sub_name, sub_body) in sub_sections {
        let Value::Mapping(inner) = &sub_body else {
            continue;
        };
        let entries: Vec<(String, Value)> = inner
            .iter()
            .filter_map(|(k, v)| Some((k.as_str()?.to_string(), v.clone())))
            .collect();

        let groups = if is_niche && (sub_name == "Aggregated nodes" || sub_name == "Per sample") {
            grouped(
                &entries,
                &[
                    ("Reduction", NICHE_REDUCTION_KEYS),
                    ("Clustering", NICHE_CLUSTERING_KEYS),
                    ("Niche Normalisation", NICHE_NORM_KEYS),
                ],
            )
        } else {
            vec![Group {
                title: sub_name.clone(),
                fields: entries.iter().map(|(k, v)| Field::for_key(k, v)).collect(),
            }]
        };

        tabs.push(Tab {
            name: sub_name,
            groups,
        });
    }

    Section {
        name: name.to_string(),
        tabs,
    }
}

/// Split entries into the named boxes, with anything left over under "Other".
fn grouped(entries: &[(String, Value)], layout: &[(&str, &[&str])]) -> Vec<Group> {
    let mut groups = Vec::new();
    let mut claimed: Vec<&str> = Vec::new();

    for (title, keys) in layout {
        let fields: Vec<Field> = entries
            .iter()
            .filter(|(key, _)| keys.contains(&key.as_str()))
            .map(|(key, value)| Field::for_key(key, value))
            .collect();
        if !fields.is_empty() {
            claimed.extend(keys.iter().copied());
            groups.push(Group {
                title: (*title).to_string(),
                fields,
            });
        }
    }

    let leftover: Vec<Field> = entries
        .iter()
        .filter(|(key, _)| !layout.iter().any(|(_, keys)| keys.contains(&key.as_str())))
        .map(|(key, value)| Field::for_key(key, value))
        .collect();
    if !leftover.is_empty() {
        groups.push(Group {
            title: "Other".to_string(),
            fields: leftover,
        });
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Tysserand:
  Nodes directory: /data
  Patient column name: patient
  Extension: parquet
  Edges method: delaunay
  Min neighbors: 3
Niche Analysis:
  Saving directory: run
  Processing method: Aggregated nodes
  Plot Network: true
  CPU: 8
  Aggregated nodes:
    reducer_type: umap
    dim_clust: 2
    n_neighbors: 20
    metric: cosine
    min_dist: 0.0
    clusterer_type: leiden
    resolution: 0.05
    n_clusters: 6
    min_cluster_size: 100
    normalize: total
";

    fn form() -> Form {
        Form::from_config(&RawConfig::from_yaml_str(SAMPLE).unwrap())
    }

    #[test]
    fn general_keys_land_in_a_general_tab() {
        let form = form();
        let tysserand = &form.sections[0];
        assert_eq!(tysserand.name, "Tysserand");
        assert_eq!(tysserand.tabs[0].name, "General");
        assert!(form.field("Tysserand", "General", "Edges method").is_some());
    }

    #[test]
    fn browser_keys_are_dropped() {
        let form = form();
        assert!(form
            .field("Tysserand", "General", "Nodes directory")
            .is_none());
        assert!(form.field("Tysserand", "General", "Extension").is_none());
    }

    #[test]
    fn leftover_keys_land_under_other() {
        let config = RawConfig::from_yaml_str(
            "Niche Analysis:\n  Saving directory: run\n  A Stray Key: 1\n",
        )
        .unwrap();
        let form = Form::from_config(&config);
        let titles: Vec<&str> = form.sections[0].tabs[0]
            .groups
            .iter()
            .map(|g| g.title.as_str())
            .collect();
        assert!(titles.contains(&"Other"), "{titles:?}");
    }

    #[test]
    fn enablement_follows_the_algorithm() {
        let mut form = form();
        // The sample starts on leiden.
        assert!(form.is_enabled("Niche Analysis", "Aggregated nodes", "resolution"));
        assert!(!form.is_enabled("Niche Analysis", "Aggregated nodes", "n_clusters"));

        form.set_clusterer("Niche Analysis", "Aggregated nodes", "spectral");
        assert!(form.is_enabled("Niche Analysis", "Aggregated nodes", "n_clusters"));
        assert!(!form.is_enabled("Niche Analysis", "Aggregated nodes", "resolution"));
    }

    /// Turning the reduction off must grey out the settings that only the
    /// reducer reads, or the panel invites the user to tune something the run
    /// will never look at.
    #[test]
    fn no_reduction_greys_out_the_reduction_parameters() {
        let mut form = form();
        for key in ["dim_clust", "min_dist", "metric"] {
            assert!(
                form.is_enabled("Niche Analysis", "Aggregated nodes", key),
                "`{key}` should start editable"
            );
        }

        form.set_reducer("Niche Analysis", "Aggregated nodes", "none");
        for key in ["dim_clust", "min_dist", "metric"] {
            assert!(
                !form.is_enabled("Niche Analysis", "Aggregated nodes", key),
                "`{key}` is only read by the reducer"
            );
        }

        // The picker itself has to stay live, or the choice could not be undone.
        assert!(form.is_enabled("Niche Analysis", "Aggregated nodes", "reducer_type"));

        form.set_reducer("Niche Analysis", "Aggregated nodes", "umap");
        assert!(form.is_enabled("Niche Analysis", "Aggregated nodes", "dim_clust"));
    }

    /// Choosing no reduction must reach the file as the string `none`.
    ///
    /// The panel's own coercion reads `none` as "no value", so this used to be
    /// written as YAML's null — a document the validator refuses for a choice
    /// the panel offered.
    #[test]
    fn choosing_no_reduction_is_written_back_as_a_string() {
        let original = RawConfig::from_yaml_str(SAMPLE).unwrap();
        let mut form = Form::from_config(&original);
        form.set_reducer("Niche Analysis", "Aggregated nodes", "none");

        let mut updated = original.clone();
        form.apply_to(&mut updated);

        let aggregated = updated
            .section("Niche Analysis")
            .unwrap()
            .get("Aggregated nodes")
            .unwrap();
        assert_eq!(
            aggregated.get("reducer_type"),
            Some(&Value::String("none".into())),
            "`none` must stay the name of a reducer, not become a missing value"
        );
    }

    /// `n_neighbors` is not the reducer's alone: `effective_k_cluster` caps
    /// leiden's graph degree with it, so it stays editable without a reducer.
    #[test]
    fn no_reduction_leaves_the_shared_parameters_alone() {
        let mut form = form();
        form.set_reducer("Niche Analysis", "Aggregated nodes", "none");
        assert!(form.is_enabled("Niche Analysis", "Aggregated nodes", "n_neighbors"));
        assert!(form.is_enabled("Niche Analysis", "Aggregated nodes", "resolution"));
        assert!(form.is_enabled("Niche Analysis", "Aggregated nodes", "clusterer_type"));
    }

    /// The two rules are independent: switching the reducer off must not
    /// re-enable a clustering parameter the algorithm does not use.
    #[test]
    fn the_two_enablement_rules_do_not_interfere() {
        let mut form = form();
        form.set_clusterer("Niche Analysis", "Aggregated nodes", "gmm");
        form.set_reducer("Niche Analysis", "Aggregated nodes", "none");
        assert!(form.is_enabled("Niche Analysis", "Aggregated nodes", "n_clusters"));
        assert!(!form.is_enabled("Niche Analysis", "Aggregated nodes", "resolution"));
    }

    #[test]
    fn writing_back_reaches_nested_sub_sections() {
        let original = RawConfig::from_yaml_str(SAMPLE).unwrap();
        let mut form = Form::from_config(&original);
        form.set_text("Niche Analysis", "Aggregated nodes", "metric", "euclidean");

        let mut updated = original.clone();
        form.apply_to(&mut updated);

        let niche = updated.section("Niche Analysis").unwrap();
        let aggregated = niche.get("Aggregated nodes").unwrap();
        assert_eq!(
            aggregated.get("metric"),
            Some(&Value::String("euclidean".into()))
        );
    }

    #[test]
    fn writing_back_leaves_browser_keys_alone() {
        let original = RawConfig::from_yaml_str(SAMPLE).unwrap();
        let mut updated = original.clone();
        Form::from_config(&original).apply_to(&mut updated);
        assert_eq!(
            updated.get("Tysserand", "Nodes directory"),
            Some(&Value::String("/data".into())),
            "the Browser panel owns this key, the form must not clear it"
        );
    }

    #[test]
    fn offering_columns_reaches_every_picker() {
        let config = RawConfig::from_yaml_str(
            "Niche Analysis:\n  Phenotype column: null\n  Column to aggregate: null\n",
        )
        .unwrap();
        let mut form = Form::from_config(&config);
        form.set_available_columns(&["Cluster".into(), "Type".into()]);

        let field = form
            .field("Niche Analysis", "General", "Phenotype column")
            .unwrap();
        match &field.kind {
            crate::model::field::FieldKind::ColumnPicker { columns, .. } => {
                assert_eq!(columns.len(), 2)
            }
            other => panic!("expected a column picker, got {other:?}"),
        }
    }
}
