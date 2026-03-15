mod app;
mod autocorrelation;
mod document;
mod export;
mod filters;
mod run_histogram;
mod viewer;

use app::BitViewerApp;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let initial_path = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Bit Viewer Desktop")
            .with_inner_size([1600.0, 900.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Bit Viewer Desktop",
        options,
        Box::new(move |cc| Ok(Box::new(BitViewerApp::new(cc, initial_path.clone())))),
    )
}
