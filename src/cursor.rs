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
    epaint::{Color32, Mesh, PathShape, Stroke},
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

    /// Force-capture the cursor at a specific logical position.
    ///
    /// Useful at kiosk-mode entry when the OS cursor is grabbed (e.g. via
    /// `egui::ViewportCommand::CursorGrab(CursorGrab::Locked)` on Wayland).
    /// Under that grab the OS cursor is frozen — `Event::PointerMoved` is
    /// no longer fired, only relative-motion `Event::MouseMoved` events
    /// flow. Without a `PointerMoved` to seed the capture state, the cursor
    /// would never start tracking. Call this once at activation with the
    /// window centre to bootstrap.
    pub fn set_virtual_pos(&mut self, pos: Pos2) {
        self.captured = true;
        self.virtual_pos = Some(pos);
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
    /// `cursor_icon` is the icon as set by egui (read from
    /// [`egui::PlatformOutput::cursor_icon`]). **Pass it un-rotated** — the
    /// shape is drawn in *logical* (rotated) UI space, and the inverse rotation
    /// applied at paint time by [`crate::transform_clipped_primitives`] produces
    /// the correct visual orientation on screen automatically. Pre-rotating the
    /// icon via [`crate::CursorIconExt::rotate`] would double the rotation and
    /// flip the shape the wrong way (e.g. text I-beam parallel to text instead
    /// of perpendicular).
    ///
    /// [`crate::CursorIconExt::rotate`] is for the *other* scenario — when the
    /// OS cursor is visible (no software cursor) and you want to remap the icon
    /// (set via [`egui::Context::set_cursor_icon`]) so that directional cursors
    /// like resize arrows visually match the user's perception of the rotated
    /// screen.
    pub fn draw(&self, painter: &Painter, cursor_icon: CursorIcon) {
        let Some(pos) = self.virtual_pos else { return };
        paint_cursor_shape(painter, cursor_icon, pos, self.scale);
    }
}

