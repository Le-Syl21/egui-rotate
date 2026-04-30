use egui::CursorIcon;

use crate::Rotation;

/// Extension trait for [`CursorIcon`] adding rotation-aware remapping.
///
/// Directional cursors (resize arrows, text caret) are remapped so they
/// visually point in the correct direction after rotation.
pub trait CursorIconExt {
    /// Rotate a cursor icon to match a [`Rotation`].
    fn rotate(self, rotation: Rotation) -> Self;
}

impl CursorIconExt for CursorIcon {
    fn rotate(self, rotation: Rotation) -> Self {
        if rotation.is_none() {
            return self;
        }

        match self {
            // Bidirectional resize cursors
            Self::ResizeHorizontal => match rotation {
                Rotation::CW90 | Rotation::CW270 => Self::ResizeVertical,
                _ => self,
            },
            Self::ResizeVertical => match rotation {
                Rotation::CW90 | Rotation::CW270 => Self::ResizeHorizontal,
                _ => self,
            },
            Self::ResizeNeSw => match rotation {
                Rotation::CW90 | Rotation::CW270 => Self::ResizeNwSe,
                _ => self,
            },
            Self::ResizeNwSe => match rotation {
                Rotation::CW90 | Rotation::CW270 => Self::ResizeNeSw,
                _ => self,
            },

            // Column / Row resize
            Self::ResizeColumn => match rotation {
                Rotation::CW90 | Rotation::CW270 => Self::ResizeRow,
                _ => self,
            },
            Self::ResizeRow => match rotation {
                Rotation::CW90 | Rotation::CW270 => Self::ResizeColumn,
                _ => self,
            },

            // Text cursors
            Self::Text => match rotation {
                Rotation::CW90 | Rotation::CW270 => Self::VerticalText,
                _ => self,
            },
            Self::VerticalText => match rotation {
                Rotation::CW90 | Rotation::CW270 => Self::Text,
                _ => self,
            },

            // Single-direction resize cursors: rotate clockwise
            Self::ResizeEast => match rotation {
                Rotation::CW90 => Self::ResizeSouth,
                Rotation::CW180 => Self::ResizeWest,
                Rotation::CW270 => Self::ResizeNorth,
                _ => self,
            },
            Self::ResizeSouthEast => match rotation {
                Rotation::CW90 => Self::ResizeSouthWest,
                Rotation::CW180 => Self::ResizeNorthWest,
                Rotation::CW270 => Self::ResizeNorthEast,
                _ => self,
            },
            Self::ResizeSouth => match rotation {
                Rotation::CW90 => Self::ResizeWest,
                Rotation::CW180 => Self::ResizeNorth,
                Rotation::CW270 => Self::ResizeEast,
                _ => self,
            },
            Self::ResizeSouthWest => match rotation {
                Rotation::CW90 => Self::ResizeNorthWest,
                Rotation::CW180 => Self::ResizeNorthEast,
                Rotation::CW270 => Self::ResizeSouthEast,
                _ => self,
            },
            Self::ResizeWest => match rotation {
                Rotation::CW90 => Self::ResizeNorth,
                Rotation::CW180 => Self::ResizeEast,
                Rotation::CW270 => Self::ResizeSouth,
                _ => self,
            },
            Self::ResizeNorthWest => match rotation {
                Rotation::CW90 => Self::ResizeNorthEast,
                Rotation::CW180 => Self::ResizeSouthEast,
                Rotation::CW270 => Self::ResizeSouthWest,
                _ => self,
            },
            Self::ResizeNorth => match rotation {
                Rotation::CW90 => Self::ResizeEast,
                Rotation::CW180 => Self::ResizeSouth,
                Rotation::CW270 => Self::ResizeWest,
                _ => self,
            },
            Self::ResizeNorthEast => match rotation {
                Rotation::CW90 => Self::ResizeSouthEast,
                Rotation::CW180 => Self::ResizeSouthWest,
                Rotation::CW270 => Self::ResizeNorthWest,
                _ => self,
            },

            // All other cursors are rotation-invariant
            _ => self,
        }
    }
}
