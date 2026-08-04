//! The window's icon.
//!
//! Separate from the installer's copy, and for a different reason. The
//! installer writes an icon into the desktop's theme so a *launcher* can show
//! it; this one is handed to the window itself, which is what a taskbar shows
//! for a program started any other way — from a terminal, from `cargo run`,
//! from a file manager. An install can do everything right and the running
//! window still be blank without it.
//!
//! # The format is read from the bytes, not the name
//!
//! `assets/logo.ico` is a PNG carrying an `.ico` name, which is what most icon
//! converters produce. `mosna-install` already sniffs the magic number rather
//! than trusting the extension; this does the same, and the `image` crate is
//! given the same decoders here as there, so the two cannot disagree about
//! what the shipped logo is.

use mosna_paths::layout::ICON_SIZE;

/// The shipped logo, compiled in.
///
/// Read from a path at run time it would be missing exactly when it is needed:
/// an installed copy has no `assets` directory beside it.
const LOGO: &[u8] = include_bytes!("../../../assets/logo.ico");

/// The icon to give the window, or `None` if the shipped logo cannot be read.
///
/// `None` rather than an error: a window with the default icon is a small loss,
/// and refusing to open one over it would be a large one.
pub fn window_icon() -> Option<egui::IconData> {
    decode(LOGO)
}

/// Decode an image into what egui wants: straight RGBA, and its size.
fn decode(bytes: &[u8]) -> Option<egui::IconData> {
    let image = image::load_from_memory(bytes).ok()?;
    // Down to the size the installer uses. A 550-pixel logo is a megabyte of
    // pixels held for the lifetime of the process to draw a 32-pixel corner.
    let image = image
        .resize_exact(ICON_SIZE, ICON_SIZE, image::imageops::FilterType::Lanczos3)
        .into_rgba8();

    Some(egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one that matters: the shipped file has to decode with the decoders
    /// this crate was actually built with. It is a PNG named `.ico` today, and
    /// a real `.ico` tomorrow if someone re-exports it — both must work, and
    /// the failure mode of neither is visible until a window opens.
    #[test]
    fn the_shipped_logo_decodes() {
        let icon = window_icon().expect("assets/logo.ico did not decode");
        assert_eq!(icon.width, ICON_SIZE);
        assert_eq!(icon.height, ICON_SIZE);
        assert_eq!(
            icon.rgba.len(),
            (ICON_SIZE * ICON_SIZE * 4) as usize,
            "four bytes a pixel, straight RGBA"
        );
    }

    /// Whatever the name says.
    #[test]
    fn a_real_icon_file_decodes_too() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(32, 32, image::Rgba([117, 89, 12, 255]))
            .write_to(&mut bytes, image::ImageFormat::Ico)
            .unwrap();

        assert!(decode(&bytes.into_inner()).is_some());
    }

    #[test]
    fn a_file_that_is_no_image_yields_no_icon_rather_than_a_panic() {
        assert!(decode(b"this is not an image").is_none());
    }

    /// The logo is not blank. A fully transparent icon would pass every check
    /// above and show nothing.
    #[test]
    fn the_logo_has_something_in_it() {
        let icon = window_icon().unwrap();
        let opaque = icon.rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(
            opaque > (ICON_SIZE * ICON_SIZE / 10) as usize,
            "only {opaque} pixels of the icon are visible"
        );
    }
}
