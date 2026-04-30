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
