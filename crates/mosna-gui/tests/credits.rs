//! Tests of the citations, written before them.
//!
//! A credits page is worth exactly as much as its accuracy. One that names a
//! crate the project stopped using, or omits one it depends on, is worse than
//! none: it looks authoritative and is not. So these tests read the real
//! manifests and compare.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use mosna_gui::docs::model::{Block, Language};
use mosna_gui::docs::Documentation;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every third-party package the project declares, whatever language it is
/// written in.
///
/// The figures are drawn by a Python package now, so a credits page that read
/// only the Cargo manifests would acknowledge everything *except* the library
/// that produces what the user actually looks at.
fn declared_dependencies() -> BTreeSet<String> {
    let mut found = declared_crates();
    found.extend(declared_python_packages());
    found
}

/// The Python packages the renderer needs, from `python/pyproject.toml`.
///
/// The shape being read is `dependencies = ["xy==0.0.6", "numpy>=1.24"]`: a
/// name, then a version constraint in one of a handful of spellings.
fn declared_python_packages() -> BTreeSet<String> {
    let manifest = root().join("python/pyproject.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();

    let mut found = BTreeSet::new();
    let Some(list) = text.split("dependencies = [").nth(1) else {
        return found;
    };
    let Some(list) = list.split(']').next() else {
        return found;
    };

    for item in list.split(',') {
        let item = item.trim().trim_matches('"').trim_matches('\'');
        let name = item
            .split(['=', '>', '<', '!', '~', ';', '['])
            .next()
            .unwrap_or("")
            .trim();
        if !name.is_empty() {
            found.insert(name.to_string());
        }
    }
    found
}

/// Every third-party crate the workspace declares, from the manifests.
///
/// Parsed by hand rather than with a TOML crate: adding a dependency in order
/// to check the dependency list is a circularity worth avoiding, and the shape
/// being read is three lines of `name = ...` under a known header.
fn declared_crates() -> BTreeSet<String> {
    let mut manifests = vec![
        root().join("Cargo.toml"),
        root().join("benchmark/Cargo.toml"),
    ];
    for entry in std::fs::read_dir(root().join("crates")).unwrap().flatten() {
        let manifest = entry.path().join("Cargo.toml");
        if manifest.is_file() {
            manifests.push(manifest);
        }
    }

    let mut found = BTreeSet::new();
    for manifest in manifests {
        let text = std::fs::read_to_string(&manifest).unwrap();
        let mut inside = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                inside = line.ends_with("dependencies]");
                continue;
            }
            if !inside || line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((name, _)) = line.split_once('=') else {
                continue;
            };
            // `serde.workspace = true` names the crate before the dot.
            let name = name.trim().split('.').next().unwrap().trim();
            // The workspace's own crates are not third-party.
            if !name.is_empty() && !name.starts_with("mosna-") {
                found.insert(name.to_string());
            }
        }
    }
    found
}

fn cited() -> BTreeSet<&'static str> {
    Documentation::build().cited_crates()
}

// ---------------------------------------------------------------------------
// The page exists and is where a reader expects it
// ---------------------------------------------------------------------------

#[test]
fn the_manual_ends_with_its_credits() {
    let documentation = Documentation::build();
    let last = documentation.chapters.last().expect("a chapter");
    assert_eq!(
        last.id, "credits",
        "the credits are not the last chapter; got `{}`",
        last.id
    );
}

#[test]
fn the_credits_are_written_in_both_languages() {
    let documentation = Documentation::build();
    let chapter = documentation
        .chapters
        .iter()
        .find(|chapter| chapter.id == "credits")
        .unwrap();

    for section in &chapter.sections {
        for block in &section.blocks {
            if let Block::Citations(citations) = block {
                for citation in citations {
                    let (en, fr) = (
                        citation.role.get(Language::English),
                        citation.role.get(Language::French),
                    );
                    assert!(!en.trim().is_empty(), "`{}` has no English", citation.name);
                    assert!(!fr.trim().is_empty(), "`{}` has no French", citation.name);
                    assert_ne!(en, fr, "`{}` is not translated", citation.name);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Accuracy, against the manifests
// ---------------------------------------------------------------------------

/// The manifests are the truth. A dependency that is not cited is a use that
/// went unacknowledged.
#[test]
fn every_dependency_is_cited() {
    let cited = cited();
    let missing: Vec<String> = declared_dependencies()
        .into_iter()
        .filter(|name| !cited.contains(name.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "these crates are used but not cited: {missing:?}"
    );
}

/// And a citation that names a crate the project no longer uses is a claim
/// that is simply false.
#[test]
fn nothing_is_cited_that_is_not_used() {
    let declared = declared_dependencies();
    let stale: Vec<&str> = cited()
        .into_iter()
        .filter(|name| !declared.contains(*name))
        .collect();
    assert!(
        stale.is_empty(),
        "these crates are cited but no longer used: {stale:?}"
    );
}

/// The manifest reader has to actually find something, or the two tests above
/// would pass on an empty set and prove nothing.
#[test]
fn the_manifests_are_actually_read() {
    let declared = declared_dependencies();
    assert!(
        declared.len() > 20,
        "only {} dependencies found — the parser is broken",
        declared.len()
    );
    for expected in ["egui", "rayon", "parquet"] {
        assert!(declared.contains(expected), "`{expected}` was not found");
    }
}

/// The renderer's own dependencies have to be found too, or the page would
/// quietly stop covering the half of the project that draws the figures.
#[test]
fn the_python_manifest_is_read_as_well() {
    let python = declared_python_packages();
    assert!(
        python.contains("xy"),
        "the charting library was not found in the Python manifest: {python:?}"
    );
    assert!(
        declared_dependencies().contains("xy"),
        "the Python packages did not reach the dependency list"
    );
}

/// Every citation says what the crate is for. A bare list of names credits
/// nobody and explains nothing.
#[test]
fn every_citation_gives_a_reason() {
    let documentation = Documentation::build();
    for chapter in &documentation.chapters {
        for section in &chapter.sections {
            for block in &section.blocks {
                if let Block::Citations(citations) = block {
                    for citation in citations {
                        assert!(
                            citation.role.get(Language::English).len() > 20,
                            "`{}` has no real explanation",
                            citation.name
                        );
                    }
                }
            }
        }
    }
}

/// A crate cited twice would appear twice on the page.
#[test]
fn no_crate_is_cited_twice() {
    let documentation = Documentation::build();
    let mut seen = BTreeSet::new();
    for chapter in &documentation.chapters {
        for section in &chapter.sections {
            for block in &section.blocks {
                if let Block::Citations(citations) = block {
                    for citation in citations {
                        assert!(
                            seen.insert(citation.name),
                            "`{}` is cited twice",
                            citation.name
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The credits must not disturb the parameter check
// ---------------------------------------------------------------------------

/// `documented_parameters` feeds the test that keeps every setting explained.
/// A crate name is not a setting, and must not leak into that set.
#[test]
fn a_crate_name_is_not_a_parameter() {
    let parameters = Documentation::build().documented_parameters();
    for name in ["egui", "rayon", "serde", "parquet"] {
        assert!(
            !parameters.contains(name),
            "`{name}` leaked into the documented parameters"
        );
    }
}
