use egui::CursorIcon;
use egui_rotate::{CursorIconExt, Rotation};

#[test]
fn none_passthrough() {
    assert_eq!(
        CursorIcon::ResizeHorizontal.rotate(Rotation::None),
        CursorIcon::ResizeHorizontal
    );
    assert_eq!(CursorIcon::Text.rotate(Rotation::None), CursorIcon::Text);
}

#[test]
fn horizontal_vertical_swap_at_90_270() {
    assert_eq!(
        CursorIcon::ResizeHorizontal.rotate(Rotation::CW90),
        CursorIcon::ResizeVertical
    );
    assert_eq!(
        CursorIcon::ResizeHorizontal.rotate(Rotation::CW270),
        CursorIcon::ResizeVertical
    );
    // 180° preserves horizontal/vertical (axes still aligned)
    assert_eq!(
        CursorIcon::ResizeHorizontal.rotate(Rotation::CW180),
        CursorIcon::ResizeHorizontal
    );
}

#[test]
fn text_becomes_vertical_text() {
    assert_eq!(
        CursorIcon::Text.rotate(Rotation::CW90),
        CursorIcon::VerticalText
    );
    assert_eq!(CursorIcon::Text.rotate(Rotation::CW180), CursorIcon::Text);
    assert_eq!(
        CursorIcon::Text.rotate(Rotation::CW270),
        CursorIcon::VerticalText
    );
}

#[test]
fn directional_rotates_clockwise() {
    assert_eq!(
        CursorIcon::ResizeEast.rotate(Rotation::CW90),
        CursorIcon::ResizeSouth
    );
    assert_eq!(
        CursorIcon::ResizeEast.rotate(Rotation::CW180),
        CursorIcon::ResizeWest
    );
    assert_eq!(
        CursorIcon::ResizeEast.rotate(Rotation::CW270),
        CursorIcon::ResizeNorth
    );
}

#[test]
fn diagonal_resize_remap() {
    assert_eq!(
        CursorIcon::ResizeNorthEast.rotate(Rotation::CW90),
        CursorIcon::ResizeSouthEast
    );
    assert_eq!(
        CursorIcon::ResizeNorthEast.rotate(Rotation::CW180),
        CursorIcon::ResizeSouthWest
    );
    assert_eq!(
        CursorIcon::ResizeNorthEast.rotate(Rotation::CW270),
        CursorIcon::ResizeNorthWest
    );
}

#[test]
fn invariant_cursors_unchanged() {
    for r in [Rotation::CW90, Rotation::CW180, Rotation::CW270] {
        assert_eq!(CursorIcon::Default.rotate(r), CursorIcon::Default);
        assert_eq!(CursorIcon::Help.rotate(r), CursorIcon::Help);
        assert_eq!(CursorIcon::PointingHand.rotate(r), CursorIcon::PointingHand);
        assert_eq!(CursorIcon::Wait.rotate(r), CursorIcon::Wait);
    }
}
