//! [`RotationPlugin`] — viewport rotation as a self-contained [`egui::Plugin`].
//!
//! Register it once and rotation becomes transparent for the whole pipeline,
//! with **no integration code** and **no eframe hooks** — it works the same on
//! `egui_glow`, `egui_wgpu`, or any custom backend:
//!
//! ```no_run
//! # let ctx = egui::Context::default();
//! use egui_rotate::{Rotation, RotationPlugin};
//!
//! ctx.add_plugin(RotationPlugin::new(Rotation::CW90));
//! ```
//!
//! ## Multiple windows (child viewports)
//!
//! Rotation is **per-viewport and opt-in**: [`RotationPlugin::new`] configures the
//! root window, and any viewport you don't configure passes through untouched. So
//! a rotated cabinet window can coexist with normal child windows (settings
//! dialogs, etc.). Configure a child explicitly with
//! [`RotationPlugin::set_viewport_rotation`].
//!
//! What the plugin does each frame, per viewport:
//! - [`input_hook`](egui::Plugin::input_hook): rotates that viewport's pointer/touch
//!   input into logical space and remembers its logical size for the output stage.
//! - [`on_end_pass`](egui::Plugin::on_end_pass): with a [`SoftwareCursor`], draws
//!   the virtual cursor on top (only on the cursor's viewport), in logical space,
//!   manages the OS pointer grab ([`SoftwareCursor::with_os_grab`], via
//!   [`egui::ViewportCommand::CursorGrab`]) and — with
//!   [`SoftwareCursor::with_os_cursor_pin`] — re-centres the real OS cursor via
//!   [`egui::ViewportCommand::CursorPosition`] when it strays.
//! - [`output_hook`](egui::Plugin::output_hook): rotates that viewport's
//!   pre-tessellation shapes back to physical space and either remaps the OS cursor
//!   icon or hides it while a software cursor is active.
//!
//! This plugin supersedes the now-deprecated free helpers
//! ([`crate::transform_raw_input`] / [`crate::transform_clipped_primitives`]):
//! use one or the other, never both, or rotation is applied twice.

use std::collections::HashMap;

use egui::{Context, FullOutput, RawInput, Vec2, ViewportId};

use crate::{CursorIconExt, Rotation};

#[cfg(feature = "software-cursor")]
use crate::SoftwareCursor;
#[cfg(feature = "software-cursor")]
use egui::Pos2;

/// Per-pass state, pushed in `input_hook` and popped in `output_hook`.
///
/// `output_hook` is not told which viewport its `FullOutput` belongs to, but the
/// two hooks are called as a strict begin/end pair per pass — and nested
/// (immediate) child viewports pair up LIFO — so a stack reunites each output
/// with the rotation and logical size computed for its input.
#[derive(Clone, Copy, Debug)]
struct PassState {
    rotation: Rotation,
    logical_size: Vec2,
    /// Whether this pass is the software cursor's viewport.
    #[cfg(feature = "software-cursor")]
    cursor_here: bool,
}

impl PassState {
    fn passthrough() -> Self {
        Self {
            rotation: Rotation::None,
            logical_size: Vec2::ZERO,
            #[cfg(feature = "software-cursor")]
            cursor_here: false,
        }
    }
}

/// A [`egui::Plugin`] that applies per-viewport rotation transparently.
///
/// One instance per [`egui::Context`]. Change the root rotation at runtime through
/// the registered handle:
///
/// ```no_run
/// # let ctx = egui::Context::default();
/// # use egui_rotate::{Rotation, RotationPlugin};
/// # ctx.add_plugin(RotationPlugin::new(Rotation::None));
/// ctx.plugin::<RotationPlugin>().lock().set_rotation(Rotation::CW270);
/// ```
#[derive(Clone, Debug, Default)]
pub struct RotationPlugin {
    /// Rotation per viewport. A viewport absent from the map is not rotated.
    rotations: HashMap<ViewportId, Rotation>,
    /// Begin/end pairing across (possibly nested) viewport passes.
    pass_stack: Vec<PassState>,

