use std::{error::Error, fmt, path::Path};

use image::RgbaImage;

use crate::pet::AnimationGroup;
use crate::pet::manifest::FrameGeometry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpriteRow {
    Idle,
    Blink,
    Happy,
    Curious,
    Sleepy,
    HoverCalm,
    HoverCheerful,
    HoverLively,
    WalkRight,
    Drag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub enum SpriteError {
    Image(image::ImageError),
    InvalidDimensions {
        width: u32,
        height: u32,
        expected_width: Option<u32>,
        expected_height: Option<u32>,
        frame_size: u32,
    },
}

impl From<image::ImageError> for SpriteError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
    }
}

impl fmt::Display for SpriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Image(error) => write!(f, "failed to load sprite image: {error}"),
            Self::InvalidDimensions {
                width,
                height,
                expected_width,
                expected_height,
                frame_size,
            } => {
                let expected = match (expected_width, expected_height) {
                    (Some(width), Some(height)) => format!("{width}x{height}"),
                    _ => "overflow".to_string(),
                };

                write!(
                    f,
                    "invalid sprite sheet dimensions: actual {width}x{height}, expected {expected}, frame size {frame_size}"
                )
            }
        }
    }
}

impl Error for SpriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Image(error) => Some(error),
            Self::InvalidDimensions { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpriteSheet {
    image: RgbaImage,
    frame_size: u32,
}

const EXPECTED_COLUMNS: u32 = 4;
const EXPECTED_ROWS: u32 = 10;

impl SpriteSheet {
    pub fn load(path: impl AsRef<Path>, frame_size: u32) -> Result<Self, SpriteError> {
        let image = image::open(path)?.into_rgba8();
        Self::from_image(image, frame_size)
    }

    pub fn from_image(image: RgbaImage, frame_size: u32) -> Result<Self, SpriteError> {
        let width = image.width();
        let height = image.height();
        let expected_width = frame_size.checked_mul(EXPECTED_COLUMNS);
        let expected_height = frame_size.checked_mul(EXPECTED_ROWS);

        if frame_size == 0 || Some(width) != expected_width || Some(height) != expected_height {
            return Err(SpriteError::InvalidDimensions {
                width,
                height,
                expected_width,
                expected_height,
                frame_size,
            });
        }

        Ok(Self { image, frame_size })
    }

    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    pub fn frame_size(&self) -> u32 {
        self.frame_size
    }

    pub fn frame_count(&self) -> u32 {
        EXPECTED_COLUMNS
    }

    pub fn row_count(&self) -> u32 {
        EXPECTED_ROWS
    }

    pub fn frame_rect(&self, row: SpriteRow, frame_index: usize) -> FrameRect {
        let row_index = match row {
            SpriteRow::Idle => 0,
            SpriteRow::Blink => 1,
            SpriteRow::Happy => 2,
            SpriteRow::Curious => 3,
            SpriteRow::Sleepy => 4,
            SpriteRow::HoverCalm => 5,
            SpriteRow::HoverCheerful => 6,
            SpriteRow::HoverLively => 7,
            SpriteRow::WalkRight => 8,
            SpriteRow::Drag => 9,
        };
        let column = (frame_index % EXPECTED_COLUMNS as usize) as u32;

        FrameRect {
            x: column * self.frame_size,
            y: row_index * self.frame_size,
            width: self.frame_size,
            height: self.frame_size,
        }
    }

    pub fn from_image_with_geometry(
        image: RgbaImage,
        geometry: &FrameGeometry,
    ) -> Result<Self, SpriteError> {
        let width = image.width();
        let height = image.height();
        let expected_width = geometry.width.checked_mul(geometry.columns);
        let expected_height = geometry.height.checked_mul(geometry.rows);

        if geometry.width == 0
            || geometry.height == 0
            || geometry.columns == 0
            || geometry.rows == 0
            || Some(width) != expected_width
            || Some(height) != expected_height
        {
            return Err(SpriteError::InvalidDimensions {
                width,
                height,
                expected_width,
                expected_height,
                frame_size: geometry.width.max(geometry.height),
            });
        }

        Ok(Self { image, frame_size: geometry.width })
    }

    pub fn load_with_geometry(
        path: impl AsRef<std::path::Path>,
        geometry: &FrameGeometry,
    ) -> Result<Self, SpriteError> {
        let image = image::open(path)?.into_rgba8();
        Self::from_image_with_geometry(image, geometry)
    }

    pub fn frame_rect_by_index(&self, sprite_index: u32) -> FrameRect {
        let columns = (self.image.width() / self.frame_size).max(1);
        let row = sprite_index / columns;
        let col = sprite_index % columns;
        FrameRect {
            x: col * self.frame_size,
            y: row * self.frame_size,
            width: self.frame_size,
            height: self.frame_size,
        }
    }
}

impl From<AnimationGroup> for SpriteRow {
    fn from(value: AnimationGroup) -> Self {
        match value {
            AnimationGroup::Idle => Self::Idle,
            AnimationGroup::Blink => Self::Blink,
            AnimationGroup::Happy => Self::Happy,
            AnimationGroup::Curious => Self::Curious,
            AnimationGroup::Sleepy => Self::Sleepy,
            AnimationGroup::HoverCalm => Self::HoverCalm,
            AnimationGroup::HoverCheerful => Self::HoverCheerful,
            AnimationGroup::HoverLively => Self::HoverLively,
            AnimationGroup::WalkRight => Self::WalkRight,
            AnimationGroup::Drag => Self::Drag,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 0]))
    }

    #[test]
    fn accepts_ten_rows_and_four_columns_for_happy_cappy() {
        let sheet = SpriteSheet::from_image(sheet(256, 640), 64).unwrap();
        assert_eq!(sheet.frame_count(), 4);
        assert_eq!(sheet.row_count(), 10);
    }

    #[test]
    fn rejects_dimensions_that_do_not_match_grid() {
        let err = SpriteSheet::from_image(sheet(250, 640), 64).unwrap_err();
        assert!(matches!(err, SpriteError::InvalidDimensions { .. }));
    }

    #[test]
    fn rejects_zero_frame_size() {
        let err = SpriteSheet::from_image(sheet(0, 0), 0).unwrap_err();
        assert!(matches!(err, SpriteError::InvalidDimensions { .. }));
    }

    #[test]
    fn rejects_frame_size_that_overflows_expected_dimensions() {
        let err = SpriteSheet::from_image(sheet(1, 1), u32::MAX).unwrap_err();
        assert!(matches!(err, SpriteError::InvalidDimensions { .. }));
    }

    #[test]
    fn invalid_dimensions_display_includes_actual_expected_and_frame_size() {
        let err = SpriteSheet::from_image(sheet(250, 640), 64).unwrap_err();
        let message = err.to_string();

        assert!(message.contains("actual 250x640"));
        assert!(message.contains("expected 256x640"));
        assert!(message.contains("frame size 64"));
    }

    #[test]
    fn returns_frame_rect_for_state_row_and_index() {
        let sheet = SpriteSheet::from_image(sheet(256, 640), 64).unwrap();
        let rect = sheet.frame_rect(SpriteRow::WalkRight, 2);
        assert_eq!(
            rect,
            FrameRect {
                x: 128,
                y: 8 * 64,
                width: 64,
                height: 64
            }
        );
    }

    #[test]
    fn returns_frame_rect_for_hover_lively_group() {
        let sheet = SpriteSheet::from_image(sheet(256, 640), 64).unwrap();
        let rect = sheet.frame_rect(SpriteRow::HoverLively, 3);
        assert_eq!(
            rect,
            FrameRect {
                x: 192,
                y: 7 * 64,
                width: 64,
                height: 64
            }
        );
    }

    #[test]
    fn frame_rect_wraps_frame_index_at_four_columns() {
        let sheet = SpriteSheet::from_image(sheet(256, 640), 64).unwrap();
        let rect = sheet.frame_rect(SpriteRow::Idle, 5);

        assert_eq!(
            rect,
            FrameRect {
                x: 64,
                y: 0,
                width: 64,
                height: 64
            }
        );
    }

    #[test]
    fn maps_animation_group_to_sprite_row() {
        let cases = [
            (AnimationGroup::Idle, SpriteRow::Idle),
            (AnimationGroup::Blink, SpriteRow::Blink),
            (AnimationGroup::Happy, SpriteRow::Happy),
            (AnimationGroup::Curious, SpriteRow::Curious),
            (AnimationGroup::Sleepy, SpriteRow::Sleepy),
            (AnimationGroup::HoverCalm, SpriteRow::HoverCalm),
            (AnimationGroup::HoverCheerful, SpriteRow::HoverCheerful),
            (AnimationGroup::HoverLively, SpriteRow::HoverLively),
            (AnimationGroup::WalkRight, SpriteRow::WalkRight),
            (AnimationGroup::Drag, SpriteRow::Drag),
        ];

        for (group, row) in cases {
            assert_eq!(SpriteRow::from(group), row);
        }
    }

    use crate::pet::manifest::FrameGeometry;

    fn happy_cappy_geometry() -> FrameGeometry {
        FrameGeometry { width: 64, height: 64, columns: 4, rows: 10 }
    }

    #[test]
    fn frame_rect_by_index_for_zero_returns_top_left() {
        let sheet =
            SpriteSheet::from_image_with_geometry(sheet(256, 640), &happy_cappy_geometry())
                .unwrap();
        assert_eq!(
            sheet.frame_rect_by_index(0),
            FrameRect { x: 0, y: 0, width: 64, height: 64 }
        );
    }

    #[test]
    fn frame_rect_by_index_for_32_returns_walk_row_first_column() {
        let sheet =
            SpriteSheet::from_image_with_geometry(sheet(256, 640), &happy_cappy_geometry())
                .unwrap();
        assert_eq!(
            sheet.frame_rect_by_index(32),
            FrameRect { x: 0, y: 8 * 64, width: 64, height: 64 }
        );
    }

    #[test]
    fn frame_rect_by_index_for_39_returns_drag_row_last_column() {
        let sheet =
            SpriteSheet::from_image_with_geometry(sheet(256, 640), &happy_cappy_geometry())
                .unwrap();
        assert_eq!(
            sheet.frame_rect_by_index(39),
            FrameRect { x: 3 * 64, y: 9 * 64, width: 64, height: 64 }
        );
    }

    #[test]
    fn from_image_with_geometry_rejects_mismatched_dimensions() {
        let err = SpriteSheet::from_image_with_geometry(sheet(250, 640), &happy_cappy_geometry())
            .unwrap_err();
        assert!(matches!(err, SpriteError::InvalidDimensions { .. }));
    }

    #[test]
    fn from_image_with_geometry_accepts_matching_image() {
        let result =
            SpriteSheet::from_image_with_geometry(sheet(256, 640), &happy_cappy_geometry());
        assert!(result.is_ok());
    }
}
