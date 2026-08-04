//! Tests of how a field row divides the width it has.
//!
//! Two failures to avoid, pulling in opposite directions.
//!
//! The first: widths taken from constants rather than from the space actually
//! available. A label sized that way overflows its group box, the frame clips
//! what sticks out, and the text is not shrunk or wrapped — it is gone.
//!
//! The second, which is the answer to the first done badly: making the panels
//! wider until everything fits. The side panels are not the point of the
//! window; the figures in the middle are. So the label is given what it needs
//! and **the control gives way**, rather than the panel growing.

use mosna_gui::panels::layout::{
    buttons_in_a_row, content_width, field_widths, label_column, margin, text_and_button,
    LONGEST_LABEL_WIDTH,
};
use mosna_gui::theme;

/// The gap a real row leaves between its two columns.
const SPACING: f32 = 9.0;

/// The clearance a row has to leave at its far end.
///
/// Written out as a number rather than read from the interface's own
/// `TRAILING_GAP`, which is what it is checking: a test that
/// takes its expectation from the code it is testing cannot fail when that code
/// changes, and this one did not until it was written this way.
///
/// Ten pixels is what the interface was asked for, and it happens to be exactly
/// the width of the scroll bar egui floats *over* the content — see
/// [`the_clearance_covers_a_floating_scroll_bar`].
const CLEARANCE: f32 = 10.0;

/// What a panel of `width` leaves inside a group box, once the panel's own
/// margin and the box's have been paid for.
fn inside_a_group(width: f32) -> f32 {
    width - 2.0 * theme::PANEL_MARGIN - 2.0 * theme::GROUP_MARGIN
}

// ---------------------------------------------------------------------------
// Nothing is ever clipped
// ---------------------------------------------------------------------------

/// Whatever the panel's width, the two columns and the gap between them fit
/// inside it.
#[test]
fn a_row_never_overflows_the_space_it_was_given() {
    for available in [80.0, 120.0, 200.0, 320.0, 400.0, 520.0, 900.0, 2000.0] {
        let widths = field_widths(available, SPACING, LONGEST_LABEL_WIDTH);
        let used = widths.label + widths.control + SPACING;
        assert!(
            used <= available + 0.01,
            "at {available} px the row uses {used} px (label {}, control {})",
            widths.label,
            widths.control
        );
    }
}

/// And it does not merely fit: it **stops short of the edge**.
///
/// A row that ended flush against the group box's border read as overflowing
/// it — that is what was reported about the Browser's path fields and column
/// names. The same pixels also keep the row out from under egui's scroll bar,
/// which floats over the content instead of taking space beside it.
#[test]
fn a_row_stops_short_of_the_edge_of_what_contains_it() {
    for available in [120.0, 200.0, 260.0, 300.0, 400.0, 900.0, 2000.0] {
        let widths = field_widths(available, SPACING, LONGEST_LABEL_WIDTH);
        let right_edge = widths.label + SPACING + widths.control;
        assert!(
            right_edge <= available - CLEARANCE + 0.01,
            "at {available} px the row reaches {right_edge} px, leaving only {} px \
             of clearance",
            available - right_edge
        );
    }
}

/// And that clearance has to be at least as wide as the scroll bar it hides
/// under, or the last few pixels of every control are still overpainted.
#[test]
fn the_clearance_covers_a_floating_scroll_bar() {
    let bar = egui::style::ScrollStyle::floating().bar_width;
    assert!(
        CLEARANCE >= bar,
        "a {CLEARANCE} px clearance does not cover a {bar} px scroll bar"
    );
}

/// Neither column may collapse or go negative — a negative width is what
/// `available_width() - 32.0` produces in a narrow panel.
#[test]
fn neither_column_ever_vanishes() {
    for available in [0.0, 1.0, 20.0, 60.0, 100.0] {
        let widths = field_widths(available, SPACING, LONGEST_LABEL_WIDTH);
        assert!(
            widths.label > 0.0,
            "label is {} at {available}",
            widths.label
        );
        assert!(
            widths.control > 0.0,
            "control is {} at {available}",
            widths.control
        );
    }
}

// ---------------------------------------------------------------------------
// The label is served first
// ---------------------------------------------------------------------------

