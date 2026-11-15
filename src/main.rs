#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window on Windows release builds

mod app;
mod canvas;
mod examples;
mod io;
mod model;
mod panels;
mod sim;
mod theme;

fn main() -> eframe::Result {
    // `netlix path/to/file.netlix` opens that file (also used by OS file associations).
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("netlix")
            .with_app_id("netlix")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "netlix",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, open)))),
    )
}
