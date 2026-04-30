# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/Le-Syl21/egui-rotate/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Le-Syl21/egui-rotate/releases/tag/v0.1.0
