//! Floating point accessors over a YAML mapping.

use serde_yaml::Value;

use crate::error::{ConfigError, Result};
use crate::value::type_name::type_name;

/// Read a mandatory float.
///
/// Note that this *rejects* an integer literal, because `assert_params` uses
/// `isinstance(v, float)` for `resolution` and `min_dist`: writing
/// `min_dist: 0` instead of `min_dist: 0.0` is an error on the Python side too,
/// and silently accepting it here would let a config through that the Python
/// implementation refuses.
pub fn get_float(section: &Value, section_name: &str, key: &str) -> Result<f64> {
    match section.get(key) {
        None | Some(Value::Null) => Err(ConfigError::MissingKey {
            section: section_name.to_string(),
            key: key.to_string(),
        }),
        Some(Value::Number(n)) if n.is_f64() => Ok(n.as_f64().unwrap()),
        Some(other) => Err(ConfigError::WrongType {
            section: section_name.to_string(),
            key: key.to_string(),
            expected: "float",
            found: type_name(other),
        }),
    }
}

/// Read a float, falling back to `default` when absent, null or malformed.
///
/// Unlike [`get_float`], this mirrors the `float(config.get(...))` call sites
/// and therefore does accept an integer.
pub fn get_float_or(section: &Value, key: &str, default: f64) -> f64 {
    match section.get(key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
        Some(Value::String(s)) => s.trim().parse::<f64>().unwrap_or(default),
        _ => default,
    }
}
