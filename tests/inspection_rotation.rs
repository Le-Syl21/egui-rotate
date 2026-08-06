//! Pin the plugin-ordering contract between egui-rotate and egui_inspection.
//!
//! The inspector injects events in LOGICAL space (it reasons on the logical
//! AccessKit tree). `RotationPlugin` rotates PHYSICAL window events into
//! logical space. egui runs plugin `input_hook`s in registration order, so:
//!
//! * rotation first, inspection second → injected events bypass the
//!   rotation and land where the inspector aimed (the supported order);
//! * inspection first → injected events get rotated a second time and the
//!   click misses.
//!
//! Both directions are pinned here so a future refactor cannot silently
//! swap them.
#![cfg(feature = "inspection")]

use std::sync::{Arc, Mutex};

use egui::{Context, Event, Modifiers, Pos2, RawInput, Rect, Vec2};
use egui_inspection::{InspectionPlugin, Request};
use egui_rotate::{Rotation, RotationPlugin};

const PHYSICAL: Vec2 = Vec2::new(800.0, 600.0);

fn raw() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, PHYSICAL)),
        ..Default::default()
    }
}

fn click_at(pos: Pos2) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::default(),
        },
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::default(),
        },
    ]
}

/// Run one frame, remembering the button rect and whether it was clicked.
fn frame(ctx: &Context, clicked: &Arc<Mutex<bool>>, rect_out: &Arc<Mutex<Option<Rect>>>) {
    let clicked = Arc::clone(clicked);
    let rect_out = Arc::clone(rect_out);
    let mut out = ctx.run_ui(raw(), move |ui| {
        let r = ui.button("target");
        *rect_out.lock().unwrap() = Some(r.rect);
        if r.clicked() {
            *clicked.lock().unwrap() = true;
        }
    });
    out.textures_delta.clear(); // headless: no painter
}

fn run_scenario(inspection_first: bool) -> bool {
    let ctx = Context::default();
    let clicked = Arc::new(Mutex::new(false));
    let rect = Arc::new(Mutex::new(None));

    if inspection_first {
        ctx.add_plugin(InspectionPlugin::new(Some("test".into())));
        ctx.add_plugin(RotationPlugin::new(Rotation::CW90));
    } else {
        ctx.add_plugin(RotationPlugin::new(Rotation::CW90));
        ctx.add_plugin(InspectionPlugin::new(Some("test".into())));
    }

    // Frame 1: lay out the button, learn its LOGICAL rect.
    frame(&ctx, &clicked, &rect);
    let target = rect.lock().unwrap().expect("button laid out").center();

    // Inject a click at the button's logical centre, like the inspector would.
    ctx.with_plugin(|p: &mut InspectionPlugin| {
        p.submit(
            Request::ApplyEvents {
                events: click_at(target),
            },
            |_| {},
        );
    });

    // Frame 2 executes the injected events; frame 3 settles the response.
    frame(&ctx, &clicked, &rect);
    frame(&ctx, &clicked, &rect);

    let hit = *clicked.lock().unwrap();
    hit
}

#[test]
fn injected_logical_click_lands_when_inspection_registers_after_rotation() {
    assert!(
        run_scenario(false),
        "supported order (rotation → inspection): the logical click must hit"
    );
}

#[test]
fn injected_click_misses_when_inspection_registers_first() {
    assert!(
        !run_scenario(true),
        "inverse order would double-rotate injected events — if this now \
         HITS, egui changed its plugin pipeline: re-audit the ordering \
         contract in src/inspection.rs"
    );
}
