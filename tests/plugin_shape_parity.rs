#![allow(deprecated)] // exercises the deprecated manual helpers on purpose
//! Parity + plugin wiring tests.
//!
//! The crate's proven path is post-tessellation ([`transform_clipped_primitives`]):
//! tessellate in logical space, then rotate the mesh vertices. The new pure-plugin
//! path rotates the *pre-tessellation* shapes instead ([`rotate_clipped_shapes`]).
//!
//! These tests assert the two paths produce the **same tessellated geometry** for a
//! realistic egui frame (text, a rounded button, a circle, a line) across all four
//! rotations — which empirically validates the per-variant shape rotation,
//! including the clockwise `angle` sign used for text / rect / ellipse.

use egui::{
    epaint::{ClippedPrimitive, ClippedShape, Primitive},
    Color32, Pos2, Rect, Stroke, Vec2,
};
use egui_rotate::{rotate_clipped_shapes, transform_clipped_primitives, Rotation, RotationPlugin};

const PHYSICAL: Vec2 = Vec2 { x: 800.0, y: 600.0 };

const ALL: [Rotation; 4] = [
    Rotation::None,
    Rotation::CW90,
    Rotation::CW180,
    Rotation::CW270,
];

/// Drive a realistic frame in logical space and return its pre-tessellation
/// shapes plus the pixels-per-point used.
fn logical_frame(ctx: &egui::Context, logical_size: Vec2) -> (Vec<ClippedShape>, f32) {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, logical_size)),
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| {
        ui.label("Rotated text sample 123");
        let _ = ui.button("Click me");
        let painter = ui.painter().clone();
        painter.circle_filled(Pos2::new(120.0, 140.0), 24.0, Color32::RED);
        painter.line_segment(
            [Pos2::new(10.0, 10.0), Pos2::new(60.0, 90.0)],
            Stroke::new(3.0, Color32::GREEN),
        );
    });
    (out.shapes, out.pixels_per_point)
}

fn mesh_vertices(prim: &ClippedPrimitive) -> Vec<Pos2> {
    match &prim.primitive {
        Primitive::Mesh(m) => m.vertices.iter().map(|v| v.pos).collect(),
        Primitive::Callback(_) => Vec::new(),
    }
}

/// Symmetric Hausdorff distance between two vertex clouds.
///
/// Order-independent and robust to the differing vertex *multiplicity* that two
/// tessellations of the same geometry can produce (anti-aliasing triangle fans
/// emit shared positions a different number of times). A real positional error —
/// e.g. a wrong rotation angle or sign — moves the cloud by tens of pixels, so a
/// small tolerance still catches it.
fn hausdorff(a: &[Pos2], b: &[Pos2]) -> f32 {
    let directed = |from: &[Pos2], to: &[Pos2]| {
        from.iter()
            .map(|p| {
                to.iter()
                    .map(|q| (p.x - q.x).hypot(p.y - q.y))
                    .fold(f32::MAX, f32::min)
            })
            .fold(0.0f32, f32::max)
    };
    directed(a, b).max(directed(b, a))
}

#[test]
fn shape_rotation_matches_primitive_rotation() {
    let ctx = egui::Context::default();

    // Pixel snapping rounds to integers independently in logical vs physical space,
    // which can differ by ≤1px between the two paths — orthogonal to the rotation
    // geometry under test. Disable it so the comparison is exact.
    ctx.tessellation_options_mut(|o| {
        o.round_text_to_pixels = false;
        o.round_rects_to_pixels = false;
        o.round_line_segments_to_pixels = false;
    });

    for rotation in ALL {
        let logical_rect =
            rotation.transform_screen_rect(Rect::from_min_size(Pos2::ZERO, PHYSICAL));
        let logical_size = logical_rect.size();

        let (shapes, ppp) = logical_frame(&ctx, logical_size);
        assert!(
            !shapes.is_empty(),
            "frame produced no shapes for {rotation:?}"
        );

        // Old path: tessellate, then rotate primitives.
        let mut prims_old = ctx.tessellate(shapes.clone(), ppp);
        transform_clipped_primitives(&mut prims_old, rotation, logical_size);

        // New path: rotate shapes, then tessellate.
        let mut shapes_new = shapes.clone();
        rotate_clipped_shapes(&mut shapes_new, rotation, logical_size);
        let prims_new = ctx.tessellate(shapes_new, ppp);

        assert_eq!(
            prims_old.len(),
            prims_new.len(),
            "primitive count differs for {rotation:?}"
        );

        // The two paths are geometrically identical; allow a hair for any
        // pixel-snapping noise. A wrong angle/sign moves vertices by tens of px.
        const EPS: f32 = 1.0;

        for (i, (a, b)) in prims_old.iter().zip(prims_new.iter()).enumerate() {
            // Clip rects are transformed by the identical formula in both paths.
            let dclip = (a.clip_rect.min - b.clip_rect.min).abs()
                + (a.clip_rect.max - b.clip_rect.max).abs();
            assert!(
                dclip.x < 0.05 && dclip.y < 0.05,
                "{rotation:?} prim {i}: clip_rect mismatch {:?} vs {:?}",
                a.clip_rect,
                b.clip_rect
            );

            let d = hausdorff(&mesh_vertices(a), &mesh_vertices(b));
            assert!(
                d < EPS,
                "{rotation:?} prim {i}: vertex clouds diverge by {d}px"
            );
        }
    }
}

