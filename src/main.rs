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
    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    // Set a panic hook to get better error messages
    console_error_panic_hook::set_once();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        use eframe::wasm_bindgen::JsCast as _;

        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(TabApp::new(cc)))),
            )
            .await;

        // Remove the loading text and spinner:
        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(_) => {
                    loading_text.remove();
                }
                Err(e) => {
                    loading_text.set_inner_html(
                        "<p>The app has crashed. See the developer console for details.</p>",
                    );
                    panic!("Failed to start eframe: {e:?}");
                }
            }
        }
    });
}
