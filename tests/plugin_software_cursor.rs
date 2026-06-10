//! Software-cursor integration through [`RotationPlugin`] (feature-gated).
#![cfg(feature = "software-cursor")]

use egui::{CursorIcon, Event, Pos2, Rect, Vec2};
use egui_rotate::{Rotation, RotationPlugin, SoftwareCursor};

const PHYSICAL: Vec2 = Vec2 { x: 800.0, y: 600.0 };

fn raw(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, PHYSICAL)),
        events,
        ..Default::default()
    }
}

/// While the software cursor is captured, the OS cursor must be hidden
/// (`CursorIcon::None`) since the plugin draws its own.
#[test]
fn locked_cursor_hides_os_cursor() {
    let ctx = egui::Context::default();
    ctx.add_plugin(
        RotationPlugin::new(Rotation::CW90)
            .with_software_cursor(SoftwareCursor::new().with_lock(true)),
    );

    // A pointer event seeds capture.
    let out = ctx.run_ui(
        raw(vec![Event::PointerMoved(Pos2::new(400.0, 300.0))]),
        |_| {},
    );

    assert_eq!(
        out.platform_output.cursor_icon,
        CursorIcon::None,
        "OS cursor should be hidden while the software cursor is captured"
    );

    // Locked mode never releases to the OS.
    let warp = ctx.plugin::<RotationPlugin>().lock().take_pending_warp();
    assert!(warp.is_none(), "locked mode must not request a warp");
}

/// In non-locked mode, moving past the screen edge releases the cursor to the OS
/// and surfaces a warp request the integration can act on.
#[test]
fn nonlocked_cursor_requests_warp_at_edge() {
    let ctx = egui::Context::default();
    ctx.add_plugin(RotationPlugin::new(Rotation::CW90).with_software_cursor(SoftwareCursor::new()));

    // Seed capture at the logical centre, then push far past the right edge via a
    // raw mouse delta in the same batch.
    let out = ctx.run_ui(
        raw(vec![
            Event::PointerMoved(Pos2::new(400.0, 300.0)),
            Event::MouseMoved(Vec2::new(400.0, 0.0)),
        ]),
        |_| {},
    );

    // Released → OS cursor visible again (not forced to None).
    assert_ne!(out.platform_output.cursor_icon, CursorIcon::None);

    let warp = ctx.plugin::<RotationPlugin>().lock().take_pending_warp();
    assert!(
        warp.is_some(),
        "edge release in non-locked mode should request an OS-cursor warp"
    );
    // Draining it once leaves nothing behind.
    assert!(ctx
        .plugin::<RotationPlugin>()
        .lock()
        .take_pending_warp()
        .is_none());
}

/// Without a software cursor, the plugin remaps directional OS cursor icons.
#[test]
fn no_software_cursor_remaps_icon() {
    let ctx = egui::Context::default();
    ctx.add_plugin(RotationPlugin::new(Rotation::CW90));

    let out = ctx.run_ui(raw(vec![]), |ui| {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    });

    // Under 90°, a horizontal resize handle is visually vertical.
    assert_eq!(out.platform_output.cursor_icon, CursorIcon::ResizeVertical);
}