/// The plugin wires `input_hook` (rotate input into logical space) and
/// `output_hook` (rotate shapes back to physical space + remap cursor) together.
#[test]
fn plugin_rotates_output_shapes() {
    let rotation = Rotation::CW90;
    let ctx = egui::Context::default();
    ctx.add_plugin(RotationPlugin::new(rotation));

    // Integration passes the *physical* screen rect; the plugin swaps it to logical.
    let logical_size = rotation
        .transform_screen_rect(Rect::from_min_size(Pos2::ZERO, PHYSICAL))
        .size();

    // Draw a circle at a known *logical* position.
    let logical_center = Pos2::new(120.0, 140.0);
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, PHYSICAL)),
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| {
        ui.painter()
            .circle_filled(logical_center, 24.0, Color32::RED);
    });

    // After the plugin's output_hook, the circle must sit at the physical position.
    let expected = rotation.inverse_transform_pos(logical_center, logical_size);
    let found = out.shapes.iter().find_map(|cs| match &cs.shape {
        egui::Shape::Circle(c) => Some(c.center),
        _ => None,
    });
    let center = found.expect("no circle shape in output");
    assert!(
        (center - expected).abs().max_elem() < 0.01,
        "plugin output circle at {center:?}, expected {expected:?}"
    );
}

/// A textured (image) rect must rotate *with* the viewport. egui's `RectShape::angle`
/// leaves the brush texture screen-aligned, so the crate converts textured rects to a
/// rotated quad mesh — this guards that conversion (corners + UVs).
#[test]
fn textured_rect_rotates_with_viewport() {
    use egui::epaint::{Brush, RectShape};
    use std::sync::Arc;

    let rotation = Rotation::CW90;
    let logical_size = rotation
        .transform_screen_rect(Rect::from_min_size(Pos2::ZERO, PHYSICAL))
        .size();

    let rect = Rect::from_min_max(Pos2::new(100.0, 200.0), Pos2::new(300.0, 260.0));
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    let mut rs = RectShape::filled(rect, egui::CornerRadius::ZERO, Color32::WHITE);
    rs.brush = Some(Arc::new(Brush {
        fill_texture_id: egui::TextureId::Managed(7),
        uv,
    }));

    let mut shape = egui::Shape::Rect(rs);
    egui_rotate::rotate_shape(&mut shape, rotation, logical_size);

    let egui::Shape::Mesh(mesh) = shape else {
        panic!("a textured rect should become a Mesh after rotation");
    };
    assert_eq!(mesh.texture_id, egui::TextureId::Managed(7));
    assert_eq!(mesh.vertices.len(), 4);
    // Each corner sits at the rotated position, with its original UV preserved.
    for (corner_pos, corner_uv, v) in [
        (rect.left_top(), uv.left_top(), &mesh.vertices[0]),
        (rect.right_top(), uv.right_top(), &mesh.vertices[1]),
        (rect.left_bottom(), uv.left_bottom(), &mesh.vertices[2]),
        (rect.right_bottom(), uv.right_bottom(), &mesh.vertices[3]),
    ] {
        let expected = rotation.inverse_transform_pos(corner_pos, logical_size);
        assert!(
            (v.pos - expected).length() < 0.01,
            "corner at {corner_pos:?} → {:?}, expected {expected:?}",
            v.pos
        );
        assert_eq!(v.uv, corner_uv);
    }
}

/// A textured rect with a visible border keeps its border after rotation: the
/// fill becomes a mesh, and the stroke is re-emitted as a closed path over the
/// rotated corners.
#[test]
fn textured_rect_keeps_stroke() {
    use egui::epaint::{Brush, RectShape, StrokeKind};
    use std::sync::Arc;

    let rotation = Rotation::CW90;
    let logical_size = rotation
        .transform_screen_rect(Rect::from_min_size(Pos2::ZERO, PHYSICAL))
        .size();

    let rect = Rect::from_min_max(Pos2::new(100.0, 200.0), Pos2::new(300.0, 260.0));
    let mut rs = RectShape::new(
        rect,
        egui::CornerRadius::ZERO,
        Color32::WHITE,
        Stroke::new(2.0, Color32::BLACK),
        StrokeKind::Middle,
    );
    rs.brush = Some(Arc::new(Brush {
        fill_texture_id: egui::TextureId::Managed(7),
        uv: Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
    }));

    let mut shape = egui::Shape::Rect(rs);
    egui_rotate::rotate_shape(&mut shape, rotation, logical_size);

    let egui::Shape::Vec(shapes) = shape else {
        panic!("a stroked textured rect should become mesh + outline");
    };
    assert!(matches!(shapes[0], egui::Shape::Mesh(_)));
    let egui::Shape::Path(path) = &shapes[1] else {
        panic!("second shape should be the outline path");
    };
    assert!(path.closed);
    // Middle stroke: the outline follows the rect edge exactly, rotated.
    for (corner, p) in [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ]
    .iter()
    .zip(&path.points)
    {
        let expected = rotation.inverse_transform_pos(*corner, logical_size);
        assert!(
            (*p - expected).length() < 0.01,
            "outline corner {corner:?} → {p:?}, expected {expected:?}"
        );
    }
}

#[test]
fn plugin_none_is_passthrough() {
    let ctx = egui::Context::default();
    ctx.add_plugin(RotationPlugin::new(Rotation::None));

    let logical_center = Pos2::new(120.0, 140.0);
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, PHYSICAL)),
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| {
        ui.painter()
            .circle_filled(logical_center, 24.0, Color32::RED);
    });

    let center = out
        .shapes
        .iter()
        .find_map(|cs| match &cs.shape {
            egui::Shape::Circle(c) => Some(c.center),
            _ => None,
        })
        .expect("no circle shape in output");
    assert!((center - logical_center).abs().max_elem() < 0.01);
}
