//! Software cursor: a virtual cursor drawn in logical (rotated) space.
//!
//! Why this exists: when a viewport is rotated, the OS cursor still moves in
//! physical screen space — moving your hand "up" makes it move sideways in the
//! rotated UI, which is disorienting. The fix is:
//!
//! 1. Hide the OS cursor and warp it to the centre of the window.
//! 2. Read **raw mouse deltas** (`Event::MouseMoved`), rotate them to logical
//!    space, and update a virtual cursor position.
//! 3. Replace `PointerMoved` / `PointerButton` event positions with the virtual
//!    cursor position so egui interaction works as expected.
//! 4. Draw a small cursor shape at the virtual position each frame.
//!
//! The integration layer (your SDL3 / winit / etc. code) is responsible for
//! actually hiding the OS cursor, warping it, and re-showing it on release.
//! [`SoftwareCursor::process_input`] returns a [`SoftwareCursorOutput`] that
//! signals when a release/warp should happen.

use egui::{
    epaint::{Color32, PathShape, Stroke},
    vec2, CursorIcon, Event, Painter, Pos2, RawInput, Rect, Shape, Vec2,
};

use crate::Rotation;

/// State for the software cursor.
///
/// One instance per egui [`Context`](egui::Context) (or per window).
#[derive(Clone, Debug)]
pub struct SoftwareCursor {
    /// Current virtual cursor position, in **logical** (rotated) UI space.
    /// `None` when the cursor is not currently captured (OS cursor visible).
    virtual_pos: Option<Pos2>,

    /// `true` when the OS cursor is hidden and we are tracking via raw deltas.
    captured: bool,

    /// Visual scale applied to the drawn cursor (default `1.0`).
    /// Useful for far-viewing displays (e.g. pinball cabinets).
    scale: f32,

    /// If `true`, virtual cursor is clamped inside the window — no edge release.
    /// Use for kiosk / fullscreen scenarios.
    locked: bool,
}

impl Default for SoftwareCursor {
    fn default() -> Self {
        Self {
            virtual_pos: None,
            captured: false,
            scale: 1.0,
            locked: false,
        }
    }
}

/// Result of [`SoftwareCursor::process_input`] — signals integration actions to take.
#[derive(Clone, Debug, Default)]
pub struct SoftwareCursorOutput {
    /// If `Some`, the OS cursor should be **warped** to this **physical** position
    /// and made visible. Indicates that the user moved past a window edge and the
    /// cursor was released to the OS. The position is already 3px outside the
    /// window so the OS treats it as "the user left."
    pub release_os_cursor_to: Option<Pos2>,
}

