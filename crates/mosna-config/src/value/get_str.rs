//! String accessors over a YAML mapping.

use serde_yaml::Value;

use crate::error::{ConfigError, Result};
use crate::value::type_name::type_name;

/// Read a mandatory string, erroring when the key is missing or not a string.
pub fn get_str(section: &Value, section_name: &str, key: &str) -> Result<String> {
    match section.get(key) {
        None | Some(Value::Null) => Err(ConfigError::MissingKey {
            section: section_name.to_string(),
            key: key.to_string(),
        }),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(other) => Err(ConfigError::WrongType {
            section: section_name.to_string(),
            key: key.to_string(),
            expected: "str",
            found: type_name(other),
        }),
    }
}

/// Read an optional string. A missing key and an explicit `null` are both
/// mapped to `None`, exactly like `config.get(key, None)` in Python.
///
/// An empty string is also treated as `None`: the GUI writes `""` when the user
/// clears a text field, and the Python side checks `if value:` in those spots.
pub fn get_opt_str(section: &Value, key: &str) -> Option<String> {
    match section.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// Read a string, falling back to `default` when absent or null.
pub fn get_str_or(section: &Value, key: &str, default: &str) -> String {
    get_opt_str(section, key).unwrap_or_else(|| default.to_string())
}
