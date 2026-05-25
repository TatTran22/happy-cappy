use std::path::Path;

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
    InvalidDimensions { width: u32, height: u32 },
}

impl From<image::ImageError> for SpriteError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
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
        let expected_width = frame_size * EXPECTED_COLUMNS;
        let expected_height = frame_size * EXPECTED_ROWS;

        if width != expected_width || height != expected_height {
            return Err(SpriteError::InvalidDimensions { width, height });
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
        let column = (frame_index as u32) % EXPECTED_COLUMNS;

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
}