impl SoftwareCursor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set visual scale of the drawn cursor (default `1.0`).
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// Lock the virtual cursor inside the window (no edge release).
    pub fn with_lock(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    pub fn set_lock(&mut self, locked: bool) {
        self.locked = locked;
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn is_captured(&self) -> bool {
        self.captured
    }

    /// Current virtual cursor position in logical UI space, if captured.
    pub fn virtual_pos(&self) -> Option<Pos2> {
        self.virtual_pos
    }

    /// Process raw input: update virtual cursor state, rewrite events.
    ///
    /// Call **after** [`crate::transform_raw_input`] (or in place of it — this
    /// function performs the same rotation but also adds capture/release logic).
    ///
    /// `physical_size` is the **pre-rotation** window size.
    ///
    /// Returns hints for the integration layer (cursor warps).
    pub fn process_input(
        &mut self,
        raw: &mut RawInput,
        rotation: Rotation,
        physical_size: Vec2,
    ) -> SoftwareCursorOutput {
        let mut out = SoftwareCursorOutput::default();

        if rotation.is_none() {
            return out;
        }

        let physical_rect = match raw.screen_rect {
            Some(r) => r,
            None => return out,
        };

        // Rotate screen_rect to logical space (idempotent if already rotated).
        let logical_rect = rotation.transform_screen_rect(physical_rect);
        raw.screen_rect = Some(logical_rect);
        let logical_size = logical_rect.size();
        let edge_margin = 1.0;

        // ── Pass 1 — update capture state from raw deltas / pointer events ──
        for event in &raw.events {
            match event {
                Event::MouseMoved(delta) => {
                    if !self.captured {
                        continue;
                    }
                    // Use the raw OS delta directly — the inverse rotation applied at
                    // draw time produces the rotated visual motion. Rotating here
                    // would cancel that out and make the cursor track the OS frame
                    // instead of the logical UI frame.
                    let virtual_pos = self.virtual_pos.unwrap_or_else(|| logical_rect.center());
                    let new_x = virtual_pos.x + delta.x;
                    let new_y = virtual_pos.y + delta.y;

                    let at_edge = new_x <= 0.0
                        || new_x >= logical_size.x
                        || new_y <= 0.0
                        || new_y >= logical_size.y;

                    if at_edge && !self.locked {
                        // Release: warp 3px outside the window in logical space,
                        // then convert to physical for the OS.
                        let overshoot = 3.0;
                        let edge_pos = Pos2::new(
                            if new_x <= 0.0 {
                                -overshoot
                            } else if new_x >= logical_size.x {
                                logical_size.x + overshoot
                            } else {
                                new_x
                            },
                            if new_y <= 0.0 {
                                -overshoot
                            } else if new_y >= logical_size.y {
                                logical_size.y + overshoot
                            } else {
                                new_y
                            },
                        );
                        out.release_os_cursor_to =
                            Some(rotation.inverse_transform_pos(edge_pos, logical_size));
                        self.captured = false;
                        self.virtual_pos = None;
                    } else {
                        let clamped = Pos2::new(
                            new_x.clamp(edge_margin, logical_size.x - edge_margin),
                            new_y.clamp(edge_margin, logical_size.y - edge_margin),
                        );
                        self.virtual_pos = Some(clamped);
                    }
                }
                Event::PointerMoved(pos) if !self.captured && self.virtual_pos.is_none() => {
                    let entry = rotation.transform_pos(*pos, physical_size);
                    self.captured = true;
                    self.virtual_pos = Some(entry);
                }
                Event::PointerGone => {
                    self.captured = false;
                    self.virtual_pos = None;
                }
                _ => {}
            }
        }

        // ── Pass 2 — rewrite event positions ──
        for event in &mut raw.events {
            match event {
                Event::PointerMoved(pos) => {
                    *pos = self
                        .virtual_pos
                        .unwrap_or_else(|| rotation.transform_pos(*pos, physical_size));
                }
                Event::PointerButton { pos, .. } => {
                    *pos = self
                        .virtual_pos
                        .unwrap_or_else(|| rotation.transform_pos(*pos, physical_size));
                }
                Event::Touch { pos, .. } => {
                    *pos = rotation.transform_pos(*pos, physical_size);
                }
                Event::MouseWheel { delta, .. } => {
                    *delta = rotation.transform_vec(*delta);
                }
                // MouseMoved deltas are already consumed by Pass 1 to update the
                // virtual cursor; we leave them untransformed for downstream
                // egui consumers (egui itself doesn't use them for hit-testing).
                _ => {}
            }
        }

        out
    }

    /// Draw the software cursor at its current virtual position.
    ///
    /// Call once per frame, **after** the UI is laid out so the cursor draws on top.
    /// Use a foreground layer / top-most painter (e.g. `ctx.layer_painter(LayerId::new(Order::Foreground, …))`).
    ///
    /// `cursor_icon` is the icon you want drawn. Typically read from
    /// [`egui::PlatformOutput::cursor_icon`] **before** rotation, then rotated
    /// via [`crate::CursorIconExt::rotate`].
    pub fn draw(&self, painter: &Painter, cursor_icon: CursorIcon) {
        let Some(pos) = self.virtual_pos else { return };
        paint_cursor_shape(painter, cursor_icon, pos, self.scale);
    }
}

fn paint_cursor_shape(painter: &Painter, cursor: CursorIcon, pos: Pos2, scale: f32) {
    let s = scale;

    let (shapes, clip_rect) = match cursor {
        CursorIcon::None => return,

        CursorIcon::Text => {
            let half_h = 8.0 * s;
            let sw = 3.0 * s;
            let shapes = vec![
                Shape::line_segment(
                    [pos + vec2(0.0, -half_h), pos + vec2(0.0, half_h)],
                    Stroke::new(2.0 * s, Color32::WHITE),
                ),
                Shape::line_segment(
                    [pos + vec2(-sw, -half_h), pos + vec2(sw, -half_h)],
                    Stroke::new(1.5 * s, Color32::WHITE),
                ),
                Shape::line_segment(
                    [pos + vec2(-sw, half_h), pos + vec2(sw, half_h)],
                    Stroke::new(1.5 * s, Color32::WHITE),
                ),
            ];
            (
                shapes,
                Rect::from_center_size(pos, vec2(20.0 * s, 24.0 * s)),
            )
        }

        CursorIcon::VerticalText => {
            let half_w = 8.0 * s;
            let sw = 3.0 * s;
            let shapes = vec![
                Shape::line_segment(
                    [pos + vec2(-half_w, 0.0), pos + vec2(half_w, 0.0)],
                    Stroke::new(2.0 * s, Color32::WHITE),
                ),
                Shape::line_segment(
                    [pos + vec2(-half_w, -sw), pos + vec2(-half_w, sw)],
                    Stroke::new(1.5 * s, Color32::WHITE),
                ),
                Shape::line_segment(
                    [pos + vec2(half_w, -sw), pos + vec2(half_w, sw)],
                    Stroke::new(1.5 * s, Color32::WHITE),
                ),
            ];
            (
                shapes,
                Rect::from_center_size(pos, vec2(24.0 * s, 20.0 * s)),
            )
        }

        CursorIcon::PointingHand | CursorIcon::Grab | CursorIcon::Grabbing => {
            let r = 6.0 * s;
            let shapes = vec![
                Shape::circle_filled(pos, r, Color32::WHITE),
                Shape::circle_stroke(pos, r, Stroke::new(1.5 * s, Color32::BLACK)),
            ];
            (
                shapes,
                Rect::from_center_size(pos, vec2(20.0 * s, 20.0 * s)),
            )
        }

        CursorIcon::Crosshair => {
            let h = 8.0 * s;
            let shapes = vec![
                Shape::line_segment(
                    [pos + vec2(-h, 0.0), pos + vec2(h, 0.0)],
                    Stroke::new(1.5 * s, Color32::WHITE),
                ),
                Shape::line_segment(
                    [pos + vec2(0.0, -h), pos + vec2(0.0, h)],
                    Stroke::new(1.5 * s, Color32::WHITE),
                ),
            ];
            (
                shapes,
                Rect::from_center_size(pos, vec2(24.0 * s, 24.0 * s)),
            )
        }

        CursorIcon::NotAllowed | CursorIcon::NoDrop => {
            let r = 8.0 * s;
            let d = 5.0 * s;
            let shapes = vec![
                Shape::circle_stroke(pos, r, Stroke::new(2.0 * s, Color32::RED)),
                Shape::line_segment(
                    [pos + vec2(-d, -d), pos + vec2(d, d)],
                    Stroke::new(2.0 * s, Color32::RED),
                ),
            ];
            (
                shapes,
                Rect::from_center_size(pos, vec2(24.0 * s, 24.0 * s)),
            )
        }

        _ => {
            let tip = pos;
            let left = pos + vec2(0.0, 16.0 * s);
            let right = pos + vec2(11.0 * s, 11.0 * s);

            let arrow = PathShape::convex_polygon(
                vec![tip, left, right],
                Color32::WHITE,
                Stroke::new(1.5 * s, Color32::BLACK),
            );
            (
                vec![Shape::Path(arrow)],
                Rect::from_min_size(pos, vec2(16.0 * s, 20.0 * s)),
            )
        }
    };

    for shape in shapes {
        painter.with_clip_rect(clip_rect).add(shape);
    }
}
