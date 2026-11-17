#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autosave;
mod blocks;
mod code;
mod edit;
mod format;
mod library;
mod model;
mod pdf;
mod pets;
mod render;
mod settings;
mod theme;

use eframe::egui;

fn main() -> eframe::Result {
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("Hazelnut")
            .with_app_id("hazelnut")
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/hazelnut.png"))
                .expect("bundled Hazelnut icon"))
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([680.0, 440.0])
            // Hazelnut draws its own title bar (see app/titlebar.rs).
            .with_decorations(false),
        ..Default::default()
    };

    eframe::run_native(
        "Hazelnut",
        options,
        Box::new(move |cc| Ok(Box::new(app::HazelnutApp::new(cc, path)))),
    )
}
