//! End-to-end pipeline test: drives a real egui::Context through a frame with
//! rotation applied. Verifies the input/tessellate/output pipeline composes
//! without panic and produces sane primitives.

use egui::{
    epaint::{ClippedPrimitive, Primitive},
    Event, PointerButton, Pos2, Rect, Vec2,
};
use egui_rotate::{transform_clipped_primitives, transform_raw_input, Rotation};

const PHYSICAL: Vec2 = Vec2 { x: 800.0, y: 600.0 };

fn make_raw(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, PHYSICAL)),
        events,
        ..Default::default()
    }
}

#[test]
fn input_rotation_lands_in_logical_space_at_ctx_run() {
    // Send a physical-center click. After CW90 rotation, the logical-center
    // pointer position must match what `ctx.input(...)` reports.
    let rotation = Rotation::CW90;
    let ctx = egui::Context::default();

    let physical_center = Pos2::new(PHYSICAL.x / 2.0, PHYSICAL.y / 2.0); // (400, 300)
    let mut raw = make_raw(vec![
        Event::PointerMoved(physical_center),
        Event::PointerButton {
            pos: physical_center,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ]);
    transform_raw_input(&mut raw, rotation);

    // Logical screen for CW90 of 800x600 physical → 600x800
    let expected_logical_center = rotation.transform_pos(physical_center, PHYSICAL); // (300, 400)
    assert_eq!(expected_logical_center, Pos2::new(300.0, 400.0));

    let mut seen_pointer = None;
    let _ = ctx.run_ui(raw, |ui| {
        ui.ctx().input(|i| {
            seen_pointer = i.pointer.interact_pos();
        });
    });

    let pointer = seen_pointer.expect("ctx.input must report the click");
    assert!(
        (pointer.x - expected_logical_center.x).abs() < 0.5
            && (pointer.y - expected_logical_center.y).abs() < 0.5,
        "egui saw {pointer:?}, expected {expected_logical_center:?}"
    );
}

#[test]
fn full_pipeline_runs_clean_in_all_rotations() {
    // For each rotation, run a full frame and verify no panic, primitives are
    // generated, and rotation didn't drop anything.
    for rotation in [
        Rotation::None,
        Rotation::CW90,
        Rotation::CW180,
        Rotation::CW270,
    ] {
        let ctx = egui::Context::default();

        let mut raw = make_raw(vec![]);
        transform_raw_input(&mut raw, rotation);
        let logical_size = raw.screen_rect.unwrap().size();

        let full_output = ctx.run_ui(raw, |ui| {
            ui.heading("Frame test");
            ui.label("body text body text body text");
            let _ = ui.button("ok");
        });

        let pre = ctx.tessellate(full_output.shapes, 1.0);
        let pre_vertex_count: usize = pre
            .iter()
            .map(|p| {
                if let Primitive::Mesh(m) = &p.primitive {
                    m.vertices.len()
                } else {
                    0
                }
            })
            .sum();

        let mut post = pre.clone();
        transform_clipped_primitives(&mut post, rotation, logical_size);
        let post_vertex_count: usize = post
            .iter()
            .map(|p| {
                if let Primitive::Mesh(m) = &p.primitive {
                    m.vertices.len()
                } else {
                    0
                }
            })
            .sum();

        assert_eq!(
            pre.len(),
            post.len(),
            "{rotation:?}: primitive count must not change"
        );
        assert_eq!(
            pre_vertex_count, post_vertex_count,
            "{rotation:?}: vertex count must not change"
        );
        assert!(pre_vertex_count > 0, "{rotation:?}: empty frame");

        // Sanity: post-transform vertices should be finite (no NaN from the rotation math).
        for prim in &post {
            if let Primitive::Mesh(m) = &prim.primitive {
                for v in &m.vertices {
                    assert!(
                        v.pos.x.is_finite() && v.pos.y.is_finite(),
                        "{rotation:?}: non-finite vertex {:?}",
                        v.pos
                    );
                }
            }
        }
    }
}

#[test]
fn pipeline_zero_cost_when_rotation_is_none() {
    let ctx = egui::Context::default();
    let mut raw = make_raw(vec![Event::PointerMoved(Pos2::new(123.0, 45.0))]);
    let raw_before = raw.clone();
    transform_raw_input(&mut raw, Rotation::None);
    assert_eq!(raw.screen_rect, raw_before.screen_rect);
    if let (Event::PointerMoved(a), Event::PointerMoved(b)) =
        (&raw.events[0], &raw_before.events[0])
    {
        assert_eq!(a, b);
    }

    let full_output = ctx.run_ui(raw, |ui| {
        ui.label("hi");
    });
    let prims_before: Vec<ClippedPrimitive> = ctx.tessellate(full_output.shapes, 1.0);
    let mut prims_after = prims_before.clone();
    transform_clipped_primitives(&mut prims_after, Rotation::None, PHYSICAL);

    for (a, b) in prims_before.iter().zip(prims_after.iter()) {
        assert_eq!(a.clip_rect, b.clip_rect);
        if let (Primitive::Mesh(ma), Primitive::Mesh(mb)) = (&a.primitive, &b.primitive) {
            for (va, vb) in ma.vertices.iter().zip(mb.vertices.iter()) {
                assert_eq!(va.pos, vb.pos);
            }
        }
    }
}
