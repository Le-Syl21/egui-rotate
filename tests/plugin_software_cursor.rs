//! Software-cursor integration through [`RotationPlugin`] (feature-gated).
#![cfg(feature = "software-cursor")]

use egui::{CursorIcon, Event, Pos2, Rect, Vec2};
use egui_rotate::{Rotation, RotationPlugin, SoftwareCursor};

const PHYSICAL: Vec2 = Vec2 { x: 800.0, y: 600.0 };

fn raw(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, PHYSICAL)),
        events,
        ..Default::default()
    }
}

/// While the software cursor is captured, the OS cursor must be hidden
/// (`CursorIcon::None`) since the plugin draws its own.
#[test]
fn locked_cursor_hides_os_cursor() {
    let ctx = egui::Context::default();
    ctx.add_plugin(
        RotationPlugin::new(Rotation::CW90)
            .with_software_cursor(SoftwareCursor::new().with_lock(true)),
    );

    // A pointer event seeds capture.
    let out = ctx.run_ui(
        raw(vec![Event::PointerMoved(Pos2::new(400.0, 300.0))]),
        |_| {},
    );

    assert_eq!(
        out.platform_output.cursor_icon,
        CursorIcon::None,
        "OS cursor should be hidden while the software cursor is captured"
    );

    // Locked mode never releases to the OS.
    let warp = ctx.plugin::<RotationPlugin>().lock().take_pending_warp();
    assert!(warp.is_none(), "locked mode must not request a warp");
}

/// With no lock at all (zero edge resistance), moving past the screen edge
/// releases the cursor to the OS and surfaces a warp request the integration
/// can act on. (The default is a soft lock — opt out explicitly here.)
#[test]
fn nonlocked_cursor_requests_warp_at_edge() {
    let ctx = egui::Context::default();
    ctx.add_plugin(
        RotationPlugin::new(Rotation::CW90)
            .with_software_cursor(SoftwareCursor::new().with_edge_resistance(0.0)),
    );

    // Seed capture at the logical centre, then push far past the right edge via a
    // raw mouse delta in the same batch.
    let out = ctx.run_ui(
        raw(vec![
            Event::PointerMoved(Pos2::new(400.0, 300.0)),
            Event::MouseMoved(Vec2::new(400.0, 0.0)),
        ]),
        |_| {},
    );

    // Released → OS cursor visible again (not forced to None).
    assert_ne!(out.platform_output.cursor_icon, CursorIcon::None);

    let warp = ctx.plugin::<RotationPlugin>().lock().take_pending_warp();
    assert!(
        warp.is_some(),
        "edge release in non-locked mode should request an OS-cursor warp"
    );
    // Draining it once leaves nothing behind.
    assert!(ctx
        .plugin::<RotationPlugin>()
        .lock()
        .take_pending_warp()
        .is_none());
}

/// Dormancy (auto-hide): keyboard input hides the cursor and clears egui's
/// hover so keyboard/gamepad navigation is not overridden by a stale pointer;
/// a burst of mouse motion wakes it and re-asserts the hover.
#[test]
fn keyboard_puts_cursor_dormant_and_mouse_wakes_it() {
    let ctx = egui::Context::default();
    // Auto-hide on keyboard is opt-in.
    ctx.add_plugin(
        RotationPlugin::new(Rotation::CW90)
            .with_software_cursor(SoftwareCursor::new().with_dormant_on_keys(true)),
    );

    let is_dormant = || {
        let handle = ctx.plugin::<RotationPlugin>();
        let plugin = handle.lock();
        plugin.software_cursor().unwrap().is_dormant()
    };

    // Capture at the physical centre → logical (300, 400).
    let _ = ctx.run_ui(
        raw(vec![Event::PointerMoved(Pos2::new(400.0, 300.0))]),
        |_| {},
    );
    assert!(!is_dormant());

    // A key press puts the cursor to sleep…
    let _ = ctx.run_ui(
        raw(vec![Event::Key {
            key: egui::Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }]),
        |_| {},
    );
    assert!(
        is_dormant(),
        "keyboard input should put the cursor to sleep"
    );

    // …and egui has forgotten the pointer: hover follows the keyboard now.
    let mut has_pointer = true;
    let _ = ctx.run_ui(raw(vec![]), |ui| {
        has_pointer = ui.input(|i| i.pointer.has_pointer());
    });
    assert!(!has_pointer, "hover must be cleared while dormant");

    // A deliberate mouse burst (≥ wake threshold) wakes it, hover re-asserted
    // at the updated virtual position.
    let mut pointer = None;
    let _ = ctx.run_ui(raw(vec![Event::MouseMoved(Vec2::new(10.0, 0.0))]), |ui| {
        pointer = ui.input(|i| i.pointer.latest_pos());
    });
    assert!(!is_dormant(), "mouse motion should wake the cursor");
    let pointer = pointer.expect("hover must be back after waking");
    assert!(
        (pointer - Pos2::new(310.0, 400.0)).length() < 0.01,
        "pointer should reappear at the remembered position (+ the wake motion), got {pointer:?}"
    );
}

