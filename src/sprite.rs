use std::{error::Error, fmt, path::Path};

use image::RgbaImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpriteRow {
    Idle,
    WalkRight,
    Sleep,
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
const EXPECTED_ROWS: u32 = 3;

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
            SpriteRow::WalkRight => 1,
            SpriteRow::Sleep => 2,
        };
        let column = (frame_index % EXPECTED_COLUMNS as usize) as u32;

        FrameRect {
            x: column * self.frame_size,
            y: row_index * self.frame_size,
            width: self.frame_size,
            height: self.frame_size,
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
    fn accepts_three_rows_and_four_columns() {
        let sheet = SpriteSheet::from_image(sheet(256, 192), 64).unwrap();
        assert_eq!(sheet.frame_count(), 4);
        assert_eq!(sheet.row_count(), 3);
    }

    #[test]
    fn rejects_dimensions_that_do_not_match_grid() {
        let err = SpriteSheet::from_image(sheet(250, 192), 64).unwrap_err();
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
        let err = SpriteSheet::from_image(sheet(250, 192), 64).unwrap_err();
        let message = err.to_string();

        assert!(message.contains("actual 250x192"));
        assert!(message.contains("expected 256x192"));
        assert!(message.contains("frame size 64"));
    }

    #[test]
    fn returns_frame_rect_for_state_row_and_index() {
        let sheet = SpriteSheet::from_image(sheet(256, 192), 64).unwrap();
        let rect = sheet.frame_rect(SpriteRow::WalkRight, 2);
        assert_eq!(
            rect,
            FrameRect {
                x: 128,
                y: 64,
                width: 64,
                height: 64
            }
        );
    }

    #[test]
    fn frame_rect_wraps_frame_index_at_four_columns() {
        let sheet = SpriteSheet::from_image(sheet(256, 192), 64).unwrap();
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
}
