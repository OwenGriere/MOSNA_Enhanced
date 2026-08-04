//! Thin helpers over `plotters`.

use std::path::Path;

use plotters::prelude::*;

use crate::colormap::Rgb;
use crate::theme::{Theme, BACKGROUND, TEXT};

/// Convert one of our colours into a `plotters` one.
pub fn colour(rgb: Rgb) -> RGBColor {
    RGBColor(rgb[0], rgb[1], rgb[2])
}

/// Create the parent directory of a figure, so a first run does not fail on a
/// missing folder.
pub fn ensure_parent(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", parent.display()))?;
    }
    Ok(())
}

/// Draw a figure of `width_inches` by `height_inches` into `path`.
///
/// The canvas is filled with the theme background before `draw` runs, so a
/// figure that leaves gaps comes out white rather than transparent — a
/// transparent PNG viewed on a dark background is unreadable.
pub fn figure<F>(
    path: &Path,
    theme: &Theme,
    width_inches: f64,
    height_inches: f64,
    draw: F,
) -> anyhow::Result<()>
where
    F: FnOnce(&Surface<'_>) -> anyhow::Result<()>,
{
    ensure_parent(path)?;
    let (width, height) = theme.canvas(width_inches, height_inches);

    {
        let root = BitMapBackend::new(path, (width, height)).into_drawing_area();
        root.fill(&colour(BACKGROUND))
            .map_err(|e| anyhow::anyhow!("cannot fill {}: {e}", path.display()))?;
        draw(&root)?;
        root.present()
            .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display()))?;
    }
    Ok(())
}

/// A text style at the given point size.
pub fn label_style(theme: &Theme, points: f64) -> TextStyle<'static> {
    ("sans-serif", theme.font(points))
        .into_font()
        .color(&colour(TEXT))
}

/// A bold text style, for titles.
pub fn title_style(theme: &Theme, points: f64) -> TextStyle<'static> {
    ("sans-serif", theme.font(points))
        .into_font()
        .style(FontStyle::Bold)
        .color(&colour(TEXT))
}

/// A text style rotated a quarter turn, for a dense axis.
///
/// matplotlib rotates its column labels 45 degrees; `plotters` offers quarter
/// turns only, and 90 degrees is the standard choice for a heatmap with more
/// labels than fit side by side. Drawn horizontally they overlap into an
/// unreadable smear, which is worse than a steeper angle.
pub fn rotated_label_style(theme: &Theme, points: f64) -> TextStyle<'static> {
    ("sans-serif", theme.font(points))
        .into_font()
        .transform(FontTransform::Rotate270)
        .color(&colour(TEXT))
}

/// The drawing surface a figure is composed onto.
pub type Surface<'a> = DrawingArea<BitMapBackend<'a>, plotters::coord::Shift>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_figure_is_written_with_the_requested_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/figure.png");
        let theme = Theme { dpi: 100.0 };

        figure(&path, &theme, 4.0, 3.0, |_| Ok(())).unwrap();

        assert!(path.is_file(), "the parent directory was not created");
        let image = image::open(&path).unwrap();
        assert_eq!((image.width(), image.height()), (400, 300));
    }

    #[test]
    fn an_empty_figure_is_the_background_colour() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blank.png");
        figure(&path, &Theme::default(), 1.0, 1.0, |_| Ok(())).unwrap();

        let image = image::open(&path).unwrap().to_rgb8();
        assert!(
            image.pixels().all(|p| p.0 == BACKGROUND),
            "an untouched canvas must be the background colour"
        );
    }

    #[test]
    fn a_rotated_label_is_a_quarter_turn() {
        let style = rotated_label_style(&Theme::default(), 10.0);
        assert!(matches!(
            style.font.get_transform(),
            FontTransform::Rotate270
        ));
    }

    #[test]
    fn a_failing_draw_propagates_its_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("figure.png");
        let error = figure(&path, &Theme::default(), 1.0, 1.0, |_| {
            Err(anyhow::anyhow!("drawing failed"))
        })
        .unwrap_err();
        assert!(error.to_string().contains("drawing failed"));
    }
}
