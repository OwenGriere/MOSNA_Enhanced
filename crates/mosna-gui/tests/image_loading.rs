//! The viewer shows figures that live on disk.
//!
//! `panels::viewer` hands egui a `file://` URI and lets the loaders installed
//! by `egui_extras` fetch and decode it. Which loaders those are is decided by
//! the *features* enabled on `egui_extras` in `Cargo.toml` — a decision no
//! amount of correct drawing code can rescue, and one that fails at run time
//! with `No matching BytesLoader`, in a panel, on a user's machine.
//!
//! These tests take the same two steps the viewer takes, on a real file.

use std::path::Path;
use std::time::{Duration, Instant};

use egui::load::{BytesPoll, ImagePoll, SizeHint};

/// A four-pixel PNG on disk, and the URI the viewer would build for it.
fn a_figure_on_disk(directory: &Path) -> String {
    let path = directory.join("cluster_labels.png");
    let mut figure = image::RgbaImage::new(2, 2);
    figure.put_pixel(0, 0, image::Rgba([0xC7, 0xA5, 0x4A, 0xFF]));
    figure.save(&path).unwrap();
    // The viewer's own URI, not one written for the test: a test that spelled
    // it itself would keep passing while the panel built a different one.
    mosna_gui::model::viewer::file_uri(&path)
}

/// Poll `attempt` until it stops reporting "pending".
///
/// The file loader reads on a worker thread, so the first call always comes
/// back pending; a test that asserted on that first answer would be asserting
/// on the timing and not on the loader.
fn settled<T, E>(mut attempt: impl FnMut() -> Result<T, E>, pending: impl Fn(&T) -> bool) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match attempt() {
            Ok(value) if !pending(&value) => return value,
            Ok(_) => {}
            Err(_) if Instant::now() < deadline => {}
            Err(_) => break,
        }
        assert!(Instant::now() < deadline, "the loader never settled");
        std::thread::sleep(Duration::from_millis(10));
    }
    attempt().ok().expect("the loader reported an error")
}

/// The failure the interface actually shows: the bytes never arrive, because
/// nothing installed knows what to do with a `file://` URI.
#[test]
fn a_figure_is_read_from_its_file_uri() {
    let directory = tempfile::tempdir().unwrap();
    let uri = a_figure_on_disk(directory.path());

    let ctx = egui::Context::default();
    egui_extras::install_image_loaders(&ctx);

    // The error this asserts against is the one the user sees in the viewer,
    // so it is worth naming rather than folding into `unwrap`.
    if let Err(error) = ctx.try_load_bytes(&uri) {
        panic!("the figure could not be read: {error}");
    }

    let bytes = settled(
        || ctx.try_load_bytes(&uri),
        |poll| matches!(poll, BytesPoll::Pending { .. }),
    );
    match bytes {
        BytesPoll::Ready { bytes, .. } => {
            assert!(bytes.starts_with(b"\x89PNG"), "the file came back wrong")
        }
        BytesPoll::Pending { .. } => unreachable!("settled by construction"),
    }
}

/// And the bytes have to be decodable: the figures the pipeline writes are
/// PNGs, so the PNG decoder must be compiled in.
#[test]
fn a_figure_is_decoded_once_it_is_read() {
    let directory = tempfile::tempdir().unwrap();
    let uri = a_figure_on_disk(directory.path());

    let ctx = egui::Context::default();
    egui_extras::install_image_loaders(&ctx);

    let image = settled(
        || ctx.try_load_image(&uri, SizeHint::default()),
        |poll| matches!(poll, ImagePoll::Pending { .. }),
    );
    match image {
        ImagePoll::Ready { image } => assert_eq!(image.size, [2, 2]),
        ImagePoll::Pending { .. } => unreachable!("settled by construction"),
    }
}

/// The manual's own figures are compiled into the binary and handed over as
/// bytes, so they must keep working through a different loader entirely.
#[test]
fn a_figure_compiled_into_the_binary_is_decoded_too() {
    let ctx = egui::Context::default();
    egui_extras::install_image_loaders(&ctx);

    let asset = mosna_gui::docs::assets::EMBEDDED[0];
    let uri = format!("bytes://{asset}");
    ctx.include_bytes(
        uri.clone(),
        mosna_gui::docs::assets::image(asset).expect("a listed figure"),
    );

    let image = settled(
        || ctx.try_load_image(&uri, SizeHint::default()),
        |poll| matches!(poll, ImagePoll::Pending { .. }),
    );
    assert!(matches!(image, ImagePoll::Ready { .. }));
}
