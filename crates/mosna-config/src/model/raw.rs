//! The untyped, order-preserving view of `configuration.yaml`.

use serde_yaml::{Mapping, Value};

use crate::error::{ConfigError, Result};
use crate::model::yaml_emit;

/// The whole configuration document, kept as an ordered tree.
///
/// This is what the GUI edits. Sections and keys keep the order they had in the
/// file, and keys the Rust code knows nothing about survive a load/save cycle
/// untouched — the same guarantee the Python GUI offers by editing the `dict`
/// returned by `yaml.safe_load` in place.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawConfig {
    root: Mapping,
}

impl RawConfig {
    /// Parse a YAML document.
    pub fn from_yaml_str(text: &str) -> std::result::Result<Self, serde_yaml::Error> {
        let value: Value = serde_yaml::from_str(text)?;
        let root = match value {
            Value::Mapping(map) => map,
            // An empty file yields `null`; Python's `yaml.safe_load(f) or {}`
            // turns that into an empty dict.
            Value::Null => Mapping::new(),
            other => {
                let mut map = Mapping::new();
                map.insert(Value::String("value".into()), other);
                map
            }
        };
        Ok(Self { root })
    }

    /// Render the document back to YAML, PyYAML-style.
    pub fn to_yaml_string(&self) -> std::result::Result<String, serde_yaml::Error> {
        Ok(yaml_emit::emit(&Value::Mapping(self.root.clone())))
    }

    /// Borrow a top-level section such as `"Tysserand"`.
    pub fn section(&self, name: &str) -> Result<&Value> {
        self.root
            .get(Value::String(name.to_string()))
            .ok_or_else(|| ConfigError::MissingSection(name.to_string()))
    }

    /// Borrow a top-level section mutably, creating it if absent.
    pub fn section_mut(&mut self, name: &str) -> &mut Value {
        let key = Value::String(name.to_string());
        if !self.root.contains_key(&key) {
            self.root
                .insert(key.clone(), Value::Mapping(Mapping::new()));
        }
        self.root.get_mut(&key).expect("just inserted")
    }

    /// Section names, in file order.
    pub fn section_names(&self) -> Vec<String> {
        self.root
            .keys()
            .filter_map(|k| k.as_str().map(str::to_string))
            .collect()
    }

    /// Set `section[key] = value`, creating the section when needed.
    pub fn set(&mut self, section: &str, key: &str, value: Value) {
        if let Value::Mapping(map) = self.section_mut(section) {
            map.insert(Value::String(key.to_string()), value);
        }
    }

    /// Read `section[key]`.
    pub fn get(&self, section: &str, key: &str) -> Option<&Value> {
        self.root.get(Value::String(section.to_string()))?.get(key)
    }

    /// Borrow the whole document.
    pub fn as_mapping(&self) -> &Mapping {
        &self.root
    }

    /// Borrow the whole document mutably.
    pub fn as_mapping_mut(&mut self) -> &mut Mapping {
        &mut self.root
    }

    /// Wrap an existing mapping.
    pub fn from_mapping(root: Mapping) -> Self {
        Self { root }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Tysserand:
  Nodes directory: /data/mIF
  Sample column name: sample
  CPU: 20
Niche Analysis:
  Aggregated nodes:
    min_dist: 0.0
    order: '1'
    stat_names: [mean, std]
";

    #[test]
    fn preserves_order_and_formatting() {
        let cfg = RawConfig::from_yaml_str(SAMPLE).unwrap();
        assert_eq!(cfg.to_yaml_string().unwrap(), SAMPLE);
    }

    #[test]
    fn section_names_follow_file_order() {
        let cfg = RawConfig::from_yaml_str(SAMPLE).unwrap();
        assert_eq!(cfg.section_names(), vec!["Tysserand", "Niche Analysis"]);
    }

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let mut cfg = RawConfig::from_yaml_str(SAMPLE).unwrap();
        cfg.set("Tysserand", "CPU", Value::Number(8.into()));
        let text = cfg.to_yaml_string().unwrap();
        assert!(text.contains("Nodes directory: /data/mIF"));
        assert!(text.contains("CPU: 8"));
    }

    #[test]
    fn empty_document_loads_as_empty_mapping() {
        let cfg = RawConfig::from_yaml_str("").unwrap();
        assert!(cfg.section_names().is_empty());
    }
}
