//! Binary entry point of the interface.

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let config_path = config_path();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1600.0, 980.0])
        .with_min_inner_size([1120.0, 700.0])
        .with_title("MOSNA Graphic Interface")
        // The name of the desktop entry the installer writes, which is how a
        // Wayland compositor finds the icon for a running window: it matches
        // the window's application id against `mosna.desktop`. `StartupWMClass`
        // in that file does the same job on X11. Without this the taskbar has
        // nothing to match and shows a blank window, however well the icon was
        // installed.
        .with_app_id(mosna_paths::layout::DESKTOP_ID);

    // And the icon on the window itself, for every way of starting it that
    // does not go through the desktop entry at all.
    if let Some(icon) = mosna_gui::icon::window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "MOSNA Graphic Interface",
        options,
        Box::new(|cc| {
            // Lets the viewer load PNG and JPEG figures from disk.
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(mosna_gui::MosnaApp::new(config_path)))
        }),
    )
}

/// Where `configuration.yaml` lives.
///
/// The precedence order — an explicit argument, `MOSNA_CONFIG`, the user's own
/// copy, the installed copy, the repository — lives in `mosna-paths`, so the
/// interface, the command line tool and the installer cannot disagree on it.
fn config_path() -> PathBuf {
    let explicit = std::env::args().nth(1).map(PathBuf::from);
    let environment = mosna_paths::Environment::detect();
    mosna_paths::config_file::resolve(explicit.as_deref(), &environment)
}
