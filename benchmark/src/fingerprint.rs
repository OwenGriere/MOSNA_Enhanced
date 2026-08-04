//! A compact, comparable record of what a computation produced.
//!
//! Level 1 asks whether the deterministic core still computes what it computed
//! yesterday. Storing whole outputs would mean megabytes of parquet in git;
//! storing nothing would mean trusting that nothing drifted. A fingerprint sits
//! between: exact data is reduced to a hash, and the small numeric results —
//! assortativity coefficients, mixing matrices, niche compositions — are kept
//! in full, because those are the numbers a reader wants to see when one moves.
//!
//! Three rules the comparison follows, each of which exists because the naive
//! version is wrong:
//!
//! * **Edges are sorted before hashing.** A parallel pipeline returns them in
//!   whatever order the threads finished in, and that is not a result.
//! * **Floats are compared with a relative tolerance.** A parallel sum does not
//!   add in the same order twice; the last bits are noise, not signal.
//! * **`NaN` equals `NaN`.** A grey cell of a mixing matrix — a pair of
//!   phenotypes that never occur together — is absent in both runs, and that is
//!   agreement.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// One recorded stage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Entry {
    /// Data whose every bit matters, reduced to a hash and a count.
    Exact { count: usize, digest: u64 },
    /// Numbers compared with a tolerance, kept in full.
    ///
    /// `NaN` does not survive JSON, so absent values are stored as `null`.
    Floats { values: Vec<Option<f64>> },
}

/// The record of one run.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Fingerprint {
    entries: BTreeMap<String, Entry>,
}

impl Fingerprint {
    /// Record a set of edges. Order is not part of the result.
    pub fn pairs(&mut self, name: &str, pairs: &[(u32, u32)]) {
        let mut canonical: Vec<(u32, u32)> = pairs
            .iter()
            .map(|&(a, b)| if a <= b { (a, b) } else { (b, a) })
            .collect();
        canonical.sort_unstable();

        let mut digest = Hasher::new();
        for (a, b) in &canonical {
            digest.write_u64(*a as u64);
            digest.write_u64(*b as u64);
        }
        self.entries.insert(
            name.to_string(),
            Entry::Exact {
                count: canonical.len(),
                digest: digest.finish(),
            },
        );
    }

    /// Record a labelling. Order *is* part of the result: cell `i` belongs to
    /// niche `n`, and a permutation would be a different answer.
    pub fn labels(&mut self, name: &str, labels: &[u32]) {
        let mut digest = Hasher::new();
        for label in labels {
            digest.write_u64(*label as u64);
        }
        self.entries.insert(
            name.to_string(),
            Entry::Exact {
                count: labels.len(),
                digest: digest.finish(),
            },
        );
    }

    /// Record numbers, kept in full and compared with a tolerance.
    pub fn floats(&mut self, name: &str, values: &[f64]) {
        self.entries.insert(
            name.to_string(),
            Entry::Floats {
                values: values
                    .iter()
                    .map(|value| if value.is_nan() { None } else { Some(*value) })
                    .collect(),
            },
        );
    }

    pub fn stages(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Everything that differs from `reference`, in words.
    ///
    /// An empty result is the whole point of level 1.
    pub fn differences(&self, reference: &Self, tolerance: f64) -> Vec<String> {
        let mut differences = Vec::new();

        for (name, expected) in &reference.entries {
            let Some(actual) = self.entries.get(name) else {
                differences.push(format!("`{name}` is missing from this run"));
                continue;
            };
            compare(name, actual, expected, tolerance, &mut differences);
        }

        for name in self.entries.keys() {
            if !reference.entries.contains_key(name) {
                differences.push(format!("`{name}` is not in the reference"));
            }
        }

        differences
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Pretty-printed and key-sorted: the reference lives in git, and a diff
        // should show which stage moved, not one long line.
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json + "\n")?;
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|error| anyhow::anyhow!("cannot read {}: {error}", path.display()))?;
        Ok(serde_json::from_str(&text)?)
    }
}

