use image::RgbaImage;

use crate::sprite::FrameRect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlitOptions {
    pub dest_x: i32,
    pub dest_y: i32,
    pub flip_x: bool,
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
    for local_y in 0..rect.height {
        let dest_y = options.dest_y + local_y as i32;
        if dest_y < 0 || dest_y >= frame_height as i32 {
            continue;
        }

        for local_x in 0..rect.width {
            let dest_x = options.dest_x + local_x as i32;
            if dest_x < 0 || dest_x >= frame_width as i32 {
                continue;
            }

            let source_x = if options.flip_x {
                rect.x + rect.width - 1 - local_x
            } else {
                rect.x + local_x
            };
            let source_y = rect.y + local_y;
            let src = source.get_pixel(source_x, source_y).0;
            if src[3] == 0 {
                continue;
            }

            let offset = ((dest_y as u32 * frame_width + dest_x as u32) * 4) as usize;
            alpha_blend_pixel(src, &mut frame[offset..offset + 4]);
        }
    }
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
}
