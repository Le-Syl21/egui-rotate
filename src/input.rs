use egui::{Event, RawInput};

use crate::Rotation;

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
