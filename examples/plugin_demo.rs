//! Plugin demo: viewport rotation driven entirely by [`RotationPlugin`].
//!
//! Compared to `rotated_demo` (which wires the helper functions by hand), the
//! integration here is minimal: register the plugin once, then run egui normally.
//! The plugin rotates input, rotates the rendered shapes, draws the software
//! cursor and hides the OS cursor — the redraw loop never calls a transform.
//!
//! A small toolbar offers a **↻ rotate** button (90° per click) and a **Lock
//! cursor** checkbox. Keyboard shortcuts: `R` = rotate, `L` = lock/unlock,
//! `Esc` = quit.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::unwrap_used, unsafe_code, clippy::undocumented_unsafe_blocks)]

use std::num::NonZeroU32;
use std::sync::Arc;

use egui_rotate::{Rotation, RotationPlugin, SoftwareCursor};
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
            .with_title("egui-rotate — plugin demo  (R rotate · L lock · Esc quit)")
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

    // UI-facing state, mirrored into the plugin each frame.
    rotation: Rotation,
    locked: bool,

    // Demo widgets.
    counter: u32,
    text: String,
    slider: f32,
    ferris: Option<egui::TextureHandle>,
}

/// Decode the embedded Ferris PNG into an egui texture (loaded once).
fn load_ferris(ctx: &egui::Context) -> egui::TextureHandle {
    let bytes = include_bytes!("../assets/ferris.png");
    let image = image::load_from_memory(bytes)
        .expect("decode ferris.png")
        .to_rgba8();
    let (w, h) = image.dimensions();
    let color_image =
        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], image.as_raw());
    ctx.load_texture("ferris", color_image, egui::TextureOptions::LINEAR)
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
            locked: true,
            counter: 0,
            text: String::from("type here"),
            slider: 0.5,
            ferris: None,
        }
    }

    /// Is the software cursor currently captured?
    fn cursor_captured(&self) -> bool {
        let handle = self.egui_ctx.plugin::<RotationPlugin>();
        let plugin = handle.lock();
        plugin.software_cursor().is_some_and(|c| c.is_captured())
    }

    /// Confine the OS cursor while locked + captured, so it can't leave the
    /// window (visibility is handled by the plugin via `CursorIcon::None`).
    fn refresh_grab(&self) {
        let Some(gl_window) = &self.gl_window else {
            return;
        };
        let confine = self.locked && !self.rotation.is_none() && self.cursor_captured();
        let mode = if confine {
            winit::window::CursorGrabMode::Confined
        } else {
            winit::window::CursorGrabMode::None
        };
        let _ = gl_window.window().set_cursor_grab(mode);
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

        // The whole integration: register the plugin once.
        self.egui_ctx
            .add_plugin(RotationPlugin::new(self.rotation).with_software_cursor(
                SoftwareCursor::new().with_lock(self.locked).with_scale(1.5),
            ));

        self.ferris = Some(load_ferris(&self.egui_ctx));

        self.gl_window = Some(gl_window);
        self.gl = Some(gl);
        self.egui_winit = Some(egui_winit);
        self.painter = Some(painter);
        self.refresh_grab();
    }

    fn device_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        // Raw mouse deltas drive the software cursor (`Event::MouseMoved`).
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

        // Esc quits (R / L shortcuts are handled inside the egui pass).
        if let WindowEvent::KeyboardInput {
            event:
                winit::event::KeyEvent {
                    logical_key: winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape),
                    state: winit::event::ElementState::Pressed,
                    ..
                },
            ..
        } = &event
        {
            event_loop.exit();
            return;
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

        // ── Mirror UI state into the plugin (before the pass: `input_hook` reads it).
        {
            let handle = self.egui_ctx.plugin::<RotationPlugin>();
            let mut plugin = handle.lock();
            plugin.set_rotation(self.rotation);
            if let Some(cursor) = plugin.software_cursor_mut() {
                cursor.set_lock(self.locked);
            }
        }

        let was_captured = self.cursor_captured();

        // ── Gather input. egui must see the *physical* screen rect; the plugin
        //    swaps it to logical in `input_hook`.
        let mut raw_input = self.egui_winit.as_mut().unwrap().take_egui_input(window);
        if raw_input.screen_rect.is_none() {
            raw_input.screen_rect =
                Some(egui::Rect::from_min_size(egui::Pos2::ZERO, physical_size));
        }

        // ── Run the UI. The plugin does everything else.
        let mut rotation = self.rotation;
        let mut locked = self.locked;
        let counter = &mut self.counter;
        let text = &mut self.text;
        let slider = &mut self.slider;
        let ferris = self.ferris.as_ref();
        let full_output = self.egui_ctx.run_ui(raw_input, |ui| {
            demo_ui(
                ui,
                &mut rotation,
                &mut locked,
                counter,
                text,
                slider,
                ferris,
            );
        });
        self.rotation = rotation;
        self.locked = locked;

        // ── If the cursor was released to the OS (non-locked edge), warp it there.
        let pending_warp = {
            let handle = self.egui_ctx.plugin::<RotationPlugin>();
            let mut plugin = handle.lock();
            plugin.take_pending_warp()
        };
        if let Some(warp) = pending_warp {
            let _ = window.set_cursor_position(winit::dpi::PhysicalPosition::new(
                warp.x as f64,
                warp.y as f64,
            ));
        }
        if was_captured != self.cursor_captured() {
            self.refresh_grab();
        }
        self.refresh_grab();

        // ── Platform output: the plugin already set cursor_icon (None while the
        //    software cursor is captured, remapped otherwise).
        self.egui_winit
            .as_mut()
            .unwrap()
            .handle_platform_output(window, full_output.platform_output);

        // ── Shapes are already rotated by the plugin — just tessellate and paint.
        let clipped_primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

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
    rotation: &mut Rotation,
    locked: &mut bool,
    counter: &mut u32,
    text: &mut String,
    slider: &mut f32,
    ferris: Option<&egui::TextureHandle>,
) {
    // ── Keyboard shortcuts (consumed from egui input, so they work in any backend).
    let (rotate_key, lock_key) =
        ui.input(|i| (i.key_pressed(egui::Key::R), i.key_pressed(egui::Key::L)));
    if rotate_key {
        *rotation = rotation.next_cw();
    }
    // The software cursor only exists while rotated; at 0° it's the plain OS
    // cursor, so lock is meaningless there.
    let lock_available = *rotation != Rotation::None;
    if lock_key && lock_available {
        *locked = !*locked;
    }
    if !lock_available {
        *locked = false;
    }

    // ── Toolbar.
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            let rotate = ui
                .add(egui::Button::new(egui::RichText::new("↻").size(22.0)))
                .on_hover_text("Rotate 90° clockwise  (R)");
            if rotate.clicked() {
                *rotation = rotation.next_cw();
            }
            ui.separator();
            // Greyed out at 0° (OS cursor — nothing to lock).
            ui.add_enabled(
                lock_available,
                egui::Checkbox::new(locked, "🔒 Lock cursor"),
            )
            .on_hover_text("Confine the software cursor inside the window  (L)");
            ui.separator();
            ui.label(format!("Rotation: {rotation:?}"));
        });
    });

    ui.add_space(8.0);
    ui.heading("egui-rotate — plugin demo");
    ui.label("Input, rendering and the software cursor are all handled by RotationPlugin.");
    ui.label("Shortcuts:  R = rotate 90°   ·   L = lock/unlock   ·   Esc = quit");
    ui.separator();

    ui.horizontal(|ui| {
        if ui.button("− 1").clicked() {
            *counter = counter.saturating_sub(1);
        }
        ui.label(format!("counter = {counter}"));
        if ui.button("+ 1").clicked() {
            *counter += 1;
        }
        ui.separator();
        ui.add(egui::Slider::new(slider, 0.0..=1.0).text("slider"));
    });
    ui.text_edit_singleline(text);

    // ── Ferris rides along with the rotation — the clearest proof the whole
    //    viewport (text, widgets, image) is remapped uniformly.
    if let Some(tex) = ferris {
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            let aspect = tex.size_vec2().x / tex.size_vec2().y;
            let w = (ui.available_width() - 16.0).clamp(80.0, 360.0);
            ui.image(egui::load::SizedTexture::new(
                tex.id(),
                egui::vec2(w, w / aspect),
            ));
        });
    }

    ui.add_space(8.0);
    let size = ui.ctx().content_rect().size();
    ui.monospace(format!("logical size: {:.0} × {:.0}", size.x, size.y));
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    let mut app = DemoApp::new();
    event_loop.run_app(&mut app).expect("event loop failed");
}
