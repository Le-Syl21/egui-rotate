//! Plugin demo: viewport rotation driven entirely by [`RotationPlugin`].
//!
//! Compared to `rotated_demo` (which wires the helper functions by hand), the
//! integration here is minimal: register the plugin once, then run egui normally.
//! The plugin rotates input, rotates the rendered shapes, draws the software
//! cursor and hides the OS cursor — the redraw loop never calls a transform.
//!
//! A small toolbar offers a **↻ rotate** button (90° per click), a **📷
//! screenshot** button, a no/soft/hard **lock mode** selector with an edge
//! resistance slider, and an **auto-hide** toggle with a fade slider. Keyboard
//! shortcuts: `R` = rotate, `L` = cycle lock mode, `H` = auto-hide,
//! `S` = screenshot, `Esc` = quit.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::unwrap_used, unsafe_code, clippy::undocumented_unsafe_blocks)]

use std::num::NonZeroU32;
use std::sync::Arc;

use egui_rotate::{
    Rotation, RotationPlugin, SoftwareCursor, DEFAULT_EDGE_RESISTANCE, DEFAULT_FADE,
};
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
            .with_title(
                "egui-rotate — plugin demo  (R rotate · L lock · H hide · S shot · Esc quit)",
            )
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
    cursor: CursorSettings,

    // Demo widgets.
    widgets: DemoWidgets,
    ferris: Option<egui::TextureHandle>,
}

/// How the software cursor is confined to the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LockMode {
    /// Any edge contact releases the cursor to the OS.
    None,
    /// Edge resists: a deliberate push (fast flick or sustained pressure)
    /// breaks through and releases; casual contact stays confined.
    Soft,
    /// The cursor never leaves the window (kiosk / cabinet mode).
    Hard,
}

impl LockMode {
    fn next(self) -> Self {
        match self {
            Self::None => Self::Soft,
            Self::Soft => Self::Hard,
            Self::Hard => Self::None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::None => "No lock",
            Self::Soft => "Soft lock",
            Self::Hard => "Hard lock",
        }
    }
}

/// Software-cursor settings, mirrored into the plugin each frame.
#[derive(Clone, Copy)]
struct CursorSettings {
    lock_mode: LockMode,
    /// Soft-lock: px of outward push needed to break out.
    edge_resistance: f32,
    /// Auto-hide (dormancy) on keyboard input — off by default, like the lib.
    auto_hide: bool,
    /// Dissolve/reform duration for auto-hide, in ms.
    fade_ms: f32,
}

impl Default for CursorSettings {
    fn default() -> Self {
        Self {
            // Once rotated, soft lock by default: confined against casual edge
            // contact, but a deliberate push still escapes.
            lock_mode: LockMode::Soft,
            edge_resistance: DEFAULT_EDGE_RESISTANCE,
            auto_hide: false,
            fade_ms: DEFAULT_FADE.as_millis() as f32,
        }
    }
}

/// Interactive widget state, grouped so it can be passed to `demo_ui` as one.
struct DemoWidgets {
    counter: u32,
    text: String,
    slider: f32,
}

