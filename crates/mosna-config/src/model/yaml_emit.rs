//! A YAML emitter matching PyYAML's `safe_dump` output for MOSNA documents.
//!
//! The Python GUI writes the configuration with
//!
//! ```python
//! yaml.safe_dump(force_inline_lists(config), f,
//!                default_flow_style=False, sort_keys=False,
//!                allow_unicode=True, width=4096)
//! ```
//!
//! where `force_inline_lists` installs a representer that emits every sequence
//! in flow style. `serde_yaml` has no knob for per-node flow style, so this
//! module emits the document directly. The configuration is a shallow tree of
//! mappings, scalars and string lists, which keeps the emitter small.

use std::fmt::Write as _;

use serde_yaml::Value;

/// Serialise `value` the way PyYAML's `safe_dump` would.
pub fn emit(value: &Value) -> String {
    let mut out = String::new();
    match value {
        Value::Mapping(map) if !map.is_empty() => emit_mapping(map, 0, &mut out),
        other => {
            let _ = writeln!(out, "{}", emit_scalar(other));
        }
    }
    out
}

fn emit_mapping(map: &serde_yaml::Mapping, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    for (key, val) in map {
        let key_str = match key {
            Value::String(s) => emit_scalar(&Value::String(s.clone())),
            other => emit_scalar(other),
        };
        match val {
            Value::Mapping(inner) if !inner.is_empty() => {
                let _ = writeln!(out, "{pad}{key_str}:");
                emit_mapping(inner, indent + 2, out);
            }
            Value::Mapping(_) => {
                let _ = writeln!(out, "{pad}{key_str}: {{}}");
            }
            Value::Sequence(seq) => {
                let _ = writeln!(out, "{pad}{key_str}: {}", emit_flow_sequence(seq));
            }
            other => {
                let _ = writeln!(out, "{pad}{key_str}: {}", emit_scalar(other));
            }
        }
    }
}

/// Sequences are always emitted in flow style, per `force_inline_lists`.
fn emit_flow_sequence(seq: &[Value]) -> String {
    let items: Vec<String> = seq
        .iter()
        .map(|item| match item {
            Value::Sequence(inner) => emit_flow_sequence(inner),
            Value::Mapping(inner) => emit_flow_mapping(inner),
            other => emit_scalar(other),
        })
        .collect();
    format!("[{}]", items.join(", "))
}

fn emit_flow_mapping(map: &serde_yaml::Mapping) -> String {
    let items: Vec<String> = map
        .iter()
        .map(|(k, v)| format!("{}: {}", emit_scalar(k), emit_scalar(v)))
        .collect();
    format!("{{{}}}", items.join(", "))
}

fn emit_scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Number(n) => {
            // PyYAML renders a float that happens to be integral as `1.0`,
            // never as `1`, which matters for `min_dist: 0.0`.
            if n.is_f64() {
                let f = n.as_f64().unwrap_or_default();
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e16 {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            } else {
                n.to_string()
            }
        }
        Value::String(s) => emit_string(s),
        Value::Sequence(seq) => emit_flow_sequence(seq),
        Value::Mapping(map) => emit_flow_mapping(map),
        Value::Tagged(t) => emit_scalar(&t.value),
    }
}

/// Emit a string as a plain scalar when that is unambiguous, otherwise with
/// single quotes — the same choice PyYAML makes.
fn emit_string(s: &str) -> String {
    if needs_quotes(s) {
        // Single-quoted style escapes an embedded quote by doubling it.
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    }
}

fn needs_quotes(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if s != s.trim() {
        return true;
    }
    if resolves_to_non_string(s) {
        return true;
    }
    // Indicator characters that change the meaning of a plain scalar when they
    // open it.
    let first = s.chars().next().unwrap();
    if "-?:,[]{}#&*!|>'\"%@`".contains(first) {
        return true;
    }
    // `: ` starts a mapping value and ` #` starts a comment, anywhere in the
    // scalar.
    if s.contains(": ") || s.contains(" #") || s.ends_with(':') {
        return true;
    }
    if s.chars().any(|c| c == '\n' || c == '\r' || c == '\t') {
        return true;
    }
    false
}

/// `true` when a plain scalar would be resolved by PyYAML's safe resolver as
/// something other than a string.
fn resolves_to_non_string(s: &str) -> bool {
    const BOOLS: [&str; 22] = [
        "y", "Y", "yes", "Yes", "YES", "n", "N", "no", "No", "NO", "true", "True", "TRUE", "false",
        "False", "FALSE", "on", "On", "ON", "off", "Off", "OFF",
    ];
    const NULLS: [&str; 5] = ["~", "null", "Null", "NULL", ""];

    if BOOLS.contains(&s) || NULLS.contains(&s) {
        return true;
    }
    // Integers, including the `0x`/`0o` and underscore-separated forms, and
    // floats such as `1e-3` or `.5`.
    let compact = s.replace('_', "");
    if compact.parse::<i64>().is_ok() || compact.parse::<f64>().is_ok() {
        return true;
    }
    if compact.starts_with("0x") || compact.starts_with("0o") {
        return true;
    }
    // Sexagesimal and timestamp-looking scalars.
    if s.contains(':') && s.split(':').all(|p| p.parse::<i64>().is_ok()) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(yaml: &str) -> Value {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn numeric_strings_are_quoted() {
        assert_eq!(emit_string("1"), "'1'");
        assert_eq!(emit_string("0.05"), "'0.05'");
        assert_eq!(emit_string("mean"), "mean");
    }

    #[test]
    fn booleanish_strings_are_quoted() {
        assert_eq!(emit_string("yes"), "'yes'");
        assert_eq!(emit_string("null"), "'null'");
    }

    #[test]
    fn sequences_use_flow_style() {
        let doc = v("Niche:\n  stat_names:\n    - mean\n    - std\n");
        assert_eq!(emit(&doc), "Niche:\n  stat_names: [mean, std]\n");
    }

    #[test]
    fn floats_keep_a_decimal_point() {
        let doc = v("Niche:\n  min_dist: 0.0\n");
        assert_eq!(emit(&doc), "Niche:\n  min_dist: 0.0\n");
    }

    #[test]
    fn nesting_is_indented_by_two_spaces() {
        let doc = v("A:\n  B:\n    c: 1\n");
        assert_eq!(emit(&doc), "A:\n  B:\n    c: 1\n");
    }

    #[test]
    fn round_trips_through_the_parser() {
        let doc = v("Tysserand:\n  Extension: parquet\n  CPU: 20\n  Phenotype column: null\n");
        let text = emit(&doc);
        let reparsed: Value = serde_yaml::from_str(&text).unwrap();
        assert_eq!(doc, reparsed);
    }
}
