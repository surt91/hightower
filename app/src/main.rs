//! Interactive demo of Hightower's line router.
//!
//! Runs natively (`cargo run --release -p hightower-app`) and in the browser
//! via trunk (`trunk serve` inside `app/`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 760.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Hightower line router",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    eframe::WebLogger::init(log::LevelFilter::Info).ok();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");
        let canvas = document
            .get_element_by_id("hightower_canvas")
            .expect("canvas element missing")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("hightower_canvas is not a canvas");

        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
            )
            .await;

        if let Some(loading) = document.get_element_by_id("loading_text") {
            match result {
                Ok(()) => loading.remove(),
                Err(err) => {
                    loading.set_inner_html(
                        "<p>The demo could not start. See the browser console for details.</p>",
                    );
                    panic!("failed to start eframe: {err:?}");
                }
            }
        }
    });
}
