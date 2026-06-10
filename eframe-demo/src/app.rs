//! Shared eframe application for the egui-rotate demo (native + web).
//!
//! Shows the plugin doing per-window rotation: the main window and a **child
//! window** each rotate independently, and an animated stress test demonstrates
//! that the per-frame rotation cost is negligible.

use eframe::egui;
use egui::{Color32, Pos2, Sense, TextureHandle, Vec2, ViewportBuilder, ViewportClass, ViewportId};
use egui_rotate::{Rotation, RotationPlugin};

/// Stable id of the child viewport (its own rotation is keyed on this).
fn child_id() -> ViewportId {
    ViewportId::from_hash_of("egui_rotate::child_window")
}

pub struct EframeDemo {
    root_rotation: Rotation,
    child_rotation: Rotation,
    show_child: bool,
    shapes: usize,
    ferris: Option<TextureHandle>,
}

impl Default for EframeDemo {
    fn default() -> Self {
        Self {
            root_rotation: Rotation::CW90,
            child_rotation: Rotation::CW270,
            show_child: false,
            shapes: 600,
            ferris: None,
        }
    }
}

/// Register the plugin on a fresh context (call once at startup).
pub fn install_plugin(ctx: &egui::Context) {
    ctx.add_plugin(RotationPlugin::new(Rotation::CW90));
}

fn load_ferris(ctx: &egui::Context) -> TextureHandle {
    let bytes = include_bytes!("../assets/ferris.png");
    let image = image::load_from_memory(bytes)
        .expect("decode ferris.png")
        .to_rgba8();
    let (w, h) = image.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], image.as_raw());
    ctx.load_texture("ferris", color, egui::TextureOptions::LINEAR)
}

impl eframe::App for EframeDemo {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Animate continuously so the FPS readout is meaningful.
        ctx.request_repaint();

        // Mirror our state into the plugin: one rotation per viewport.
        let handle = ctx.plugin::<RotationPlugin>();
        let mut plugin = handle.lock();
        plugin.set_rotation(self.root_rotation);
        plugin.set_viewport_rotation(child_id(), self.child_rotation);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.ferris.is_none() {
            self.ferris = Some(load_ferris(&ctx));
        }

        if ui.input(|i| i.key_pressed(egui::Key::R)) {
            self.root_rotation = self.root_rotation.next_cw();
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            // ── Toolbar
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(egui::RichText::new("↻").size(22.0)))
                        .on_hover_text("Rotate the main window 90°  (R)")
                        .clicked()
                    {
                        self.root_rotation = self.root_rotation.next_cw();
                    }
                    ui.separator();
                    let label = if self.show_child {
                        "🪟 Close child window"
                    } else {
                        "🪟 Open child window"
                    };
                    if ui.button(label).clicked() {
                        self.show_child = !self.show_child;
                    }
                    ui.separator();
                    let dt = ui.input(|i| i.stable_dt).max(1e-6);
                    ui.monospace(format!("{:>4.0} FPS · {:>5.2} ms", 1.0 / dt, dt * 1000.0));
                });
            });

            ui.add_space(6.0);
            ui.heading("egui-rotate — eframe demo");
            ui.label(format!(
                "Main window: {:?}. The whole UI — including the stress test below — is rotated by the plugin.",
                self.root_rotation
            ));
            ui.add(egui::Slider::new(&mut self.shapes, 0..=4000).text("animated shapes (stress test)"));
            ui.separator();

            // ── Perf stress: N animated shapes. They rotate with the viewport too,
            //    and the per-frame rotation cost stays negligible — watch the FPS.
            let time = ui.input(|i| i.time) as f32;
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 220.0), Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 4.0, Color32::from_gray(18));
            let c = rect.center();
            let (rx, ry) = (rect.width() * 0.45, rect.height() * 0.42);
            for i in 0..self.shapes {
                let fi = i as f32;
                let x = c.x + rx * (time * 0.8 + fi * 0.7).sin();
                let y = c.y + ry * (time * 1.1 + fi * 0.9).cos();
                let g = 120 + ((fi * 0.21).sin() * 120.0) as i32;
                let col = Color32::from_rgb(80, g.clamp(0, 255) as u8, 220);
                painter.circle_filled(Pos2::new(x, y), 3.0, col);
            }

            // ── Ferris
            if let Some(tex) = &self.ferris {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    let aspect = tex.size_vec2().x / tex.size_vec2().y;
                    let w = (ui.available_width() - 16.0).clamp(80.0, 260.0);
                    ui.image(egui::load::SizedTexture::new(tex.id(), Vec2::new(w, w / aspect)));
                });
            }
        });

        // ── Child window: a real second OS window with its OWN rotation.
        if self.show_child {
            ctx.show_viewport_immediate(
                child_id(),
                ViewportBuilder::default()
                    .with_title("egui-rotate — child window")
                    .with_inner_size([340.0, 440.0]),
                |ui, class| {
                    if class == ViewportClass::EmbeddedWindow {
                        ui.label("This backend doesn't support separate windows (embedded).");
                        return;
                    }
                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .add(egui::Button::new(egui::RichText::new("↻").size(20.0)))
                                    .on_hover_text("Rotate THIS window 90°")
                                    .clicked()
                                {
                                    self.child_rotation = self.child_rotation.next_cw();
                                }
                                ui.label(format!("rotation: {:?}", self.child_rotation));
                            });
                        });
                        ui.add_space(8.0);
                        ui.heading("Child window");
                        ui.label(
                            "A separate OS window with its OWN rotation, independent of the main one.",
                        );
                        ui.label("Same plugin, same Context — just a different viewport id.");
                    });

                    if ui.input(|i| i.viewport().close_requested()) {
                        self.show_child = false;
                    }
                },
            );
        }
    }
}
