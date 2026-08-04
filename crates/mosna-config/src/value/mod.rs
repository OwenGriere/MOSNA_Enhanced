//! Typed accessors over a `serde_yaml::Value` mapping.
//!
//! The Python code indexes the parsed YAML dictionary directly
//! (`config["Patient column name"]`) and relies on duck typing. These helpers
//! provide the same ergonomics while producing precise errors, and they apply
//! the same coercions the Python side gets for free (e.g. an integer literal
//! being acceptable where a float is expected).

pub mod get_bool;
pub mod get_float;
pub mod get_int;
pub mod get_list;
pub mod get_str;
pub mod type_name;

pub use get_bool::{get_bool, get_bool_or};
pub use get_float::{get_float, get_float_or};
pub use get_int::{get_int, get_int_or};
pub use get_list::{get_string_list, get_string_or_list};
pub use get_str::{get_opt_str, get_str, get_str_or};
pub use type_name::type_name;

/// A column selector: either a single column or a list of columns.
///
/// Mirrors the Python `Union[str, list]` accepted by `Column to aggregate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnSelector {
    One(String),
    Many(Vec<String>),
}

impl ColumnSelector {
    /// Flatten to a plain list of column names.
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            ColumnSelector::One(c) => vec![c.clone()],
            ColumnSelector::Many(c) => c.clone(),
        }
    }

    /// `true` when a single column was selected, which is the condition the
    /// Python code uses to decide whether one-hot encoding must be applied.
    pub fn is_single(&self) -> bool {
        matches!(self, ColumnSelector::One(_))
            || matches!(self, ColumnSelector::Many(v) if v.len() == 1)
    }

    /// The single column name, when there is exactly one.
    pub fn as_single(&self) -> Option<&str> {
        match self {
            ColumnSelector::One(c) => Some(c.as_str()),
            ColumnSelector::Many(v) if v.len() == 1 => Some(v[0].as_str()),
            _ => None,
        }
    }
}