fn compare(name: &str, actual: &Entry, expected: &Entry, tolerance: f64, into: &mut Vec<String>) {
    match (actual, expected) {
        (
            Entry::Exact { count, digest },
            Entry::Exact {
                count: expected_count,
                digest: expected_digest,
            },
        ) => {
            if count != expected_count {
                into.push(format!(
                    "`{name}` changed length: {count} against {expected_count}"
                ));
            } else if digest != expected_digest {
                into.push(format!(
                    "`{name}` differs: {count} items, different content"
                ));
            }
        }
        (Entry::Floats { values }, Entry::Floats { values: expected }) => {
            if values.len() != expected.len() {
                into.push(format!(
                    "`{name}` changed length: {} against {}",
                    values.len(),
                    expected.len()
                ));
                return;
            }
            for (index, (actual, expected)) in values.iter().zip(expected).enumerate() {
                match (actual, expected) {
                    (None, None) => {}
                    (Some(actual), Some(expected)) => {
                        if !within(*actual, *expected, tolerance) {
                            into.push(format!(
                                "`{name}`[{index}]: {actual:.12e} against {expected:.12e}"
                            ));
                        }
                    }
                    (actual, expected) => into.push(format!(
                        "`{name}`[{index}]: {} against {}",
                        describe(actual),
                        describe(expected)
                    )),
                }
            }
        }
        _ => into.push(format!("`{name}` changed kind")),
    }
}

fn describe(value: &Option<f64>) -> String {
    match value {
        Some(value) => format!("{value:.12e}"),
        None => "absent".to_string(),
    }
}

/// Relative comparison, falling back to absolute near zero where a relative
/// tolerance means nothing.
fn within(actual: f64, expected: f64, tolerance: f64) -> bool {
    let difference = (actual - expected).abs();
    if difference == 0.0 {
        return true;
    }
    let scale = actual.abs().max(expected.abs());
    if scale < 1.0 {
        difference <= tolerance
    } else {
        difference / scale <= tolerance
    }
}

/// FNV-1a, written out.
///
/// `DefaultHasher` is explicitly not stable across Rust releases, and a golden
/// reference that changes when the compiler is upgraded is not a golden
/// reference.
struct Hasher(u64);

impl Hasher {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self(Self::OFFSET)
    }

    fn write_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_fixed_across_releases() {
        // A literal, so an accidental change to the hashing is caught here
        // rather than as a mysterious failure of every golden reference.
        //
        // The value is FNV-1a over the sixteen little-endian bytes of `1` then
        // `2`, computed independently rather than copied from this code's own
        // output — otherwise the test would only say the code agrees with
        // itself.
        let mut hasher = Hasher::new();
        hasher.write_u64(1);
        hasher.write_u64(2);
        assert_eq!(hasher.finish(), 0x7717_9803_63c8_e066);
    }

    #[test]
    fn the_hash_depends_on_the_order() {
        let mut one = Hasher::new();
        one.write_u64(1);
        one.write_u64(2);
        let mut two = Hasher::new();
        two.write_u64(2);
        two.write_u64(1);
        assert_ne!(one.finish(), two.finish());
    }

    #[test]
    fn near_zero_the_tolerance_is_absolute() {
        // A relative tolerance around zero would demand infinite precision.
        assert!(within(0.0, 1e-15, 1e-9));
        assert!(!within(0.0, 1e-3, 1e-9));
    }

    #[test]
    fn far_from_zero_the_tolerance_is_relative() {
        assert!(within(1e6, 1e6 + 1e-4, 1e-9));
        assert!(!within(1e6, 1e6 + 1.0, 1e-9));
    }

    #[test]
    fn an_edge_is_recorded_the_same_in_either_direction() {
        let mut one = Fingerprint::default();
        one.pairs("edges", &[(3, 1)]);
        let mut two = Fingerprint::default();
        two.pairs("edges", &[(1, 3)]);
        assert!(one.differences(&two, 0.0).is_empty());
    }
}
