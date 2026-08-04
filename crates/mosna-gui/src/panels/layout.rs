//! How a field row divides the width it has.
//!
//! Pulled out of the panels and into arithmetic because the failure it fixes is
//! arithmetic: widths taken from constants rather than from the space actually
//! available. A label sized that way overflows its group box, the frame clips
//! whatever sticks out, and the text is not shrunk or wrapped — it is gone. No
//! amount of widening brings it back, because the constants do not move.
//!
//! Everything here is a pure function of the available width, so the rules can
//! be stated as tests rather than discovered by dragging a window.

use crate::theme;

/// Width the longest label in the shipped configuration needs on one line.
///
/// `X coordinates column for niches` is thirty-one characters, and the label
/// size averages a little under seven pixels a character. Nothing is *capped*
/// at this — it is only what the tests use to check that the panel's default
/// width shows that name without wrapping.
pub const LONGEST_LABEL_WIDTH: f32 = 218.0;

/// Space a row leaves between its last pixel and the edge of what contains it.
///
/// Two different failures want the same number of pixels, which is why there is
/// one constant and not two.
///
/// A row that used *all* of its width ended flush against the group box's
/// border. Flush reads as overflowing — the eye cannot tell a control that
/// stops at the frame from one that has been cut by it, and the report that
/// started this was exactly that: the path fields and the column names looked
/// like they came out of the Browser's frame.
///
/// And egui's scroll bars float by default: [`egui::style::ScrollStyle`]'s
/// `allocated_width` is zero for them, so the bar is painted *over* the last
/// ten pixels of the content rather than taking space beside it. A control
/// using the full width therefore really did end up underneath one.
pub const TRAILING_GAP: f32 = 10.0;

/// Smallest share of a row the control keeps, whatever the label asks for.
///
/// A *share*, not a number of pixels. A constant floor is what made the Niche
/// Analysis tab behave unlike the others: its names are the only ones long
/// enough to reach the old ceiling, so they alone wrapped while every other tab
/// laid out cleanly. Expressed as a proportion, a row behaves the same way
/// whatever is written in it and whatever the panel is dragged to.
const CONTROL_SHARE: f32 = 0.34;
/// And a floor for the pathological case, so a control cannot vanish outright
/// on a panel dragged to nothing.
const CONTROL_FLOOR: f32 = 56.0;

/// Narrowest control still worth putting *beside* its label.
///
/// Measured against what the controls actually are, not chosen for looks. A
/// path field carries a browse button of about forty pixels and has to show
/// enough of a directory to recognise it. A combo box spends its own padding
/// and its arrow — some fifty pixels — before a single character of the value
/// appears, which is how `parquet` came to be drawn as `…`.
///
/// Below this a row is better off with its label on one line and its control
/// across the full width of the next.
const CONTROL_BESIDE_FLOOR: f32 = 132.0;

/// The two column widths of one field row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldWidths {
    pub label: f32,
    pub control: f32,
}

/// How one row arranges its label and its control.
///
/// The choice is a function of the width available and of the name the row's
/// group agreed on, and both are the same for every row of that group — so a
/// group is always laid out one way or the other, never half and half.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Row {
    /// Label on the left, control on the right.
    Beside(FieldWidths),
    /// Label on its own line, control across the full width beneath it.
    Stacked { width: f32 },
}

/// Decide how a row of `available` pixels arranges itself.
///
/// Two columns while the control keeps a width it can be used at; one column
/// each once it does not. This is what lets the side panels be narrow: a panel
/// too tight for a label *and* a control beside it is not too tight for the
/// same two things stacked, and the alternative — a panel wide enough for two
/// columns whatever is in them — is width taken from the figures.
pub fn row(available: f32, spacing: f32, label_needed: f32) -> Row {
    let widths = field_widths(available, spacing, label_needed);
    if widths.control >= CONTROL_BESIDE_FLOOR {
        Row::Beside(widths)
    } else {
        Row::Stacked {
            width: content_width(available),
        }
    }
}

/// The width a row may actually use out of the `available` it was handed.
///
/// Everything that lays itself out against a container's edge goes through
/// this, so [`TRAILING_GAP`] is subtracted **once** and in one place. A row
/// that reserved it and then handed its remainder to a control that reserved it
/// again would lose twice the gap, which on a narrow panel is visible.
pub fn content_width(available: f32) -> f32 {
    (available - TRAILING_GAP).max(1.0)
}

/// What one field row has to divide between its two columns.
fn usable(available: f32, spacing: f32) -> f32 {
    (available - spacing - TRAILING_GAP).max(0.0)
}