/// At every width a panel can be put at — its default included — the longest
/// parameter name takes **at most two lines**.
///
/// The rule is that the label never loses a word, not that it never takes two.
/// Buying the one-line version cost a four-hundred-pixel panel, and the panels
/// are the frame, not the picture: `X coordinates column for niches` on two
/// lines is a smaller price than a quarter of the window.
#[test]
fn the_longest_name_takes_at_most_two_lines_anywhere_in_a_panel_s_range() {
    for width in [
        theme::PARAMETERS_MIN_WIDTH,
        theme::PARAMETERS_WIDTH,
        theme::PARAMETERS_MAX_WIDTH,
    ] {
        let widths = field_widths(inside_a_group(width), SPACING, LONGEST_LABEL_WIDTH);
        assert!(
            widths.label >= LONGEST_LABEL_WIDTH / 2.0,
            "in a {width} px panel the label column is {} px — not even half of \
             the {LONGEST_LABEL_WIDTH} px name, so it would take three lines",
            widths.label
        );
    }
}

/// And the label is still never *cut*: whatever the panel is dragged to, the
/// row fits inside the group box.
#[test]
fn the_longest_name_wraps_rather_than_being_cut_at_the_minimum_width() {
    let available = inside_a_group(theme::PARAMETERS_MIN_WIDTH);
    let widths = field_widths(available, SPACING, LONGEST_LABEL_WIDTH);
    assert!(
        widths.label + widths.control + SPACING <= available + 0.01,
        "the row does not fit inside the group box"
    );
}

/// And once the label has all it needs, the rest goes to the control rather
/// than to more empty label column.
#[test]
fn the_label_stops_growing_once_it_has_enough() {
    let wide = field_widths(900.0, SPACING, LONGEST_LABEL_WIDTH);
    let wider = field_widths(2000.0, SPACING, LONGEST_LABEL_WIDTH);
    assert_eq!(
        wide.label, wider.label,
        "the label kept growing pointlessly"
    );
    assert!(wider.control > wide.control, "the extra width went nowhere");
}

// ---------------------------------------------------------------------------
// Nothing is pinned to a constant
// ---------------------------------------------------------------------------

/// A name longer than any the interface ships must still get the room the
/// panel can spare. Capping the label at a fixed number of pixels is what made
/// the Niche Analysis tab behave differently from the others: its names are the
/// only ones long enough to reach the cap, so they alone wrapped.
#[test]
fn a_very_long_name_is_not_cut_off_at_a_fixed_ceiling() {
    let narrow = field_widths(400.0, SPACING, 900.0).label;
    let wide = field_widths(800.0, SPACING, 900.0).label;
    assert!(
        wide > narrow * 1.5,
        "a long name got {narrow} px in a 400 px row and only {wide} in an \
         800 px one — it is meeting a ceiling instead of following the panel"
    );
}

/// Everything follows the panel: double the room, double both columns.
#[test]
fn both_columns_scale_with_the_panel() {
    let small = field_widths(400.0, SPACING, 900.0);
    let large = field_widths(800.0, SPACING, 900.0);

    let ratio = |before: f32, after: f32| after / before;
    assert!(
        (ratio(small.label, large.label) - 2.0).abs() < 0.15,
        "the label did not scale: {} -> {}",
        small.label,
        large.label
    );
    assert!(
        (ratio(small.control, large.control) - 2.0).abs() < 0.15,
        "the control did not scale: {} -> {}",
        small.control,
        large.control
    );
}

/// However long the name, the control keeps a real share of the row rather
/// than a token few pixels.
#[test]
fn the_control_keeps_a_share_of_the_row_not_a_constant() {
    for available in [300.0, 500.0, 900.0] {
        let widths = field_widths(available, SPACING, 5000.0);
        let share = widths.control / (widths.label + widths.control);
        assert!(
            share > 0.25,
            "at {available} px the control gets only {:.0}% of the row",
            share * 100.0
        );
    }
}

