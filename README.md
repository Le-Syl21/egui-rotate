# egui-rotate

[![Crates.io](https://img.shields.io/crates/v/egui-rotate.svg)](https://crates.io/crates/egui-rotate)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Viewport rotation (0° / 90° / 180° / 270°) for [egui](https://github.com/emilk/egui).

Built for use cases where the OS *cannot* rotate the screen: virtual pinball
cabinets, kiosks, embedded panels, multi-monitor setups where one display is
mounted physically rotated and the others are not.

This crate **does not modify egui** — it ships pure helper functions you call
in your integration loop, plus an optional rotated software cursor.

## Why a separate crate?

Integrating rotation directly into egui was [proposed](https://github.com/emilk/egui/pull/8113)
and declined as out-of-scope for the upstream maintainers. The use case is real
(see below) but niche enough that a dedicated companion crate is the right home.

### Why app-level rotation, not OS rotation?

| OS | Per-window rotation? | Notes |
|---|---|---|
| **Windows** | No | `SetDisplayConfig` rotates the **entire desktop** for the affected display — taskbar, login screen, every other app. |
| **macOS** | No public API | Setting rotation requires private SPI (`IOMobileFramebuffer`). Tools like `displayplacer` break across releases. |
| **Wayland** | No cross-compositor protocol | `wlr-output-management` is wlroots-only, `kde-output-management-v2` is KWin-only, GNOME doesn't expose rotation via Wayland. |
| **X11** | No | `xrandr` rotates the whole output. Same problem as Windows. |
| **Web (wasm)** | No | There is no "rotate the OS" inside a browser canvas. |

For multi-monitor cabinet/kiosk apps, only one window/display needs to be
rotated. OS-level rotation would break every other monitor sharing the same
session. App-level rotation is the only correct answer.

## Features

- `Rotation` enum: `None` / `CW90` / `CW180` / `CW270`. Pure integer math, no FP drift.
- `transform_raw_input` — rotate input events before egui sees them.
- `transform_clipped_primitives` — rotate tessellated output back to physical screen space.
- `CursorIconExt::rotate` — remap directional cursors (resize arrows, text caret) to match the rotation.
- `SoftwareCursor` (feature `software-cursor`, **opt-in**) — virtual cursor drawn in logical space, with capture/release at window edges, scale, and lock mode for kiosk use.

## Usage

### Pipeline

```rust
use egui_rotate::{Rotation, transform_raw_input, transform_clipped_primitives};

let rotation = Rotation::CW90;

// 1. Rotate the input before egui sees it.
transform_raw_input(&mut raw_input, rotation);

// 2. Run your app normally — UI sees a rotated coordinate space.
let full_output = ctx.run_ui(raw_input, |ui| {
    ui.label("Hello, rotated world!");
});

// 3. Tessellate as usual.
let mut primitives = ctx.tessellate(full_output.shapes, pixels_per_point);

// 4. Rotate primitives back to physical screen space before painting.
let logical_size = ctx.screen_rect().size();
transform_clipped_primitives(&mut primitives, rotation, logical_size);

// 5. Hand `primitives` to your painter (egui_glow, egui_wgpu, custom).
```

### With `SoftwareCursor` (opt-in)

When the viewport is rotated, the OS cursor still moves in physical space, which
is disorienting. The crate provides a virtual cursor that follows raw mouse
deltas in logical space. **Enable the `software-cursor` feature** in your
`Cargo.toml`:

```toml
egui-rotate = { version = "0.1", features = ["software-cursor"] }
```

```rust
use egui_rotate::{Rotation, SoftwareCursor};

// Persist across frames.
let mut cursor = SoftwareCursor::new().with_scale(2.0);

// In your input handling:
let cursor_out = cursor.process_input(&mut raw_input, rotation, physical_size);

// Hide the OS cursor while captured (your integration's job — depends on SDL3 / winit).
if cursor.is_captured() {
    integration.hide_os_cursor();
} else if let Some(release_to) = cursor_out.release_os_cursor_to {
    integration.show_os_cursor();
    integration.warp_os_cursor_to(release_to);
}

// Run UI…

// Draw the cursor on top.
let painter = ctx.layer_painter(egui::LayerId::new(
    egui::Order::Foreground,
    egui::Id::new("software-cursor"),
));
let icon_from_egui = full_output.platform_output.cursor_icon;
use egui_rotate::CursorIconExt;
cursor.draw(&painter, icon_from_egui.rotate(rotation));
```

For kiosk / fullscreen scenarios where the cursor must never leave the window:

```rust
cursor.set_lock(true);
```

## Run the demo

A complete winit + glow + egui_glow integration is shipped as an example:

```bash
cargo run --example rotated_demo
```

Press `R` to cycle through `None / CW90 / CW180 / CW270`. Press `Esc` to quit.
The demo shows a regular egui UI (heading, slider, text edit, scroll area)
rendered in the rotation of your choice — input is remapped transparently.

## What this crate does *not* do

- It does **not** integrate with `eframe` automatically. eframe owns the
  tessellate→paint pipeline and there's no hook between them today. Use this
  crate from a custom integration (winit + wgpu/glow, SDL3, etc.).
- It does **not** rotate paint callbacks (`Primitive::Callback`). Custom
  callbacks are responsible for their own coordinate space.
- It does **not** rotate at arbitrary angles — only 0°/90°/180°/270°.
  Arbitrary angles would require a different design (and lossy resampling).

## Compatibility

Tested with `egui = 0.34` and `0.35`. The dependency is range-versioned
(`>=0.34, <0.36`) so the crate floats with your egui.

## License

Dual-licensed under MIT or Apache-2.0, matching egui itself.
