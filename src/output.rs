use egui::{
    epaint::{ClippedPrimitive, Primitive},
    Pos2, Rect, Vec2,
};

use crate::Rotation;

/// Transform tessellated primitives from logical UI space back to physical screen space.
///
/// Call this **after** [`egui::Context::tessellate`], **before** passing primitives
/// to your painter (egui_glow, egui_wgpu, custom).
///
/// What gets transformed:
/// - `clip_rect` (inverse-transformed and re-normalised via `Rect::from_two_pos`)
/// - mesh `vertex.pos` (inverse-transformed)
///
/// `PaintCallback` primitives are **not** rotated — custom callbacks are responsible
/// for handling their own coordinate space (or you can wrap them).
///
/// `logical_size` is the rotated (post-rotation) screen size — i.e. what your egui
/// app sees as `ctx.screen_rect().size()`.
pub fn transform_clipped_primitives(
    primitives: &mut [ClippedPrimitive],
    rotation: Rotation,
    logical_size: Vec2,
) {
    if rotation.is_none() {
        return;
    }

    for primitive in primitives.iter_mut() {
        let min = rotation.inverse_transform_pos(primitive.clip_rect.min, logical_size);
        let max = rotation.inverse_transform_pos(primitive.clip_rect.max, logical_size);
        primitive.clip_rect = Rect::from_two_pos(min, max);

        if let Primitive::Mesh(mesh) = &mut primitive.primitive {
            for vertex in &mut mesh.vertices {
                vertex.pos = rotation
                    .inverse_transform_pos(Pos2::new(vertex.pos.x, vertex.pos.y), logical_size);
            }
        }
    }
}
