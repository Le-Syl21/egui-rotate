# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
