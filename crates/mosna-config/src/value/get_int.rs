//! Integer accessors over a YAML mapping.

use serde_yaml::Value;

use crate::error::{ConfigError, Result};
use crate::value::type_name::type_name;

/// Read a mandatory integer.
///
/// Booleans are rejected: Python's `isinstance(True, int)` is `True`, but the
/// configuration never legitimately carries a boolean where an integer is
/// expected, and accepting one would silently turn `CPU: true` into `CPU: 1`.
pub fn get_int(section: &Value, section_name: &str, key: &str) -> Result<i64> {
    match section.get(key) {
        None | Some(Value::Null) => Err(ConfigError::MissingKey {
            section: section_name.to_string(),
            key: key.to_string(),
        }),
        Some(Value::Number(n)) if n.is_i64() => Ok(n.as_i64().unwrap()),
        Some(Value::Number(n)) if n.is_u64() => Ok(n.as_u64().unwrap() as i64),
        Some(other) => Err(ConfigError::WrongType {
            section: section_name.to_string(),
            key: key.to_string(),
            expected: "int",
            found: type_name(other),
        }),
    }
}

/// Read an integer, falling back to `default` when absent, null or malformed.
pub fn get_int_or(section: &Value, key: &str, default: i64) -> i64 {
    match section.get(key) {
        Some(Value::Number(n)) if n.is_i64() => n.as_i64().unwrap(),
        Some(Value::Number(n)) if n.is_u64() => n.as_u64().unwrap() as i64,
        // The Python code wraps most of these reads in `int(...)`, which also
        // accepts a float or a numeric string.
        Some(Value::Number(n)) => n.as_f64().map(|f| f as i64).unwrap_or(default),
        Some(Value::String(s)) => s.trim().parse::<i64>().unwrap_or(default),
        _ => default,
    }
}
