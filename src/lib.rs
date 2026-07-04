//! # egui-rotate
//!
//! Viewport rotation (0° / 90° / 180° / 270°) for [egui](https://github.com/emilk/egui).
//!
//! Use cases: virtual pinball cabinets, kiosks, embedded displays, industrial
//! panels — any setup where the physical screen is mounted rotated and the OS
//! cannot (or should not) rotate the whole desktop.
//!
//! This crate **does not modify egui**: it ships [`RotationPlugin`], a
//! self-contained [`egui::Plugin`]. Register it once and rotation becomes
//! transparent for input, rendering and the OS cursor — on any backend
//! (`egui_glow`, `egui_wgpu`, eframe, custom), with no other integration code.
//!
//! ## Quick start
//!
//! ```no_run
//! use egui_rotate::{Rotation, RotationPlugin};
//!
//! # let ctx = egui::Context::default();
//! ctx.add_plugin(RotationPlugin::new(Rotation::CW90));
//! ```
//!
//! That's it: pointer/touch input is remapped into the rotated space, the whole
//! UI is rendered rotated, and directional OS cursor icons are remapped.
//!
//! ## Multiple windows
//!
//! Rotation is **per-viewport and opt-in**. [`RotationPlugin::new`] configures the
//! root window; child windows pass through untouched unless you configure them
//! with [`RotationPlugin::set_viewport_rotation`]. So a rotated cabinet window can
//! coexist with upright settings dialogs.
//!
//! ## Software cursor (feature `software-cursor`, opt-in)
//!
//! Enable with `egui-rotate = { version = "…", features = ["software-cursor"] }`.
//! See [`SoftwareCursor`] for the rotated virtual cursor used by pinball cabinets
//! and kiosks where the OS cursor cannot be rotated; attach it with
//! [`RotationPlugin::with_software_cursor`]. Out of the box it soft-locks at the
//! window edge ([`SoftwareCursor::with_edge_resistance`]) and holds an OS pointer
//! grab while captured ([`SoftwareCursor::with_os_grab`]).
//!
//! For keyboard/gamepad-navigated front-ends, the cursor can **auto-hide**
//! ([`SoftwareCursor::set_dormant`]): it fades out and egui's hover is cleared so
//! the keyboard/gamepad selection wins; mouse use fades it back in. Keyboard
//! triggering is opt-in ([`SoftwareCursor::with_dormant_on_keys`]); for gamepads
//! call `set_dormant(true)` yourself when handling stick input — egui never sees
//! those events.
//!
//! ## Limitations
//!
//! - [`Shape::Callback`](egui::Shape::Callback) primitives are not rotated —
//!   backend paint callbacks own their coordinate space (see
//!   [`rotate_clipped_shapes`]).
//! - AccessKit accessibility node geometry is reported in logical (un-rotated)
//!   space; assistive technologies see pre-rotation coordinates.
//!
//! ## Custom integration (without the plugin)
//!
//! If you cannot use the plugin, the building blocks are public: [`Rotation`]'s
//! transform methods for input, and [`rotate_clipped_shapes`] / [`rotate_shape`]
//! to rotate pre-tessellation shapes. The older manual helpers
//! [`transform_raw_input`] and [`transform_clipped_primitives`] are **deprecated
//! since 1.0** — prefer the plugin.

mod input;
mod output;
mod plugin;
mod rotation;
mod shape_rotate;

#[allow(deprecated)]
pub use input::transform_raw_input;
#[allow(deprecated)]
pub use output::transform_clipped_primitives;
pub use plugin::RotationPlugin;
pub use rotation::Rotation;
pub use shape_rotate::{rotate_clipped_shapes, rotate_shape};

mod cursor_icon;
pub use cursor_icon::CursorIconExt;

#[cfg(feature = "software-cursor")]
mod cursor;
#[cfg(feature = "software-cursor")]
pub use cursor::{
    SoftwareCursor, SoftwareCursorOutput, DEFAULT_EDGE_RESISTANCE, DEFAULT_FADE,
    DEFAULT_WAKE_THRESHOLD, EDGE_PRESSURE_RESET_SECS,
};