/// The margin follows the panel too. A constant margin is a large fraction of
/// a narrow panel and a rounding error in a wide one.
#[test]
fn the_margin_follows_the_panel() {
    let narrow = margin(280.0);
    let wide = margin(900.0);
    assert!(wide > narrow, "the margin is fixed: {narrow} then {wide}");
    assert!(
        narrow >= 3.0,
        "a margin of {narrow} px lets the text touch the border"
    );
    assert!(wide <= 20.0, "a margin of {wide} px is wasted width");
}

// ---------------------------------------------------------------------------
// The control gives way, not the panel
// ---------------------------------------------------------------------------

/// Squeezing the panel shrinks the control and leaves the label alone: the
/// label is text that has to be read, the control is a box that can be narrow.
#[test]
fn squeezing_the_panel_shrinks_the_control_first() {
    let roomy = field_widths(520.0, SPACING, LONGEST_LABEL_WIDTH);
    let tight = field_widths(400.0, SPACING, LONGEST_LABEL_WIDTH);

    assert_eq!(
        roomy.label, tight.label,
        "the label gave way before the control did"
    );
    assert!(
        tight.control < roomy.control,
        "the control did not take the loss"
    );
}

/// A control still has to be usable, but "usable" here is narrower than it was:
/// a combo box shows its caption truncated and its arrow, and that is enough.
#[test]
fn the_control_keeps_a_usable_width() {
    // At the panel's *default* width, which is the width almost everybody will
    // use. Dragged narrower it gets narrower, as everything else does.
    let widths = field_widths(
        inside_a_group(theme::PARAMETERS_WIDTH),
        SPACING,
        LONGEST_LABEL_WIDTH,
    );
    // Enough for a combo box's own padding and its arrow, plus a few characters
    // of the caption. Below that the control says nothing at all.
    assert!(
        widths.control >= 80.0,
        "the control is only {} px wide at the default panel width",
        widths.control
    );
}

// ---------------------------------------------------------------------------
// The panels stay out of the way
// ---------------------------------------------------------------------------

/// The side panels exist to drive the analysis; the figures in the middle are
/// what the user looks at. So each panel's default is the narrowest width that
/// shows its content comfortably — not the widest that looks roomy. How far it
/// can then be *dragged* is a separate question, answered below.
#[test]
fn a_panel_defaults_to_the_narrowest_comfortable_width() {
    // Not a great deal of slack: the control keeps its share of the row and no
    // more, and the label takes the rest.
    let needed = inside_a_group(theme::PARAMETERS_WIDTH);
    let slack = field_widths(needed, SPACING, LONGEST_LABEL_WIDTH).control;
    assert!(
        slack < needed * 0.6,
        "the control gets {slack} px of a {needed} px row — the panel is wider \
         than its contents need, and that width is taken from the figures"
    );

    for (name, width, minimum) in [
        (
            "parameters",
            theme::PARAMETERS_WIDTH,
            theme::PARAMETERS_MIN_WIDTH,
        ),
        ("browser", theme::BROWSER_WIDTH, theme::BROWSER_MIN_WIDTH),
    ] {
        assert!(
            width >= minimum,
            "{name} defaults below its own minimum: {width} < {minimum}"
        );
    }
}

/// Together the two panels must leave the middle the larger share of a window.
/// They are the frame, not the picture.
///
/// Checked on a modest laptop screen as well as a roomy desktop one: it is the
/// small window where two fixed side panels crowd the figures out, and a
/// threshold that only holds at 1600 px says nothing about the machine the
/// interface is actually being run on.
#[test]
fn the_middle_keeps_most_of_the_window() {
    let sides = theme::BROWSER_WIDTH + theme::PARAMETERS_WIDTH;
    for window in [1280.0, 1600.0, 1920.0] {
        assert!(
            sides < window * 0.45,
            "the side panels take {sides} px of {window} — {:.0}% of the window \
             before a single figure is drawn",
            100.0 * sides / window
        );
    }
}

/// A panel must be draggable across a real range, not nudged inside a narrow
/// one. The user resizes to suit the screen and the data, not to suit a
/// constant somebody picked.
#[test]
fn a_panel_can_be_resized_across_a_wide_range() {
    for (name, min, max) in [
        (
            "parameters",
            theme::PARAMETERS_MIN_WIDTH,
            theme::PARAMETERS_MAX_WIDTH,
        ),
        (
            "browser",
            theme::BROWSER_MIN_WIDTH,
            theme::BROWSER_MAX_WIDTH,
        ),
    ] {
        assert!(
            max >= min * 2.5,
            "{name} can only be dragged from {min} to {max} — barely a range"
        );
    }
}

