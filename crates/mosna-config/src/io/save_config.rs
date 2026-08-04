//! Port of `package/utils/save_config.py::save_config`.

use std::path::Path;

use serde_yaml::Value;

use crate::error::{ConfigError, Result};

/// Dump the *Niche Analysis* section next to its results as `parameters.json`.
///
/// Behaviour is kept identical to the Python original, including the fact that
/// it drops the sub-section matching the selected processing method:
///
/// ```python
/// if config['Processing method'] == "Aggregated nodes":
///     config.pop("Aggregated nodes", None)
/// elif config['Processing method'] == "Per sample":
///     config.pop("Per sample", None)
/// ```
///
/// The dropped block is the one that *was* used for the run, so the file
/// records the general parameters plus the settings of the method that was not
/// selected. That is surprising, but reproducing it is what keeps the two
/// implementations interchangeable; changing it would silently alter an
/// artefact users may already be parsing.
pub fn save_config(save_path: impl AsRef<Path>, config: &Value) -> Result<()> {
    let mut results = config.clone();

    let processing_method = config
        .get("Processing method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if let Value::Mapping(map) = &mut results {
        match processing_method.as_str() {
            "Aggregated nodes" => {
                map.shift_remove(Value::String("Aggregated nodes".into()));
            }
            "Per sample" => {
                map.shift_remove(Value::String("Per sample".into()));
            }
            _ => {}
        }
    }

    let json = yaml_to_json(&results);
    let text = serde_json::to_string_pretty(&json)?;

    let dir = save_path.as_ref();
    std::fs::create_dir_all(dir).map_err(|source| ConfigError::Write {
        path: dir.to_path_buf(),
        source,
    })?;
    let file_path = dir.join("parameters.json");
    // `json.dump(..., indent=4)` — serde_json's pretty printer also uses 4
    // spaces, and `ensure_ascii=False` matches serde_json's default of
    // emitting UTF-8 directly.
    std::fs::write(&file_path, text).map_err(|source| ConfigError::Write {
        path: file_path,
        source,
    })
}

/// Convert a YAML node into the equivalent JSON node.
fn yaml_to_json(value: &Value) -> serde_json::Value {
    use serde_json::Value as J;
    match value {
        Value::Null => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                J::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                J::Number(u.into())
            } else {
                n.as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map(J::Number)
                    .unwrap_or(J::Null)
            }
        }
        Value::String(s) => J::String(s.clone()),
        Value::Sequence(seq) => J::Array(seq.iter().map(yaml_to_json).collect()),
        Value::Mapping(map) => {
            let mut obj = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                let key = match k {
                    Value::String(s) => s.clone(),
                    other => serde_yaml::to_string(other)
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                };
                obj.insert(key, yaml_to_json(v));
            }
            J::Object(obj)
        }
        Value::Tagged(tagged) => yaml_to_json(&tagged.value),
    }
}
