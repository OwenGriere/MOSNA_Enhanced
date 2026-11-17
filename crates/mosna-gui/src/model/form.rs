//! The Parameters panel's layout — port of `ParametersPanel._add_section_tab`,
//! `_add_niche_general_tab` and `_add_niche_subsection_tab`.

use mosna_config::model::tysserand::{
    DEFAULT_TRIM_DISTANCE_RATIO, DEFAULT_TRIM_MIN_DISTANCE, DEFAULT_TRIM_NUMBER_OF_EDGES,
    TRIM_DISTANCE, TRIM_DISTANCE_RATIO, TRIM_KEYS, TRIM_MIN_DISTANCE, TRIM_NUMBER_OF_EDGES,
};
use mosna_config::RawConfig;
use serde_yaml::Value;

use crate::model::field::{Field, BROWSER_KEYS};

/// Keys of the niche General tab, split into the two boxes the Python builds.
const NICHE_GENERAL_KEYS: &[&str] = &[
    "Network directory",
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
    "init",
];
const NICHE_CLUSTERING_KEYS: &[&str] = &[
    "clusterer_type",
    "k_cluster",
    "resolution",
    "n_clusters",
    "min_cluster_size",
];
const NICHE_NORM_KEYS: &[&str] = &["normalize"];

/// Title of the Tysserand box holding the Delaunay trimming settings.
pub const DELAUNAY_TRIMMING_GROUP: &str = "Delaunay trimming";
/// Title of the Tysserand box below it: the reconnection and the resources.
const TYSSERAND_AFTER_TRIMMING_GROUP: &str = "Reconnection and resources";

/// Sections shown first, in workflow order.
const PREFERRED_ORDER: &[&str] = &["Tysserand", "Assortativity", "Niche Analysis"];

/// A titled box of fields.
#[derive(Debug, Clone)]
pub struct Group {
    pub title: String,
    pub fields: Vec<Field>,
    /// Cleared when the choice that governs the whole box makes it irrelevant;
    /// the panel then does not draw it. Its fields are still written back, so
    /// hiding a box never loses what was typed in it.
    pub visible: bool,
}

impl Group {
    fn new(title: impl Into<String>, fields: Vec<Field>) -> Self {
        Self {
            title: title.into(),
            fields,
            visible: true,
        }
    }
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

