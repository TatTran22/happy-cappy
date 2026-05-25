use std::sync::Arc;

use image::RgbaImage;
use pixels::{wgpu, Pixels, PixelsBuilder, SurfaceTexture};
use winit::window::Window;

use crate::sprite::FrameRect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlitOptions {
    pub dest_x: i32,
    pub dest_y: i32,
    pub flip_x: bool,
}

pub struct PetRenderer {
    pixels: Pixels<'static>,
    buffer_width: u32,
    buffer_height: u32,
}

impl PetRenderer {
    pub fn new(
        window: Arc<Window>,
        surface_width: u32,
        surface_height: u32,
        buffer_width: u32,
        buffer_height: u32,
    ) -> Result<Self, pixels::Error> {
        let surface_texture = SurfaceTexture::new(surface_width, surface_height, window);
        let pixels = PixelsBuilder::new(buffer_width, buffer_height, surface_texture)
            .alpha_mode(wgpu::CompositeAlphaMode::PostMultiplied)
            .blend_state(wgpu::BlendState::REPLACE)
            .clear_color(wgpu::Color::TRANSPARENT)
            .build()?;

        Ok(Self {
            pixels,
            buffer_width,
            buffer_height,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), pixels::TextureError> {
        self.pixels.resize_surface(width, height)
    }

    pub fn draw(
        &mut self,
        sprite_sheet: &RgbaImage,
        rect: FrameRect,
        flip_x: bool,
    ) -> Result<(), pixels::Error> {
        let frame = self.pixels.frame_mut();
        clear_rgba(frame);
        blit_frame(
            sprite_sheet,
            rect,
            frame,
            self.buffer_width,
            self.buffer_height,
            BlitOptions {
                dest_x: 0,
                dest_y: 0,
                flip_x,
            },
        );
        self.pixels.render()
    }
}

pub fn clear_rgba(frame: &mut [u8]) {
    frame.fill(0);
}

pub fn blit_frame(
    source: &RgbaImage,
    rect: FrameRect,
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    options: BlitOptions,
) {
    let Some(required_frame_len) = frame_len(frame_width, frame_height) else {
        return;
    };
    if frame.len() < required_frame_len {
        return;
    }

    let frame_width_i64 = i64::from(frame_width);
    let frame_height_i64 = i64::from(frame_height);

    for local_y in 0..rect.height {
        let dest_y = i64::from(options.dest_y) + i64::from(local_y);
        if dest_y < 0 || dest_y >= frame_height_i64 {
            continue;
        }

        for local_x in 0..rect.width {
            let dest_x = i64::from(options.dest_x) + i64::from(local_x);
            if dest_x < 0 || dest_x >= frame_width_i64 {
                continue;
            }

            let Some(source_x) = source_x(rect, local_x, options.flip_x) else {
                continue;
            };
            let Some(source_y) = rect.y.checked_add(local_y) else {
                continue;
            };
            if source_x >= source.width() || source_y >= source.height() {
                continue;
            }

            let src = source.get_pixel(source_x, source_y).0;
            if src[3] == 0 {
                continue;
            }

            let Some(offset) = pixel_offset(dest_x, dest_y, frame_width) else {
                continue;
            };
            alpha_blend_pixel(src, &mut frame[offset..offset + 4]);
        }
    }
}

fn frame_len(frame_width: u32, frame_height: u32) -> Option<usize> {
    let pixels = usize::try_from(frame_width)
        .ok()?
        .checked_mul(usize::try_from(frame_height).ok()?)?;
    pixels.checked_mul(4)
}

fn source_x(rect: FrameRect, local_x: u32, flip_x: bool) -> Option<u32> {
    if flip_x {
        rect.x
            .checked_add(rect.width)?
            .checked_sub(1)?
            .checked_sub(local_x)
    } else {
        rect.x.checked_add(local_x)
    }
}

fn pixel_offset(dest_x: i64, dest_y: i64, frame_width: u32) -> Option<usize> {
    let dest_x = usize::try_from(dest_x).ok()?;
    let dest_y = usize::try_from(dest_y).ok()?;
    let frame_width = usize::try_from(frame_width).ok()?;
    let row_offset = dest_y.checked_mul(frame_width)?;
    let pixel_index = row_offset.checked_add(dest_x)?;
    pixel_index.checked_mul(4)
}

fn alpha_blend_pixel(src: [u8; 4], dst: &mut [u8]) {
    let src_alpha = src[3] as f32 / 255.0;
    let dst_alpha = dst[3] as f32 / 255.0;
    let out_alpha = src_alpha + dst_alpha * (1.0 - src_alpha);

    if out_alpha <= f32::EPSILON {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }

    for channel in 0..3 {
        let src_channel = src[channel] as f32 / 255.0;
        let dst_channel = dst[channel] as f32 / 255.0;
        let out =
            (src_channel * src_alpha + dst_channel * dst_alpha * (1.0 - src_alpha)) / out_alpha;
        dst[channel] = (out * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    dst[3] = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    #[test]
    fn clear_sets_every_byte_to_zero() {
        let mut frame = vec![255; 16];
        clear_rgba(&mut frame);
        assert_eq!(frame, vec![0; 16]);
    }

    #[test]
    fn blit_copies_opaque_source_pixel() {
        let source = RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 255]));
        let mut frame = vec![0; 4 * 2 * 2];
        blit_frame(
            &source,
            FrameRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            &mut frame,
            2,
            2,
            BlitOptions {
                dest_x: 1,
                dest_y: 1,
                flip_x: false,
            },
        );
        assert_eq!(&frame[12..16], &[10, 20, 30, 255]);
    }

    #[test]
    fn blit_alpha_blends_translucent_source_pixel() {
        let source = RgbaImage::from_pixel(1, 1, image::Rgba([100, 0, 0, 128]));
        let mut frame = vec![0; 4];
        frame.copy_from_slice(&[0, 0, 100, 255]);
        blit_frame(
            &source,
            FrameRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            &mut frame,
            1,
            1,
            BlitOptions {
                dest_x: 0,
                dest_y: 0,
                flip_x: false,
            },
        );
        assert_eq!(frame[3], 255);
        assert!(frame[0] >= 49 && frame[0] <= 51);
        assert!(frame[2] >= 49 && frame[2] <= 51);
    }

    #[test]
    fn blit_flip_x_mirrors_source_columns() {
        let mut source = RgbaImage::new(2, 1);
        source.put_pixel(0, 0, image::Rgba([10, 0, 0, 255]));
        source.put_pixel(1, 0, image::Rgba([20, 0, 0, 255]));
        let mut frame = vec![0; 4 * 2];

        blit_frame(
            &source,
            FrameRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
            &mut frame,
            2,
            1,
            BlitOptions {
                dest_x: 0,
                dest_y: 0,
                flip_x: true,
            },
        );

        assert_eq!(&frame[0..4], &[20, 0, 0, 255]);
        assert_eq!(&frame[4..8], &[10, 0, 0, 255]);
    }

    #[test]
    fn blit_clips_negative_destination_without_panicking() {
        let mut source = RgbaImage::new(2, 1);
        source.put_pixel(0, 0, image::Rgba([10, 0, 0, 255]));
        source.put_pixel(1, 0, image::Rgba([20, 0, 0, 255]));
        let mut frame = vec![0; 4];

        blit_frame(
            &source,
            FrameRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
            &mut frame,
            1,
            1,
            BlitOptions {
                dest_x: -1,
                dest_y: 0,
                flip_x: false,
            },
        );

        assert_eq!(frame, vec![20, 0, 0, 255]);
    }

    #[test]
    fn blit_skips_fully_transparent_source_pixels() {
        let source = RgbaImage::from_pixel(1, 1, image::Rgba([100, 0, 0, 0]));
        let mut frame = vec![1, 2, 3, 4];

        blit_frame(
            &source,
            FrameRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            &mut frame,
            1,
            1,
            BlitOptions {
                dest_x: 0,
                dest_y: 0,
                flip_x: false,
            },
        );

        assert_eq!(frame, vec![1, 2, 3, 4]);
    }

    #[test]
    fn blit_short_frame_buffer_returns_without_panicking() {
        let source = RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 255]));
        let mut frame = vec![1, 2, 3];

        let result = catch_unwind(AssertUnwindSafe(|| {
            blit_frame(
                &source,
                FrameRect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                &mut frame,
                1,
                1,
                BlitOptions {
                    dest_x: 0,
                    dest_y: 0,
                    flip_x: false,
                },
            );
        }));

        assert!(result.is_ok());
        assert_eq!(frame, vec![1, 2, 3]);
    }

    #[test]
    fn blit_skips_source_pixels_outside_source_bounds() {
        let source = RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 255]));
        let mut frame = vec![0; 4 * 2];

        let result = catch_unwind(AssertUnwindSafe(|| {
            blit_frame(
                &source,
                FrameRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 1,
                },
                &mut frame,
                2,
                1,
                BlitOptions {
                    dest_x: 0,
                    dest_y: 0,
                    flip_x: false,
                },
            );
        }));

        assert!(result.is_ok());
        assert_eq!(&frame[0..4], &[10, 20, 30, 255]);
        assert_eq!(&frame[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn blit_alpha_blends_onto_transparent_destination() {
        let source = RgbaImage::from_pixel(1, 1, image::Rgba([100, 50, 0, 128]));
        let mut frame = vec![0, 0, 0, 0];

        blit_frame(
            &source,
            FrameRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            &mut frame,
            1,
            1,
            BlitOptions {
                dest_x: 0,
                dest_y: 0,
                flip_x: false,
            },
        );

        assert_eq!(frame, vec![100, 50, 0, 128]);
    }

    #[test]
    fn blit_alpha_blends_with_partially_transparent_destination() {
        let source = RgbaImage::from_pixel(1, 1, image::Rgba([100, 0, 0, 128]));
        let mut frame = vec![0, 0, 100, 128];

        blit_frame(
            &source,
            FrameRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            &mut frame,
            1,
            1,
            BlitOptions {
                dest_x: 0,
                dest_y: 0,
                flip_x: false,
            },
        );

        assert_eq!(frame[3], 192);
        assert!(frame[0] >= 66 && frame[0] <= 67);
        assert!(frame[2] >= 33 && frame[2] <= 34);
    }
}
