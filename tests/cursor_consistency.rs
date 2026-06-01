//! Regression test pinning `CursorIconExt::rotate` to the SAME orientation the
//! (visually validated) software cursor gets, derived independently here.
//!
//! Reasoning:
//! - The software cursor is drawn in LOGICAL space, then every vertex is mapped
//!   to physical space by `inverse_transform_pos` (via `transform_clipped_primitives`).
//!   That drawn cursor is the ground truth for "correct on-screen orientation".
//! - The OS cursor (`set_cursor_icon`) is placed by the OS directly in PHYSICAL
//!   space with NO transform. So to look like the software cursor, `rotate()`
//!   must map the icon's pointing direction by the directional part of
//!   `inverse_transform_pos`.
//!
//! We compute that directional part FROM the real function (difference of two
//! transformed points — the size terms cancel), then compare.

use egui::{CursorIcon, Pos2, Vec2};
use egui_rotate::{CursorIconExt, Rotation};

/// Pointing direction of a directional resize cursor, in egui screen coords
/// (x = east+, y = south+).
fn dir_of(icon: CursorIcon) -> Option<(i32, i32)> {
    Some(match icon {
        CursorIcon::ResizeEast => (1, 0),
        CursorIcon::ResizeWest => (-1, 0),
        CursorIcon::ResizeNorth => (0, -1),
        CursorIcon::ResizeSouth => (0, 1),
        CursorIcon::ResizeNorthEast => (1, -1),
        CursorIcon::ResizeNorthWest => (-1, -1),
        CursorIcon::ResizeSouthEast => (1, 1),
        CursorIcon::ResizeSouthWest => (-1, 1),
        _ => return None,
    })
}

fn icon_of(dir: (i32, i32)) -> CursorIcon {
    match dir {
        (1, 0) => CursorIcon::ResizeEast,
        (-1, 0) => CursorIcon::ResizeWest,
        (0, -1) => CursorIcon::ResizeNorth,
        (0, 1) => CursorIcon::ResizeSouth,
        (1, -1) => CursorIcon::ResizeNorthEast,
        (-1, -1) => CursorIcon::ResizeNorthWest,
        (1, 1) => CursorIcon::ResizeSouthEast,
        (-1, 1) => CursorIcon::ResizeSouthWest,
        other => panic!("unmapped dir {other:?}"),
    }
}

/// Directional part of `inverse_transform_pos`, taken from the REAL function:
/// transform two points and subtract — the translation/size terms cancel.
fn inverse_dir(rot: Rotation, dir: (i32, i32)) -> (i32, i32) {
    let size = Vec2::new(100.0, 100.0); // arbitrary; cancels out
    let o = rot.inverse_transform_pos(Pos2::new(0.0, 0.0), size);
    let p = rot.inverse_transform_pos(Pos2::new(dir.0 as f32, dir.1 as f32), size);
    let d = p - o;
    (d.x.round() as i32, d.y.round() as i32)
}

#[test]
fn directional_resize_matches_software_cursor_orientation() {
    let icons = [
        CursorIcon::ResizeEast,
        CursorIcon::ResizeWest,
        CursorIcon::ResizeNorth,
        CursorIcon::ResizeSouth,
        CursorIcon::ResizeNorthEast,
        CursorIcon::ResizeNorthWest,
        CursorIcon::ResizeSouthEast,
        CursorIcon::ResizeSouthWest,
    ];
    let rots = [Rotation::CW90, Rotation::CW180, Rotation::CW270];

    let mut mismatches = Vec::new();
    for &icon in &icons {
        for &rot in &rots {
            let want = icon_of(inverse_dir(rot, dir_of(icon).unwrap()));
            let got = icon.rotate(rot);
            if got != want {
                mismatches.push(format!(
                    "{icon:?}.rotate({rot:?}) = {got:?}, expected {want:?}"
                ));
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "{} mismatch(es):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
