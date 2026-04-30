//! Rotated demo: an 800×600 window rendered as if it were 600×800 portrait, rotated 90° clockwise.
//!
//! Demonstrates the lite-crate integration pattern, including the locked
//! [`SoftwareCursor`] (kiosk-mode virtual cursor that stays inside the window).
//!
//! Press `R` to cycle rotation, `L` to toggle the cursor lock, `Esc` to quit.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::unwrap_used, unsafe_code, clippy::undocumented_unsafe_blocks)]

use std::num::NonZeroU32;
use std::sync::Arc;

use egui_rotate::{transform_clipped_primitives, Rotation, SoftwareCursor};
use egui_winit::winit;
use winit::raw_window_handle::HasWindowHandle as _;

const PHYSICAL_WIDTH: u32 = 800;
const PHYSICAL_HEIGHT: u32 = 600;

// ─────────────────────────────────────────────────────────────────────────────
// Glutin/winit window plumbing (taken verbatim from egui_glow's pure_glow example)
// ─────────────────────────────────────────────────────────────────────────────

struct GlutinWindowContext {
    window: winit::window::Window,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl_display: glutin::display::Display,
    gl_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
}

impl GlutinWindowContext {
    unsafe fn new(event_loop: &winit::event_loop::ActiveEventLoop) -> Self {
        use glutin::context::NotCurrentGlContext as _;
        use glutin::display::GetGlDisplay as _;
        use glutin::display::GlDisplay as _;
        use glutin::prelude::GlSurface as _;

        let winit_window_builder = winit::window::WindowAttributes::default()
            .with_resizable(true)
            .with_inner_size(winit::dpi::PhysicalSize {
                width: PHYSICAL_WIDTH,
                height: PHYSICAL_HEIGHT,
            })
            .with_title("egui-rotate — rotated demo  (press R to cycle, Esc to quit)")
            .with_visible(false);

        let config_template_builder = glutin::config::ConfigTemplateBuilder::new()
            .prefer_hardware_accelerated(None)
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false);

        let (mut window, gl_config) = glutin_winit::DisplayBuilder::new()
            .with_preference(glutin_winit::ApiPreference::FallbackEgl)
            .with_window_attributes(Some(winit_window_builder.clone()))
            .build(event_loop, config_template_builder, |mut it| {
                it.next().expect("no GL config")
            })
            .expect("failed to create gl_config");

        let gl_display = gl_config.display();
        let raw_window_handle = window.as_ref().map(|w| w.window_handle().unwrap().as_raw());

