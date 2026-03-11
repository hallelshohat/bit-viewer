mod app;
mod document;
mod filters;
mod viewer;

use app::BitViewerApp;

fn main() -> eframe::Result<()> {
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
        Box::new(|_cc| Ok(Box::<BitViewerApp>::default())),
    )
}
