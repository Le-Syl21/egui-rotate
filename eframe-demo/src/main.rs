//! Native entry point for the egui-rotate eframe demo.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([760.0, 580.0])
            .with_title("egui-rotate — eframe demo"),
        ..Default::default()
    };

    eframe::run_native(
        "egui-rotate eframe demo",
        options,
        Box::new(|cc| {
            egui_rotate_eframe_demo::install_plugin(&cc.egui_ctx);
            Ok(Box::new(egui_rotate_eframe_demo::EframeDemo::default()))
        }),
    )
}

// On wasm the entry point is `WebHandle` in `lib.rs`; `main` is unused.
#[cfg(target_arch = "wasm32")]
fn main() {}
