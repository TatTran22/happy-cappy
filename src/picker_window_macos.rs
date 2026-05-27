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
