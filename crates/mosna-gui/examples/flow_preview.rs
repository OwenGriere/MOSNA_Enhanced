//! Renders the progress bar's metal, so it can be looked at without a screen.
//!
//! The interface cannot be screenshotted on this project's machines — Wayland
//! refuses — so the one part of it that is a *picture* rather than a layout gets
//! a way to be seen anyway. Six bars: three of a run at 62 %, at three points of
//! the cycle, and three of the wash a run shows when it has no count yet.
//!
//! ```bash
//! cargo run --example flow_preview -p mosna-gui -- /tmp/flow.png
//! ```
//!
//! It draws with the same `flow` the panel draws with — the ramp, the grain, the
//! reflection and the bevel — so what it shows is what the bar does. What it
//! does not show is the round ends, which are `egui`'s to draw.
use mosna_gui::model::flow;

fn main() {
    let (width, height, rows) = (900u32, 26u32, 6u32);
    let gap = 14u32;
    let total = rows * (height + gap);
    let mut buffer = image::RgbImage::from_pixel(width, total, image::Rgb([216, 219, 223]));

    for row in 0..rows {
        // Three points of one cycle, so the reflection is caught arriving,
        // crossing and leaving rather than always in the same place.
        let phase = 0.28 + 0.2 * (row % 3) as f32;
        let top = row * (height + gap);
        let counted = row < 3;
        let end = if counted { 0.62 } else { 1.0 };

        for x in 0..width {
            let t = (x as f32 + 0.5) / width as f32;
            if t > end {
                // The track the bar is drawn on, so the fill is seen against
                // what it is actually seen against.
                for y in top..top + height {
                    let track = flow::TRACK;
                    buffer.put_pixel(x, y, image::Rgb([track.r(), track.g(), track.b()]));
                }
                continue;
            }

            let along = t / end;
            let point = x as f32;
            let colour = if counted {
                flow::shade(along, point, phase)
            } else {
                flow::wash(along, point, phase)
            };
            for y in top..top + height {
                let u = (y - top) as f32 / (height - 1) as f32;
                let lit = flow::lit(colour, flow::bevel(u));
                buffer.put_pixel(x, y, image::Rgb([lit.r(), lit.g(), lit.b()]));
            }
        }
    }
    buffer.save(std::env::args().nth(1).unwrap()).unwrap();
    println!("written");
}
