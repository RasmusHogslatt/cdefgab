use egui::ViewportBuilder;
use gui::gui::TabApp;

mod audio_listener;
mod audio_player;
mod gui;
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
