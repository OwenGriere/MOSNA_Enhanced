//! List accessors over a YAML mapping.

use serde_yaml::Value;

use crate::error::{ConfigError, Result};
use crate::value::type_name::type_name;
use crate::value::ColumnSelector;

/// Read a mandatory list of strings.
///
/// A comma-separated scalar is also accepted, because the GUI stores
/// `stat_funcs` as the single string `"np.mean,np.std"` in its combo box while
/// `stat_names` is a genuine YAML list.
pub fn get_string_list(section: &Value, section_name: &str, key: &str) -> Result<Vec<String>> {
    match section.get(key) {
        None | Some(Value::Null) => Err(ConfigError::MissingKey {
            section: section_name.to_string(),
            key: key.to_string(),
        }),
        Some(Value::Sequence(seq)) => seq
            .iter()
            .map(|v| match v {
                Value::String(s) => Ok(s.clone()),
                Value::Number(n) => Ok(n.to_string()),
                Value::Bool(b) => Ok(b.to_string()),
                other => Err(ConfigError::WrongType {
                    section: section_name.to_string(),
                    key: key.to_string(),
                    expected: "list of str",
                    found: type_name(other),
                }),
            })
            .collect(),
        Some(Value::String(s)) => Ok(s
            .split(',')
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect()),
        Some(other) => Err(ConfigError::WrongType {
            section: section_name.to_string(),
            key: key.to_string(),
            expected: "list",
            found: type_name(other),
        }),
    }
}

/// Read a value that may be a single column name or a list of them.
///
/// This is the shape of `Column to aggregate`, which the Python code checks
/// with `isinstance(config["Column to aggregate"], (str, list))`.
pub fn get_string_or_list(
    section: &Value,
    section_name: &str,
    key: &str,
) -> Result<ColumnSelector> {
    match section.get(key) {
        None | Some(Value::Null) => Err(ConfigError::MissingKey {
            section: section_name.to_string(),
            key: key.to_string(),
        }),
        Some(Value::String(s)) => Ok(ColumnSelector::One(s.clone())),
        Some(Value::Sequence(_)) => {
            let items = get_string_list(section, section_name, key)?;
            Ok(ColumnSelector::Many(items))
        }
        Some(other) => Err(ConfigError::WrongType {
            section: section_name.to_string(),
            key: key.to_string(),
            expected: "str or list",
            found: type_name(other),
        }),
    }
}
