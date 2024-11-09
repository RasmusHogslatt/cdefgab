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
