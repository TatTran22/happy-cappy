//! Native AppKit Pet Library picker window.
//!
//! Mirrors the structure of [`crate::settings_window_macos`]: an `NSPanel`
//! created lazily, populated synchronously from `DesktopPetApp`, and
//! dispatching user actions back through [`crate::app::AppCommand`] via
//! [`crate::command_target_macos::CommandTarget`].

use crate::picker_entries::PickerEntryBase;

#[cfg(not(target_os = "macos"))]
pub struct PickerWindowController;

#[cfg(not(target_os = "macos"))]
impl PickerWindowController {
    pub fn new(
        _proxy: winit::event_loop::EventLoopProxy<crate::app::AppCommand>,
    ) -> Option<Self> {
        None
    }

    pub fn show(&self) {}

    pub fn hide(&self) {}

    pub fn is_visible(&self) -> bool {
        false
    }

    pub fn sync_entries(&self, _entries: Vec<PickerEntryBase>, _active_id: &str) {}
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use winit::event_loop::EventLoopProxy;

    use crate::app::AppCommand;
    use crate::sprite::SpriteSheet;

    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_app_kit::NSImage;
    use objc2_core_foundation::CFRetained;
    use objc2_core_graphics::{
        CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage,
        CGImageAlphaInfo, CGImageByteOrderInfo,
    };
    use objc2_core_foundation::CGSize;
    use objc2_foundation::MainThreadMarker;

    /// Placeholder — the real implementation arrives in subsequent tasks.
    pub struct PickerWindowController;

    impl PickerWindowController {
        pub fn new(_proxy: EventLoopProxy<AppCommand>) -> Option<Self> {
            None
        }

        pub fn show(&self) {}
        pub fn hide(&self) {}
        pub fn is_visible(&self) -> bool {
            false
        }

        pub fn sync_entries(&self, _entries: Vec<PickerEntryBase>, _active_id: &str) {}
    }

    /// Copy the pixel rectangle for frame `index` out of `sheet.image()` into a
    /// freshly-allocated packed RGBA buffer. Width and height match the frame
    /// geometry; row stride is `width * 4`.
    #[allow(dead_code)]
    pub(super) fn crop_frame_rgba(sheet: &SpriteSheet, index: usize) -> Vec<u8> {
        let rect = sheet.frame_rect(index as u32);
        let image = sheet.image();
        let mut out = Vec::with_capacity((rect.width * rect.height * 4) as usize);
        for y in rect.y..(rect.y + rect.height) {
            for x in rect.x..(rect.x + rect.width) {
                let pixel = image.get_pixel(x, y);
                out.extend_from_slice(&pixel.0);
            }
        }
        out
    }

    /// Convert a packed RGBA byte buffer into a `Retained<NSImage>`.
    ///
    /// `rgba` must have exactly `width * height * 4` bytes (row stride is
    /// `width * 4`). The buffer is borrowed only for the duration of this
    /// call; Core Graphics copies the pixel data before the function returns.
    #[allow(dead_code)]
    pub(super) fn rgba_to_nsimage(
        rgba: &[u8],
        width: u32,
        height: u32,
        _mtm: MainThreadMarker,
    ) -> Retained<NSImage> {
        let row_bytes = (width as usize) * 4;
        debug_assert_eq!(rgba.len(), row_bytes * height as usize);

        // SAFETY: `rgba` is valid for `rgba.len()` bytes. We pass `None` for
        // the release callback because `CGDataProvider` won't outlive the call
        // site — the `CGImage` (and hence the provider) is consumed when
        // `initWithCGImage_size` copies the pixels into NSImage.
        let provider: CFRetained<CGDataProvider> = unsafe {
            CGDataProvider::with_data(
                std::ptr::null_mut(),
                rgba.as_ptr().cast(),
                rgba.len(),
                None,
            )
        }
        .expect("CGDataProvider::with_data returned null");

        let color_space: CFRetained<CGColorSpace> =
            CGColorSpace::new_device_rgb().expect("CGColorSpace::new_device_rgb returned null");

        // ByteOrderDefault (0) | PremultipliedLast (1) = 1
        let bitmap_info = CGBitmapInfo(
            CGImageByteOrderInfo::OrderDefault.0 | CGImageAlphaInfo::PremultipliedLast.0,
        );

        // SAFETY: `decode` is null (use default), which is explicitly allowed
        // per the CG API contract.
        let cg_image: CFRetained<CGImage> = unsafe {
            CGImage::new(
                width as usize,
                height as usize,
                8,
                32,
                row_bytes,
                Some(&color_space),
                bitmap_info,
                Some(&provider),
                std::ptr::null(),
                false,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
        }
        .expect("CGImage::new returned null");

        let size = CGSize {
            width: width as f64,
            height: height as f64,
        };

        // NSImage is AnyThread, so alloc() does not require mtm.
        NSImage::initWithCGImage_size(NSImage::alloc(), &cg_image, size)
    }
}

#[cfg(target_os = "macos")]
pub use macos::PickerWindowController;

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::macos::crop_frame_rgba;
    use crate::pet::manifest::FrameGeometry;
    use crate::sprite::SpriteSheet;
    use image::{Rgba, RgbaImage};

    fn checkered_sheet() -> SpriteSheet {
        // 2 frames wide × 1 frame tall, each frame 2×2 px.
        let mut img = RgbaImage::new(4, 2);
        img.put_pixel(0, 0, Rgba([10, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([20, 0, 0, 255]));
        img.put_pixel(0, 1, Rgba([30, 0, 0, 255]));
        img.put_pixel(1, 1, Rgba([40, 0, 0, 255]));
        img.put_pixel(2, 0, Rgba([50, 0, 0, 255]));
        img.put_pixel(3, 0, Rgba([60, 0, 0, 255]));
        img.put_pixel(2, 1, Rgba([70, 0, 0, 255]));
        img.put_pixel(3, 1, Rgba([80, 0, 0, 255]));
        let geometry = FrameGeometry {
            width: 2,
            height: 2,
            columns: 2,
            rows: 1,
        };
        SpriteSheet::from_image(img, &geometry).unwrap()
    }

    #[test]
    fn crop_first_frame_returns_top_left_pixels() {
        let sheet = checkered_sheet();
        let rgba = crop_frame_rgba(&sheet, 0);
        assert_eq!(rgba.len(), 2 * 2 * 4);
        assert_eq!(rgba[0], 10);
        assert_eq!(rgba[4], 20);
        assert_eq!(rgba[8], 30);
        assert_eq!(rgba[12], 40);
    }

    #[test]
    fn crop_second_frame_returns_top_right_pixels() {
        let sheet = checkered_sheet();
        let rgba = crop_frame_rgba(&sheet, 1);
        assert_eq!(rgba[0], 50);
        assert_eq!(rgba[4], 60);
        assert_eq!(rgba[8], 70);
        assert_eq!(rgba[12], 80);
    }
}
