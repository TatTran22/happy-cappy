use std::{error::Error, fmt, path::Path};

use image::RgbaImage;

use crate::pet::manifest::FrameGeometry;

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
    geometry: FrameGeometry,
}

impl SpriteSheet {
    pub fn load(path: impl AsRef<Path>, geometry: &FrameGeometry) -> Result<Self, SpriteError> {
        let image = image::open(path)?.into_rgba8();
        Self::from_image(image, geometry)
    }

    pub fn from_image(image: RgbaImage, geometry: &FrameGeometry) -> Result<Self, SpriteError> {
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
        Ok(Self {
            image,
            geometry: *geometry,
        })
    }

    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    pub fn geometry(&self) -> &FrameGeometry {
        &self.geometry
    }

    pub fn frame_rect(&self, sprite_index: u32) -> FrameRect {
        let row = sprite_index / self.geometry.columns;
        let col = sprite_index % self.geometry.columns;
        FrameRect {
            x: col * self.geometry.width,
            y: row * self.geometry.height,
            width: self.geometry.width,
            height: self.geometry.height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::pet::manifest::FrameGeometry;

    fn sheet(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 0]))
    }

    fn happy_cappy_geometry() -> FrameGeometry {
        FrameGeometry {
            width: 64,
            height: 64,
            columns: 4,
            rows: 10,
        }
    }

    #[test]
    fn frame_rect_for_zero_returns_top_left() {
        let sheet = SpriteSheet::from_image(sheet(256, 640), &happy_cappy_geometry()).unwrap();
        assert_eq!(
            sheet.frame_rect(0),
            FrameRect {
                x: 0,
                y: 0,
                width: 64,
                height: 64
            }
        );
    }

    #[test]
    fn frame_rect_for_32_returns_walk_row_first_column() {
        let sheet = SpriteSheet::from_image(sheet(256, 640), &happy_cappy_geometry()).unwrap();
        assert_eq!(
            sheet.frame_rect(32),
            FrameRect {
                x: 0,
                y: 8 * 64,
                width: 64,
                height: 64
            }
        );
    }

    #[test]
    fn frame_rect_for_39_returns_drag_row_last_column() {
        let sheet = SpriteSheet::from_image(sheet(256, 640), &happy_cappy_geometry()).unwrap();
        assert_eq!(
            sheet.frame_rect(39),
            FrameRect {
                x: 3 * 64,
                y: 9 * 64,
                width: 64,
                height: 64
            }
        );
    }

    #[test]
    fn from_image_rejects_mismatched_dimensions() {
        let err = SpriteSheet::from_image(sheet(250, 640), &happy_cappy_geometry()).unwrap_err();
        assert!(matches!(err, SpriteError::InvalidDimensions { .. }));
    }

    #[test]
    fn from_image_accepts_matching_image() {
        let result = SpriteSheet::from_image(sheet(256, 640), &happy_cappy_geometry());
        assert!(result.is_ok());
    }
}
