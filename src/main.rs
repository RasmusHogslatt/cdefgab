// main.rs

use crate::gui::gui::TabApp;
use egui::ViewportBuilder;

mod audio;
mod gui;
mod guitar;
mod karplus_strong;
mod music_representation;
mod renderer;
mod time_scrubber;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder {
            maximized: Some(true),
            resizable: Some(true),
            ..Default::default()
        },
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Tab App",
        native_options,
        Box::new(|cc| Ok(Box::new(TabApp::new(cc)))),
    );
}

#[cfg(target_arch = "wasm32")]
fn main() {
    // Run the eframe app on the web
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::spawn_local;

    // Set a panic hook to get better error messages
    console_error_panic_hook::set_once();

    let web_options = eframe::WebOptions::default();

    spawn_local(async move {
        eframe::start_web(
            "the_canvas_id", // The id of the canvas element in index.html
            web_options,
            Box::new(|cc| Box::new(TabApp::new(cc))),
        )
        .await
        .expect("Failed to start eframe");
    });
}