    /// Optional software cursor (cabinet / kiosk displays). See
    /// [`Self::with_software_cursor`].
    #[cfg(feature = "software-cursor")]
    cursor: Option<SoftwareCursor>,
    /// The viewport the software cursor belongs to.
    #[cfg(feature = "software-cursor")]
    cursor_viewport: ViewportId,
    /// Pending OS-cursor warp request (non-locked edge release), drained via
    /// [`Self::take_pending_warp`].
    #[cfg(feature = "software-cursor")]
    pending_warp: Option<Pos2>,
    /// Pending pseudo-lock re-centre of the OS cursor
    /// ([`SoftwareCursor::with_os_cursor_pin`]), applied in `on_end_pass` via
    /// [`egui::ViewportCommand::CursorPosition`].
    #[cfg(feature = "software-cursor")]
    pending_pin: Option<Pos2>,
    /// Whether the plugin currently holds an OS pointer grab
    /// ([`SoftwareCursor::with_os_grab`]), so commands are only sent on change.
    #[cfg(feature = "software-cursor")]
    grab_active: bool,
}

impl RotationPlugin {
    /// Create a plugin rotating the **root** viewport by `rotation`.
    ///
    /// Child viewports are left untouched unless configured with
    /// [`Self::set_viewport_rotation`].
    pub fn new(rotation: Rotation) -> Self {
        let mut plugin = Self::default();
        plugin.set_rotation(rotation);
        plugin
    }

    fn rotation_for(&self, viewport: ViewportId) -> Rotation {
        self.rotations
            .get(&viewport)
            .copied()
            .unwrap_or(Rotation::None)
    }

    /// The root viewport's rotation.
    pub fn rotation(&self) -> Rotation {
        self.rotation_for(ViewportId::ROOT)
    }

    /// Set the root viewport's rotation. Takes effect on the next frame.
    pub fn set_rotation(&mut self, rotation: Rotation) {
        self.set_viewport_rotation(ViewportId::ROOT, rotation);
    }

    /// The rotation configured for a specific viewport (`None` if unconfigured).
    pub fn viewport_rotation(&self, viewport: ViewportId) -> Rotation {
        self.rotation_for(viewport)
    }

    /// Set the rotation for a specific viewport (e.g. a child window). Pass
    /// [`Rotation::None`] to stop rotating it.
    ///
    /// [`Rotation::None`] removes the map entry (absent ≡ not rotated), so the
    /// map does not grow when viewports come and go.
    pub fn set_viewport_rotation(&mut self, viewport: ViewportId, rotation: Rotation) {
        if rotation.is_none() {
            self.rotations.remove(&viewport);
        } else {
            self.rotations.insert(viewport, rotation);
        }
    }
}

#[cfg(feature = "software-cursor")]
impl RotationPlugin {
    /// Attach a [`SoftwareCursor`] to the **root** viewport. See
    /// [`Self::with_software_cursor_on`].
    pub fn with_software_cursor(self, cursor: SoftwareCursor) -> Self {
        self.with_software_cursor_on(ViewportId::ROOT, cursor)
    }

    /// Attach a [`SoftwareCursor`] to a specific viewport: the plugin then captures
    /// the OS cursor, draws a virtual cursor in logical space on that viewport, and
    /// hides the OS cursor while captured.
    ///
    /// In **locked** mode (see [`SoftwareCursor::with_lock`]) this is fully
    /// self-contained — no integration code (ideal for fullscreen kiosk / pinball
    /// cabinets). In **non-locked** mode the cursor is released to the OS at the
    /// screen edge; the integration must warp the OS cursor to the position
    /// returned by [`Self::take_pending_warp`] each frame.
    pub fn with_software_cursor_on(mut self, viewport: ViewportId, cursor: SoftwareCursor) -> Self {
        self.cursor = Some(cursor);
        self.cursor_viewport = viewport;
        self
    }

    /// Shared access to the attached [`SoftwareCursor`] (e.g. to query
    /// [`SoftwareCursor::is_captured`]). Returns `None` if none was attached.
    pub fn software_cursor(&self) -> Option<&SoftwareCursor> {
        self.cursor.as_ref()
    }

    /// Mutable access to the attached [`SoftwareCursor`], e.g. to change scale or
    /// lock at runtime. Returns `None` if no software cursor was attached.
    pub fn software_cursor_mut(&mut self) -> Option<&mut SoftwareCursor> {
        self.cursor.as_mut()
    }

    /// Take a pending OS-cursor warp request (physical-space position), if any.
    ///
    /// In non-locked software-cursor mode, call this once per frame after running
    /// egui: when `Some`, warp the OS cursor to that position and make it visible.
    /// Always `None` in locked mode.
    pub fn take_pending_warp(&mut self) -> Option<Pos2> {
        self.pending_warp.take()
    }
}

