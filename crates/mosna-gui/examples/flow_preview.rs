//! Renders the progress bar's gold, so it can be looked at without a screen.
//!
//! The interface cannot be screenshotted on this project's machines — Wayland
//! refuses — so the one part of it that is a *picture* rather than a layout
//! gets a way to be seen anyway. Six bars: three of a run at 62 %, at three
//! points of the cycle, and three of the band that sweeps when there is no
//! count yet.
//!
//! ```bash
//! cargo run --example flow_preview -p mosna-gui -- /tmp/flow.png
//! ```
//!
//! It draws with the same `flow` the panel draws with, so what it shows is what
//! the bar does. What it does not show is the track underneath or the rounded
//! ends, which are `egui`'s to draw.
use mosna_gui::model::flow;

fn main() {
    let (width, height, rows) = (900u32, 44u32, 6u32);
    let gap = 12u32;
    let total = rows * (height + gap);
    let mut buffer = image::RgbImage::from_pixel(width, total, image::Rgb([231, 233, 236]));

    for row in 0..rows {
        let phase = row as f32 / rows as f32;
        let top = row * (height + gap);
        // Half determinate at 62 %, half the sweeping band.
        let (start, end) = if row < 3 {
            (0.0, 0.62)
        } else {
            flow::band(phase)
        };
        for x in 0..width {
            let t = x as f32 / width as f32;
            if t < start || t > end {
                continue;
            }
            let local = (t - start) / (end - start).max(1e-6);
            let colour = if row < 3 {
                flow::shade(local, phase)
            } else {
                flow::shade(local, 0.5)
            };
            for y in top..top + height {
                buffer.put_pixel(x, y, image::Rgb([colour.r(), colour.g(), colour.b()]));
            }
        }
    }
    buffer.save(std::env::args().nth(1).unwrap()).unwrap();
    println!("written");
}
