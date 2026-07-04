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
//! Through the [`crate::RotationPlugin`] all of this is automatic: the plugin
//! hides the OS cursor, holds an OS pointer grab while captured
//! ([`SoftwareCursor::with_os_grab`]) and can pin the real cursor to the window
//! centre ([`SoftwareCursor::with_os_cursor_pin`]). Only two things may involve
//! the integration layer:
//!
//! - warping the OS cursor to the exit point on an edge release
//!   ([`crate::RotationPlugin::take_pending_warp`], unsupported on Wayland);
//! - putting the cursor to sleep on **gamepad/joystick** input
//!   ([`SoftwareCursor::set_dormant`]) — egui never sees those events, so the
//!   front-end must signal them (keyboard triggering is built in, opt-in via
//!   [`SoftwareCursor::with_dormant_on_keys`]).
//!
//! Fully custom pipelines (without the plugin) drive everything themselves:
//! [`SoftwareCursor::process_input`] returns a [`SoftwareCursorOutput`] that
//! signals when a release/warp or pin should happen.

use egui::{
    epaint::{Color32, Mesh, PathShape, Stroke},
    vec2,
    viewport::CursorGrab,
    CursorIcon, Event, Painter, Pos2, RawInput, Rect, Shape, Vec2,
};

use crate::Rotation;

/// Pause (seconds) after which soft-lock edge pressure resets.
/// See [`SoftwareCursor::with_edge_resistance`].
pub const EDGE_PRESSURE_RESET_SECS: f64 = 0.25;

/// Default soft-lock edge resistance (px), used by [`SoftwareCursor::new`].
/// See [`SoftwareCursor::with_edge_resistance`].
pub const DEFAULT_EDGE_RESISTANCE: f32 = 150.0;

/// Default mouse motion (px, within one burst) that wakes a dormant cursor.
/// See [`SoftwareCursor::with_wake_threshold`].
pub const DEFAULT_WAKE_THRESHOLD: f32 = 6.0;

/// Pause (seconds) after which accumulated wake motion resets, so slow ambient
/// jitter (e.g. cabinet nudging) never wakes a dormant cursor.
const WAKE_RESET_SECS: f64 = 0.25;

/// Default fade duration for dormancy transitions.
/// See [`SoftwareCursor::with_fade`].
pub const DEFAULT_FADE: std::time::Duration = std::time::Duration::from_millis(500);

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

    /// If `true`, the **real** OS cursor is re-warped to the window centre
    /// whenever it strays ("pseudo-lock"). See [`Self::with_os_cursor_pin`].
    pin_os_cursor: bool,

    /// OS pointer grab the plugin applies while captured (`None` = the
    /// integration manages grabbing itself). See [`Self::with_os_grab`].
    os_grab: Option<CursorGrab>,

    /// Soft-lock: accumulated outward push (px) needed to break through a window
    /// edge. `0.0` releases immediately; defaults to [`DEFAULT_EDGE_RESISTANCE`].
    /// See [`Self::with_edge_resistance`].
    edge_resistance: f32,
    /// Pressure accumulated against the edge so far (soft-lock state).
    edge_pressure: f32,
    /// `RawInput::time` of the last edge push, for the pressure-reset pause.
    last_edge_push: Option<f64>,

    /// Dormant (auto-hide): cursor hidden and egui hover cleared, e.g. while
    /// navigating by keyboard/gamepad. See [`Self::set_dormant`].
    dormant: bool,
    /// Go dormant automatically on keyboard/text input (default `false`).
    dormant_on_keys: bool,
    /// Mouse motion (px, within one burst) that wakes a dormant cursor.
    wake_threshold: f32,
    /// Motion accumulated towards waking (dormant state).
    wake_accum: f32,
    /// `RawInput::time` of the last wake motion, for the burst-reset pause.
    last_wake_motion: Option<f64>,
    /// One-shot: inject `PointerGone` on the next pass (entered dormancy).
    pending_hover_clear: bool,
    /// One-shot: re-assert hover on the next pass (woke via [`Self::set_dormant`]).
    pending_hover_wake: bool,

    /// Fade duration (seconds) for dormancy transitions. See [`Self::with_fade`].
    fade_secs: f32,
    /// Current draw opacity, animated towards 0 (dormant) or 1 (awake).
    opacity: f32,
    /// `RawInput::time` of the last fade step, for the animation delta.
    last_fade_time: Option<f64>,
}

