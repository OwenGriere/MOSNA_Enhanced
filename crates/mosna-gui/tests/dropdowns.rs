//! Which drop-downs stay open across clicks, and which do not.
//!
//! egui closes a combo box on *any* click by default, inside the popup or
//! outside it. That is right for a menu where one value is picked — the menu's
//! work is then done, and one that stayed open would cover the value it had
//! just set — and wrong for every menu where several are ticked: choosing four
//! phenotypes meant opening the menu four times.
//!
//! The distinction cannot be checked by drawing a frame, so it is checked at
//! the source: a menu that ticks several things goes through
//! `panels::multi_select`, which is the one place the behaviour is stated.

const MULTI_SELECT: &str = "multi_select";
const BARE_COMBO: &str = "ComboBox::from_id_salt";

fn source(path: &str) -> String {
    std::fs::read_to_string(format!("{}/src/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The three menus where several values are ticked at once.
#[test]
fn every_menu_that_ticks_several_things_goes_through_the_shared_one() {
    for (path, what) in [
        ("panels/parameters.rs", "the column picker"),
        (
            "panels/sensitivity.rs",
            "the whole-number menus of the sweep",
        ),
        ("panels/modals.rs", "the confirmation dialog's column list"),
    ] {
        assert!(
            source(path).contains(MULTI_SELECT),
            "{what} does not use `panels::{MULTI_SELECT}`, so it closes on every click"
        );
    }
}

/// And the one place the behaviour is stated says which behaviour it is.
#[test]
fn the_shared_menu_closes_only_on_a_click_outside_it() {
    let text = source("panels/mod.rs");
    assert!(
        text.contains("CloseOnClickOutside"),
        "the shared menu does not set a close behaviour, so it takes egui's default"
    );
    assert!(
        !text.contains("PopupCloseBehavior::CloseOnClick,")
            && !text.contains("PopupCloseBehavior::IgnoreClicks"),
        "the shared menu sets a behaviour that is not the one it is for"
    );
}

/// The single-choice menus keep egui's default on purpose: a menu that stayed
/// open after setting its one value would cover the value it had just set.
#[test]
fn the_single_choice_menus_are_left_alone() {
    for (path, what) in [
        ("panels/viewer.rs", "the patient picker"),
        (
            "panels/browser.rs",
            "the network mode and extension pickers",
        ),
    ] {
        let text = source(path);
        assert!(
            text.contains(BARE_COMBO),
            "{what} no longer draws a combo box; this test is out of date"
        );
        assert!(
            !text.contains(MULTI_SELECT),
            "{what} was made to stay open, and it picks one value"
        );
    }
}
