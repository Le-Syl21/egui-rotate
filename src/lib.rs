//! # egui-rotate
//!
//! Viewport rotation (0° / 90° / 180° / 270°) for [egui](https://github.com/emilk/egui).
//!
//! Use cases: virtual pinball cabinets, kiosks, embedded displays, industrial
//! panels — any setup where the physical screen is mounted rotated and the OS
//! cannot (or should not) rotate the whole desktop.
//!
//! This crate **does not modify egui** — it ships pure helper functions you
//! call in your integration loop, plus an optional software cursor.
//!
//! ## Quick start
//!
//! ```no_run
//! use egui_rotate::{Rotation, transform_raw_input, transform_clipped_primitives};
//!
//! # let ctx = egui::Context::default();
//! # let mut raw_input = egui::RawInput::default();
//! # let pixels_per_point = 1.0;
//! let rotation = Rotation::CW90;
//!
//! // 1. Rotate input before egui sees it.
//! transform_raw_input(&mut raw_input, rotation);
//!
//! // 2. Run your app normally — it sees a rotated coordinate space.
//! let full_output = ctx.run_ui(raw_input, |ui| {
//!     ui.label("Hello, rotated world!");
//! });
//!
//! // 3. Tessellate as usual.
//! let mut primitives = ctx.tessellate(full_output.shapes, pixels_per_point);
//!
//! // 4. Rotate primitives back to physical screen space before painting.
//! let logical_size = ctx.screen_rect().size();
//! transform_clipped_primitives(&mut primitives, rotation, logical_size);
//!
//! // 5. Hand `primitives` to your painter (egui_glow, egui_wgpu, custom).
//! ```
//!
//! ## Software cursor (feature `software-cursor`, opt-in)
//!
//! Enable with `egui-rotate = { version = "…", features = ["software-cursor"] }`.
//! See [`SoftwareCursor`] for the rotated-cursor flow used by pinball cabinets
//! and kiosks where the OS cursor cannot be rotated.

mod input;
mod output;
mod rotation;

pub use input::transform_raw_input;
pub use output::transform_clipped_primitives;
pub use rotation::Rotation;

mod cursor_icon;
pub use cursor_icon::CursorIconExt;

#[cfg(feature = "software-cursor")]
mod cursor;
#[cfg(feature = "software-cursor")]
pub use cursor::{SoftwareCursor, SoftwareCursorOutput};