impl Default for SoftwareCursor {
    fn default() -> Self {
        Self {
            virtual_pos: None,
            captured: false,
            scale: 1.0,
            locked: false,
            pin_os_cursor: false,
            os_grab: Some(CursorGrab::Confined),
            edge_resistance: DEFAULT_EDGE_RESISTANCE,
            edge_pressure: 0.0,
            last_edge_push: None,
            dormant: false,
            dormant_on_keys: false,
            wake_threshold: DEFAULT_WAKE_THRESHOLD,
            wake_accum: 0.0,
            last_wake_motion: None,
            pending_hover_clear: false,
            pending_hover_wake: false,
            fade_secs: DEFAULT_FADE.as_secs_f32(),
            opacity: 1.0,
            last_fade_time: None,
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

    /// If `Some`, the OS cursor should be **warped** to this **physical** position
    /// (the window centre) while staying hidden — the pseudo-lock re-centring of
    /// [`SoftwareCursor::with_os_cursor_pin`]. The [`crate::RotationPlugin`]
    /// handles this itself via [`egui::ViewportCommand::CursorPosition`]; only
    /// fully custom pipelines need to act on it.
    pub pin_os_cursor_to: Option<Pos2>,
}

impl SoftwareCursor {
    /// A software cursor with the default behavior: **soft lock** at
    /// [`DEFAULT_EDGE_RESISTANCE`] px (see [`Self::with_edge_resistance`]),
    /// an automatic `Confined` OS grab while captured (see
    /// [`Self::with_os_grab`]), no hard lock, no OS-cursor pin, no auto-hide
    /// on keyboard (see [`Self::with_dormant_on_keys`]), scale `1.0`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set visual scale of the drawn cursor (default `1.0`).
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// Lock the virtual cursor inside the window (no edge release).
    ///
    /// Note this clamps the *virtual* cursor only — the real OS cursor still
    /// travels the window physically and can even leave it (events then stop,
    /// freezing the virtual cursor). Combine with [`Self::with_os_cursor_pin`]
    /// to keep the real cursor pinned too.
    pub fn with_lock(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }

    /// Pin the **real** OS cursor near the window centre ("pseudo-lock").
    ///
    /// While the software cursor is captured, the hidden OS cursor keeps moving
    /// physically and can wander to the window edge or out of the window
    /// entirely — at which point events stop and the virtual cursor freezes.
    /// With pinning enabled, whenever the real cursor strays past a dead zone
    /// (a quarter of the smaller window dimension) it is warped back to the
    /// window centre, so it can never escape. Raw-delta tracking is unaffected.
    ///
    /// The [`crate::RotationPlugin`] applies the warp automatically through
    /// [`egui::ViewportCommand::CursorPosition`], which any egui-winit / eframe
    /// backend honours. **Wayland** does not support cursor warping — there the
    /// plugin's automatic OS grab ([`Self::with_os_grab`]) confines the cursor
    /// instead.
    pub fn with_os_cursor_pin(mut self, pin: bool) -> Self {
        self.pin_os_cursor = pin;
        self
    }

    /// Which OS pointer grab the plugin applies while the cursor is captured.
    ///
    /// With `Some(mode)` — the default is `Some(CursorGrab::Confined)` — the
    /// [`crate::RotationPlugin`] sends [`egui::ViewportCommand::CursorGrab`]
    /// automatically: `mode` when the software cursor captures, and
    /// `CursorGrab::None` when it releases (edge breakout, [`Self::release`],
    /// or rotation switched off). The grab keeps the *real* OS cursor from
    /// leaving the window at the OS level — the complement of
    /// [`Self::with_os_cursor_pin`], and the mechanism that works on Wayland
    /// (where cursor warping is unsupported).
    ///
    /// Platform support (winit): `Confined` works on Windows, X11 and Wayland;
    /// `Locked` on macOS and Wayland. An unsupported mode logs a warning and
    /// grabs nothing — on macOS use `Locked` (with
    /// [`Self::set_virtual_pos`] to seed tracking) or rely on the pin.
    ///
    /// Pass `None` if your integration manages the pointer grab itself.
    pub fn with_os_grab(mut self, grab: Option<CursorGrab>) -> Self {
        self.os_grab = grab;
        self
    }

    /// Go dormant automatically on keyboard/text input (**off** by default).
    /// See [`Self::set_dormant`] for what dormancy does.
    pub fn with_dormant_on_keys(mut self, on: bool) -> Self {
        self.dormant_on_keys = on;
        self
    }

    /// Mouse motion (px, accumulated within one burst) needed to wake a dormant
    /// cursor. Default [`DEFAULT_WAKE_THRESHOLD`]. Pausing resets the
    /// accumulation, so ambient jitter — e.g. a pinball cabinet shaking under
    /// nudges — never wakes the cursor; raise this on vibration-prone setups.
    pub fn with_wake_threshold(mut self, px: f32) -> Self {
        self.wake_threshold = px.max(0.0);
        self
    }

    /// Fade the cursor out/in over this duration on dormancy transitions,
    /// instead of popping (default [`DEFAULT_FADE`], 500 ms).
    /// [`Duration::ZERO`](std::time::Duration::ZERO) disables the animation.
    pub fn with_fade(mut self, fade: std::time::Duration) -> Self {
        self.fade_secs = fade.as_secs_f32();
        self
    }

    /// Soft-lock: require `resistance` pixels of accumulated outward push before
    /// an edge releases the cursor to the OS. **This is the default behavior**,
    /// at [`DEFAULT_EDGE_RESISTANCE`] px.
    ///
    /// A middle ground between no lock (`0.0`: any edge contact releases) and
    /// [`Self::with_lock`] (hard lock: never releases): the cursor stays confined
    /// against casual contact, but a deliberate push — a fast flick, or holding
    /// against the edge — breaks through once the summed overshoot reaches
    /// `resistance`. Pausing for [`EDGE_PRESSURE_RESET_SECS`] resets the
    /// accumulated pressure, so resting near the edge never slowly builds up a
    /// breakout.
    ///
    /// This is the "pressure barrier" model used by GNOME Shell's hot corners
    /// and X11 pointer barriers. Typical values: 100–300 px. Ignored while
    /// [`Self::with_lock`] is on.
    pub fn with_edge_resistance(mut self, resistance: f32) -> Self {
        self.edge_resistance = resistance.max(0.0);
        self
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    pub fn set_lock(&mut self, locked: bool) {
        self.locked = locked;
    }

    /// See [`Self::with_os_cursor_pin`].
    pub fn set_os_cursor_pin(&mut self, pin: bool) {
        self.pin_os_cursor = pin;
    }

    /// See [`Self::with_edge_resistance`].
    pub fn set_edge_resistance(&mut self, resistance: f32) {
        self.edge_resistance = resistance.max(0.0);
    }

    /// See [`Self::with_os_grab`].
    pub fn set_os_grab(&mut self, grab: Option<CursorGrab>) {
        self.os_grab = grab;
    }

    pub fn os_grab(&self) -> Option<CursorGrab> {
        self.os_grab
    }

    /// Put the cursor to sleep ("dormant") or wake it.
    ///
    /// While dormant the virtual cursor is hidden and frozen, and egui receives
    /// a [`Event::PointerGone`] so hover and selection follow the keyboard
    /// instead of the stale pointer position — the mouse no longer overrides
    /// keyboard/gamepad navigation. Any deliberate mouse use (a click, wheel,
    /// touch, or [`Self::with_wake_threshold`] px of motion) wakes it, hover
    /// re-asserted at the remembered position.
    ///
    /// Keyboard/text input can trigger dormancy automatically (opt-in, see
    /// [`Self::with_dormant_on_keys`]). Call this yourself for input egui
    /// cannot see — e.g. gamepad/joystick navigation in a pinball front-end:
    /// `plugin.software_cursor_mut().unwrap().set_dormant(true)` on stick input.
    /// Transitions fade over [`Self::with_fade`].
    pub fn set_dormant(&mut self, dormant: bool) {
        if dormant == self.dormant {
            return;
        }
        self.dormant = dormant;
        self.wake_accum = 0.0;
        self.last_wake_motion = None;
        if dormant {
            self.pending_hover_clear = true;
        } else {
            self.pending_hover_wake = true;
        }
    }

    pub fn is_dormant(&self) -> bool {
        self.dormant
    }

    /// See [`Self::with_dormant_on_keys`].
    pub fn set_dormant_on_keys(&mut self, on: bool) {
        self.dormant_on_keys = on;
    }

    /// See [`Self::with_wake_threshold`].
    pub fn set_wake_threshold(&mut self, px: f32) {
        self.wake_threshold = px.max(0.0);
    }

    /// See [`Self::with_fade`].
    pub fn set_fade(&mut self, fade: std::time::Duration) {
        self.fade_secs = fade.as_secs_f32();
    }

    /// Current draw opacity: `1.0` awake, `0.0` fully dormant, in between
    /// while fading.
    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    /// `true` while a dormancy fade is in progress. Integrations driving their
    /// own repaint loop should keep repainting while this returns `true` (the
    /// [`crate::RotationPlugin`] requests repaints itself).
    pub fn is_fading(&self) -> bool {
        if self.dormant {
            self.opacity > 0.0
        } else {
            self.opacity < 1.0
        }
    }

    /// Advance the fade animation towards the current dormancy target.
    /// Snaps when the animation is disabled or no clock is available.
    fn step_fade(&mut self, now: Option<f64>) {
        let target = if self.dormant { 0.0 } else { 1.0 };
        match now {
            Some(now) if self.fade_secs > 0.0 => {
                let dt = self
                    .last_fade_time
                    .map_or(0.0, |last| (now - last).max(0.0) as f32);
                self.last_fade_time = Some(now);
                let step = dt / self.fade_secs;
                self.opacity = if target > self.opacity {
                    (self.opacity + step).min(target)
                } else {
                    (self.opacity - step).max(target)
                };
            }
            _ => self.opacity = target,
        }
    }

    pub fn edge_resistance(&self) -> f32 {
        self.edge_resistance
    }

    pub fn os_cursor_pin(&self) -> bool {
        self.pin_os_cursor
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

    /// Release the cursor: stop capturing and forget the virtual position.
    ///
    /// The counterpart of [`Self::set_virtual_pos`]. The plugin calls this when
    /// its viewport stops rotating, so a captured cursor is not frozen with
    /// stale state; integrations that release the OS grab themselves can call
    /// it too.
    pub fn release(&mut self) {
        self.captured = false;
        self.virtual_pos = None;
        self.edge_pressure = 0.0;
        self.last_edge_push = None;
        self.dormant = false;
        self.wake_accum = 0.0;
        self.last_wake_motion = None;
        self.pending_hover_clear = false;
        self.pending_hover_wake = false;
        self.opacity = 1.0;
        self.last_fade_time = None;
    }

    /// Soft-lock accounting for one edge push: accumulate its overshoot (how far
    /// past the window bounds it tried to go) and decide whether the total breaks
    /// through. Zero resistance breaks immediately — the default hard release.
    fn edge_push_breaks_out(
        &mut self,
        new_x: f32,
        new_y: f32,
        logical_size: Vec2,
        now: Option<f64>,
    ) -> bool {
        if self.edge_resistance <= 0.0 {
            return true;
        }

        // A pause between pushes drops the pressure, so resting against the
        // edge can't slowly build up an accidental breakout.
        if let (Some(now), Some(last)) = (now, self.last_edge_push) {
            if now - last > EDGE_PRESSURE_RESET_SECS {
                self.edge_pressure = 0.0;
            }
        }
        self.last_edge_push = now;

        let over_x = (-new_x).max(new_x - logical_size.x).max(0.0);
        let over_y = (-new_y).max(new_y - logical_size.y).max(0.0);
        self.edge_pressure += Vec2::new(over_x, over_y).length();

        if self.edge_pressure >= self.edge_resistance {
            self.edge_pressure = 0.0;
            self.last_edge_push = None;
            true
        } else {
            false
        }
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
        // A notch on a physical side covers a different logical side once
        // rotated (idempotent too: `None` insets stay `None`).
        if let Some(insets) = raw.safe_area_insets {
            raw.safe_area_insets = Some(crate::input::rotate_safe_area_insets(insets, rotation));
        }
        let logical_size = logical_rect.size();
        let edge_margin = 1.0;
        let now = raw.time;

        // ── Dormancy (auto-hide) — see `set_dormant` ──
        let mut woke = false;
        if self.captured {
            woke = self.update_dormancy(&raw.events, now)
                || std::mem::take(&mut self.pending_hover_wake);
        }
        self.step_fade(now);
        if self.captured && self.dormant {
            // Asleep: freeze the virtual cursor and swallow pointer motion
            // so egui regains no hover; keyboard/text pass through, and the
            // transition frame ends with a `PointerGone` clearing the hover.
            raw.events.retain(|e| {
                !matches!(
                    e,
                    Event::PointerMoved(_) | Event::MouseMoved(_) | Event::MouseWheel { .. }
                )
            });
            if std::mem::take(&mut self.pending_hover_clear) {
                raw.events.push(Event::PointerGone);
            }
            return out;
        }

        // ── Pass 1 — update capture state from raw deltas / pointer events ──
        // Also remember where the real OS cursor is (physical space, before the
        // rewrite in Pass 2) for the pseudo-lock stray check below.
        let mut last_physical_pointer = None;
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

                    if at_edge
                        && !self.locked
                        && self.edge_push_breaks_out(new_x, new_y, logical_size, now)
                    {
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
                        if !at_edge {
                            // Moved freely inside: any soft-lock pressure is stale.
                            self.edge_pressure = 0.0;
                        }
                        let clamped = Pos2::new(
                            new_x.clamp(edge_margin, logical_size.x - edge_margin),
                            new_y.clamp(edge_margin, logical_size.y - edge_margin),
                        );
                        self.virtual_pos = Some(clamped);
                    }
                }
                Event::PointerMoved(pos) => {
                    last_physical_pointer = Some(*pos);
                    if !self.captured && self.virtual_pos.is_none() {
                        let entry = rotation.transform_pos(*pos, physical_size);
                        self.captured = true;
                        self.virtual_pos = Some(entry);
                        // Capture by mouse re-entry is mouse use: never dormant,
                        // and instantly visible (no fade-in on entry).
                        self.dormant = false;
                        self.opacity = 1.0;
                    }
                }
                Event::PointerGone => {
                    self.release();
                }
                _ => {}
            }
        }

        // ── Pseudo-lock — keep the real OS cursor pinned near the centre ──
        // Re-warp only when it strays past a dead zone: each warp requests a
        // repaint, so warping every frame would spin the event loop for nothing.
        if self.pin_os_cursor && self.captured {
            if let Some(pos) = last_physical_pointer {
                let center = (physical_size / 2.0).to_pos2();
                if (pos - center).length() > 0.25 * physical_size.min_elem() {
                    out.pin_os_cursor_to = Some(center);
                }
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

        // Waking re-asserts the hover at the remembered position, so the UI
        // under the reappearing cursor highlights again immediately. Pushed
        // after the passes so our own Pass 1 never sees this logical-space
        // event (it would pollute the pseudo-lock stray check).
        if woke {
            if let Some(pos) = self.virtual_pos {
                raw.events.push(Event::PointerMoved(pos));
            }
        }

        out
    }

    /// Dormancy state machine (see [`Self::set_dormant`]): keyboard/text use
    /// puts the captured cursor to sleep; deliberate mouse use — click, wheel,
    /// touch, or a burst of motion past the wake threshold — wakes it. Returns
    /// `true` when the cursor woke during this call.
    fn update_dormancy(&mut self, events: &[Event], now: Option<f64>) -> bool {
        let mut motion = 0.0f32;
        let mut deliberate = false;
        let mut keyed = false;
        for event in events {
            match event {
                Event::MouseMoved(delta) => motion += delta.length(),
                Event::PointerButton { .. } | Event::MouseWheel { .. } | Event::Touch { .. } => {
                    deliberate = true;
                }
                Event::Key { .. } | Event::Text(_) => keyed = true,
                _ => {}
            }
        }

        if self.dormant {
            // The wake motion must come as one deliberate burst: a pause resets
            // the accumulator, so ambient jitter (cabinet nudges) never wakes.
            if let (Some(now), Some(last)) = (now, self.last_wake_motion) {
                if now - last > WAKE_RESET_SECS {
                    self.wake_accum = 0.0;
                }
            }
            if motion > 0.0 {
                self.last_wake_motion = now;
                self.wake_accum += motion;
            }
            if deliberate || self.wake_accum >= self.wake_threshold {
                self.dormant = false;
                self.wake_accum = 0.0;
                self.last_wake_motion = None;
                return true;
            }
        } else if self.dormant_on_keys && keyed && !deliberate && motion < 1.0 {
            self.set_dormant(true);
        }
        false
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
        if self.opacity <= 0.0 {
            return;
        }
        let Some(pos) = self.virtual_pos else { return };
        if self.opacity >= 1.0 {
            paint_cursor_shape(painter, cursor_icon, pos, self.scale);
        } else {
            // Mid-fade (dormancy transition): draw semi-transparent.
            let mut faded = painter.clone();
            faded.set_opacity(self.opacity);
            paint_cursor_shape(&faded, cursor_icon, pos, self.scale);
        }
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
            // Navigation arrow pointing right, baked from an SVG outline (curves
            // flattened and the concave outline ear-clipped offline — see the
            // const tables below). The tip sits exactly at `pos` (hot-spot),
            // matching the link-pointer convention.
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
            // Default arrow pointing up-left, baked from an SVG outline.
            // The tip is at `pos` (hot-spot).
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
// indices, generated offline from SVG outlines. Curves are flattened to line
// segments; the triangle list lets the concave fill render without the
// convexity artefacts of epaint's closed-path fill.

/// Default arrow (tip at origin, pointing up-left).
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

/// Navigation arrow (tip at origin, pointing right). Used for
/// `PointingHand` / `Grab` / `Grabbing`.
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
