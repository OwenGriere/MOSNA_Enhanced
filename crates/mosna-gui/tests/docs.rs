//! Tests of the embedded documentation, written before the content exists.
//!
//! A manual is only useful if it is complete and stays complete. The tests that
//! matter here are not about wording: they check that every parameter the
//! interface offers is explained, that nothing is written in one language and
//! forgotten in the other, and that the installation instructions actually name
//! the commands a user has to type.

use std::collections::BTreeSet;

use mosna_config::RawConfig;
use mosna_gui::docs::model::{Block, Language};
use mosna_gui::docs::Documentation;
use mosna_gui::model::form::Form;

fn documentation() -> Documentation {
    Documentation::build()
}

/// Every string in the manual, in one language.
fn all_text(documentation: &Documentation, language: Language) -> Vec<String> {
    let mut collected = Vec::new();
    for chapter in &documentation.chapters {
        collected.push(chapter.title.get(language).to_string());
        for section in &chapter.sections {
            collected.push(section.title.get(language).to_string());
            for block in &section.blocks {
                collect_block(block, language, &mut collected);
            }
        }
    }
    collected
}

fn collect_block(block: &Block, language: Language, into: &mut Vec<String>) {
    match block {
        Block::Paragraph(text) | Block::Heading(text) => into.push(text.get(language).into()),
        Block::List(items) => into.extend(items.iter().map(|t| t.get(language).to_string())),
        Block::Callout { text, .. } => into.push(text.get(language).into()),
        Block::Table { headers, rows } => {
            into.extend(headers.iter().map(|t| t.get(language).to_string()));
            // A row's name and type are deliberately the same in both
            // languages — they are what the interface shows and what the
            // configuration file contains — so they are not collected here.
            // `every_documented_parameter_states_its_type` checks them instead.
            into.extend(rows.iter().map(|row| row.description.get(language).into()));
        }
        // The crate name is untranslated by design — it is what appears in
        // `Cargo.toml` — so only the reason is collected.
        Block::Citations(citations) => {
            into.extend(citations.iter().map(|c| c.role.get(language).to_string()))
        }
        // A command is the same in both languages by design: translating
        // `cargo build` would be actively harmful.
        Block::Code { .. } | Block::Image { .. } => {}
    }
}

// ---------------------------------------------------------------------------
// Structure
// ---------------------------------------------------------------------------

#[test]
fn the_manual_is_not_empty() {
    let documentation = documentation();
    assert!(documentation.chapters.len() >= 4, "too few chapters");
    for chapter in &documentation.chapters {
        assert!(
            !chapter.sections.is_empty(),
            "chapter `{}` has no section",
            chapter.id
        );
        for section in &chapter.sections {
            assert!(
                !section.blocks.is_empty(),
                "section `{}` is empty",
                section.id
            );
        }
    }
}

/// Identifiers address a section from the navigation, so a duplicate would
/// make one of them unreachable.
#[test]
fn every_identifier_is_unique() {
    let documentation = documentation();
    let mut seen = BTreeSet::new();
    for chapter in &documentation.chapters {
        assert!(
            seen.insert(chapter.id),
            "duplicate chapter id `{}`",
            chapter.id
        );
        for section in &chapter.sections {
            assert!(
                seen.insert(section.id),
                "duplicate section id `{}`",
                section.id
            );
        }
    }
}

/// Looking a section up by its identifier is what the navigation does.
#[test]
fn every_section_can_be_found_by_its_identifier() {
    let documentation = documentation();
    for chapter in &documentation.chapters {
        for section in &chapter.sections {
            assert!(
                documentation.section(section.id).is_some(),
                "`{}` cannot be looked up",
                section.id
            );
        }
    }
    assert!(documentation.section("no-such-section").is_none());
}

// ---------------------------------------------------------------------------
// Both languages
// ---------------------------------------------------------------------------

