//! Binary entry point of the interface.

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let config_path = config_path();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1600.0, 980.0])
            .with_min_inner_size([1120.0, 700.0])
            .with_title("MOSNA Graphic Interface"),
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
