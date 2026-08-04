//! Tests of how the reader moves through the manual, written before the
//! implementation.
//!
//! The rendering itself cannot be asserted — a pixel is not a test — but
//! everything the buttons do is state, and state is testable: which page is
//! open, which language is shown, what the search box finds, what the previous
//! and next buttons reach.

use mosna_gui::docs::model::Language;
use mosna_gui::docs::state::ManualState;
use mosna_gui::docs::Documentation;

fn manual() -> (Documentation, ManualState) {
    let documentation = Documentation::build();
    let state = ManualState::new(&documentation);
    (documentation, state)
}

// ---------------------------------------------------------------------------
// Opening
// ---------------------------------------------------------------------------

/// A reader who opens the tab must land on a page, never on nothing.
#[test]
fn the_manual_opens_on_its_first_page() {
    let (documentation, state) = manual();
    assert_eq!(
        state.current(&documentation).map(|s| s.id),
        documentation.first_section().map(|s| s.id)
    );
    assert_eq!(state.language, Language::English);
}

/// The chapter holding the open page starts unfolded, so the reader can see
/// where they are.
#[test]
fn the_open_chapter_starts_unfolded() {
    let (documentation, state) = manual();
    let chapter = documentation.chapters[0].id;
    assert!(state.is_expanded(chapter));
}

// ---------------------------------------------------------------------------
// Navigating
// ---------------------------------------------------------------------------

#[test]
fn selecting_a_section_opens_it() {
    let (documentation, mut state) = manual();
    state.select("install-windows");
    assert_eq!(
        state.current(&documentation).map(|s| s.id),
        Some("install-windows")
    );
}

/// Following a link into a folded chapter must unfold it, or the navigation
/// shows no highlight at all.
#[test]
fn opening_a_section_unfolds_its_chapter() {
    let (documentation, mut state) = manual();
    let last = documentation.chapters.last().unwrap();
    state.collapse_all();
    state.select(last.sections[0].id);
    assert!(state.is_expanded(last.id));
}

/// Previous and next walk the whole manual in reading order, chapters included.
#[test]
fn previous_and_next_walk_the_whole_manual() {
    let (documentation, mut state) = manual();

    let order: Vec<&str> = documentation
        .chapters
        .iter()
        .flat_map(|chapter| chapter.sections.iter())
        .map(|section| section.id)
        .collect();

    for expected in order.iter().skip(1) {
        state.next(&documentation);
        assert_eq!(state.current(&documentation).map(|s| s.id), Some(*expected));
    }

    for expected in order.iter().rev().skip(1) {
        state.previous(&documentation);
        assert_eq!(state.current(&documentation).map(|s| s.id), Some(*expected));
    }
}

/// The buttons are disabled at the ends rather than wrapping around, which
/// would silently send a reader back to the beginning.
#[test]
fn the_ends_of_the_manual_are_dead_ends() {
    let (documentation, mut state) = manual();
    assert!(!state.has_previous(&documentation));
    assert!(state.has_next(&documentation));

    let last = documentation
        .chapters
        .last()
        .unwrap()
        .sections
        .last()
        .unwrap();
    state.select(last.id);
    assert!(state.has_previous(&documentation));
    assert!(!state.has_next(&documentation));

    // And pressing anyway changes nothing.
    state.next(&documentation);
    assert_eq!(state.current(&documentation).map(|s| s.id), Some(last.id));
}

/// A chapter is folded and unfolded by clicking its title.
#[test]
fn a_chapter_folds_and_unfolds() {
    let (documentation, mut state) = manual();
    let chapter = documentation.chapters[0].id;

    state.toggle_chapter(chapter);
    assert!(!state.is_expanded(chapter));
    state.toggle_chapter(chapter);
    assert!(state.is_expanded(chapter));
}

// ---------------------------------------------------------------------------
// The language button
// ---------------------------------------------------------------------------

/// Switching language keeps the reader on the page they were reading. Sending
/// them back to the top would make the button useless for comparing.
#[test]
fn switching_language_stays_on_the_same_page() {
    let (documentation, mut state) = manual();
    state.select("parameters-niches");

    state.toggle_language();
    assert_eq!(state.language, Language::French);
    assert_eq!(
        state.current(&documentation).map(|s| s.id),
        Some("parameters-niches")
    );

    state.toggle_language();
    assert_eq!(state.language, Language::English);
}

/// The button says which language it switches *to*, not the one being read —
/// a button labelled with the current state reads as already pressed.
#[test]
fn the_language_button_announces_where_it_goes() {
    let (_, mut state) = manual();
    assert_eq!(state.language_button(), "Français");
    state.toggle_language();
    assert_eq!(state.language_button(), "English");
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn searching_narrows_the_navigation() {
    let (documentation, mut state) = manual();
    state.query = "install".into();

    let results = state.results(&documentation);
    assert!(!results.is_empty());
    assert!(
        results.iter().any(|section| section.id == "install-linux"),
        "{:?}",
        results.iter().map(|s| s.id).collect::<Vec<_>>()
    );
}

/// Search follows the language on screen: a French reader typing a French word
/// must find the French text.
#[test]
fn searching_follows_the_chosen_language() {
    let (documentation, mut state) = manual();
    state.query = "voisinage".into();
    assert!(
        state.results(&documentation).is_empty(),
        "a French word has no business matching the English text"
    );

    state.toggle_language();
    assert!(!state.results(&documentation).is_empty());
}

/// An empty box means "show me everything", not "show me nothing".
#[test]
fn an_empty_search_is_not_a_filter() {
    let (documentation, state) = manual();
    assert!(state.query.is_empty());
    assert!(!state.is_searching());
    assert!(state.results(&documentation).is_empty());
}

#[test]
fn a_search_that_finds_nothing_says_so() {
    let (documentation, mut state) = manual();
    state.query = "quantum chromodynamics".into();
    assert!(state.is_searching());
    assert!(state.results(&documentation).is_empty());
}

/// Clearing the box returns to the ordinary navigation without moving the page.
#[test]
fn clearing_the_search_keeps_the_open_page() {
    let (documentation, mut state) = manual();
    state.select("results-niches");
    state.query = "niche".into();
    state.clear_search();

    assert!(!state.is_searching());
    assert_eq!(
        state.current(&documentation).map(|s| s.id),
        Some("results-niches")
    );
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

/// A stale identifier — from a link that no longer exists — must not leave the
/// panel blank.
#[test]
fn an_unknown_page_falls_back_to_the_first() {
    let (documentation, mut state) = manual();
    state.select("no-such-section");
    assert_eq!(
        state.current(&documentation).map(|s| s.id),
        documentation.first_section().map(|s| s.id),
        "an unknown page should fall back rather than show nothing"
    );
}
