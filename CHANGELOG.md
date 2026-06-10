# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0] - 2026-06-10

The crate becomes **plugin-first**. Where 0.1.x asked you to call helper
functions in your own integration loop, the whole thing is now a single
`egui::Plugin` you register once — it works on any backend, including eframe
and the web, with no other integration code.

```rust
ctx.add_plugin(RotationPlugin::new(Rotation::CW90));
```

### Added
- **`RotationPlugin`** — a self-contained `egui::Plugin` that rotates input,
  rendering and the OS cursor transparently. One `ctx.add_plugin(...)` is the
  entire integration; works with `egui_glow`, `egui_wgpu`, eframe and custom
  backends.
- **Per-viewport rotation.** Rotation is opt-in per window: `RotationPlugin::new`
  configures the root viewport, `set_viewport_rotation` configures children, and
  unconfigured viewports pass through untouched. Nested (immediate) child
  viewports are paired correctly via a begin/end stack.
- **Software cursor in the plugin.** `with_software_cursor` / `with_software_cursor_on`
  attach a `SoftwareCursor`; in locked (kiosk) mode the plugin hides the OS cursor
  and draws the virtual one with zero integration code. `take_pending_warp` exposes
  the edge-release warp for non-locked mode.
- `rotate_clipped_shapes` / `rotate_shape` — public pre-tessellation shape
  rotation (the plugin's output stage), for custom pipelines.
- `Rotation::next_cw` / `prev_cw` (cycle a rotation) and `Rotation::inverse_angle`.
- New demos: `plugin_demo` (winit + glow, plugin-based) and `eframe-demo/`
  (a child window with its own rotation + an animated perf stress test, running
  both natively and on the web).

### Fixed
- **Textured rects (images) now rotate with the viewport.** egui's tessellator
  keeps a brushed `RectShape`'s texture screen-aligned under `RectShape::angle`,
  so an image rendered upright while the rest rotated. Textured rects are now
  converted to a rotated textured quad mesh.

### Deprecated
- `transform_raw_input` and `transform_clipped_primitives` — register a
  `RotationPlugin` instead. They still work (and `rotated_demo` shows the manual
  path); they will be removed in a future release.

## [0.1.5] - 2026-06-01

### Changed
- Reworked the software-cursor visuals. The default arrow and the
  `PointingHand` / `Grab` / `Grabbing` cursor are now baked from SVG outlines
  (`assets/`), with their concave shapes triangulated offline so the fill
  renders without the convexity artefacts of epaint's closed-path fill.
- The drawn cursor colour now follows the egui theme — white ink on a dark
  theme, black ink on a light one — so it always contrasts the background.

### Added
- `rotated_demo`: in-UI "Rotate" button and a crates.io hyperlink (the latter
  exercises the pointing-hand cursor over a clickable element).

## [0.1.4] - 2026-05-31

### Changed
- Replaced the `PointingHand` cursor's placeholder circle with a stylised
  pointing-hand shape.

## [0.1.3] - 2026-04-30

### Added
- `SoftwareCursor::set_virtual_pos(pos)` — force-capture the cursor at a
  given logical position. Required when entering a kiosk mode that grabs
  the OS cursor (e.g. via `ViewportCommand::CursorGrab(CursorGrab::Locked)`
  on Wayland) — under such a grab the OS cursor is frozen, no
  `Event::PointerMoved` is delivered, and the cursor would otherwise
  never start tracking relative-motion deltas.

## [0.1.2] - 2026-04-30

### Fixed
- Fix `cargo doc -D warnings` (broken intra-doc link to a non-existent
  `egui::ViewportCommand::CursorIcon` variant — replaced by a reference to
  `egui::Context::set_cursor_icon`). No code change vs. 0.1.1.

## [0.1.1] - 2026-04-30

### Fixed
- Clarified that `SoftwareCursor::draw` expects the un-rotated cursor icon
  (as set by egui). Pre-rotating via `CursorIconExt::rotate` would double the
  rotation and flip the shape — e.g. the text I-beam would render parallel to
  the text instead of perpendicular. `CursorIconExt::rotate` is now documented
  as the API for the OS-cursor scenario only.
- Updated `examples/rotated_demo` accordingly (no API change).

## [0.1.0] - TBD

### Added
- `Rotation` enum (`None` / `CW90` / `CW180` / `CW270`) with pixel-perfect integer math
- `transform_raw_input` — rotate `RawInput` (pointer positions, touch, wheel, mouse moved)
  before egui sees it
- `transform_clipped_primitives` — rotate tessellated mesh vertices and clip rects
  back to physical screen space before painting
- `CursorIconExt::rotate` — extension trait remapping directional cursors
  (resize arrows, text caret, etc.) to match the rotation
- `SoftwareCursor` (feature `software-cursor`, opt-in) — virtual cursor drawn in
  logical space, with capture/release at window edges, configurable scale, and
  optional lock mode for kiosk / pinball cabinet use
- `rotated_demo` example — winit + glow + egui_glow integration demonstrating
  every feature, with `R` to cycle rotation and `L` to toggle the cursor lock

[Unreleased]: https://github.com/Le-Syl21/egui-rotate/compare/v0.1.5...HEAD
[0.1.5]: https://github.com/Le-Syl21/egui-rotate/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/Le-Syl21/egui-rotate/compare/v0.1.3...v0.1.4
[0.1.0]: https://github.com/Le-Syl21/egui-rotate/releases/tag/v0.1.0
