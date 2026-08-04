//! Boolean accessors over a YAML mapping.

use serde_yaml::Value;

use crate::error::{ConfigError, Result};
use crate::value::type_name::type_name;

/// Read a mandatory boolean.
pub fn get_bool(section: &Value, section_name: &str, key: &str) -> Result<bool> {
    match section.get(key) {
        None | Some(Value::Null) => Err(ConfigError::MissingKey {
            section: section_name.to_string(),
            key: key.to_string(),
        }),
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(ConfigError::WrongType {
            section: section_name.to_string(),
            key: key.to_string(),
            expected: "bool",
            found: type_name(other),
        }),
    }
}

/// Read a boolean, falling back to `default` when absent or null.
///
/// The string forms `"True"`/`"False"` are accepted because the GUI stores the
/// value of its two-item combo box as text before the YAML round-trip.
pub fn get_bool_or(section: &Value, key: &str, default: bool) -> bool {
    match section.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "true" => true,
            "false" => false,
            _ => default,
        },
        _ => default,
    }
}
