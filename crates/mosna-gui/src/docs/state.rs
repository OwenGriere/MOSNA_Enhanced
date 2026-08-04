//! Where the reader is in the manual.
//!
//! Kept apart from the drawing so that every button has a testable meaning: the
//! panel only reads this state and writes back what was clicked.

use std::collections::BTreeSet;

use super::model::{Language, Section};
use super::Documentation;

/// The open page, the chosen language, and the search box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualState {
    /// Identifier of the open section.
    pub section: &'static str,
    pub language: Language,
    /// What is typed in the search box.
    pub query: String,
    /// Chapters currently unfolded in the navigation.
    expanded: BTreeSet<&'static str>,
}

impl ManualState {
    /// Open the manual on its first page.
    pub fn new(documentation: &Documentation) -> Self {
        let section = documentation.first_section().map(|s| s.id).unwrap_or("");
        let mut state = Self {
            section,
            language: Language::default(),
            query: String::new(),
            expanded: BTreeSet::new(),
        };
        // The chapter being read starts open: a navigation that shows only
        // folded titles hides where the reader is.
        if let Some(chapter) = documentation.chapter_of(section) {
            state.expanded.insert(chapter.id);
        }
        state
    }

    /// The open section, falling back to the first page if the identifier no
    /// longer exists.
    pub fn current<'a>(&self, documentation: &'a Documentation) -> Option<&'a Section> {
        documentation
            .section(self.section)
            .or_else(|| documentation.first_section())
    }

    /// Open a section, unfolding the chapter it belongs to.
    ///
    /// The identifier is kept even when unknown; `current` resolves it, so a
    /// stale link degrades to the first page instead of a blank panel.
    ///
    /// The manual is rebuilt here to find the chapter. That is a handful of
    /// vectors of static strings, built once per click — the alternative, a
    /// borrow of the document held inside the state, would tie the reader's
    /// position to a lifetime for no gain.
    pub fn select(&mut self, id: &'static str) {
        self.section = id;
        if let Some(chapter) = Documentation::build().chapter_of(id) {
            self.expanded.insert(chapter.id);
        }
    }

    // -- the navigation tree ------------------------------------------------

    pub fn is_expanded(&self, chapter: &str) -> bool {
        self.expanded.contains(chapter)
    }

    pub fn toggle_chapter(&mut self, chapter: &'static str) {
        if !self.expanded.remove(chapter) {
            self.expanded.insert(chapter);
        }
    }

    pub fn collapse_all(&mut self) {
        self.expanded.clear();
    }

    // -- previous / next ----------------------------------------------------

    /// Every section, in reading order.
    fn order(documentation: &Documentation) -> Vec<&Section> {
        documentation
            .chapters
            .iter()
            .flat_map(|chapter| chapter.sections.iter())
            .collect()
    }

    fn position(&self, documentation: &Documentation) -> usize {
        Self::order(documentation)
            .iter()
            .position(|section| section.id == self.section)
            .unwrap_or(0)
    }

    pub fn has_previous(&self, documentation: &Documentation) -> bool {
        self.position(documentation) > 0
    }

    pub fn has_next(&self, documentation: &Documentation) -> bool {
        self.position(documentation) + 1 < Self::order(documentation).len()
    }

    /// Move one page back, stopping at the beginning rather than wrapping.
    pub fn previous(&mut self, documentation: &Documentation) {
        let index = self.position(documentation);
        if index > 0 {
            let id = Self::order(documentation)[index - 1].id;
            self.select(id);
        }
    }

    /// Move one page on, stopping at the end rather than wrapping.
    pub fn next(&mut self, documentation: &Documentation) {
        let order = Self::order(documentation);
        let index = self.position(documentation);
        if index + 1 < order.len() {
            let id = order[index + 1].id;
            self.select(id);
        }
    }

    // -- language -----------------------------------------------------------

    pub fn toggle_language(&mut self) {
        self.language = self.language.other();
    }

    /// What the toggle button reads: the language it switches *to*.
    pub fn language_button(&self) -> &'static str {
        self.language.other().label()
    }

    // -- search -------------------------------------------------------------

    /// Whether the navigation should show results instead of the tree.
    pub fn is_searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Sections matching the search box, in the language being read.
    pub fn results<'a>(&self, documentation: &'a Documentation) -> Vec<&'a Section> {
        documentation.search(&self.query, self.language)
    }

    pub fn clear_search(&mut self) {
        self.query.clear();
    }
}

impl Default for ManualState {
    fn default() -> Self {
        Self::new(&Documentation::build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_state_is_a_freshly_opened_manual() {
        let documentation = Documentation::build();
        assert_eq!(ManualState::default(), ManualState::new(&documentation));
    }

    #[test]
    fn folding_a_chapter_does_not_close_the_page() {
        let documentation = Documentation::build();
        let mut state = ManualState::new(&documentation);
        let chapter = documentation.chapters[0].id;

        state.toggle_chapter(chapter);
        assert!(state.current(&documentation).is_some());
    }
}
