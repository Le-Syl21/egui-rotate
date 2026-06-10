//! egui-rotate eframe demo — shared app, plus the web (wasm) entry point.

mod app;
pub use app::{install_plugin, EframeDemo};

// Web entry point. With `#[wasm_bindgen(start)]`, Trunk auto-invokes this on page
// load — `index.html` only needs the `<canvas id="the_canvas_id">`.
#[cfg(target_arch = "wasm32")]
mod web {
    use super::{install_plugin, EframeDemo};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(start)]
    pub fn start() {
        // Forward Rust panics to the browser console.
        console_error_panic_hook::set_once();

        wasm_bindgen_futures::spawn_local(async {
            let canvas = web_sys::window()
                .expect("no window")
                .document()
                .expect("no document")
                .get_element_by_id("the_canvas_id")
                .expect("missing <canvas id=\"the_canvas_id\">")
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .expect("#the_canvas_id is not a <canvas>");

            eframe::WebRunner::new()
                .start(
                    canvas,
                    eframe::WebOptions::default(),
                    Box::new(|cc| {
                        install_plugin(&cc.egui_ctx);
                        Ok(Box::new(EframeDemo::default()))
                    }),
                )
                .await
                .expect("failed to start eframe");
        });
    }
}