impl egui::Plugin for RotationPlugin {
    fn debug_name(&self) -> &'static str {
        "egui_rotate::RotationPlugin"
    }

    fn input_hook(&mut self, _ctx: &Context, input: &mut RawInput) {
        let viewport = input.viewport_id;
        let rotation = self.rotation_for(viewport);

        if rotation.is_none() {
            // Rotation switched off: the OS cursor takes over again, so release a
            // captured software cursor rather than freezing it with stale state.
            #[cfg(feature = "software-cursor")]
            if viewport == self.cursor_viewport {
                if let Some(cursor) = self.cursor.as_mut() {
                    cursor.release();
                }
                self.pending_pin = None;
            }
            self.pass_stack.push(PassState::passthrough());
            return;
        }

        #[cfg(feature = "software-cursor")]
        let cursor_here = match self.cursor.as_mut() {
            Some(cursor) if viewport == self.cursor_viewport => {
                // `screen_rect` is still physical here.
                let physical_size = input.screen_rect.map(|r| r.size()).unwrap_or_default();
                let out = cursor.process_input(input, rotation, physical_size);
                if let Some(warp) = out.release_os_cursor_to {
                    self.pending_warp = Some(warp);
                }
                if let Some(pin) = out.pin_os_cursor_to {
                    self.pending_pin = Some(pin);
                }
                true
            }
            _ => {
                crate::input::rotate_raw_input(input, rotation);
                false
            }
        };
        #[cfg(not(feature = "software-cursor"))]
        crate::input::rotate_raw_input(input, rotation);

        // After rotation, `screen_rect` is in logical space.
        let logical_size = input.screen_rect.map(|r| r.size()).unwrap_or_default();
        self.pass_stack.push(PassState {
            rotation,
            logical_size,
            #[cfg(feature = "software-cursor")]
            cursor_here,
        });
    }

    #[cfg(feature = "software-cursor")]
    fn on_end_pass(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let viewport = ctx.viewport_id();
        if viewport != self.cursor_viewport {
            return;
        }
        let rotated = !self.rotation_for(viewport).is_none();
        let Some(cursor) = &self.cursor else { return };

        // ── OS pointer grab lifecycle: grab while captured, release otherwise —
        // including when rotation is switched off at runtime, which is why this
        // runs before the `rotated` early-return below.
        if let Some(mode) = cursor.os_grab() {
            let desired = rotated && cursor.is_captured();
            if desired != self.grab_active {
                self.grab_active = desired;
                ctx.send_viewport_cmd_to(
                    viewport,
                    egui::ViewportCommand::CursorGrab(if desired {
                        mode
                    } else {
                        egui::viewport::CursorGrab::None
                    }),
                );
            }
        }

        if !rotated {
            return;
        }

        // Pseudo-lock: re-centre the real OS cursor while captured, so it can
        // never physically reach the window edge or leave the window.
        if let Some(pos) = self.pending_pin.take() {
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::CursorPosition(pos));
        }

        // Keep frames coming while the cursor dissolves or reforms.
        if cursor.is_fading() {
            ctx.request_repaint();
        }

        if cursor.virtual_pos().is_none() {
            return;
        }

        // Draw the virtual cursor in logical space, on a top-most layer; the
        // `output_hook` below rotates it into physical space along with the rest.
        let icon = ctx.output(|o| o.cursor_icon);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("egui_rotate::software_cursor"),
        ));
        cursor.draw(&painter, icon);
    }

    fn output_hook(&mut self, _ctx: &Context, output: &mut FullOutput) {
        let Some(state) = self.pass_stack.pop() else {
            return;
        };
        if state.rotation.is_none() {
            return;
        }

        // A pass without a `screen_rect` has no usable size (`logical_size` is
        // zero); skip the geometric transforms rather than mapping through it.
        if state.logical_size != Vec2::ZERO {
            crate::rotate_clipped_shapes(&mut output.shapes, state.rotation, state.logical_size);

            // The IME area is computed in logical space, but the backend positions
            // the OS composition window in physical screen space.
            if let Some(ime) = &mut output.platform_output.ime {
                ime.rect = state
                    .rotation
                    .inverse_transform_rect(ime.rect, state.logical_size);
                ime.cursor_rect = state
                    .rotation
                    .inverse_transform_rect(ime.cursor_rect, state.logical_size);
            }
        }

        // On the software cursor's viewport, hide the OS cursor while captured (we
        // draw our own). Otherwise remap directional icons so the OS cursor, which
        // the OS draws un-rotated, still points the right way on screen.
        #[cfg(feature = "software-cursor")]
        if state.cursor_here && self.cursor.as_ref().is_some_and(|c| c.is_captured()) {
            output.platform_output.cursor_icon = egui::CursorIcon::None;
            return;
        }

        output.platform_output.cursor_icon =
            output.platform_output.cursor_icon.rotate(state.rotation);
    }
}
