mod app;
mod document;
mod object;
mod plugin;
mod render;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("IRIS"),
        ..Default::default()
    };

    eframe::run_native(
        "IRIS",
        native_options,
        Box::new(|cc| Ok(Box::new(app::IrisApp::new(cc)))),
    )
}
