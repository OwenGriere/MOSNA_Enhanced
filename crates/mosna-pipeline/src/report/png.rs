//! The shape of a PNG, read from its header.
//!
//! The report gives each chart the aspect ratio of the image beside it, so a
//! tall composition heatmap is shown tall and a wide network is shown wide. The
//! two files describe the same figure, so the image's proportions are the
//! chart's — and reading eight bytes is cheaper than decoding a megapixel, or
//! than taking a decoder as a dependency to learn one number.

use std::path::Path;

/// The PNG signature, then the length and tag of the header chunk.
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Width and height in pixels, or `None` when the file is not a PNG.
///
/// The first twenty-four bytes of a PNG are the signature, then the length and
/// tag of the header chunk, then the two dimensions as big-endian integers.
pub fn dimensions(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;

    let mut header = [0u8; 24];
    std::fs::File::open(path)
        .ok()?
        .read_exact(&mut header)
        .ok()?;

    if header[..8] != SIGNATURE || &header[12..16] != b"IHDR" {
        return None;
    }

    let number = |at: usize| {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&header[at..at + 4]);
        u32::from_be_bytes(bytes)
    };
    let (width, height) = (number(16), number(20));

    // Zero would become a division by zero in an aspect ratio.
    (width > 0 && height > 0).then_some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG header and nothing else: the twenty-four bytes every PNG starts
    /// with, which is all this reads.
    fn header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    fn written(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("figure.png");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn the_size_is_read_from_the_header() {
        let (_dir, path) = written(&header(1800, 1400));
        assert_eq!(dimensions(&path), Some((1800, 1400)));
    }

    #[test]
    fn a_file_that_is_not_a_png_has_no_size() {
        let (_dir, path) = written(b"GIF89a and then some");
        assert_eq!(dimensions(&path), None);
    }

    /// A figure still being written, or one truncated by a full disk.
    #[test]
    fn a_truncated_png_has_no_size() {
        let (_dir, path) = written(&header(1800, 1400)[..20]);
        assert_eq!(dimensions(&path), None);
    }

    #[test]
    fn a_missing_file_has_no_size() {
        assert_eq!(dimensions(Path::new("/nowhere/figure.png")), None);
    }

    /// Zero would become a division by zero in an aspect ratio.
    #[test]
    fn a_degenerate_size_is_refused() {
        let (_dir, path) = written(&header(0, 100));
        assert_eq!(dimensions(&path), None);
        let (_dir, path) = written(&header(100, 0));
        assert_eq!(dimensions(&path), None);
    }

    /// Real output, if it is there: the check that the twenty-four bytes above
    /// are really what a PNG this project writes starts with.
    #[test]
    fn a_real_figure_is_read_correctly() {
        let candidates = [
            "../../target/report-fixture.png",
            "target/report-fixture.png",
        ];
        for candidate in candidates {
            if Path::new(candidate).is_file() {
                let size = dimensions(Path::new(candidate)).expect("a real PNG has a size");
                assert!(size.0 > 0 && size.1 > 0);
            }
        }
    }
}
