//! Figure styling — port of `package/utils/style_figures.py`.

use crate::colormap::Rgb;

/// Background of every figure: white, as `style_figures.py` sets.
pub const BACKGROUND: Rgb = [0xff, 0xff, 0xff];
/// Body text.
pub const TEXT: Rgb = [0x1a, 0x1a, 0x2e];
/// Axis lines and ticks.
pub const EDGE: Rgb = [0xcc, 0xcc, 0xcc];
pub const TICK: Rgb = [0x2d, 0x2d, 0x2d];
/// Fill of a cell with no data, in the mean-assortativity figure.
pub const EMPTY_CELL: Rgb = [0xf0, 0xf0, 0xf0];

/// Sizes and resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// Pixels per inch, converting matplotlib's figure sizes into a canvas.
    pub dpi: f64,
}

impl Default for Theme {
    /// # Resolution
    ///
    /// The Python saves at `dpi=300`, which turns its `figsize=(30, 30)`
    /// network plot into a 9000x9000 image — 243 MB of pixels while it is
    /// being drawn. The pipeline renders samples in parallel, so on a
    /// sixteen-core machine that is nearly four gigabytes of canvases at once.
    ///
    /// The default here is 100 dpi, giving a 3000x3000 network figure: still
    /// far beyond a screen, still zoomable into a ten-thousand-cell network,
    /// and a sixteenth of the memory. Set `dpi` to 300 to match the Python's
    /// pixel dimensions exactly.
    fn default() -> Self {
        Self { dpi: 100.0 }
    }
}

impl Theme {
    /// Convert a matplotlib figure size, in inches, into a pixel canvas.
    pub fn canvas(&self, width_inches: f64, height_inches: f64) -> (u32, u32) {
        let scale = |inches: f64| (inches * self.dpi).round().max(64.0) as u32;
        (scale(width_inches), scale(height_inches))
    }

    /// A font size in points, scaled to this resolution.
    ///
    /// matplotlib's sizes are in points at 72 per inch; at a different dpi the
    /// text has to grow with the canvas or it becomes unreadable.
    pub fn font(&self, points: f64) -> u32 {
        ((points * self.dpi / 72.0).round() as u32).max(6)
    }

    /// A line width in points, scaled to this resolution.
    pub fn stroke(&self, points: f64) -> u32 {
        ((points * self.dpi / 72.0).round() as u32).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_resolution_keeps_the_network_figure_affordable() {
        let (width, height) = Theme::default().canvas(30.0, 30.0);
        assert_eq!((width, height), (3000, 3000));

        let bytes = width as u64 * height as u64 * 3;
        assert!(bytes < 32_000_000, "a canvas of {bytes} bytes is too large");
    }

    #[test]
    fn the_python_resolution_can_be_asked_for() {
        let theme = Theme { dpi: 300.0 };
        assert_eq!(theme.canvas(30.0, 30.0), (9000, 9000));
        assert_eq!(theme.canvas(18.0, 9.0), (5400, 2700));
    }

    #[test]
    fn text_grows_with_the_canvas() {
        let small = Theme { dpi: 72.0 };
        let large = Theme { dpi: 144.0 };
        assert_eq!(small.font(20.0), 20);
        assert_eq!(large.font(20.0), 40);
    }

    #[test]
    fn nothing_collapses_to_zero() {
        let tiny = Theme { dpi: 1.0 };
        let (width, height) = tiny.canvas(0.01, 0.01);
        assert!(width >= 64 && height >= 64);
        assert!(tiny.font(1.0) >= 6);
        assert!(tiny.stroke(0.1) >= 1);
    }
}
