use egui::{epaint::MarginF32, Event, RawInput, SafeAreaInsets};

use crate::Rotation;

/// Map safe-area insets (physical sides — e.g. a phone notch) onto the logical
/// sides they cover after rotation, mirroring [`Rotation::transform_pos`]:
/// under CW90 the physical top strip lands on the logical *right* edge, etc.
pub(crate) fn rotate_safe_area_insets(
    insets: SafeAreaInsets,
    rotation: Rotation,
) -> SafeAreaInsets {
    let m = insets.0;
    SafeAreaInsets(match rotation {
        Rotation::None => m,
        Rotation::CW90 => MarginF32 {
            left: m.bottom,
            right: m.top,
            top: m.left,
            bottom: m.right,
        },
        Rotation::CW180 => MarginF32 {
            left: m.right,
            right: m.left,
            top: m.bottom,
            bottom: m.top,
        },
        Rotation::CW270 => MarginF32 {
            left: m.top,
            right: m.bottom,
            top: m.right,
            bottom: m.left,
        },
    })
}

/// Transform a [`RawInput`] from physical screen space to logical UI space.
///
/// Call this **before** passing `RawInput` to [`egui::Context::run`] / `begin_pass`.
///
/// What gets transformed:
/// - `screen_rect` is rotated (axes swapped for 90°/270°)
/// - `Event::PointerMoved`, `PointerButton`, `Touch` positions
/// - `Event::MouseWheel` and `MouseMoved` deltas (vectors only)
///
/// `physical_size` must match the original (pre-rotation) `screen_rect.size()` —
/// pass it explicitly because `screen_rect` is mutated in-place.
#[deprecated(
    since = "1.0.0",
    note = "register a `RotationPlugin` instead — it rotates input transparently on any backend. \
            Kept for fully custom pipelines; will be removed in a future release."
)]
pub fn transform_raw_input(raw: &mut RawInput, rotation: Rotation) {
    rotate_raw_input(raw, rotation);
}

/// Implementation shared by the deprecated [`transform_raw_input`] and the
/// `RotationPlugin` (which is not deprecated).
pub(crate) fn rotate_raw_input(raw: &mut RawInput, rotation: Rotation) {
    if rotation.is_none() {
        return;
    }

    let Some(physical_rect) = raw.screen_rect else {
        return;
    };
    let physical_size = physical_rect.size();

    raw.screen_rect = Some(rotation.transform_screen_rect(physical_rect));
    if let Some(insets) = raw.safe_area_insets {
        raw.safe_area_insets = Some(rotate_safe_area_insets(insets, rotation));
    }

    for event in &mut raw.events {
        match event {
            Event::PointerMoved(pos) => {
                *pos = rotation.transform_pos(*pos, physical_size);
            }
            Event::PointerButton { pos, .. } => {
                *pos = rotation.transform_pos(*pos, physical_size);
            }
            Event::Touch { pos, .. } => {
                *pos = rotation.transform_pos(*pos, physical_size);
            }
            Event::MouseWheel { delta, .. } => {
                *delta = rotation.transform_vec(*delta);
            }
            Event::MouseMoved(delta) => {
                *delta = rotation.transform_vec(*delta);
            }
            _ => {}
        }
    }
}