        let context_attributes =
            glutin::context::ContextAttributesBuilder::new().build(raw_window_handle);
        let fallback_context_attributes = glutin::context::ContextAttributesBuilder::new()
            .with_context_api(glutin::context::ContextApi::Gles(None))
            .build(raw_window_handle);
        let not_current_gl_context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    gl_display
                        .create_context(&gl_config, &fallback_context_attributes)
                        .expect("failed to create context")
                })
        };

        let window = window.take().unwrap_or_else(|| {
            glutin_winit::finalize_window(event_loop, winit_window_builder.clone(), &gl_config)
                .expect("failed to finalize window")
        });
        let (w, h): (u32, u32) = window.inner_size().into();
        let surface_attributes =
            glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
                .build(
                    window.window_handle().unwrap().as_raw(),
                    NonZeroU32::new(w).unwrap_or(NonZeroU32::MIN),
                    NonZeroU32::new(h).unwrap_or(NonZeroU32::MIN),
                );
        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attributes)
                .unwrap()
        };
        let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

        gl_surface
            .set_swap_interval(
                &gl_context,
                glutin::surface::SwapInterval::Wait(NonZeroU32::MIN),
            )
            .unwrap();

        Self {
            window,
            gl_context,
            gl_display,
            gl_surface,
        }
    }

    fn window(&self) -> &winit::window::Window {
        &self.window
    }

    fn resize(&self, size: winit::dpi::PhysicalSize<u32>) {
        use glutin::surface::GlSurface as _;
        self.gl_surface.resize(
            &self.gl_context,
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
        );
    }

    fn swap_buffers(&self) -> glutin::error::Result<()> {
        use glutin::surface::GlSurface as _;
        self.gl_surface.swap_buffers(&self.gl_context)
    }

    fn get_proc_address(&self, addr: &std::ffi::CStr) -> *const std::ffi::c_void {
        use glutin::display::GlDisplay as _;
        self.gl_display.get_proc_address(addr)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// App
// ─────────────────────────────────────────────────────────────────────────────

struct DemoApp {
    gl_window: Option<GlutinWindowContext>,
    gl: Option<Arc<glow::Context>>,
    egui_ctx: egui::Context,
    egui_winit: Option<egui_winit::State>,
    painter: Option<egui_glow::Painter>,
    rotation: Rotation,
    cursor: SoftwareCursor,
    last_cursor_icon: egui::CursorIcon,
    counter: u32,
    text: String,
    slider: f32,
}

impl DemoApp {
    fn new() -> Self {
        Self {
            gl_window: None,
            gl: None,
            egui_ctx: egui::Context::default(),
            egui_winit: None,
            painter: None,
            rotation: Rotation::CW90,
            cursor: SoftwareCursor::new().with_lock(false).with_scale(1.5),
            last_cursor_icon: egui::CursorIcon::Default,
            counter: 0,
            text: String::from("type here"),
            slider: 0.5,
        }
    }

    /// Apply OS-level cursor visibility / grab to match the current state.
    ///
    /// - When rotation is on AND the software cursor is captured: hide OS cursor.
    ///   Confine only in lock mode so it can release at edges otherwise.
    /// - Otherwise: show OS cursor, no grab.
    fn refresh_cursor_grab(&self) {
        let Some(gl_window) = &self.gl_window else {
            return;
        };
        let window = gl_window.window();
        let active = !self.rotation.is_none() && self.cursor.is_captured();
        window.set_cursor_visible(!active);
        let mode = if active && self.cursor.is_locked() {
            winit::window::CursorGrabMode::Confined
        } else {
            winit::window::CursorGrabMode::None
        };
        let _ = window.set_cursor_grab(mode);
    }
}

impl winit::application::ApplicationHandler for DemoApp {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let gl_window = unsafe { GlutinWindowContext::new(event_loop) };
        let gl = unsafe {
            glow::Context::from_loader_function(|s| {
                let s = std::ffi::CString::new(s).unwrap();
                gl_window.get_proc_address(&s)
            })
        };
        let gl = Arc::new(gl);
        gl_window.window().set_visible(true);

        let painter = egui_glow::Painter::new(Arc::clone(&gl), "", None, true)
            .expect("failed to create painter");

        let egui_winit = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            event_loop,
            None,
            event_loop.system_theme(),
            Some(painter.max_texture_side()),
        );

        self.gl_window = Some(gl_window);
        self.gl = Some(gl);
        self.egui_winit = Some(egui_winit);
        self.painter = Some(painter);
        self.refresh_cursor_grab();
    }

    fn device_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        // Forward raw mouse deltas to egui as `Event::MouseMoved`. The
        // SoftwareCursor consumes these in `process_input`.
        if let winit::event::DeviceEvent::MouseMotion { delta } = event {
            if let Some(state) = self.egui_winit.as_mut() {
                if state.on_mouse_motion(delta) {
                    self.gl_window.as_ref().unwrap().window().request_redraw();
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _wid: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        use winit::event::WindowEvent;

        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            event_loop.exit();
            return;
        }

        if let WindowEvent::KeyboardInput {
            event:
                winit::event::KeyEvent {
                    logical_key,
                    state: winit::event::ElementState::Pressed,
                    ..
                },
            ..
        } = &event
        {
            use winit::keyboard::{Key, NamedKey};
            match logical_key {
                Key::Named(NamedKey::Escape) => {
                    event_loop.exit();
                    return;
                }
                Key::Character(c) if c.eq_ignore_ascii_case("r") => {
                    self.rotation = match self.rotation {
                        Rotation::None => Rotation::CW90,
                        Rotation::CW90 => Rotation::CW180,
                        Rotation::CW180 => Rotation::CW270,
                        Rotation::CW270 => Rotation::None,
                    };
                    log::info!("rotation → {:?}", self.rotation);
                    self.refresh_cursor_grab();
                    self.gl_window.as_ref().unwrap().window().request_redraw();
                }
                Key::Character(c) if c.eq_ignore_ascii_case("l") => {
                    let new_lock = !self.cursor.is_locked();
                    self.cursor.set_lock(new_lock);
                    log::info!("cursor lock → {}", new_lock);
                    self.refresh_cursor_grab();
                    self.gl_window.as_ref().unwrap().window().request_redraw();
                }
                _ => {}
            }
        }

        if let WindowEvent::Resized(physical_size) = &event {
            self.gl_window.as_ref().unwrap().resize(*physical_size);
        }

        if matches!(event, WindowEvent::RedrawRequested) {
            self.redraw();
            return;
        }

        let response = self
            .egui_winit
            .as_mut()
            .unwrap()
            .on_window_event(self.gl_window.as_ref().unwrap().window(), &event);
        if response.repaint {
            self.gl_window.as_ref().unwrap().window().request_redraw();
        }
    }

    fn exiting(&mut self, _: &winit::event_loop::ActiveEventLoop) {
        if let Some(painter) = &mut self.painter {
            painter.destroy();
        }
    }
}

