//! Human readable name of a YAML node, used in error messages.

use serde_yaml::Value;

/// Return the YAML type name of `value`, matching the vocabulary used in the
/// Python assertion messages (`str`, `int`, `float`, `bool`, `list`, `None`).
pub fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "None",
        Value::Bool(_) => "bool",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "int"
            } else {
                "float"
            }
        }
        Value::String(_) => "str",
        Value::Sequence(_) => "list",
        Value::Mapping(_) => "dict",
        Value::Tagged(_) => "tagged",
    }
}