fn paint_cursor_shape(painter: &Painter, cursor: CursorIcon, pos: Pos2, scale: f32) {
    let s = scale;

    // Cursor ink adapts to the egui theme: white on a dark theme, black on a
    // light one, so the cursor always contrasts the background it sits on.
    let ink = if painter.ctx().theme() == egui::Theme::Dark {
        Color32::WHITE
    } else {
        Color32::BLACK
    };

    let (shapes, clip_rect) = match cursor {
        CursorIcon::None => return,

        CursorIcon::Text => {
            let half_h = 8.0 * s;
            let sw = 3.0 * s;
            let shapes = vec![
                Shape::line_segment(
                    [pos + vec2(0.0, -half_h), pos + vec2(0.0, half_h)],
                    Stroke::new(2.0 * s, ink),
                ),
                Shape::line_segment(
                    [pos + vec2(-sw, -half_h), pos + vec2(sw, -half_h)],
                    Stroke::new(1.5 * s, ink),
                ),
                Shape::line_segment(
                    [pos + vec2(-sw, half_h), pos + vec2(sw, half_h)],
                    Stroke::new(1.5 * s, ink),
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
                    Stroke::new(2.0 * s, ink),
                ),
                Shape::line_segment(
                    [pos + vec2(-half_w, -sw), pos + vec2(-half_w, sw)],
                    Stroke::new(1.5 * s, ink),
                ),
                Shape::line_segment(
                    [pos + vec2(half_w, -sw), pos + vec2(half_w, sw)],
                    Stroke::new(1.5 * s, ink),
                ),
            ];
            (
                shapes,
                Rect::from_center_size(pos, vec2(24.0 * s, 20.0 * s)),
            )
        }

        CursorIcon::PointingHand | CursorIcon::Grab | CursorIcon::Grabbing => {
            // Navigation arrow pointing right. Baked from
            // `assets/cursor-svgrepo-com.svg` (curves flattened and the concave
            // outline ear-clipped offline — see the const tables below). The tip
            // sits exactly at `pos` (hot-spot), matching the link-pointer convention.
            baked_cursor_colored(pos, s, NAV_CURSOR_PTS, NAV_CURSOR_TRIS, ink, ink)
        }

        CursorIcon::Crosshair => {
            let h = 8.0 * s;
            let shapes = vec![
                Shape::line_segment(
                    [pos + vec2(-h, 0.0), pos + vec2(h, 0.0)],
                    Stroke::new(1.5 * s, ink),
                ),
                Shape::line_segment(
                    [pos + vec2(0.0, -h), pos + vec2(0.0, h)],
                    Stroke::new(1.5 * s, ink),
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
            // Default arrow pointing up-left, baked from
            // `assets/mouse-cursor-svgrepo-com.svg`. The tip is at `pos` (hot-spot).
            baked_cursor_colored(pos, s, ARROW_PTS, ARROW_TRIS, ink, ink)
        }
    };

    for shape in shapes {
        painter.with_clip_rect(clip_rect).add(shape);
    }
}

/// Build a filled-and-outlined cursor from a baked outline polygon (`verts`,
/// in cursor-local pixels with the hot-spot at the origin) and its triangle
/// index list (`tris`, ear-clipped offline so concave outlines fill cleanly —
/// epaint's path fill assumes convexity). The fill is a single shared-vertex
/// [`Mesh`] (no anti-aliasing seams between triangles); the outline is a closed
/// stroke over the same points. Returns the shapes plus a tight clip rect.
fn baked_cursor_colored(
    pos: Pos2,
    s: f32,
    verts: &[[f32; 2]],
    tris: &[u32],
    fill: Color32,
    outline: Color32,
) -> (Vec<Shape>, Rect) {
    let pts: Vec<Pos2> = verts
        .iter()
        .map(|p| pos + vec2(p[0] * s, p[1] * s))
        .collect();

    let mut mesh = Mesh::default();
    for p in &pts {
        mesh.colored_vertex(*p, fill);
    }
    for t in tris.chunks_exact(3) {
        mesh.add_triangle(t[0], t[1], t[2]);
    }

    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in &pts {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    let clip = Rect::from_min_max(Pos2::new(min_x, min_y), Pos2::new(max_x, max_y)).expand(2.0 * s);

    let shapes = vec![
        Shape::mesh(mesh),
        Shape::Path(PathShape::closed_line(pts, Stroke::new(1.5 * s, outline))),
    ];
    (shapes, clip)
}

// ── Baked cursor geometry ───────────────────────────────────────────────────
// Outline points (local pixels, hot-spot at origin) and ear-clipped triangle
// indices, generated offline from the SVGs in `assets/`. Curves are flattened
// to line segments; the triangle list lets the concave fill render without the
// convexity artefacts of epaint's closed-path fill.

/// Default arrow, from `assets/mouse-cursor-svgrepo-com.svg` (tip at origin,
/// pointing up-left).
const ARROW_PTS: &[[f32; 2]] = &[
    [0.000, 0.000],
    [17.000, 5.000],
    [12.000, 9.000],
    [18.000, 15.000],
    [15.000, 18.000],
    [9.000, 12.000],
    [5.000, 17.000],
];
const ARROW_TRIS: &[u32] = &[0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5, 0, 5, 6];

/// Navigation arrow, from `assets/cursor-svgrepo-com.svg` (tip at origin,
/// pointing right). Used for `PointingHand` / `Grab` / `Grabbing`.
const NAV_CURSOR_PTS: &[[f32; 2]] = &[
    [-0.050, 0.464],
    [-0.195, 0.891],
    [-0.524, 1.385],
    [-0.864, 1.684],
    [-1.275, 1.910],
    [-21.857, 10.479],
    [-22.308, 10.564],
    [-22.770, 10.546],
    [-23.266, 10.402],
    [-23.716, 10.133],
    [-24.061, 9.793],
    [-24.314, 9.394],
    [-24.467, 8.958],
    [-24.518, 8.501],
    [-24.465, 8.040],
    [-24.305, 7.590],
    [-21.055, 0.965],
    [-20.703, 0.654],
    [-20.235, 0.685],
    [-19.924, 1.037],
    [-19.955, 1.505],
    [-23.244, 8.219],
    [-23.278, 8.683],
    [-23.044, 9.096],
    [-22.621, 9.330],
    [-22.130, 9.275],
    [-1.660, 0.735],
    [-1.336, 0.419],
    [-1.231, -0.028],
    [-1.371, -0.476],
    [-1.745, -0.775],
    [-22.222, -9.308],
    [-22.687, -9.313],
    [-23.084, -9.053],
    [-23.290, -8.616],
    [-23.205, -8.130],
    [-20.110, -1.820],
    [-8.920, -0.900],
    [-8.503, -0.686],
    [-8.360, -0.240],
    [-8.572, 0.177],
    [-9.020, 0.320],
    [-20.710, -0.657],
    [-21.055, -0.965],
    [-24.423, -7.888],
    [-24.512, -8.348],
    [-24.496, -8.809],
    [-24.376, -9.254],
    [-24.156, -9.666],
    [-23.840, -10.028],
    [-23.457, -10.304],
    [-23.031, -10.484],
    [-22.577, -10.563],
    [-22.113, -10.538],
    [-21.655, -10.405],
    [-1.270, -1.905],
    [-0.861, -1.681],
    [-0.523, -1.384],
    [-0.195, -0.891],
    [-0.050, -0.464],
    [0.000, 0.000],
];
const NAV_CURSOR_TRIS: &[u32] = &[
    60, 0, 1, 60, 1, 2, 60, 2, 3, 60, 3, 4, 60, 4, 5, 60, 5, 6, 60, 6, 7, 60, 7, 8, 8, 9, 10, 8,
    10, 11, 8, 11, 12, 8, 12, 13, 8, 13, 14, 8, 14, 15, 15, 16, 17, 15, 17, 18, 15, 18, 19, 15, 19,
    20, 15, 20, 21, 15, 21, 22, 8, 15, 22, 8, 22, 23, 8, 23, 24, 8, 24, 25, 8, 25, 26, 60, 8, 26,
    60, 26, 27, 60, 27, 28, 60, 28, 29, 60, 29, 30, 36, 37, 38, 36, 38, 39, 36, 39, 40, 36, 40, 41,
    36, 41, 42, 35, 36, 42, 35, 42, 43, 35, 43, 44, 34, 35, 44, 33, 34, 44, 33, 44, 45, 32, 33, 45,
    32, 45, 46, 32, 46, 47, 32, 47, 48, 31, 32, 48, 31, 48, 49, 30, 31, 49, 30, 49, 50, 30, 50, 51,
    60, 30, 51, 60, 51, 52, 60, 52, 53, 60, 53, 54, 60, 54, 55, 60, 55, 56, 60, 56, 57, 60, 57, 58,
    58, 59, 60,
];
