//! Per-viewport rotation: the plugin rotates each viewport by its own configured
//! rotation (or not at all), and reunites each `output_hook` with the right
//! viewport via the begin/end LIFO pairing — even when child viewports nest.
//!
//! These drive the plugin hooks directly (no window needed), simulating the order
//! egui calls them in: nested immediate viewports pair up `input,input,output,output`.

use egui::{epaint::ClippedShape, Color32, Plugin, Pos2, Rect, Shape, Vec2, ViewportId};
use egui_rotate::{Rotation, RotationPlugin};

const PHYSICAL: Vec2 = Vec2 { x: 800.0, y: 600.0 };

fn raw_for(viewport: ViewportId, size: Vec2) -> egui::RawInput {
    egui::RawInput {
        viewport_id: viewport,
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        ..Default::default()
    }
}

fn circle_output(center: Pos2) -> egui::FullOutput {
    let mut out = egui::FullOutput::default();
    out.shapes.push(ClippedShape {
        clip_rect: Rect::EVERYTHING,
        shape: Shape::circle_filled(center, 5.0, Color32::RED),
    });
    out
}

fn circle_center(out: &egui::FullOutput) -> Pos2 {
    match &out.shapes[0].shape {
        Shape::Circle(c) => c.center,
        other => panic!("expected a circle, got {other:?}"),
    }
}

#[test]
fn unconfigured_child_viewport_is_not_rotated() {
    let child = ViewportId::from_hash_of("child-window");
    let mut plugin = RotationPlugin::new(Rotation::CW90); // root only

    // input: root begins, then child begins (nested).
    let mut root_in = raw_for(ViewportId::ROOT, PHYSICAL);
    plugin.input_hook(&mut root_in);
    assert_eq!(
        root_in.screen_rect.unwrap().size(),
        Vec2::new(600.0, 800.0),
        "root input is rotated to logical space"
    );

    let mut child_in = raw_for(child, Vec2::new(1024.0, 768.0));
    plugin.input_hook(&mut child_in);
    assert_eq!(
        child_in.screen_rect.unwrap().size(),
        Vec2::new(1024.0, 768.0),
        "unconfigured child input is untouched"
    );

    // output: child ends first (LIFO), then root.
    let logical = Pos2::new(100.0, 100.0);

    let mut child_out = circle_output(logical);
    plugin.output_hook(&mut child_out);
    assert_eq!(
        circle_center(&child_out),
        logical,
        "child viewport shapes must not be rotated"
    );

    let mut root_out = circle_output(logical);
    plugin.output_hook(&mut root_out);
    let expected = Rotation::CW90.inverse_transform_pos(logical, Vec2::new(600.0, 800.0));
    assert!(
        (circle_center(&root_out) - expected).length() < 0.01,
        "root viewport must be rotated with its own logical size"
    );
}

#[test]
fn each_viewport_uses_its_own_rotation_and_size() {
    let child = ViewportId::from_hash_of("child");
    let mut plugin = RotationPlugin::new(Rotation::CW90);
    plugin.set_viewport_rotation(child, Rotation::CW180);

    // Different sizes prove the stack doesn't cross root/child state.
    let mut root_in = raw_for(ViewportId::ROOT, Vec2::new(800.0, 600.0));
    plugin.input_hook(&mut root_in);
    let mut child_in = raw_for(child, Vec2::new(400.0, 300.0));
    plugin.input_hook(&mut child_in);

    let p = Pos2::new(50.0, 60.0);

    let mut child_out = circle_output(p);
    plugin.output_hook(&mut child_out);
    let expected_child = Rotation::CW180.inverse_transform_pos(p, Vec2::new(400.0, 300.0));
    assert!(
        (circle_center(&child_out) - expected_child).length() < 0.01,
        "child rotated by CW180 with its own 400×300 size"
    );

    let mut root_out = circle_output(p);
    plugin.output_hook(&mut root_out);
    let expected_root = Rotation::CW90.inverse_transform_pos(p, Vec2::new(600.0, 800.0));
    assert!(
        (circle_center(&root_out) - expected_root).length() < 0.01,
        "root rotated by CW90 with its own 600×800 size"
    );
}
