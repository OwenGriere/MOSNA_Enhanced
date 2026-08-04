//! Property tests for the configuration layer.
//!
//! The GUI loads the YAML, lets the user edit it, and writes it back. Anything
//! the round-trip loses is a setting the user silently lost, so the round-trip
//! is what these properties pin.

use mosna_config::RawConfig;
use proptest::prelude::*;
use serde_yaml::Value;

/// Keys as they appear in the file: words, possibly with spaces.
fn key() -> impl Strategy<Value = String> {
    "[A-Za-z][A-Za-z ]{0,15}[A-Za-z]"
}

/// Scalar values of every kind the configuration actually carries.
fn scalar() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        (-1000i64..1000).prop_map(|i| Value::Number(i.into())),
        (-100.0f64..100.0)
            .prop_map(serde_yaml::Number::from)
            .prop_map(Value::Number),
        // Strings that would otherwise be read back as another type are the
        // interesting case: "1", "true", "null".
        prop_oneof![
            "[A-Za-z][A-Za-z0-9_/. -]{0,20}",
            Just("1".to_string()),
            Just("true".to_string()),
            Just("null".to_string()),
            Just("0.05".to_string()),
            Just("np.mean,np.std".to_string()),
        ]
        .prop_map(Value::String),
    ]
}

/// A section: a mapping of keys to scalars and string lists.
fn section() -> impl Strategy<Value = Value> {
    proptest::collection::vec(
        (
            key(),
            prop_oneof![
                scalar(),
                proptest::collection::vec("[a-z]{1,6}", 1..4).prop_map(|items| Value::Sequence(
                    items.into_iter().map(Value::String).collect()
                )),
            ],
        ),
        1..8,
    )
    .prop_map(|pairs| {
        let mut map = serde_yaml::Mapping::new();
        for (k, v) in pairs {
            map.insert(Value::String(k), v);
        }
        Value::Mapping(map)
    })
}

/// A whole document: named sections of settings.
fn document() -> impl Strategy<Value = RawConfig> {
    proptest::collection::vec((key(), section()), 1..4).prop_map(|sections| {
        let mut root = serde_yaml::Mapping::new();
        for (name, body) in sections {
            root.insert(Value::String(name), body);
        }
        RawConfig::from_mapping(root)
    })
}

proptest! {
    /// Emitting a document and parsing it back yields the same document.
    /// This is the property the GUI's save button depends on.
    #[test]
    fn prop_yaml_round_trips_through_the_parser(config in document()) {
        let text = config.to_yaml_string().unwrap();
        let reparsed = RawConfig::from_yaml_str(&text)
            .unwrap_or_else(|e| panic!("re-emitted YAML does not parse: {e}\n{text}"));
        prop_assert_eq!(config, reparsed);
    }

    /// Emitting is idempotent: saving twice produces the same bytes, so a
    /// no-op edit leaves a clean diff.
    #[test]
    fn prop_emitting_is_idempotent(config in document()) {
        let once = config.to_yaml_string().unwrap();
        let reparsed = RawConfig::from_yaml_str(&once).unwrap();
        let twice = reparsed.to_yaml_string().unwrap();
        prop_assert_eq!(once, twice);
    }

    /// Section order is preserved, so the GUI lays its tabs out the way the
    /// file does and saving never reshuffles the file.
    #[test]
    fn prop_section_order_is_preserved(config in document()) {
        let names = config.section_names();
        let text = config.to_yaml_string().unwrap();
        let reparsed = RawConfig::from_yaml_str(&text).unwrap();
        prop_assert_eq!(names, reparsed.section_names());
    }

    /// A string that looks like another type keeps its type through the round
    /// trip: `order: '1'` must not come back as the integer 1, because
    /// `assert_params` requires `order` to be a string.
    #[test]
    fn prop_ambiguous_strings_stay_strings(
        raw in prop_oneof![
            Just("1"), Just("2"), Just("true"), Just("false"),
            Just("null"), Just("0.05"), Just("yes"), Just("no"),
        ]
    ) {
        let mut config = RawConfig::default();
        config.set("Niche", "order", Value::String(raw.to_string()));

        let text = config.to_yaml_string().unwrap();
        let reparsed = RawConfig::from_yaml_str(&text).unwrap();

        prop_assert_eq!(
            reparsed.get("Niche", "order"),
            Some(&Value::String(raw.to_string())),
            "emitted as: {}",
            text
        );
    }

    /// Setting a key then reading it back gives the value that was set.
    #[test]
    fn prop_set_then_get(section in key(), name in key(), value in scalar()) {
        let mut config = RawConfig::default();
        config.set(&section, &name, value.clone());
        prop_assert_eq!(config.get(&section, &name), Some(&value));
    }

    /// Editing one key leaves every other key of the document alone — the
    /// guarantee that unknown settings survive the GUI.
    #[test]
    fn prop_editing_preserves_unknown_keys(config in document()) {
        let section = config.section_names()[0].clone();
        let before = config.clone();

        let mut edited = config;
        edited.set(&section, "A Brand New Key", Value::Bool(true));

        for name in before.section_names() {
            let original = before.section(&name).unwrap();
            let Value::Mapping(original) = original else { continue };
            for (k, v) in original {
                let Some(k) = k.as_str() else { continue };
                prop_assert_eq!(
                    edited.get(&name, k),
                    Some(v),
                    "key `{}` of section `{}` changed",
                    k,
                    name
                );
            }
        }
    }
}
