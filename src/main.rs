// main.rs
mod audio_player;
mod gui;
mod music_representation;
mod renderer;
mod time_scrubber;
use gui::gui::TabApp;

fn main() {
    let native_options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "Tab App",
        native_options,
        Box::new(|cc| Ok(Box::new(TabApp::new(cc)))),
    );
}