impl DemoApp {
    fn redraw(&mut self) {
        let window = self.gl_window.as_ref().unwrap().window();
        let physical_dimensions: [u32; 2] = window.inner_size().into();
        let physical_size =
            egui::Vec2::new(physical_dimensions[0] as f32, physical_dimensions[1] as f32);

        // ── 1. Gather raw input from winit
        let mut raw_input = self.egui_winit.as_mut().unwrap().take_egui_input(window);

        // Make sure egui sees the *physical* screen rect so transform_raw_input
        // can compute logical-space coords from it.
        if raw_input.screen_rect.is_none() {
            raw_input.screen_rect =
                Some(egui::Rect::from_min_size(egui::Pos2::ZERO, physical_size));
        }

        // ── 2. Apply input rotation + software cursor capture BEFORE egui sees the input.
        //       process_input does the same job as transform_raw_input plus capture/release.
        let was_captured = self.cursor.is_captured();
        let cursor_out = self
            .cursor
            .process_input(&mut raw_input, self.rotation, physical_size);

        // If the cursor was just released to the OS, warp the OS cursor to that position
        // (3px outside the window so the user sees their cursor exit cleanly).
        if let Some(release_pos) = cursor_out.release_os_cursor_to {
            let _ = window.set_cursor_position(winit::dpi::PhysicalPosition::new(
                release_pos.x as f64,
                release_pos.y as f64,
            ));
        }

        // Sync OS cursor visibility / grab if capture state changed.
        if was_captured != self.cursor.is_captured() {
            self.refresh_cursor_grab();
        }

        // ── 3. Run UI in logical (rotated) space, drawing the software cursor inside the pass
        let counter = &mut self.counter;
        let text = &mut self.text;
        let slider = &mut self.slider;
        let rotation = self.rotation;
        let cursor = &self.cursor;
        let cursor_locked = cursor.is_locked();
        // Use the previous frame's cursor icon (1 frame of latency — fine for visuals).
        // Pass it un-rotated: the inverse rotation at paint time produces the
        // correct visual orientation (a logical-vertical I-beam becomes a
        // physical-horizontal one, perpendicular to the rotated text).
        let cursor_icon = self.last_cursor_icon;
        let full_output = self.egui_ctx.run_ui(raw_input, |ui| {
            demo_ui(ui, counter, text, slider, rotation, cursor_locked);

            if !rotation.is_none() {
                let painter = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("egui-rotate-software-cursor"),
                ));
                cursor.draw(&painter, cursor_icon);
            }
        });

        // Remember the icon set during this pass, for next frame's cursor visual.
        self.last_cursor_icon = full_output.platform_output.cursor_icon;

        // ── 4. Hand platform output to winit. When rotated, suppress the OS
        //       cursor icon so it doesn't flicker visible on top of our drawn one.
        let mut platform_output = full_output.platform_output.clone();
        if !self.rotation.is_none() {
            platform_output.cursor_icon = egui::CursorIcon::None;
        }
        self.egui_winit
            .as_mut()
            .unwrap()
            .handle_platform_output(window, platform_output);

        // ── 5. Tessellate (already includes the cursor shape from inside run_ui)
        let logical_size = self.egui_ctx.content_rect().size();
        let mut clipped_primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        // ── 6. Apply output rotation: logical → physical
        transform_clipped_primitives(&mut clipped_primitives, self.rotation, logical_size);

        // ── 7. Paint
        let painter = self.painter.as_mut().unwrap();
        for (id, image_delta) in full_output.textures_delta.set {
            painter.set_texture(id, &image_delta);
        }

        unsafe {
            use glow::HasContext as _;
            let gl = self.gl.as_ref().unwrap();
            gl.clear_color(0.05, 0.06, 0.08, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        painter.paint_primitives(
            physical_dimensions,
            full_output.pixels_per_point,
            &clipped_primitives,
        );
        for id in full_output.textures_delta.free {
            painter.free_texture(id);
        }

        self.gl_window.as_ref().unwrap().swap_buffers().unwrap();
    }
}

fn demo_ui(
    ui: &mut egui::Ui,
    counter: &mut u32,
    text: &mut String,
    slider: &mut f32,
    rotation: Rotation,
    cursor_locked: bool,
) {
    ui.heading(format!("egui-rotate demo — {rotation:?}"));
    ui.label(format!(
        "R = cycle rotation · L = toggle cursor lock ({}) · Esc = quit",
        if cursor_locked { "ON" } else { "OFF" }
    ));
    ui.separator();

    ui.label("Click the buttons, drag the slider, type in the text field.");
    ui.label("Mouse coordinates are remapped through the rotation transparently.");

    ui.add_space(8.0);

    ui.horizontal(|ui| {
        if ui.button("− 1").clicked() {
            *counter = counter.saturating_sub(1);
        }
        ui.label(format!("counter = {counter}"));
        if ui.button("+ 1").clicked() {
            *counter += 1;
        }
    });

    ui.add(egui::Slider::new(slider, 0.0..=1.0).text("slider"));
    ui.text_edit_singleline(text);

    ui.separator();
    ui.label("Logical screen rect:");
    ui.monospace(format!("{:?}", ui.ctx().content_rect()));

    ui.add_space(12.0);
    egui::ScrollArea::vertical()
        .max_height(160.0)
        .show(ui, |ui| {
            for i in 0..40 {
                ui.horizontal(|ui| {
                    ui.label(format!("row {i:02}"));
                    let _ = ui.button("…");
                });
            }
        });
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    let mut app = DemoApp::new();
    event_loop.run_app(&mut app).expect("event loop failed");
}
