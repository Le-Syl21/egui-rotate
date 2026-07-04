//! Plugin handling of `PlatformOutput` beyond shapes: the IME area must be
//! rotated back to physical space, since the backend positions the OS
//! composition window on the physical screen.

use egui::{output::IMEOutput, Pos2, Rect, Vec2};
use egui_rotate::{Rotation, RotationPlugin};

const PHYSICAL: Vec2 = Vec2 { x: 800.0, y: 600.0 };

fn raw() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, PHYSICAL)),
        ..Default::default()
    }
}

#[test]
fn ime_rects_are_rotated_to_physical_space() {
    for rotation in [Rotation::CW90, Rotation::CW180, Rotation::CW270] {
        let ctx = egui::Context::default();
        ctx.add_plugin(RotationPlugin::new(rotation));

        let logical_size = rotation
            .transform_screen_rect(Rect::from_min_size(Pos2::ZERO, PHYSICAL))
            .size();

        // A text-edit area and its caret, in logical space.
        let ime_rect = Rect::from_min_max(Pos2::new(10.0, 20.0), Pos2::new(30.0, 60.0));
        let cursor_rect = Rect::from_min_max(Pos2::new(28.0, 20.0), Pos2::new(30.0, 40.0));

        let out = ctx.run_ui(raw(), |ui| {
            ui.ctx().output_mut(|o| {
                o.ime = Some(IMEOutput {
                    rect: ime_rect,
                    cursor_rect,
                });
            });
        });

        let ime = out.platform_output.ime.expect("IME output must survive");
        assert_eq!(
            ime.rect,
            rotation.inverse_transform_rect(ime_rect, logical_size),
            "{rotation:?}: IME rect must land in physical space"
        );
        assert_eq!(
            ime.cursor_rect,
            rotation.inverse_transform_rect(cursor_rect, logical_size),
            "{rotation:?}: IME cursor rect must land in physical space"
        );
    }
}

/// Safe-area insets (phone notches) are given for the *physical* sides; after
/// rotation they must cover the logical sides they actually overlap, so
/// `content_rect()` shrinks the right edges (reported in PR #1 for the manual
/// path; this guards the plugin path).
#[test]
fn safe_area_insets_rotate_with_viewport() {
    let rotation = Rotation::CW90;
    let ctx = egui::Context::default();
    ctx.add_plugin(RotationPlugin::new(rotation));

    let mut input = raw();
    // A 30 px notch along the physical top edge.
    input.safe_area_insets = Some(egui::SafeAreaInsets(egui::epaint::MarginF32 {
        left: 0.0,
        right: 0.0,
        top: 30.0,
        bottom: 0.0,
    }));

    let mut content = Rect::NOTHING;
    let _ = ctx.run_ui(input, |ui| {
        content = ui.ctx().content_rect();
    });

    // 800x600 physical under CW90 → 600x800 logical; the physical top notch
    // lands along the logical *right* edge.
    assert_eq!(
        content,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(600.0 - 30.0, 800.0)),
        "physical top inset should become a logical right inset under CW90"
    );
}

#[test]
fn ime_untouched_without_rotation() {
    let ctx = egui::Context::default();
    ctx.add_plugin(RotationPlugin::new(Rotation::None));

    let ime_rect = Rect::from_min_max(Pos2::new(10.0, 20.0), Pos2::new(30.0, 60.0));
    let out = ctx.run_ui(raw(), |ui| {
        ui.ctx().output_mut(|o| {
            o.ime = Some(IMEOutput {
                rect: ime_rect,
                cursor_rect: ime_rect,
            });
        });
    });

    let ime = out.platform_output.ime.expect("IME output must survive");
    assert_eq!(ime.rect, ime_rect);
}
