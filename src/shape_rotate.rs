use std::sync::Arc;

use egui::{
    epaint::{ClippedShape, Mesh, PathShape, RectShape, Shape, StrokeKind, Vertex},
    Pos2, Rect, Vec2,
};

use crate::Rotation;

/// Rotate pre-tessellation [`ClippedShape`]s from logical UI space back to
/// physical screen space, in place.
///
/// This is the shape-level counterpart of [`crate::transform_clipped_primitives`]:
/// it produces the same visual result, but operates on `FullOutput.shapes`
/// **before** [`egui::Context::tessellate`] rather than on the tessellated
/// primitives afterwards. That is what lets the whole crate run as a pure
/// [`egui::Plugin`] (see [`crate::RotationPlugin`]) — a plugin only ever sees the
/// pre-tessellation shapes.
///
/// For 90° increments the rotation is an isometry that maps axis-aligned clip
/// rects to axis-aligned clip rects, so clipping commutes with the rotation and
/// the GPU scissor stays axis-aligned (the original blocker in egui#4130).
///
/// `logical_size` is the rotated (post-rotation) screen size — i.e. what your
/// egui app sees as `ctx.screen_rect().size()`.
///
/// [`Shape::Callback`] primitives are **not** rotated — backend callbacks own
/// their coordinate space (same limitation as [`crate::transform_clipped_primitives`]).
pub fn rotate_clipped_shapes(shapes: &mut [ClippedShape], rotation: Rotation, logical_size: Vec2) {
    if rotation.is_none() {
        return;
    }

    for clipped in shapes.iter_mut() {
        clipped.clip_rect = rotation.inverse_transform_rect(clipped.clip_rect, logical_size);

        rotate_shape(&mut clipped.shape, rotation, logical_size);
    }
}

/// Rotate a single [`Shape`] from logical UI space back to physical screen space,
/// in place.
///
/// Point-based shapes (paths, meshes, line segments, béziers, circles) have every
/// position mapped through [`Rotation::inverse_transform_pos`]. Shapes carrying a
/// clockwise `angle` field (rect, ellipse, text) are instead recentred and have
/// [`Rotation::inverse_angle`] added to their angle, so epaint's tessellator
/// renders them rotated with correct anti-aliasing.
///
/// See [`rotate_clipped_shapes`] for the limitation on [`Shape::Callback`].
pub fn rotate_shape(shape: &mut Shape, rotation: Rotation, logical_size: Vec2) {
    if rotation.is_none() {
        return;
    }

    let angle = rotation.inverse_angle();
    let map = |p: Pos2| rotation.inverse_transform_pos(p, logical_size);

    // Textured rects (images) need special handling: egui's tessellator rotates a
    // `RectShape`'s geometry via `angle` but keeps the brush texture screen-aligned
    // (its UV is remapped from the already-rotated vertex position against the
    // un-rotated rect), so a rotated image would render upright. Emit the quad
    // ourselves instead, with rotated corners and corner-matched UVs.
    if let Shape::Rect(r) = shape {
        if r.brush.is_some() {
            let mesh = Shape::Mesh(Arc::new(textured_rect_to_mesh(r, rotation, logical_size)));
            *shape = match textured_rect_outline(r, rotation, logical_size) {
                Some(outline) => Shape::Vec(vec![mesh, outline]),
                None => mesh,
            };
            return;
        }
    }

    match shape {
        Shape::Noop => {}

        Shape::Vec(shapes) => {
            for s in shapes.iter_mut() {
                rotate_shape(s, rotation, logical_size);
            }
        }

        // A circle is rotation-invariant; only its centre moves.
        Shape::Circle(c) => {
            c.center = map(c.center);
        }

        Shape::Ellipse(e) => {
            e.center = map(e.center);
            e.angle += angle;
        }

        Shape::LineSegment { points, .. } => {
            for p in points.iter_mut() {
                *p = map(*p);
            }
        }

        Shape::Path(path) => {
            for p in path.points.iter_mut() {
                *p = map(*p);
            }
        }

        // `RectShape::angle` rotates the rect clockwise around its own centre, so
        // the rect keeps its logical size and unchanged corner radii — we only move
        // the centre and add the rotation angle.
        Shape::Rect(r) => {
            let size = r.rect.size();
            r.rect = Rect::from_center_size(map(r.rect.center()), size);
            r.angle += angle;
        }

        Shape::Text(t) => {
            t.pos = map(t.pos);
            t.angle += angle;
        }

        Shape::Mesh(mesh) => {
            for v in Arc::make_mut(mesh).vertices.iter_mut() {
                v.pos = map(v.pos);
            }
        }

        Shape::QuadraticBezier(b) => {
            for p in b.points.iter_mut() {
                *p = map(*p);
            }
        }

        Shape::CubicBezier(b) => {
            for p in b.points.iter_mut() {
                *p = map(*p);
            }
        }

        // Backend-specific painting: the callback owns its coordinate space and
        // cannot be rotated here. Documented limitation.
        Shape::Callback(_) => {}
    }
}

/// Outline for a rotated textured rect, or `None` if its stroke is invisible.
///
/// [`textured_rect_to_mesh`] only emits the fill quad, so a visible border must
/// be re-added as a closed path over the rotated corners. [`StrokeKind`] is
/// honoured by insetting/outsetting the outline by half the stroke width before
/// rotation (corner radius, like the fill's, is not preserved).
fn textured_rect_outline(
    rect: &RectShape,
    rotation: Rotation,
    logical_size: Vec2,
) -> Option<Shape> {
    if rect.stroke.is_empty() {
        return None;
    }

    let half = rect.stroke.width / 2.0;
    let r = match rect.stroke_kind {
        StrokeKind::Inside => rect.rect.shrink(half),
        StrokeKind::Middle => rect.rect,
        StrokeKind::Outside => rect.rect.expand(half),
    };

    let map = |p: Pos2| rotation.inverse_transform_pos(p, logical_size);
    let pts = vec![
        map(r.left_top()),
        map(r.right_top()),
        map(r.right_bottom()),
        map(r.left_bottom()),
    ];
    Some(Shape::Path(PathShape::closed_line(pts, rect.stroke)))
}

/// Build a rotated textured quad [`Mesh`] from a brushed [`RectShape`].
///
/// Replicates [`Mesh::add_rect_with_uv`]'s vertex order / winding, but with the
/// corner positions mapped through [`Rotation::inverse_transform_pos`]. Corner
/// radius is not preserved (images are virtually always un-rounded).
fn textured_rect_to_mesh(rect: &RectShape, rotation: Rotation, logical_size: Vec2) -> Mesh {
    let map = |p: Pos2| rotation.inverse_transform_pos(p, logical_size);
    let brush = rect
        .brush
        .as_ref()
        .expect("textured_rect_to_mesh: no brush");
    let uv = brush.uv;
    let r = rect.rect;
    let color = rect.fill;

    let mut mesh = Mesh::with_texture(brush.fill_texture_id);
    mesh.indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
    mesh.vertices.extend_from_slice(&[
        Vertex {
            pos: map(r.left_top()),
            uv: uv.left_top(),
            color,
        },
        Vertex {
            pos: map(r.right_top()),
            uv: uv.right_top(),
            color,
        },
        Vertex {
            pos: map(r.left_bottom()),
            uv: uv.left_bottom(),
            color,
        },
        Vertex {
            pos: map(r.right_bottom()),
            uv: uv.right_bottom(),
            color,
        },
    ]);
    mesh
}