/// Read the freshly painted GL back buffer and save it as a timestamped PNG in
/// the current directory. Returns the file name.
fn save_screenshot(gl: &glow::Context, [w, h]: [u32; 2]) -> Result<String, String> {
    use glow::HasContext as _;

    let mut buf = vec![0u8; (w * h * 4) as usize];
    unsafe {
        gl.read_pixels(
            0,
            0,
            w as i32,
            h as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut buf)),
        );
    }

    // The framebuffer alpha is meaningless for a file; force it opaque.
    for px in buf.chunks_exact_mut(4) {
        px[3] = 255;
    }
    let mut img = image::RgbaImage::from_raw(w, h, buf).ok_or("pixel buffer size mismatch")?;
    // GL rows start at the bottom-left.
    image::imageops::flip_vertical_in_place(&mut img);

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let path = format!("screenshot-{stamp}.png");
    img.save(&path).map_err(|e| e.to_string())?;
    Ok(path)
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
            // Start upright, in a window whose proportions match: rotations then
            // resize the window from a sane baseline (see `redraw`).
            rotation: Rotation::None,
            cursor: CursorSettings::default(),
            widgets: DemoWidgets {
                counter: 0,
                text: String::from("type here"),
                slider: 0.5,
            },
            ferris: None,
        }
    }

    // Note there is no pointer-grab code here: the plugin confines the OS
    // cursor itself while the software cursor is captured (see
    // `SoftwareCursor::with_os_grab`).
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

        // The whole integration: register the plugin once. The OS-cursor pin
        // ("pseudo-lock") keeps the hidden real cursor centred so it can never
        // physically leave the window while the software cursor is captured.
        self.egui_ctx.add_plugin(
            RotationPlugin::new(self.rotation).with_software_cursor(
                SoftwareCursor::new()
                    .with_lock(self.cursor.lock_mode == LockMode::Hard)
                    .with_edge_resistance(match self.cursor.lock_mode {
                        LockMode::Soft => self.cursor.edge_resistance,
                        _ => 0.0,
                    })
                    .with_dormant_on_keys(self.cursor.auto_hide)
                    .with_fade(std::time::Duration::from_secs_f32(
                        self.cursor.fade_ms / 1000.0,
                    ))
                    .with_os_cursor_pin(true)
                    .with_scale(1.5),
            ),
        );

        self.ferris = Some(load_ferris(&self.egui_ctx));

        self.gl_window = Some(gl_window);
        self.gl = Some(gl);
        self.egui_winit = Some(egui_winit);
        self.painter = Some(painter);
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
                cursor.set_lock(self.cursor.lock_mode == LockMode::Hard);
                cursor.set_edge_resistance(match self.cursor.lock_mode {
                    LockMode::Soft => self.cursor.edge_resistance,
                    _ => 0.0,
                });
                cursor.set_dormant_on_keys(self.cursor.auto_hide);
                if !self.cursor.auto_hide && cursor.is_dormant() {
                    // Auto-hide switched off while dissolved: reform right away.
                    cursor.set_dormant(false);
                }
                cursor.set_fade(std::time::Duration::from_secs_f32(
                    self.cursor.fade_ms / 1000.0,
                ));
            }
        }

        // ── Gather input. egui must see the *physical* screen rect; the plugin
        //    swaps it to logical in `input_hook`.
        let mut raw_input = self.egui_winit.as_mut().unwrap().take_egui_input(window);
        if raw_input.screen_rect.is_none() {
            raw_input.screen_rect =
                Some(egui::Rect::from_min_size(egui::Pos2::ZERO, physical_size));
        }

        // ── Run the UI. The plugin does everything else.
        let mut rotation = self.rotation;
        let mut cursor_settings = self.cursor;
        let mut screenshot = false;
        let widgets = &mut self.widgets;
        let ferris = self.ferris.as_ref();
        let full_output = self.egui_ctx.run_ui(raw_input, |ui| {
            demo_ui(
                ui,
                &mut rotation,
                &mut cursor_settings,
                &mut screenshot,
                widgets,
                ferris,
            );
        });
        // ── Desktop nicety: on an upright monitor, swap the window's width and
        //    height when the rotation flips axes, so the rotated UI keeps natural
        //    proportions. A real kiosk — fullscreen on a physically rotated
        //    screen — must NOT do this: its physical size is fixed by hardware.
        if rotation.swaps_axes() != self.rotation.swaps_axes() {
            let size = window.inner_size();
            let swapped = winit::dpi::PhysicalSize::new(size.height, size.width);
            if let Some(applied) = window.request_inner_size(swapped) {
                // `Some` = applied immediately (e.g. Wayland) and winit will NOT
                // emit a `Resized` event — resize the GL surface ourselves, or
                // the framebuffer stays at the old size (painting cropped, black
                // bands on screenshots).
                if applied.width > 0 && applied.height > 0 {
                    self.gl_window.as_ref().unwrap().resize(applied);
                }
            }
        }
        self.rotation = rotation;
        self.cursor = cursor_settings;

        // ── If the cursor was released to the OS (no-lock edge contact, or a
        //    soft-lock breakout), warp the real cursor to the exit point. The
        //    plugin drops its OS grab itself via `ViewportCommand::CursorGrab`.
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

        // ── Screenshot: read the freshly painted back buffer, before the swap.
        if screenshot {
            match save_screenshot(self.gl.as_ref().unwrap(), physical_dimensions) {
                Ok(path) => log::info!("Screenshot saved to {path}"),
                Err(err) => log::warn!("Screenshot failed: {err}"),
            }
        }
        for id in full_output.textures_delta.free {
            painter.free_texture(id);
        }

        self.gl_window.as_ref().unwrap().swap_buffers().unwrap();
    }
}