/// Dormancy transitions fade the cursor (opacity animated over `with_fade`)
/// instead of popping.
#[test]
fn dormancy_fades_the_cursor() {
    let ctx = egui::Context::default();
    ctx.add_plugin(
        RotationPlugin::new(Rotation::CW90).with_software_cursor(
            SoftwareCursor::new()
                .with_dormant_on_keys(true)
                .with_fade(std::time::Duration::from_millis(100)),
        ),
    );

    let timed = |t: f64, events: Vec<Event>| egui::RawInput {
        time: Some(t),
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, PHYSICAL)),
        events,
        ..Default::default()
    };
    let opacity = || {
        let handle = ctx.plugin::<RotationPlugin>();
        let plugin = handle.lock();
        plugin.software_cursor().unwrap().opacity()
    };

    // Capture, fully visible.
    let _ = ctx.run_ui(
        timed(0.0, vec![Event::PointerMoved(Pos2::new(400.0, 300.0))]),
        |_| {},
    );
    assert_eq!(opacity(), 1.0);

    // Key → dormant; 50 ms into the 100 ms fade the cursor is half dissolved.
    let _ = ctx.run_ui(
        timed(
            0.05,
            vec![Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        ),
        |_| {},
    );
    let mid = opacity();
    assert!(
        mid > 0.0 && mid < 1.0,
        "cursor should be mid-fade, opacity = {mid}"
    );

    // Past the fade duration: fully dissolved.
    let _ = ctx.run_ui(timed(0.20, vec![]), |_| {});
    assert_eq!(opacity(), 0.0);

    // Mouse burst 50 ms later → awake, mid-reform.
    let _ = ctx.run_ui(
        timed(0.25, vec![Event::MouseMoved(Vec2::new(10.0, 0.0))]),
        |_| {},
    );
    let reforming = opacity();
    assert!(
        reforming > 0.0 && reforming < 1.0,
        "cursor should be reforming after waking, opacity = {reforming}"
    );
    // And fully reformed once the fade has run its course.
    let _ = ctx.run_ui(timed(0.40, vec![]), |_| {});
    assert_eq!(opacity(), 1.0);
}

