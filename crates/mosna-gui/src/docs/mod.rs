//! The manual, rendered inside the interface.
//!
//! The Python interface shows `assets/documentation.html` in a web view. Here
//! the same material is a structured document the interface draws itself, which
//! buys three things: it inherits the palette, it can be navigated with real
//! buttons, and it can switch language without reloading.

pub mod assets;
pub mod content;
pub mod model;
pub mod state;

use std::collections::BTreeSet;

pub use model::{Block, CalloutKind, Chapter, Citation, Language, ParameterRow, Section, Text};

/// The whole manual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Documentation {
    pub chapters: Vec<Chapter>,
}

impl Documentation {
    /// Assemble the manual.
    pub fn build() -> Self {
        Self {
            chapters: content::chapters(),
        }
    }

    /// Find a section by its identifier, which is what the navigation stores.
    pub fn section(&self, id: &str) -> Option<&Section> {
        self.chapters
            .iter()
            .flat_map(|chapter| chapter.sections.iter())
            .find(|section| section.id == id)
    }

    /// The chapter a section belongs to.
    pub fn chapter_of(&self, section_id: &str) -> Option<&Chapter> {
        self.chapters
            .iter()
            .find(|chapter| chapter.sections.iter().any(|s| s.id == section_id))
    }

    /// The first section, shown when the manual opens.
    pub fn first_section(&self) -> Option<&Section> {
        self.chapters.first()?.sections.first()
    }

    /// Every configuration key the manual documents.
    ///
    /// Compared against what the Parameters panel offers, so that adding a
    /// setting without explaining it fails the test suite.
    ///
    /// Scoped to the parameters chapter: any other chapter is free to use a
    /// table for something that is not a setting.
    pub fn documented_parameters(&self) -> BTreeSet<&'static str> {
        self.chapters
            .iter()
            .filter(|chapter| chapter.id == "parameters")
            .flat_map(|chapter| chapter.sections.iter())
            .flat_map(|section| section.blocks.iter())
            .filter_map(|block| match block {
                Block::Table { rows, .. } => Some(rows.iter().map(|row| row.name)),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Every crate the manual credits.
    ///
    /// Compared against the manifests, so a citation cannot go stale and a new
    /// dependency cannot go unacknowledged.
    pub fn cited_crates(&self) -> BTreeSet<&'static str> {
        self.chapters
            .iter()
            .flat_map(|chapter| chapter.sections.iter())
            .flat_map(|section| section.blocks.iter())
            .filter_map(|block| match block {
                Block::Citations(citations) => Some(citations.iter().map(|c| c.name)),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Sections whose title or text contains `needle`, for the search box.
    pub fn search(&self, needle: &str, language: Language) -> Vec<&Section> {
        let needle = needle.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }

        self.chapters
            .iter()
            .flat_map(|chapter| chapter.sections.iter())
            .filter(|section| section_matches(section, &needle, language))
            .collect()
    }
}

impl Default for Documentation {
    fn default() -> Self {
        Self::build()
    }
}

fn section_matches(section: &Section, needle: &str, language: Language) -> bool {
    if section.title.get(language).to_lowercase().contains(needle) {
        return true;
    }
    section.blocks.iter().any(|block| match block {
        Block::Heading(text) | Block::Paragraph(text) => {
            text.get(language).to_lowercase().contains(needle)
        }
        Block::Callout { text, .. } => text.get(language).to_lowercase().contains(needle),
        Block::List(items) => items
            .iter()
            .any(|item| item.get(language).to_lowercase().contains(needle)),
        Block::Table { rows, .. } => rows.iter().any(|row| {
            row.name.to_lowercase().contains(needle)
                || row
                    .description
                    .get(language)
                    .to_lowercase()
                    .contains(needle)
        }),
        Block::Code { lines, .. } => lines
            .iter()
            .any(|line| line.to_lowercase().contains(needle)),
        Block::Citations(citations) => citations.iter().any(|citation| {
            citation.name.to_lowercase().contains(needle)
                || citation.role.get(language).to_lowercase().contains(needle)
        }),
        Block::Image { .. } => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manual_opens_on_its_first_section() {
        let documentation = Documentation::build();
        let first = documentation.first_section().unwrap();
        assert_eq!(
            documentation.chapters[0].sections[0].id, first.id,
            "the manual must open on the first page"
        );
    }

    #[test]
    fn a_section_knows_its_chapter() {
        let documentation = Documentation::build();
        let section = documentation.chapters[1].sections[0].id;
        assert_eq!(
            documentation.chapter_of(section).map(|c| c.id),
            Some(documentation.chapters[1].id)
        );
    }

    #[test]
    fn searching_finds_a_parameter_by_name() {
        let documentation = Documentation::build();
        let found = documentation.search("min_dist", Language::English);
        assert!(!found.is_empty(), "min_dist should be findable");
    }

    #[test]
    fn searching_works_in_french_too() {
        let documentation = Documentation::build();
        assert!(!documentation
            .search("installation", Language::French)
            .is_empty());
    }

    #[test]
    fn an_empty_search_matches_nothing() {
        assert!(Documentation::build()
            .search("   ", Language::English)
            .is_empty());
    }

    #[test]
    fn searching_is_case_insensitive() {
        let documentation = Documentation::build();
        let lower = documentation.search("umap", Language::English).len();
        let upper = documentation.search("UMAP", Language::English).len();
        assert_eq!(lower, upper);
        assert!(lower > 0);
    }
}
