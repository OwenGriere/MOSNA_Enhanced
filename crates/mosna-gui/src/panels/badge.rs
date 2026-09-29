//! The mark in the top-left corner, saying which implementation is running.
//!
//! Two applications share this working directory, this configuration file and
//! very nearly this interface: this one runs the Rust pipeline, the other
//! drives the original Python analyses. They are told apart at a glance by the
//! badge here — a Rust cog in this one, a Python mark in the other — which
//! matters most in the situation where it is easiest to get wrong, with both
//! open side by side and a figure to attribute to one of them.
//!
//! # Why it is painted rather than shipped as an image
//!
//! An image would need a file beside an installed binary, or to be compiled in
//! and decoded at start-up; and at the size this is drawn — one line of the
//! top bar — a bitmap is resampled every time the scale changes. A handful of
//! shapes is sharp at any size, follows the theme's colours, and adds nothing
//! to the build.

use crate::theme;

/// Height of the mark.
const SIZE: f32 = 22.0;

/// How many teeth the cog has.
///
/// The real mark has twelve, and twelve is also about as many as survive being
/// drawn twenty-two pixels across.
const TEETH: usize = 12;

/// Draw the badge.
pub fn show(ui: &mut egui::Ui) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(SIZE, SIZE), egui::Sense::hover());
    paint(ui.painter(), rect);
    response.on_hover_text(
        "MOSNA — the analyses run in Rust.\n\
         Steps 1 to 3 are sub-commands of the `mosna` binary; the figures are drawn by mosna_xy.",
    );
    ui.add_space(6.0);
}

/// The Rust mark: a cog with an R in the middle.
///
/// Drawn in the theme's text colour rather than in a brand colour. The mark is
/// monochrome to begin with, and on a silver interface the near-black the rest
/// of the bar is written in is exactly the contrast it wants.
pub fn paint(painter: &egui::Painter, rect: egui::Rect) {
    let centre = rect.center();
    let radius = rect.width().min(rect.height()) * 0.5;
    let ink = theme::TEXT;

    // The teeth first, so the ring is drawn over their inner ends and the two
    // read as one piece rather than as a circle with spokes stuck on it.
    for tooth in 0..TEETH {
        let angle = std::f32::consts::TAU * tooth as f32 / TEETH as f32;
        let (sin, cos) = angle.sin_cos();
        let at = egui::pos2(
            centre.x + cos * radius * 0.88,
            centre.y + sin * radius * 0.88,
        );
        painter.circle_filled(at, radius * 0.13, ink);
    }

    // The ring.
    painter.circle_stroke(centre, radius * 0.72, egui::Stroke::new(radius * 0.22, ink));
    // And the disc it encloses, in the page colour, so the letter sits on the
    // background rather than on a filled circle.
    painter.circle_filled(centre, radius * 0.60, theme::BACKGROUND);

    painter.text(
        centre,
        egui::Align2::CENTER_CENTER,
        "R",
        egui::FontId::proportional(radius * 1.05),
        ink,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shape stays inside the square the badge was given: one that did
    /// not would be painted over the working-directory caption beside it.
    #[test]
    fn the_mark_stays_inside_its_square() {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 4.0), egui::vec2(SIZE, SIZE));

        let output = ctx.run_ui(Default::default(), |ui| paint(ui.painter(), rect));

        let painted: Vec<egui::Rect> = output
            .shapes
            .iter()
            .map(|clipped| clipped.shape.visual_bounding_rect())
            .filter(|bounds| bounds.is_finite() && bounds.area() > 0.0)
            .collect();

        assert!(!painted.is_empty(), "the badge painted nothing");
        for bounds in painted {
            assert!(
                rect.expand(1.0).contains_rect(bounds),
                "{bounds:?} escapes {rect:?}"
            );
        }
    }
}
