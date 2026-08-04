//! Converting the shipped icon into what a Linux desktop expects.

use std::path::Path;

use mosna_paths::layout::ICON_SIZE;

/// Decode `from` and write it to `to` in the format `to`'s name asks for.
///
/// # The source format is read from the bytes, not the name
///
/// `image::open` picks its decoder from the file extension. That is wrong here
/// twice over: the shipped `assets/logo.ico` is a PNG carrying an `.ico` name —
/// which is what most icon converters produce — and a user replacing the logo
/// has no reason to think the extension is load-bearing. Sniffing the magic
/// number costs nothing and accepts both.
///
/// # The destination format is read from the name
///
/// A Linux desktop reads the `hicolor` theme and expects PNG there; handed an
/// icon file it shows nothing at all, silently. Windows reads its icon from the
/// shortcut and will not read a PNG, whatever the file is called. So the
/// extension of `to` decides, and [`mosna_paths::layout::Layout`] already sets
/// it per platform.
pub fn convert(from: &Path, to: &Path) -> anyhow::Result<()> {
    let bytes = std::fs::read(from)
        .map_err(|e| anyhow::anyhow!("cannot read the icon {}: {e}", from.display()))?;

    let image = image::load_from_memory(&bytes).map_err(|e| {
        anyhow::anyhow!(
            "cannot decode the icon {}: {e}\n\
             The file must be a PNG, JPEG or ICO image; its name is not what \
             decides, its contents are.",
            from.display()
        )
    })?;

    let wants_icon = to
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ico"));

    // The icon format cannot hold a side longer than 256 pixels. `ICON_SIZE` is
    // exactly that today, but clamping here means a larger theme size later
    // cannot silently produce an icon Windows refuses.
    let side = if wants_icon {
        ICON_SIZE.min(256)
    } else {
        ICON_SIZE
    };
    let resized = image.resize_exact(side, side, image::imageops::FilterType::Lanczos3);

    let format = if wants_icon {
        image::ImageFormat::Ico
    } else {
        image::ImageFormat::Png
    };

    resized
        .save_with_format(to, format)
        .map_err(|e| anyhow::anyhow!("cannot write the icon {}: {e}", to.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny PNG standing in for the shipped icon.
    fn source(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("source.png");
        let image = image::RgbaImage::from_pixel(8, 8, image::Rgba([212, 175, 55, 255]));
        image.save(&path).unwrap();
        path
    }

    /// A PNG that calls itself an icon.
    ///
    /// This is not hypothetical: `assets/logo.ico` in this repository is a PNG
    /// with an `.ico` name, and an installer that trusts the extension refuses
    /// it with a decoding error that says nothing about the real cause.
    fn png_named_ico(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("logo.ico");
        let image = image::RgbaImage::from_pixel(64, 64, image::Rgba([212, 175, 55, 255]));
        image
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        path
    }

    /// A genuine Windows icon file.
    fn real_ico(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("real.ico");
        let image = image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 0, 0, 255]));
        image
            .save_with_format(&path, image::ImageFormat::Ico)
            .unwrap();
        path
    }

    #[test]
    fn a_png_that_calls_itself_an_icon_is_still_read() {
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("mosna.png");
        convert(&png_named_ico(dir.path()), &to).expect("the contents decide, not the name");
        assert_eq!(&std::fs::read(&to).unwrap()[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn a_real_icon_file_is_read_too() {
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("mosna.png");
        convert(&real_ico(dir.path()), &to).unwrap();
        assert_eq!(&std::fs::read(&to).unwrap()[..8], b"\x89PNG\r\n\x1a\n");
    }

    /// Windows reads an icon from a shortcut, and it will not read a PNG —
    /// whatever the file is called. So a destination named `.ico` must get a
    /// real one.
    #[test]
    fn an_ico_destination_gets_a_real_ico() {
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("mosna.ico");
        convert(&source(dir.path()), &to).unwrap();

        let bytes = std::fs::read(&to).unwrap();
        assert_eq!(
            &bytes[..4],
            &[0x00, 0x00, 0x01, 0x00],
            "the destination is named .ico but does not carry the icon header"
        );
    }

    /// The icon format cannot hold a side longer than 256 pixels, and the
    /// theme size is exactly that — so the encoder must not be handed more.
    #[test]
    fn an_ico_is_within_the_format_s_limit() {
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("mosna.ico");
        convert(&source(dir.path()), &to).unwrap();

        let decoded = image::open(&to).unwrap();
        assert!(
            decoded.width() <= 256 && decoded.height() <= 256,
            "{decoded:?}"
        );
    }

    /// The failure that started this: the message has to name the file and say
    /// what was wrong with it.
    #[test]
    fn a_file_that_is_no_image_at_all_is_reported_clearly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-an-image.ico");
        std::fs::write(&path, b"this is not an image").unwrap();

        let error = convert(&path, &dir.path().join("mosna.png")).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("not-an-image.ico"), "{message}");
    }

    #[test]
    fn the_result_is_a_png_of_the_theme_size() {
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("mosna.png");
        convert(&source(dir.path()), &to).unwrap();

        let decoded = image::open(&to).unwrap();
        assert_eq!(decoded.width(), ICON_SIZE);
        assert_eq!(decoded.height(), ICON_SIZE);
    }

    #[test]
    fn the_file_carries_the_png_signature() {
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("mosna.png");
        convert(&source(dir.path()), &to).unwrap();

        let bytes = std::fs::read(&to).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn an_unreadable_source_names_itself() {
        let dir = tempfile::tempdir().unwrap();
        let error = convert(
            Path::new("/nowhere/logo.ico"),
            &dir.path().join("mosna.png"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("/nowhere/logo.ico"), "{error}");
    }
}
