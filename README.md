# egui-rotate

[![Crates.io](https://img.shields.io/crates/v/egui-rotate.svg)](https://crates.io/crates/egui-rotate)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Viewport rotation (0° / 90° / 180° / 270°) for [egui](https://github.com/emilk/egui),
as a **plugin**.

Built for use cases where the OS *cannot* rotate the screen: virtual pinball
cabinets, kiosks, embedded panels, multi-monitor setups where one display is
mounted physically rotated and the others are not.

This crate **does not modify egui**: it ships an [`egui::Plugin`]. Register it
once and rotation becomes transparent — input, rendering and the OS cursor — on
any backend (`egui_glow`, `egui_wgpu`, **eframe**, custom), with no other code.

```rust
use egui_rotate::{Rotation, RotationPlugin};

ctx.add_plugin(RotationPlugin::new(Rotation::CW90));
```

That's the whole integration. Pointer/touch input is remapped into the rotated
space, the entire UI is rendered rotated, and directional OS cursor icons are
remapped to match.

## Highlights

- **One-line integration** — `ctx.add_plugin(...)`, works on every backend incl. eframe and the web.
- **Per-window** — rotation is per-viewport and opt-in, so a rotated cabinet window can coexist with upright child windows.
- **Exact** — only 90° increments; integer math, no FP drift, no resampling. Text, images and rounded rects all rotate correctly.
- **Software cursor** (opt-in) — a rotated virtual cursor for kiosks/cabinets where the OS cursor can't be rotated, with soft/hard edge locking, an automatic OS pointer grab, and keyboard/gamepad auto-hide.

## Multiple windows

Rotation is keyed per viewport. `RotationPlugin::new` configures the root window;
child windows pass through untouched unless you configure them:

```rust
use egui::ViewportId;
use egui_rotate::Rotation;

// On the registered plugin handle:
plugin.set_viewport_rotation(child_id, Rotation::CW270);
```

## Software cursor (feature `software-cursor`, opt-in)

When the viewport is rotated, the OS cursor still moves in physical space, which
is disorienting on a cabinet. Attach a virtual cursor that tracks raw mouse
deltas in logical space:

```toml
egui-rotate = { version = "2", features = ["software-cursor"] }
```

```rust
use egui_rotate::{Rotation, RotationPlugin, SoftwareCursor};

ctx.add_plugin(
    RotationPlugin::new(Rotation::CW90)
        .with_software_cursor(SoftwareCursor::new()),
);
```

Out of the box the plugin hides the OS cursor, draws the virtual one, holds an
OS pointer grab (`Confined`) while captured so the real cursor can't leave the
window, and **soft-locks** the virtual cursor at the window edge.

### Lock modes

| Mode | Config | Behavior at the window edge |
|---|---|---|
| No lock | `.with_edge_resistance(0.0)` | releases to the OS immediately |
| **Soft lock** *(default)* | `.with_edge_resistance(px)` | resists casual contact; a deliberate push (fast flick or ~150 px of sustained pressure) breaks out |
| Hard lock | `.with_lock(true)` | never releases (kiosk / cabinet) |

On a breakout the plugin drops its grab; on platforms that support cursor
warping (everywhere but Wayland), call `take_pending_warp()` once per frame and
warp the OS cursor to the returned position so it exits where the virtual cursor
left. `.with_os_cursor_pin(true)` additionally re-centres the real cursor
whenever it strays ("pseudo-lock") — useful where the grab is unavailable
(winit's `Confined` covers Windows/X11/Wayland; macOS is `Locked`-only, see
`with_os_grab`).

### Auto-hide (keyboard & gamepad navigation)

On a front-end navigated by keyboard or joystick, a parked mouse cursor is a
problem: its hover keeps overriding the selection (flip to the next table, and
the one under the cursor instantly re-highlights). The software cursor can go
**dormant**: it dissolves (500 ms fade by default, tune with `with_fade`) and
egui's hover is cleared, so keyboard/gamepad selection wins. Any deliberate
mouse use reforms it in place and re-asserts hover.

For the keyboard, opt in once:

```rust
SoftwareCursor::new().with_dormant_on_keys(true)
```

For a gamepad or joystick, egui never sees those events — tell the cursor
yourself wherever you process stick/button input (gilrs, sdl2, evdev, …):

```rust
// e.g. in a pinball front-end, on any gamepad navigation event:
ctx.plugin::<RotationPlugin>()
    .lock()
    .software_cursor_mut()
    .unwrap()
    .set_dormant(true);
```

Waking is automatic in both cases: a click, wheel tick, touch, or a small burst
of mouse motion (`with_wake_threshold`, default 6 px — raise it on cabinets that
shake under nudging) brings the cursor back where it was.

## Examples & demos

| Demo | Stack | Run |
|---|---|---|
| `plugin_demo` | winit + glow (no framework) | `cargo run --example plugin_demo --features software-cursor` |
| `rotated_demo` | winit + glow, **manual/legacy** API | `cargo run --example rotated_demo --features software-cursor` |
| [`eframe-demo/`](eframe-demo) | eframe — child window + perf, **native & web** | `cd eframe-demo && cargo run` (web: see its [README](eframe-demo/README.md)) |

The eframe demo shows two windows each rotating independently, plus an animated
stress test, and runs both natively and in the browser.

## Migrating from 0.1.x

0.1.x exposed manual helpers you called in your own integration loop. As of
**1.0**, the plugin does all of that for you. The free functions
`transform_raw_input` and `transform_clipped_primitives` are **deprecated** (they
still work, and `rotated_demo` shows the manual path) — replace your loop with a
single `ctx.add_plugin(RotationPlugin::new(rotation))`. `Rotation`, `SoftwareCursor`
and `CursorIconExt` are unchanged.

## Why a separate crate?

Integrating rotation directly into egui was [proposed](https://github.com/emilk/egui/pull/8113)
and declined as out-of-scope upstream. The plugin system makes a clean companion
crate the right home — no fork, no eframe hooks.

### Why app-level rotation, not OS rotation?

| OS | Per-window rotation? | Notes |
|---|---|---|
| **Windows** | No | `SetDisplayConfig` rotates the **entire desktop** for that display. |
| **macOS** | No public API | Requires private SPI (`IOMobileFramebuffer`); tools like `displayplacer` break across releases. |
| **Wayland** | No cross-compositor protocol | `wlr-output-management` is wlroots-only, `kde-output-management-v2` is KWin-only, GNOME exposes nothing. |
| **X11** | No | `xrandr` rotates the whole output. |
| **Web (wasm)** | No | There is no "rotate the OS" inside a browser canvas. |

For cabinet/kiosk apps, only one window needs to be rotated — OS-level rotation
would break every other monitor in the session. App-level rotation is the only
correct answer.

## What this crate does *not* do

- It does **not** rotate paint callbacks (`Primitive::Callback`) — custom callbacks
  own their coordinate space.
- It does **not** rotate at arbitrary angles — only 0°/90°/180°/270°.
- Per-window rotation works within one app; it does not try to manage different
  rotations across separate OS windows on different physical monitors beyond the
  per-viewport config above. The common cabinet case (one rotated fullscreen
  window) is fully covered.

## Compatibility

Each egui-rotate release targets **one egui minor**, because egui's `Plugin`
trait signatures can change between minors — egui 0.35 did exactly that
(`input_hook`/`output_hook` gained a `&Context` parameter). Pick the release
that matches your egui:

| egui-rotate | egui   |
|-------------|--------|
| `2.x`       | `0.35` |
| `1.x`       | `0.34` |

## License

Dual-licensed under MIT or Apache-2.0, matching egui itself.

[`egui::Plugin`]: https://docs.rs/egui/latest/egui/trait.Plugin.html