    /// Whether a box of fields is currently shown.
    pub fn is_group_visible(&self, section: &str, tab: &str, title: &str) -> bool {
        self.sections
            .iter()
            .find(|s| s.name == section)
            .and_then(|s| s.tabs.iter().find(|t| t.name == tab))
            .and_then(|t| t.groups.iter().find(|g| g.title == title))
            .map(|g| g.visible)
            .unwrap_or(false)
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

    /// Write back one tab of one section, leaving the rest of the document
    /// alone.
    ///
    /// What a sweep needs: the General settings every run of its grid shares,
    /// without the sub-sections, which are the thing it varies. Writing those
    /// back too would take whatever the Parameters panel happened to be showing
    /// and make it the starting point of a grid the user had already described
    /// elsewhere.
    pub fn apply_tab_to(&self, config: &mut RawConfig, section: &str, tab: &str) {
        let Some(section_state) = self.sections.iter().find(|s| s.name == section) else {
            return;
        };
        let Some(tab_state) = section_state.tabs.iter().find(|t| t.name == tab) else {
            return;
        };
        for group in &tab_state.groups {
            for field in &group.fields {
                let value = field.value();
                if tab_state.name == "General" {
                    config.set(&section_state.name, &field.key, value);
                } else {
                    set_nested(
                        config,
                        &section_state.name,
                        &tab_state.name,
                        &field.key,
                        value,
                    );
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
    /// `min_dist`, `metric` and `init` are read by the reducer alone, so
    /// `reducer_type: none` retires them. `n_neighbors` is not: it also caps
    /// leiden's graph degree through `effective_k_cluster`, so it stays
    /// editable with no reducer at all.
    ///
    /// Each rule is neutral when its own picker is absent, so a tab carrying
    /// only one of the two keeps everything the other governs.
    ///
    /// `Edges method` governs a whole box rather than single fields: the
    /// Delaunay trimming settings are shown only while `delaunay` is chosen.
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

                // The trimming box belongs to Delaunay: shown with it, hidden
                // with knn. Neutral, like the rules below, when there is no
                // picker to follow.
                let edges = chosen("Edges method");
                for group in &mut tab.groups {
                    if group.title == DELAUNAY_TRIMMING_GROUP {
                        group.visible = edges.as_deref().is_none_or(|m| m == "delaunay");
                    }
                }

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
                            "dim_clust" | "min_dist" | "metric" | "init" => reduces,
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
            } else if name == "Tysserand" {
                tysserand_groups(&general)
            } else {
                vec![Group::new(
                    "General",
                    general.iter().map(|(k, v)| Field::for_key(k, v)).collect(),
                )]
            },
        });
    }

    for (sub_name, sub_body) in sub_sections {
        let Value::Mapping(inner) = &sub_body else {
            continue;
        };
        let mut entries: Vec<(String, Value)> = inner
            .iter()
            .filter_map(|(k, v)| Some((k.as_str()?.to_string(), v.clone())))
            .collect();

        // `init` is optional in the configuration: one written before the key
        // existed has none and runs with `pca`. The box still shows it, filled
        // with that default and placed after `min_dist`, so it can be changed
        // without editing the file — the same treatment as the trimming keys.
        let is_niche_tab = is_niche && (sub_name == "Aggregated nodes" || sub_name == "Per sample");
        if is_niche_tab
            && entries.iter().any(|(key, _)| key == "reducer_type")
            && !entries.iter().any(|(key, _)| key == "init")
        {
            let at = entries
                .iter()
                .position(|(key, _)| key == "min_dist")
                .map_or(entries.len(), |i| i + 1);
            entries.insert(at, ("init".to_string(), Value::String("pca".to_string())));
        }

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
            vec![Group::new(
                sub_name.clone(),
                entries.iter().map(|(k, v)| Field::for_key(k, v)).collect(),
            )]
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

/// The Tysserand General tab: everything up to `Edges method`, then the
/// Delaunay trimming box right below it, then the rest.
///
/// The four trimming keys are optional in the configuration. When one is
/// missing or null its box still shows it, filled with the tysserand 0.6.0
/// default — the value the run would use anyway — so the user starts from the
/// defaults rather than from an empty box, and saving writes them out.
///
/// The exception is `Trim distance`, whose default is "no global cut-off": its
/// box stays empty, and an empty box is written as null. tysserand's `false`
/// is shown the same way, rather than as a True/False choice.
///
/// A section with no `Edges method` is not a network reconstruction the user
/// can drive from here, and gets a single box as before.
fn tysserand_groups(general: &[(String, Value)]) -> Vec<Group> {
    let Some(split) = general.iter().position(|(key, _)| key == "Edges method") else {
        return vec![Group::new(
            "General",
            general.iter().map(|(k, v)| Field::for_key(k, v)).collect(),
        )];
    };

    let is_trim = |key: &str| TRIM_KEYS.contains(&key);
    let before: Vec<Field> = general[..=split]
        .iter()
        .filter(|(key, _)| !is_trim(key))
        .map(|(k, v)| Field::for_key(k, v))
        .collect();
    let after: Vec<Field> = general[split + 1..]
        .iter()
        .filter(|(key, _)| !is_trim(key))
        .map(|(k, v)| Field::for_key(k, v))
        .collect();

    let configured = |key: &str| {
        general
            .iter()
            .find(|(k, v)| k == key && !v.is_null() && *v != Value::Bool(false))
            .map(|(_, v)| v.clone())
    };
    let defaults = [
        (
            TRIM_DISTANCE_RATIO,
            Value::from(DEFAULT_TRIM_DISTANCE_RATIO),
        ),
        (
            TRIM_NUMBER_OF_EDGES,
            Value::from(DEFAULT_TRIM_NUMBER_OF_EDGES as u64),
        ),
        (TRIM_MIN_DISTANCE, Value::from(DEFAULT_TRIM_MIN_DISTANCE)),
        (TRIM_DISTANCE, Value::Null),
    ];
    let trimming: Vec<Field> = defaults
        .into_iter()
        .map(|(key, default)| Field::for_key(key, &configured(key).unwrap_or(default)))
        .collect();

    let mut groups = vec![
        Group::new("General", before),
        Group::new(DELAUNAY_TRIMMING_GROUP, trimming),
    ];
    if !after.is_empty() {
        groups.push(Group::new(TYSSERAND_AFTER_TRIMMING_GROUP, after));
    }
    groups
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
            groups.push(Group::new(*title, fields));
        }
    }

    let leftover: Vec<Field> = entries
        .iter()
        .filter(|(key, _)| !layout.iter().any(|(_, keys)| keys.contains(&key.as_str())))
        .map(|(key, value)| Field::for_key(key, value))
        .collect();
    if !leftover.is_empty() {
        groups.push(Group::new("Other", leftover));
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

    fn text_of(form: &Form, key: &str) -> String {
        match &form.field("Tysserand", "General", key).unwrap().kind {
            crate::model::field::FieldKind::Text { text } => text.clone(),
            other => panic!("`{key}` should be a text box, got {other:?}"),
        }
    }

    /// The trimming box sits right below `Edges method`, and starts on the
    /// tysserand 0.6.0 defaults when the configuration does not set them.
    #[test]
    fn the_trimming_box_follows_edges_method_with_the_defaults() {
        let form = form();
        let titles: Vec<&str> = form.sections[0].tabs[0]
            .groups
            .iter()
            .map(|g| g.title.as_str())
            .collect();
        assert_eq!(
            titles,
            vec![
                "General",
                DELAUNAY_TRIMMING_GROUP,
                "Reconnection and resources"
            ]
        );
        let general = &form.sections[0].tabs[0].groups[0];
        assert_eq!(general.fields.last().unwrap().key, "Edges method");

        let keys: Vec<&str> = form.sections[0].tabs[0].groups[1]
            .fields
            .iter()
            .map(|f| f.key.as_str())
            .collect();
        assert_eq!(keys, TRIM_KEYS.to_vec());
        assert_eq!(text_of(&form, TRIM_DISTANCE_RATIO), "2.0");
        assert_eq!(text_of(&form, TRIM_NUMBER_OF_EDGES), "3");
        assert_eq!(text_of(&form, TRIM_MIN_DISTANCE), "0.0");
        // No global cut-off by default: an empty box.
        assert_eq!(text_of(&form, TRIM_DISTANCE), "");
    }

    #[test]
    fn the_global_cut_off_is_shown_and_written_back() {
        for (line, shown) in [
            ("  Trim distance: 200\n", "200"),
            ("  Trim distance: false\n", ""),
            ("  Trim distance: null\n", ""),
        ] {
            let yaml = SAMPLE.replace(
                "  Min neighbors: 3\n",
                &format!("  Min neighbors: 3\n{line}"),
            );
            let form = Form::from_config(&RawConfig::from_yaml_str(&yaml).unwrap());
            assert_eq!(text_of(&form, TRIM_DISTANCE), shown, "for `{line}`");
        }

        let original = RawConfig::from_yaml_str(SAMPLE).unwrap();
        let mut form = Form::from_config(&original);
        let mut updated = original.clone();
        form.apply_to(&mut updated);
        assert_eq!(updated.get("Tysserand", TRIM_DISTANCE), Some(&Value::Null));

        form.set_text("Tysserand", "General", TRIM_DISTANCE, "200");
        form.apply_to(&mut updated);
        assert_eq!(
            updated
                .get("Tysserand", TRIM_DISTANCE)
                .and_then(Value::as_f64),
            Some(200.0)
        );
    }

    #[test]
    fn configured_trimming_values_are_shown_instead_of_the_defaults() {
        let yaml = SAMPLE.replace(
            "  Min neighbors: 3\n",
            "  Min neighbors: 3\n  Trim distance ratio: 3.5\n  Trim number of edges: 4\n  Trim min distance: null\n",
        );
        let form = Form::from_config(&RawConfig::from_yaml_str(&yaml).unwrap());
        assert_eq!(text_of(&form, TRIM_DISTANCE_RATIO), "3.5");
        assert_eq!(text_of(&form, TRIM_NUMBER_OF_EDGES), "4");
        // A null falls back to the default, like an absent key.
        assert_eq!(text_of(&form, TRIM_MIN_DISTANCE), "0.0");
        // And the keys are not repeated in the box below.
        let after = &form.sections[0].tabs[0].groups[2];
        assert!(after
            .fields
            .iter()
            .all(|f| !TRIM_KEYS.contains(&f.key.as_str())));
    }

    #[test]
    fn the_trimming_box_is_shown_only_for_delaunay() {
        let mut form = form();
        assert!(form.is_group_visible("Tysserand", "General", DELAUNAY_TRIMMING_GROUP));

        form.set_text("Tysserand", "General", "Edges method", "knn");
        assert!(!form.is_group_visible("Tysserand", "General", DELAUNAY_TRIMMING_GROUP));
        // The rest of the tab stays.
        assert!(form.is_group_visible("Tysserand", "General", "General"));
        assert!(form.is_group_visible("Tysserand", "General", "Reconnection and resources"));

        form.set_text("Tysserand", "General", "Edges method", "delaunay");
        assert!(form.is_group_visible("Tysserand", "General", DELAUNAY_TRIMMING_GROUP));
    }

    /// The defaults reach the file as numbers the validation accepts, and a
    /// hidden box still writes its values.
    #[test]
    fn the_trimming_values_are_written_back() {
        let original = RawConfig::from_yaml_str(SAMPLE).unwrap();
        let mut form = Form::from_config(&original);
        form.set_text("Tysserand", "General", TRIM_DISTANCE_RATIO, "2.5");
        form.set_text("Tysserand", "General", "Edges method", "knn");

        let mut updated = original.clone();
        form.apply_to(&mut updated);
        assert_eq!(
            updated
                .get("Tysserand", TRIM_DISTANCE_RATIO)
                .and_then(Value::as_f64),
            Some(2.5)
        );
        assert_eq!(
            updated
                .get("Tysserand", TRIM_NUMBER_OF_EDGES)
                .and_then(Value::as_i64),
            Some(3)
        );
        assert_eq!(
            updated
                .get("Tysserand", TRIM_MIN_DISTANCE)
                .and_then(Value::as_f64),
            Some(0.0)
        );
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
            "Niche Analysis:\n  Processing method: Aggregated nodes\n  A Stray Key: 1\n",
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

    /// A sweep writes back only the General settings its grid shares, so the
    /// sub-sections it varies are not overwritten by whatever the Parameters
    /// panel happened to be showing.
    #[test]
    fn one_tab_can_be_written_back_on_its_own() {
        let mut form = form();
        form.set_text(
            "Niche Analysis",
            "General",
            "Processing method",
            "Per sample",
        );
        // Different from what the fixture holds, so the assertion below means
        // something.
        form.set_text("Niche Analysis", "Aggregated nodes", "metric", "manhattan");

        let mut config = RawConfig::from_yaml_str(SAMPLE).unwrap();
        form.apply_tab_to(&mut config, "Niche Analysis", "General");

        assert_eq!(
            config
                .get("Niche Analysis", "Processing method")
                .and_then(|v| v.as_str()),
            Some("Per sample"),
            "the General tab was not written"
        );
        assert_eq!(
            config
                .get("Niche Analysis", "Aggregated nodes")
                .and_then(|sub| sub.get("metric"))
                .and_then(|v| v.as_str()),
            Some("cosine"),
            "a sub-section the sweep varies was written back over"
        );
    }

    /// A tab that is not there writes nothing rather than panicking.
    #[test]
    fn writing_back_an_absent_tab_does_nothing() {
        let mut config = RawConfig::from_yaml_str(SAMPLE).unwrap();
        let before = config.to_yaml_string().unwrap();
        form().apply_tab_to(&mut config, "Niche Analysis", "Nowhere");
        assert_eq!(config.to_yaml_string().unwrap(), before);
    }
}