/// Dragging a panel wider is still allowed — flexibility was never the problem.
#[test]
fn a_panel_can_still_be_widened() {
    for (name, default, max) in [
        (
            "parameters",
            theme::PARAMETERS_WIDTH,
            theme::PARAMETERS_MAX_WIDTH,
        ),
        ("browser", theme::BROWSER_WIDTH, theme::BROWSER_MAX_WIDTH),
    ] {
        assert!(
            max > default,
            "{name} cannot be dragged wider than {default}"
        );
    }
}

// ---------------------------------------------------------------------------
// A row of buttons
// ---------------------------------------------------------------------------

/// `Ui::add_sized` is a maximum in name only. A button whose caption does not
/// fit the size it is handed lays itself out at its own intrinsic width, and
/// the cursor advances past what was asked for — which is how the second of the
/// Browser's two `Refresh` buttons ended up hanging outside the panel. So a row
/// of buttons is a row only while every caption fits in its share of it.
#[test]
fn buttons_share_a_row_only_while_their_captions_fit() {
    let caption = 140.0;
    assert!(buttons_in_a_row(400.0, SPACING, 2, caption).is_some());
    assert!(
        buttons_in_a_row(200.0, SPACING, 2, caption).is_none(),
        "two {caption} px captions were fitted into a 200 px row"
    );
}

/// And when they do share one, the row stays inside the panel — clearance
/// included, like every other row.
#[test]
fn a_row_of_buttons_stays_inside_the_panel() {
    for available in [200.0, 260.0, 400.0, 900.0] {
        let Some(width) = buttons_in_a_row(available, SPACING, 2, 80.0) else {
            continue;
        };
        let used = 2.0 * width + SPACING;
        assert!(
            used <= available - CLEARANCE + 0.01,
            "at {available} px two buttons of {width} px use {used} px"
        );
    }
}

/// A button given a line to itself leaves the same clearance a row does.
#[test]
fn a_stacked_button_leaves_the_same_clearance() {
    assert!(content_width(240.0) <= 240.0 - CLEARANCE + 0.01);
    assert!(content_width(0.0) > 0.0, "a button of negative width");
}

// ---------------------------------------------------------------------------
// A text field beside a browse button
// ---------------------------------------------------------------------------

/// A short name must not reserve a long name's column: those pixels belong to
/// the control, and reserving them is what forces a panel to be wider than its
/// contents need.
#[test]
fn a_short_label_does_not_reserve_a_long_label_s_column() {
    let available = inside_a_group(theme::BROWSER_WIDTH);
    let short = field_widths(available, SPACING, 70.0);
    let long = field_widths(available, SPACING, LONGEST_LABEL_WIDTH);

    assert_eq!(short.label, 70.0, "the short label was padded out");
    assert!(
        short.control > long.control,
        "the room the short label did not need went nowhere"
    );
}

/// The column is shared by a group's rows, so it is the widest of them that
/// sets it — otherwise the controls form a ragged edge.
#[test]
fn the_column_is_set_by_the_widest_name_of_the_group() {
    let available = inside_a_group(theme::BROWSER_WIDTH);
    let widest = [70.0f32, 130.0, 95.0].into_iter().fold(0.0f32, f32::max);
    assert_eq!(label_column(widest, available, SPACING), 130.0);
}

#[test]
fn a_field_and_its_button_fit_together() {
    for available in [60.0, 120.0, 300.0, 700.0] {
        let button = 38.0;
        let text = text_and_button(available, button, SPACING);
        assert!(text > 0.0, "the field vanished at {available} px");
        assert!(
            text + button + SPACING <= available + 0.01,
            "at {available} px, {text} + {button} + {SPACING} overflows"
        );
    }
}

#[test]
fn a_path_field_grows_with_the_panel() {
    let button = 38.0;
    assert!(text_and_button(500.0, button, SPACING) > text_and_button(300.0, button, SPACING));
}
