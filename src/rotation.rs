use egui::{Pos2, Rect, Vec2};

/// Viewport rotation in 90-degree increments (clockwise).
///
/// When applied, the entire UI is rendered rotated and all input coordinates
/// (mouse, touch) are remapped. Application code sees a normal coordinate
/// space — rotation is transparent.
///
/// Use cases: pinball cabinet displays, kiosks, embedded screens,
/// industrial panels mounted in non-standard orientation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum Rotation {
    /// No rotation (0 degrees).
    #[default]
    None,

    /// 90 degrees clockwise.
    CW90,

    /// 180 degrees.
    CW180,

    /// 270 degrees clockwise (= 90 degrees counter-clockwise).
    CW270,
}

impl Rotation {
    /// `true` if width and height are swapped (90 or 270 degrees).
    #[inline]
    pub fn swaps_axes(self) -> bool {
        matches!(self, Self::CW90 | Self::CW270)
    }

    /// `true` if this is [`Self::None`].
    #[inline]
    pub fn is_none(self) -> bool {
        self == Self::None
    }

    /// The next rotation, cycling clockwise: `None → CW90 → CW180 → CW270 → None`.
    ///
    /// Handy for a "rotate 90°" button or keyboard shortcut.
    #[inline]
    pub fn next_cw(self) -> Self {
        match self {
            Self::None => Self::CW90,
            Self::CW90 => Self::CW180,
            Self::CW180 => Self::CW270,
            Self::CW270 => Self::None,
        }
    }

    /// The previous rotation, cycling counter-clockwise.
    #[inline]
    pub fn prev_cw(self) -> Self {
        match self {
            Self::None => Self::CW270,
            Self::CW90 => Self::None,
            Self::CW180 => Self::CW90,
            Self::CW270 => Self::CW180,
        }
    }

    /// Transform a point from physical screen space to logical UI space.
    ///
    /// `physical_size` is the physical window size (before rotation).
    #[inline]
    pub fn transform_pos(self, pos: Pos2, physical_size: Vec2) -> Pos2 {
        match self {
            Self::None => pos,
            Self::CW90 => Pos2::new(physical_size.y - pos.y, pos.x),
            Self::CW180 => Pos2::new(physical_size.x - pos.x, physical_size.y - pos.y),
            Self::CW270 => Pos2::new(pos.y, physical_size.x - pos.x),
        }
    }

    /// Transform a point from logical UI space back to physical screen space.
    #[inline]
    pub fn inverse_transform_pos(self, pos: Pos2, logical_size: Vec2) -> Pos2 {
        match self {
            Self::None => pos,
            Self::CW90 => Pos2::new(pos.y, logical_size.x - pos.x),
            Self::CW180 => Pos2::new(logical_size.x - pos.x, logical_size.y - pos.y),
            Self::CW270 => Pos2::new(logical_size.y - pos.y, pos.x),
        }
    }

    /// Transform a delta/vector (no translation needed).
    #[inline]
    pub fn transform_vec(self, vec: Vec2) -> Vec2 {
        match self {
            Self::None => vec,
            Self::CW90 => Vec2::new(-vec.y, vec.x),
            Self::CW180 => Vec2::new(-vec.x, -vec.y),
            Self::CW270 => Vec2::new(vec.y, -vec.x),
        }
    }

    /// Clockwise angle (radians) of the logical→physical mapping.
    ///
    /// This is the rotational part of [`Self::inverse_transform_pos`], expressed as
    /// the clockwise angle used by epaint's per-shape `angle` fields
    /// ([`egui::epaint::TextShape::angle`], [`RectShape::angle`](egui::epaint::RectShape),
    /// [`EllipseShape::angle`](egui::epaint::EllipseShape)).
    ///
    /// Note the sign: it is the *opposite* of the screen's physical mounting
    /// (e.g. a screen mounted 90° CW is compensated by rendering the UI at -90°),
    /// matching the directional remapping in [`crate::CursorIconExt::rotate`].
    #[inline]
    pub fn inverse_angle(self) -> f32 {
        use std::f32::consts::{FRAC_PI_2, PI};
        match self {
            Self::None => 0.0,
            Self::CW90 => -FRAC_PI_2,
            Self::CW180 => PI,
            Self::CW270 => FRAC_PI_2,
        }
    }

    /// Logical screen rect after rotation. For 90/270, width and height are swapped.
    #[inline]
    pub fn transform_screen_rect(self, physical_rect: Rect) -> Rect {
        if self.swaps_axes() {
            Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(physical_rect.height(), physical_rect.width()),
            )
        } else {
            physical_rect
        }
    }
}