/// Nothing may be written in one language and left blank in the other: a reader
/// switching to French must not land on an empty page.
#[test]
fn nothing_is_missing_in_either_language() {
    let documentation = documentation();
    let english = all_text(&documentation, Language::English);
    let french = all_text(&documentation, Language::French);

    assert_eq!(
        english.len(),
        french.len(),
        "the two languages carry a different number of strings"
    );
    for (index, (en, fr)) in english.iter().zip(&french).enumerate() {
        assert!(!en.trim().is_empty(), "string {index} is empty in English");
        assert!(!fr.trim().is_empty(), "string {index} is empty in French");
    }
}

/// The two languages must actually differ; a French translation that is a copy
/// of the English is a translation that was never done.
#[test]
fn the_translation_is_a_translation() {
    let documentation = documentation();
    let english = all_text(&documentation, Language::English);
    let french = all_text(&documentation, Language::French);

    let differing = english
        .iter()
        .zip(&french)
        .filter(|(en, fr)| en != fr)
        .count();
    let ratio = differing as f64 / english.len() as f64;
    assert!(
        ratio > 0.8,
        "only {:.0}% of the strings differ between the two languages",
        ratio * 100.0
    );
}

#[test]
fn the_language_toggle_covers_both() {
    assert_eq!(Language::English.other(), Language::French);
    assert_eq!(Language::French.other(), Language::English);
    assert_eq!(Language::English.label(), "English");
    assert_eq!(Language::French.label(), "Français");
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

/// The user asked for the manual to explain how to install; that is the one
/// chapter a reader needs before anything else works.
#[test]
fn the_manual_explains_how_to_install() {
    let documentation = documentation();
    let chapter = documentation
        .chapters
        .iter()
        .find(|chapter| chapter.id == "installation")
        .expect("there is no installation chapter");

    let commands: Vec<String> = chapter
        .sections
        .iter()
        .flat_map(|section| section.blocks.iter())
        .filter_map(|block| match block {
            Block::Code { lines, .. } => Some(lines.join("\n")),
            _ => None,
        })
        .collect();
    let all = commands.join("\n");

    assert!(all.contains("install.sh"), "Linux is not covered:\n{all}");
    assert!(
        all.contains("install.ps1"),
        "Windows is not covered:\n{all}"
    );
    assert!(
        all.contains("--uninstall"),
        "removal is not covered:\n{all}"
    );
}

/// Both platforms are described, and the desktop icon is mentioned — that is
/// what a user looks for after installing.
#[test]
fn the_installation_chapter_covers_both_platforms_and_the_icon() {
    let documentation = documentation();
    let chapter = documentation
        .chapters
        .iter()
        .find(|chapter| chapter.id == "installation")
        .unwrap();

    let sections: Vec<&str> = chapter.sections.iter().map(|s| s.id).collect();
    assert!(sections.contains(&"install-linux"), "{sections:?}");
    assert!(sections.contains(&"install-windows"), "{sections:?}");

    for language in [Language::English, Language::French] {
        let text = all_text(&documentation, language).join(" ").to_lowercase();
        let mentions_icon = text.contains("desktop") || text.contains("bureau");
        assert!(
            mentions_icon,
            "{language:?} never mentions the desktop icon"
        );
    }
}

// ---------------------------------------------------------------------------
// Completeness against the interface
// ---------------------------------------------------------------------------

/// Every parameter the Parameters panel offers must be documented.
///
/// This is the test that keeps the manual honest: adding a setting to the
/// configuration without explaining it now fails the build.
#[test]
fn every_parameter_of_the_interface_is_documented() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../CONFIG/configuration.yaml");
    if !path.exists() {
        eprintln!("skipping: {} not found", path.display());
        return;
    }

    let config = RawConfig::from_yaml_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let form = Form::from_config(&config);

    let offered: BTreeSet<String> = form
        .sections
        .iter()
        .flat_map(|section| section.tabs.iter())
        .flat_map(|tab| tab.groups.iter())
        .flat_map(|group| group.fields.iter())
        .map(|field| field.key.clone())
        .collect();

    let documented = documentation().documented_parameters();

    let missing: Vec<&String> = offered
        .iter()
        .filter(|key| !documented.contains(key.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "these parameters are offered by the interface but not documented: {missing:?}"
    );
}

/// And nothing is documented that the interface does not offer, which would
/// send a reader looking for a control that is not there.
#[test]
fn nothing_documented_is_absent_from_the_interface() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../CONFIG/configuration.yaml");
    if !path.exists() {
        return;
    }

    let config = RawConfig::from_yaml_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let form = Form::from_config(&config);

    let mut offered: BTreeSet<String> = form
        .sections
        .iter()
        .flat_map(|section| section.tabs.iter())
        .flat_map(|tab| tab.groups.iter())
        .flat_map(|group| group.fields.iter())
        .map(|field| field.key.clone())
        .collect();
    // The Browser panel owns these five; the form deliberately omits them, but
    // the manual documents them because the user still sets them.
    for key in [
        "Nodes directory",
        "Network directory",
        "Patient column name",
        "Sample column name",
        "Extension",
    ] {
        offered.insert(key.to_string());
    }

    let stale: Vec<&str> = documentation()
        .documented_parameters()
        .into_iter()
        .filter(|key| !offered.contains(*key))
        .collect();
    assert!(
        stale.is_empty(),
        "these parameters are documented but no longer offered: {stale:?}"
    );
}