fn demo_ui(
    ui: &mut egui::Ui,
    rotation: &mut Rotation,
    cursor: &mut CursorSettings,
    screenshot: &mut bool,
    widgets: &mut DemoWidgets,
    ferris: Option<&egui::TextureHandle>,
) {
    // ── Keyboard shortcuts (consumed from egui input, so they work in any
    //    backend) — inert while a widget (the text edit) has keyboard focus.
    let (rotate_key, lock_key, shot_key, hide_key) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::R),
            i.key_pressed(egui::Key::L),
            i.key_pressed(egui::Key::S),
            i.key_pressed(egui::Key::H),
        )
    });
    let typing = ui.ctx().egui_wants_keyboard_input();
    let (rotate_key, lock_key, shot_key, hide_key) = (
        rotate_key && !typing,
        lock_key && !typing,
        shot_key && !typing,
        hide_key && !typing,
    );
    if rotate_key {
        *rotation = rotation.next_cw();
    }
    if shot_key {
        *screenshot = true;
    }
    // The software cursor only exists while rotated; at 0° it's the plain OS
    // cursor, so lock modes are meaningless there.
    let lock_available = *rotation != Rotation::None;
    if lock_key && lock_available {
        cursor.lock_mode = cursor.lock_mode.next();
    }
    if hide_key && lock_available {
        cursor.auto_hide = !cursor.auto_hide;
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
            let shot = ui
                .add(egui::Button::new(egui::RichText::new("📷").size(22.0)))
                .on_hover_text("Save a screenshot of the window (S)");
            if shot.clicked() {
                *screenshot = true;
            }
            ui.separator();
            // Greyed out at 0° (OS cursor — nothing to lock).
            // Greyed out at 0° (OS cursor — nothing to lock).  (L) cycles.
            ui.add_enabled_ui(lock_available, |ui| {
                for (mode, hover) in [
                    (LockMode::None, "Any edge contact releases the cursor"),
                    (
                        LockMode::Soft,
                        "Edge resists: a deliberate push (flick or sustained \
                         pressure) breaks through and releases",
                    ),
                    (
                        LockMode::Hard,
                        "The cursor never leaves the window (kiosk / cabinet)",
                    ),
                ] {
                    ui.radio_value(&mut cursor.lock_mode, mode, mode.label())
                        .on_hover_text(hover);
                }
            });
            ui.separator();
            ui.add_enabled(
                lock_available && cursor.lock_mode == LockMode::Soft,
                egui::Slider::new(&mut cursor.edge_resistance, 10.0..=400.0)
                    .integer()
                    .text("edge resistance"),
            )
            .on_hover_text("Soft lock: push this many px past the edge to break out");
        });
        ui.horizontal(|ui| {
            ui.add_enabled(
                lock_available,
                egui::Checkbox::new(&mut cursor.auto_hide, "🫥 Auto-hide"),
            )
            .on_hover_text(
                "Dissolve the cursor while using the keyboard (clears hover, so \
                 keyboard selection wins); any mouse use reforms it. Gamepads: \
                 call SoftwareCursor::set_dormant.  (H)",
            );
            ui.add_enabled(
                lock_available && cursor.auto_hide,
                egui::Slider::new(&mut cursor.fade_ms, 0.0..=1000.0)
                    .integer()
                    .text("fade (ms)"),
            )
            .on_hover_text("Dissolve/reform duration");
            ui.separator();
            ui.label(format!("Rotation: {rotation:?}"));
        });
    });

    ui.add_space(8.0);
    ui.heading("egui-rotate — plugin demo");
    ui.label("Input, rendering and the software cursor are all handled by RotationPlugin.");
    ui.label(
        "Shortcuts:  R = rotate 90°   ·   L = cycle lock mode   ·   \
         H = auto-hide   ·   S = screenshot   ·   Esc = quit",
    );
    ui.separator();

    ui.horizontal(|ui| {
        if ui.button("− 1").clicked() {
            widgets.counter = widgets.counter.saturating_sub(1);
        }
        ui.label(format!("counter = {}", widgets.counter));
        if ui.button("+ 1").clicked() {
            widgets.counter += 1;
        }
        ui.separator();
        ui.add(egui::Slider::new(&mut widgets.slider, 0.0..=1.0).text("slider"));
    });
    ui.text_edit_singleline(&mut widgets.text);

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