/// The plugin manages the OS pointer grab itself: `Confined` (the default) on
/// capture, `None` on release — including when rotation is switched off.
#[test]
fn plugin_manages_os_grab_lifecycle() {
    use egui::viewport::CursorGrab;

    let grab_commands = |out: &egui::FullOutput| -> Vec<CursorGrab> {
        out.viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| {
                v.commands
                    .iter()
                    .filter_map(|c| match c {
                        egui::ViewportCommand::CursorGrab(g) => Some(*g),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    let ctx = egui::Context::default();
    ctx.add_plugin(RotationPlugin::new(Rotation::CW90).with_software_cursor(SoftwareCursor::new()));

    // Capture → the plugin requests a Confined grab.
    let out = ctx.run_ui(
        raw(vec![Event::PointerMoved(Pos2::new(400.0, 300.0))]),
        |_| {},
    );
    assert_eq!(grab_commands(&out), vec![CursorGrab::Confined]);

    // Steady state → no redundant commands.
    let out = ctx.run_ui(raw(vec![]), |_| {});
    assert!(grab_commands(&out).is_empty());

    // Rotation switched off → the cursor is released and so is the grab.
    ctx.plugin::<RotationPlugin>()
        .lock()
        .set_rotation(Rotation::None);
    let out = ctx.run_ui(raw(vec![]), |_| {});
    assert_eq!(grab_commands(&out), vec![CursorGrab::None]);
}

/// Soft lock (`with_edge_resistance`): a small push past the edge stays
/// confined; accumulated deliberate pushes break through and release to the OS.
#[test]
fn soft_lock_confines_until_pushed_through() {
    let ctx = egui::Context::default();
    ctx.add_plugin(
        RotationPlugin::new(Rotation::CW90)
            .with_software_cursor(SoftwareCursor::new().with_edge_resistance(200.0)),
    );

    // Seed capture at the logical centre — (300, 400) for CW90 of 800x600.
    let _ = ctx.run_ui(
        raw(vec![Event::PointerMoved(Pos2::new(400.0, 300.0))]),
        |_| {},
    );

    // Push 50 px past the left edge: overshoot 50 < 200 → still confined.
    let _ = ctx.run_ui(raw(vec![Event::MouseMoved(Vec2::new(-350.0, 0.0))]), |_| {});
    {
        let handle = ctx.plugin::<RotationPlugin>();
        let mut plugin = handle.lock();
        assert!(
            plugin.software_cursor().unwrap().is_captured(),
            "a casual edge contact must not break the soft lock"
        );
        assert!(plugin.take_pending_warp().is_none());
    }

    // Keep pushing 160 px more: cumulative 210 ≥ 200 → breakout + warp request.
    let _ = ctx.run_ui(raw(vec![Event::MouseMoved(Vec2::new(-161.0, 0.0))]), |_| {});
    let handle = ctx.plugin::<RotationPlugin>();
    let mut plugin = handle.lock();
    assert!(
        !plugin.software_cursor().unwrap().is_captured(),
        "a sustained push must break through the soft lock"
    );
    assert!(plugin.take_pending_warp().is_some());
}

/// With OS-cursor pinning ("pseudo-lock"), a captured cursor whose *real* OS
/// position strays from the window centre triggers a `CursorPosition` viewport
/// command re-centring it — so the real cursor can never leave the window.
#[test]
fn os_pin_recentres_stray_os_cursor() {
    let ctx = egui::Context::default();
    ctx.add_plugin(
        RotationPlugin::new(Rotation::CW90).with_software_cursor(
            SoftwareCursor::new()
                .with_lock(true)
                .with_os_cursor_pin(true),
        ),
    );

    let pin_commands = |out: &egui::FullOutput| -> Vec<Pos2> {
        out.viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| {
                v.commands
                    .iter()
                    .filter_map(|c| match c {
                        egui::ViewportCommand::CursorPosition(p) => Some(*p),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    // Frame 1: capture at the physical centre — inside the dead zone, no warp.
    let out = ctx.run_ui(
        raw(vec![Event::PointerMoved(Pos2::new(400.0, 300.0))]),
        |_| {},
    );
    assert!(
        pin_commands(&out).is_empty(),
        "no re-centre needed while the OS cursor sits at the centre"
    );

    // Frame 2: the real cursor drifted near the right edge (still captured) —
    // the plugin must ask the backend to warp it back to the centre.
    let out = ctx.run_ui(
        raw(vec![Event::PointerMoved(Pos2::new(760.0, 300.0))]),
        |_| {},
    );
    assert_eq!(
        pin_commands(&out),
        vec![Pos2::new(400.0, 300.0)],
        "stray OS cursor should be warped back to the physical centre"
    );
}

/// Turning rotation off at runtime releases a captured software cursor instead
/// of freezing it with stale state.
#[test]
fn disabling_rotation_releases_software_cursor() {
    let ctx = egui::Context::default();
    ctx.add_plugin(
        RotationPlugin::new(Rotation::CW90)
            .with_software_cursor(SoftwareCursor::new().with_lock(true)),
    );

    // Capture the cursor with a pointer event.
    let _ = ctx.run_ui(
        raw(vec![Event::PointerMoved(Pos2::new(400.0, 300.0))]),
        |_| {},
    );
    assert!(ctx
        .plugin::<RotationPlugin>()
        .lock()
        .software_cursor()
        .expect("cursor attached")
        .is_captured());

    // Switch rotation off and run a frame: the cursor must be released and the
    // OS cursor visible again.
    ctx.plugin::<RotationPlugin>()
        .lock()
        .set_rotation(Rotation::None);
    let out = ctx.run_ui(raw(vec![]), |_| {});

    let handle = ctx.plugin::<RotationPlugin>();
    let plugin = handle.lock();
    let cursor = plugin.software_cursor().expect("cursor attached");
    assert!(!cursor.is_captured());
    assert!(cursor.virtual_pos().is_none());
    assert_ne!(out.platform_output.cursor_icon, CursorIcon::None);
}

/// Without a software cursor, the plugin remaps directional OS cursor icons.
#[test]
fn no_software_cursor_remaps_icon() {
    let ctx = egui::Context::default();
    ctx.add_plugin(RotationPlugin::new(Rotation::CW90));

    let out = ctx.run_ui(raw(vec![]), |ui| {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    });

    // Under 90°, a horizontal resize handle is visually vertical.
    assert_eq!(out.platform_output.cursor_icon, CursorIcon::ResizeVertical);
}