/// A parameter table is only useful if each row says what type the value takes.
#[test]
fn every_documented_parameter_states_its_type() {
    for chapter in &documentation().chapters {
        for section in &chapter.sections {
            for block in &section.blocks {
                if let Block::Table { rows, .. } = block {
                    for row in rows {
                        assert!(
                            !row.name.trim().is_empty() && !row.kind.trim().is_empty(),
                            "a row of `{}` is incomplete: {row:?}",
                            section.id
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Figures
// ---------------------------------------------------------------------------

/// Every figure the manual references must be shipped inside the binary.
///
/// The manual is read from an installed copy, with no repository anywhere near
/// it, so a figure loaded from a relative path would simply be missing.
#[test]
fn every_figure_is_embedded_in_the_binary() {
    for chapter in &documentation().chapters {
        for section in &chapter.sections {
            for block in &section.blocks {
                if let Block::Image { asset, .. } = block {
                    let bytes = mosna_gui::docs::assets::image(asset)
                        .unwrap_or_else(|| panic!("`{asset}` is referenced but not embedded"));
                    assert!(bytes.starts_with(b"\x89PNG"), "`{asset}` is not a PNG");
                }
            }
        }
    }
}

/// An asset that is not shipped is reported rather than drawn as an empty box.
#[test]
fn an_unknown_figure_is_not_invented() {
    assert!(mosna_gui::docs::assets::image("images/nowhere.png").is_none());
}

/// And nothing is carried that the manual never shows.
///
/// The symmetric check of the one above. An embedded figure that no page
/// references is dead weight in every installed copy — `architecture.png` sat
/// there unused until it was noticed by hand, which is exactly what a test is
/// for.
#[test]
fn no_figure_is_embedded_without_being_shown() {
    let documentation = documentation();
    let shown: BTreeSet<&str> = documentation
        .chapters
        .iter()
        .flat_map(|chapter| chapter.sections.iter())
        .flat_map(|section| section.blocks.iter())
        .filter_map(|block| match block {
            Block::Image { asset, .. } => Some(*asset),
            _ => None,
        })
        .collect();

    let unused: Vec<&str> = mosna_gui::docs::assets::EMBEDDED
        .iter()
        .copied()
        .filter(|asset| !shown.contains(asset))
        .collect();
    assert!(
        unused.is_empty(),
        "these figures are compiled into the binary but never shown: {unused:?}"
    );
}
