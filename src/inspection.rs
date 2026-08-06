//! Optional [`egui_inspection`] wiring for rotated apps.
//!
//! `egui_inspection` lets an external tool (e.g. the `egui_mcp` MCP server)
//! read the AccessKit tree, inject input events and capture screenshots of a
//! live app. Both it and egui-rotate are [`egui::Plugin`]s hooking the raw
//! input, so **registration order is a contract**:
//!
//! * The inspector reasons in **logical** (unrotated) space: the tree it
//!   reads is logical, so the events it injects are logical too.
//! * [`RotationPlugin`](crate::RotationPlugin) rotates **physical** window
//!   events into logical space in its `input_hook`.
//!
//! Registering the inspection plugin **after** the rotation plugin means
//! real window events get rotated first and injected events — appended by a
//! later hook — bypass the rotation entirely. Each event kind crosses
//! exactly one transform. The inverse order would rotate injected events a
//! second time and clicks would land in the wrong place (see the
//! `inspection_rotation` tests, which pin both directions).
//!
//! Screenshots are taken from the real framebuffer and therefore show the
//! **physical, rotated** image — that is what the screen shows, but it does
//! not overlay 1:1 onto logical tree coordinates; map through
//! [`Rotation`](crate::Rotation) when comparing.

use egui::Context;

/// Attach the inspection server when `EGUI_INSPECTION` is set in the
/// environment (the standard `egui_inspection` activation), with the plugin
/// ordering a rotated app needs.
///
/// Call this **after** registering the
/// [`RotationPlugin`](crate::RotationPlugin). Returns `Ok(true)` when the
/// server is listening, `Ok(false)` when the env var is absent.
pub fn attach_inspection_from_env(ctx: &Context, label: Option<String>) -> std::io::Result<bool> {
    egui_inspection::attach_from_env(ctx, label)
}
