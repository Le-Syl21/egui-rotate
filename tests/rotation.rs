use egui::{Pos2, Rect, Vec2};
use egui_rotate::Rotation;

const ALL: [Rotation; 4] = [
    Rotation::None,
    Rotation::CW90,
    Rotation::CW180,
    Rotation::CW270,
];

#[test]
fn none_is_identity() {
    let pos = Pos2::new(10.0, 20.0);
    let size = Vec2::new(800.0, 600.0);
    assert_eq!(Rotation::None.transform_pos(pos, size), pos);
    assert_eq!(Rotation::None.inverse_transform_pos(pos, size), pos);
    assert_eq!(
        Rotation::None.transform_vec(Vec2::new(3.0, 4.0)),
        Vec2::new(3.0, 4.0)
    );
}

#[test]
fn roundtrip_all_rotations() {
    let physical_size = Vec2::new(800.0, 600.0);
    let pos = Pos2::new(100.0, 200.0);

    for rotation in ALL {
        let logical_size = if rotation.swaps_axes() {
            Vec2::new(physical_size.y, physical_size.x)
        } else {
            physical_size
        };

        let transformed = rotation.transform_pos(pos, physical_size);
        let back = rotation.inverse_transform_pos(transformed, logical_size);
        assert!(
            (back.x - pos.x).abs() < 1e-6 && (back.y - pos.y).abs() < 1e-6,
            "Roundtrip failed for {rotation:?}: {pos:?} -> {transformed:?} -> {back:?}"
        );
    }
}

#[test]
fn cw90_corner_mapping() {
    let physical_size = Vec2::new(800.0, 600.0);
    // Physical top-left (0, 0) under CW90 → logical (height, 0) = (600, 0)
    let result = Rotation::CW90.transform_pos(Pos2::new(0.0, 0.0), physical_size);
    assert_eq!(result, Pos2::new(600.0, 0.0));
}

#[test]
fn cw180_corner_mapping() {
    let physical_size = Vec2::new(800.0, 600.0);
    let result = Rotation::CW180.transform_pos(Pos2::new(0.0, 0.0), physical_size);
    assert_eq!(result, Pos2::new(800.0, 600.0));
}

#[test]
fn cw270_corner_mapping() {
    let physical_size = Vec2::new(800.0, 600.0);
    let result = Rotation::CW270.transform_pos(Pos2::new(0.0, 0.0), physical_size);
    assert_eq!(result, Pos2::new(0.0, 800.0));
}

#[test]
fn screen_rect_axis_swap() {
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));

    assert_eq!(Rotation::None.transform_screen_rect(rect), rect);
    assert_eq!(
        Rotation::CW90.transform_screen_rect(rect),
        Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 800.0))
    );
    assert_eq!(Rotation::CW180.transform_screen_rect(rect), rect);
    assert_eq!(
        Rotation::CW270.transform_screen_rect(rect),
        Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 800.0))
    );
}

#[test]
fn vector_rotation() {
    let v = Vec2::new(1.0, 0.0);
    assert_eq!(Rotation::CW90.transform_vec(v), Vec2::new(0.0, 1.0));
    assert_eq!(Rotation::CW180.transform_vec(v), Vec2::new(-1.0, 0.0));
    assert_eq!(Rotation::CW270.transform_vec(v), Vec2::new(0.0, -1.0));
}

#[test]
fn swaps_axes_predicate() {
    assert!(!Rotation::None.swaps_axes());
    assert!(Rotation::CW90.swaps_axes());
    assert!(!Rotation::CW180.swaps_axes());
    assert!(Rotation::CW270.swaps_axes());
}

#[test]
fn vector_quadruple_is_identity() {
    // CW90 four times = identity for any vector
    let v = Vec2::new(3.5, -7.25);
    let result = Rotation::CW90.transform_vec(
        Rotation::CW90.transform_vec(Rotation::CW90.transform_vec(Rotation::CW90.transform_vec(v))),
    );
    assert!((result.x - v.x).abs() < 1e-6);
    assert!((result.y - v.y).abs() < 1e-6);
}
