//! Tests of the sizing scale, written before it exists.
//!
//! Font sizes were literals scattered across the panels, which is how an
//! interface ends up with eleven-pixel labels next to twelve-pixel ones for no
//! reason. A named scale makes the relationships explicit — and testable: a
//! caption must never outgrow the body text it annotates.

use mosna_gui::theme::{self, size};

/// Nothing in the interface may be smaller than this and still be read
/// comfortably on a high-density screen.
const FLOOR: f32 = 13.0;

#[test]
fn nothing_is_smaller_than_the_floor() {
    for (name, value) in size::all() {
        assert!(
            value >= FLOOR,
            "`{name}` is {value} px, below the {FLOOR} px floor"
        );
    }
}

/// The scale must descend in the order a reader expects, or the hierarchy the
/// sizes are supposed to express is a lie.
#[test]
fn the_scale_descends() {
    for pair in size::hierarchy().windows(2) {
        let ((upper, big), (lower, small)) = (pair[0], pair[1]);
        assert!(
            big > small,
            "`{upper}` ({big}) is not above `{lower}` ({small})"
        );
    }
}

/// Every step has to be visible. A difference of half a pixel reads as a
/// mistake rather than as a level.
#[test]
fn each_step_of_the_scale_is_visible() {
    for pair in size::hierarchy().windows(2) {
        let ((upper, big), (lower, small)) = (pair[0], pair[1]);
        assert!(
            big - small >= 0.9,
            "`{upper}` ({big}) and `{lower}` ({small}) are too close to tell apart"
        );
    }
}

/// The whole point of the change: everything got bigger than it was.
#[test]
fn the_scale_is_larger_than_the_egui_defaults() {
    // egui's defaults: body 12.5, button 12.5, heading 18, small 9.
    let defaults = [
        ("PAGE_TITLE", 18.0),
        ("BODY", 12.5),
        ("LABEL", 12.5),
        ("SMALL", 9.0),
        ("MONO", 12.0),
    ];
    for (name, was) in defaults {
        let now = size::all()
            .into_iter()
            .find(|(step, _)| *step == name)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("`{name}` left the scale"));
        assert!(now > was, "`{name}` did not grow: {was} -> {now}");
    }
}

// ---------------------------------------------------------------------------
// Controls
// ---------------------------------------------------------------------------

/// A button has to be big enough to hit without aiming. Forty pixels is the
/// usual minimum for a comfortable target; the interface is not touch-driven,
/// so thirty is enough — but eighteen, which is what egui gives by default,
/// is not.
#[test]
fn a_control_is_large_enough_to_hit() {
    let height = theme::MIN_INTERACT_HEIGHT;
    assert!(height >= 30.0, "controls are {height} px tall");
}

#[test]
fn a_button_has_room_around_its_label() {
    let padding = theme::BUTTON_PADDING;
    assert!(padding.x >= 10.0, "labels touch the left edge: {padding:?}");
    assert!(padding.y >= 6.0, "labels touch the top edge: {padding:?}");
}

/// A button's box must actually contain its text with room to spare.
#[test]
fn a_button_is_taller_than_the_text_inside_it() {
    let needed = size::LABEL + 2.0 * theme::BUTTON_PADDING.y;
    let height = theme::MIN_INTERACT_HEIGHT;
    assert!(
        height >= needed,
        "a {} px label needs {needed} px and the button is {height}",
        size::LABEL
    );
}

// ---------------------------------------------------------------------------
// The manual's measure
// ---------------------------------------------------------------------------

/// The user asked for the manual not to start at the very edge. A line of text
/// that begins where the panel begins is uncomfortable to read and looks
/// unfinished.
#[test]
fn the_manual_has_a_margin() {
    let margin = theme::DOC_MARGIN;
    assert!(margin >= 24.0, "the manual's margin is only {margin} px");
}

/// And a maximum measure: a line of text stretched across a wide screen is
/// hard to follow back to the next line. Typography puts the comfortable
/// maximum at roughly 60 to 90 characters.
#[test]
fn the_manual_has_a_maximum_measure() {
    let measure = theme::DOC_MAX_WIDTH;
    assert!(
        (600.0..=1000.0).contains(&measure),
        "a measure of {measure} px is outside the readable range"
    );
}

/// The four step buttons are the four things the interface exists to do, so
/// they are taller than an ordinary control.
#[test]
fn the_step_buttons_stand_out() {
    let (step, ordinary) = (theme::STEP_BUTTON_HEIGHT, theme::MIN_INTERACT_HEIGHT);
    assert!(
        step > ordinary,
        "the step buttons ({step}) are no bigger than any other control ({ordinary})"
    );
}