/// Width to give a label column whose widest entry needs `needed` pixels.
///
/// **Exactly what it needs, and no more.** A column sized to a global maximum
/// would make `Extension` reserve as much room as
/// `X coordinates column for niches`, and every one of those wasted pixels is
/// one the control beside it does not get — and one more reason for the panel
/// to have to be wider.
///
/// Bounded by what is left once the control has its share and the row has left
/// its [`TRAILING_GAP`]. Under that bound the label wraps, which is legible;
/// the control has nowhere left to go.
pub fn label_column(needed: f32, available: f32, spacing: f32) -> f32 {
    let usable = usable(available, spacing);
    // What the control keeps: a share of the row, with an absolute floor that
    // only bites on a panel narrower than any usable one.
    let reserved = (usable * CONTROL_SHARE).max(CONTROL_FLOOR.min(usable * 0.5));
    needed.min(usable - reserved).max(1.0)
}

/// Margin to leave inside a container of `available` pixels.
///
/// Proportional, because a constant is a large fraction of a narrow panel and a
/// rounding error in a wide one. Bounded at both ends: below three pixels the
/// text touches the border, above twenty the margin is just width the contents
/// do not get.
pub fn margin(available: f32) -> f32 {
    (available * 0.025).clamp(4.0, 14.0)
}

/// Divide `available` between a label needing `label_needed` pixels and its
/// control, leaving `spacing` between them.
///
/// **The label is served first.** It is text that has to be read in full; the
/// control beside it is a box that works just as well narrow, and a combo box
/// truncates its caption without losing anything the user cannot recover by
/// opening it. So squeezing the panel shrinks the control and leaves the label
/// alone — which is what keeps the side panels from having to grow, and the
/// figures in the middle from having to shrink.
pub fn field_widths(available: f32, spacing: f32, label_needed: f32) -> FieldWidths {
    let usable = usable(available, spacing);
    let label = label_column(label_needed, available, spacing);
    FieldWidths {
        label,
        control: (usable - label).max(1.0),
    }
}

/// Width left for a text field standing beside a button of `button` pixels.
///
/// The button's width is passed in rather than assumed: it is set from the
/// theme's padding, which changed once already and took the old constant of
/// thirty-two pixels out from under this calculation.
pub fn text_and_button(available: f32, button: f32, spacing: f32) -> f32 {
    (available - button - spacing).max(1.0)
}

/// Width of each of `count` equal buttons sharing a row, or `None` when the row
/// cannot show them all and they belong stacked one per line instead.
///
/// `widest` is what the widest caption really needs, measured with the font
/// that draws it. The check matters because [`egui::Ui::add_sized`] is a
/// *maximum in name only*: a button whose caption does not fit is laid out at
/// its own intrinsic width and the cursor advances past the size it was given.
/// Two `Refresh …` buttons asked for half a narrow panel each, took a hundred
/// and forty pixels each, and left the second one hanging outside the frame.
pub fn buttons_in_a_row(available: f32, spacing: f32, count: usize, widest: f32) -> Option<f32> {
    if count == 0 {
        return None;
    }
    let gaps = spacing * (count - 1) as f32;
    let each = (content_width(available) - gaps) / count as f32;
    (each >= widest).then_some(each)
}

/// Width of the small browse button beside a path field.
///
/// Derived from the theme so it follows the padding instead of drifting from
/// it; the glyph itself is one character wide.
pub fn browse_button_width() -> f32 {
    theme::BUTTON_PADDING.x * 2.0 + 12.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_label_takes_only_what_it_needs() {
        // `Extension` is about sixty pixels; the rest belongs to the control.
        let widths = field_widths(400.0, 10.0, 60.0);
        assert_eq!(widths.label, 60.0);
        assert!(widths.control > 300.0, "{widths:?}");
    }

    #[test]
    fn a_long_label_leaves_the_control_its_share() {
        let widths = field_widths(1000.0, 10.0, 900.0);
        let share = widths.control / (widths.label + widths.control);
        assert!((share - CONTROL_SHARE).abs() < 0.01, "{widths:?}");
    }

    #[test]
    fn a_panel_dragged_to_nothing_still_yields_two_columns() {
        let widths = field_widths(20.0, 10.0, 500.0);
        assert!(widths.label > 0.0 && widths.control > 0.0, "{widths:?}");
    }

    #[test]
    fn the_browse_button_follows_the_theme() {
        assert!(browse_button_width() > theme::BUTTON_PADDING.x);
    }

    #[test]
    fn two_captions_that_fit_share_their_row() {
        // 300 - 10 of gap - 9 of spacing, halved, is 140 each.
        assert_eq!(buttons_in_a_row(300.0, 9.0, 2, 140.0), Some(140.5));
    }

    #[test]
    fn two_captions_that_do_not_fit_get_a_line_each() {
        assert_eq!(buttons_in_a_row(300.0, 9.0, 2, 141.0), None);
    }

    #[test]
    fn content_never_uses_the_last_pixels_of_its_container() {
        assert_eq!(content_width(240.0), 230.0);
        // And it cannot go negative on a container dragged to nothing.
        assert!(content_width(0.0) > 0.0);
    }
}
