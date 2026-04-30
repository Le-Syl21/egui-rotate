use egui::{
    epaint::{ClippedPrimitive, Mesh, Primitive, Vertex},
    Color32, Event, PointerButton, Pos2, Rect, Vec2,
};
use egui_rotate::{transform_clipped_primitives, transform_raw_input, Rotation};

const PHYSICAL: Vec2 = Vec2 { x: 800.0, y: 600.0 };

fn raw_with_pointer_at(x: f32, y: f32) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, PHYSICAL)),
        events: vec![
            Event::PointerMoved(Pos2::new(x, y)),
            Event::PointerButton {
                pos: Pos2::new(x, y),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
        ..Default::default()
    }
}

#[test]
fn input_passthrough_when_none() {
    let mut raw = raw_with_pointer_at(10.0, 20.0);
    transform_raw_input(&mut raw, Rotation::None);
    match &raw.events[0] {
        Event::PointerMoved(p) => assert_eq!(*p, Pos2::new(10.0, 20.0)),
        _ => panic!(),
    }
    assert_eq!(raw.screen_rect.unwrap().size(), PHYSICAL);
}

#[test]
fn input_screen_rect_swaps_for_90() {
    let mut raw = raw_with_pointer_at(0.0, 0.0);
    transform_raw_input(&mut raw, Rotation::CW90);
    assert_eq!(raw.screen_rect.unwrap().size(), Vec2::new(600.0, 800.0));
}

#[test]
fn input_pointer_remapped_for_90() {
    let mut raw = raw_with_pointer_at(0.0, 0.0);
    transform_raw_input(&mut raw, Rotation::CW90);
    match &raw.events[0] {
        Event::PointerMoved(p) => assert_eq!(*p, Pos2::new(600.0, 0.0)),
        _ => panic!(),
    }
    match &raw.events[1] {
        Event::PointerButton { pos, .. } => assert_eq!(*pos, Pos2::new(600.0, 0.0)),
        _ => panic!(),
    }
}

#[test]
fn input_wheel_delta_rotated() {
    let mut raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, PHYSICAL)),
        events: vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: Vec2::new(1.0, 0.0),
            phase: egui::TouchPhase::Move,
            modifiers: Default::default(),
        }],
        ..Default::default()
    };
    transform_raw_input(&mut raw, Rotation::CW90);
    match &raw.events[0] {
        Event::MouseWheel { delta, .. } => assert_eq!(*delta, Vec2::new(0.0, 1.0)),
        _ => panic!(),
    }
}

fn mesh_with_vertex(x: f32, y: f32) -> Mesh {
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex {
        pos: Pos2::new(x, y),
        uv: Pos2::ZERO,
        color: Color32::WHITE,
    });
    mesh
}

#[test]
fn output_passthrough_when_none() {
    let mut prims = vec![ClippedPrimitive {
        clip_rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0)),
        primitive: Primitive::Mesh(mesh_with_vertex(50.0, 50.0)),
    }];
    transform_clipped_primitives(&mut prims, Rotation::None, Vec2::new(800.0, 600.0));
    if let Primitive::Mesh(m) = &prims[0].primitive {
        assert_eq!(m.vertices[0].pos, Pos2::new(50.0, 50.0));
    }
}

#[test]
fn output_vertex_inverse_transformed_for_90() {
    // logical_size for CW90 of a 800x600 physical is 600x800
    let logical_size = Vec2::new(600.0, 800.0);
    let mut prims = vec![ClippedPrimitive {
        clip_rect: Rect::from_min_size(Pos2::ZERO, logical_size),
        primitive: Primitive::Mesh(mesh_with_vertex(600.0, 0.0)),
    }];
    transform_clipped_primitives(&mut prims, Rotation::CW90, logical_size);
    if let Primitive::Mesh(m) = &prims[0].primitive {
        // (600, 0) in logical CW90 maps back to (0, 0) physical
        assert!((m.vertices[0].pos.x - 0.0).abs() < 1e-4);
        assert!((m.vertices[0].pos.y - 0.0).abs() < 1e-4);
    } else {
        panic!()
    }
}

#[test]
fn input_then_output_roundtrip_at_corner() {
    // A click at physical (0,0) should land at the same physical coords after
    // input rotation (logical) then output inverse rotation (physical).
    let mut raw = raw_with_pointer_at(0.0, 0.0);
    transform_raw_input(&mut raw, Rotation::CW90);

    // After input transform: pointer is in logical space at (600, 0).
    let logical_pos = match &raw.events[0] {
        Event::PointerMoved(p) => *p,
        _ => panic!(),
    };

    // Build a mesh containing that pointer position as a vertex.
    let logical_size = raw.screen_rect.unwrap().size();
    let mut prims = vec![ClippedPrimitive {
        clip_rect: Rect::from_min_size(Pos2::ZERO, logical_size),
        primitive: Primitive::Mesh(mesh_with_vertex(logical_pos.x, logical_pos.y)),
    }];
    transform_clipped_primitives(&mut prims, Rotation::CW90, logical_size);

    if let Primitive::Mesh(m) = &prims[0].primitive {
        assert!(m.vertices[0].pos.x.abs() < 1e-4);
        assert!(m.vertices[0].pos.y.abs() < 1e-4);
    }
}
