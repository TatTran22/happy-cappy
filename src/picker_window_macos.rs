//! Native AppKit Pet Library picker window.
//!
//! Mirrors the structure of [`crate::settings_window_macos`]: an `NSPanel`
//! created lazily, populated synchronously from `DesktopPetApp`, and
//! dispatching user actions back through [`crate::app::AppCommand`] via
//! [`crate::command_target_macos::CommandTarget`].

#[cfg(not(target_os = "macos"))]
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
    use std::cell::RefCell;

    use winit::event_loop::EventLoopProxy;

    use crate::app::AppCommand;
    use crate::sprite::SpriteSheet;

    use objc2::define_class;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObjectProtocol};
    use objc2::{sel, DefinedClass, MainThreadOnly};
    use objc2::AnyThread;
    use objc2_app_kit::{
        NSButton, NSControlTextEditingDelegate, NSImageView, NSTableView, NSTableViewDataSource,
        NSTableViewDelegate, NSTextField, NSView,
    };
    use objc2_app_kit::NSImage;
    use objc2_core_foundation::CFRetained;
    use objc2_core_graphics::{
        CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage,
        CGImageAlphaInfo, CGImageByteOrderInfo,
    };
    use objc2_core_foundation::CGSize;
    use objc2_foundation::{MainThreadMarker, NSInteger, NSObject, NSString};

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

    #[allow(dead_code)]
    pub(super) struct PickerTableSourceIvars {
        pub proxy: EventLoopProxy<AppCommand>,
        pub entries: RefCell<Vec<PickerEntry>>,
        pub active_id: RefCell<String>,
        pub selected_index: RefCell<Option<usize>>,
        pub frame_counter: RefCell<usize>,
        pub table_view: RefCell<Option<Retained<NSTableView>>>,
        pub detail_image: RefCell<Option<Retained<NSImageView>>>,
        pub detail_name: RefCell<Option<Retained<NSTextField>>>,
        pub detail_id: RefCell<Option<Retained<NSTextField>>>,
        pub detail_source: RefCell<Option<Retained<NSTextField>>>,
        pub detail_anim: RefCell<Option<Retained<NSTextField>>>,
        pub detail_error: RefCell<Option<Retained<NSTextField>>>,
        pub apply_button: RefCell<Option<Retained<NSButton>>>,
        pub reveal_button: RefCell<Option<Retained<NSButton>>>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "HappyCappyPickerTableSource"]
        #[thread_kind = MainThreadOnly]
        #[ivars = PickerTableSourceIvars]
        pub(super) struct PickerTableSource;

        unsafe impl NSObjectProtocol for PickerTableSource {}

        unsafe impl NSTableViewDataSource for PickerTableSource {
            #[unsafe(method(numberOfRowsInTableView:))]
            fn number_of_rows(&self, _table_view: &NSTableView) -> NSInteger {
                self.ivars().entries.borrow().len() as NSInteger
            }
        }

        unsafe impl NSControlTextEditingDelegate for PickerTableSource {}

        unsafe impl NSTableViewDelegate for PickerTableSource {
            #[unsafe(method_id(tableView:viewForTableColumn:row:))]
            fn view_for_row(
                &self,
                _table_view: &NSTableView,
                _column: Option<&AnyObject>,
                row: NSInteger,
            ) -> Option<Retained<NSView>> {
                make_row_view_for_index(&self.ivars().entries.borrow(), row as usize)
            }

            #[unsafe(method(tableViewSelectionDidChange:))]
            fn selection_changed(&self, _notification: &AnyObject) {
                let table = match self.ivars().table_view.borrow().clone() {
                    Some(t) => t,
                    None => return,
                };
                let selected: NSInteger = unsafe { msg_send![&*table, selectedRow] };
                *self.ivars().selected_index.borrow_mut() = if selected < 0 {
                    None
                } else {
                    Some(selected as usize)
                };
                self.refresh_detail_pane();
            }
        }

        impl PickerTableSource {
            #[unsafe(method(tickPreviewAnimation:))]
            fn tick_preview_animation(&self, _timer: &AnyObject) {
                let next = self.ivars().frame_counter.borrow().wrapping_add(1);
                *self.ivars().frame_counter.borrow_mut() = next;
                self.refresh_visible_row_images();
                self.refresh_detail_image();
            }

            #[unsafe(method(onApplyClicked:))]
            fn on_apply_clicked(&self, _sender: &AnyObject) {
                let entries = self.ivars().entries.borrow();
                let Some(idx) = *self.ivars().selected_index.borrow() else {
                    return;
                };
                let Some(entry) = entries.get(idx) else {
                    return;
                };
                if entry.base.error.is_some() {
                    return;
                }
                if entry.base.id == *self.ivars().active_id.borrow() {
                    return;
                }
                let _ = self
                    .ivars()
                    .proxy
                    .send_event(AppCommand::ActivatePet(entry.base.id.clone()));
            }

            #[unsafe(method(onRevealClicked:))]
            fn on_reveal_clicked(&self, _sender: &AnyObject) {
                let _ = self
                    .ivars()
                    .proxy
                    .send_event(AppCommand::RevealPetsFolder);
            }
        }
    );

    impl PickerTableSource {
        #[allow(dead_code)]
        pub(super) fn new(
            mtm: MainThreadMarker,
            proxy: EventLoopProxy<AppCommand>,
        ) -> Retained<Self> {
            let ivars = PickerTableSourceIvars {
                proxy,
                entries: RefCell::new(Vec::new()),
                active_id: RefCell::new(String::new()),
                selected_index: RefCell::new(None),
                frame_counter: RefCell::new(0),
                table_view: RefCell::new(None),
                detail_image: RefCell::new(None),
                detail_name: RefCell::new(None),
                detail_id: RefCell::new(None),
                detail_source: RefCell::new(None),
                detail_anim: RefCell::new(None),
                detail_error: RefCell::new(None),
                apply_button: RefCell::new(None),
                reveal_button: RefCell::new(None),
            };
            let this = mtm.alloc().set_ivars(ivars);
            unsafe { msg_send![super(this), init] }
        }

        #[allow(dead_code)]
        pub(super) fn tick_selector() -> objc2::runtime::Sel {
            sel!(tickPreviewAnimation:)
        }

        #[allow(dead_code)]
        pub(super) fn apply_selector() -> objc2::runtime::Sel {
            sel!(onApplyClicked:)
        }

        #[allow(dead_code)]
        pub(super) fn reveal_selector() -> objc2::runtime::Sel {
            sel!(onRevealClicked:)
        }

        #[allow(dead_code)]
        pub(super) fn refresh_detail_pane(&self) {
            // Stub — populated in Task 16.
        }

        #[allow(dead_code)]
        pub(super) fn refresh_detail_image(&self) {
            // Stub — populated in Task 16.
        }

        #[allow(dead_code)]
        pub(super) fn refresh_visible_row_images(&self) {
            // Stub — populated in Task 16.
        }
    }

    fn make_row_view_for_index(
        entries: &[PickerEntry],
        idx: usize,
    ) -> Option<Retained<NSView>> {
        let entry = entries.get(idx)?;
        let mtm = MainThreadMarker::new()?;
        Some(make_row_view(entry, mtm))
    }

    fn make_row_view(entry: &PickerEntry, mtm: MainThreadMarker) -> Retained<NSView> {
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(200.0, 44.0));
        let row = NSView::initWithFrame(NSView::alloc(mtm), frame);
        // Thumbnail (left)
        let thumb = NSImageView::initWithFrame(
            NSImageView::alloc(mtm),
            NSRect::new(NSPoint::new(8.0, 6.0), NSSize::new(32.0, 32.0)),
        );
        if let Some(image) = entry.frames.first() {
            thumb.setImage(Some(image));
        }
        row.addSubview(&thumb);
        // Title label (right of thumbnail)
        let title_text = if entry.base.error.is_some() {
            format!("\u{26a0} {}", entry.base.display_name)
        } else {
            entry.base.display_name.clone()
        };
        let title_field = NSTextField::labelWithString(&NSString::from_str(&title_text), mtm);
        title_field.setFrame(NSRect::new(
            NSPoint::new(48.0, 12.0),
            NSSize::new(140.0, 20.0),
        ));
        row.addSubview(&title_field);
        row
    }

    /// Copy the pixel rectangle for frame `index` out of `sheet.image()` into a
    /// freshly-allocated packed RGBA buffer. Width and height match the frame
    /// geometry; row stride is `width * 4`.
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

    use crate::pet::catalog::{CatalogEntry, PetCatalog};
    use crate::picker_entries::PickerEntryBase;
    use crate::sprite::SpriteError;

    #[derive(Debug)]
    pub enum PreviewBuildError {
        Sprite(SpriteError),
        NoAnimation,
    }

    impl From<SpriteError> for PreviewBuildError {
        fn from(error: SpriteError) -> Self {
            Self::Sprite(error)
        }
    }

    impl std::fmt::Display for PreviewBuildError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Sprite(err) => write!(f, "sprite decode failed: {err}"),
                Self::NoAnimation => write!(f, "no animations defined in manifest"),
            }
        }
    }

    /// Decode the idle animation's frames for one catalog entry into a list of
    /// `NSImage`. Falls back to the first defined animation if `idle` is
    /// missing (this matches the picker's display intent — show *some* motion).
    #[allow(dead_code)]
    pub(super) fn build_preview_frames(
        entry: &CatalogEntry,
        mtm: MainThreadMarker,
    ) -> Result<Vec<Retained<NSImage>>, PreviewBuildError> {
        let sheet = SpriteSheet::load(&entry.spritesheet_path, &entry.manifest.frame)?;
        let animation = entry
            .manifest
            .animations
            .get("idle")
            .or_else(|| entry.manifest.animations.values().next())
            .ok_or(PreviewBuildError::NoAnimation)?;
        let geometry = sheet.geometry();
        let mut frames = Vec::with_capacity(animation.frames.len());
        for &index in &animation.frames {
            let rgba = crop_frame_rgba(&sheet, index as usize);
            let image = rgba_to_nsimage(&rgba, geometry.width, geometry.height, mtm);
            frames.push(image);
        }
        Ok(frames)
    }

    /// Full AppKit-side picker entry: pure base data + decoded NSImage frames.
    #[derive(Clone)]
    pub struct PickerEntry {
        pub base: PickerEntryBase,
        pub frames: Vec<Retained<NSImage>>,
    }

    /// Walk `entries`, decode preview frames for OK rows, and surface decode
    /// failures as additional errors on the row (frames stays empty).
    pub fn attach_preview_frames(
        entries: Vec<PickerEntryBase>,
        catalog: &PetCatalog,
        mtm: MainThreadMarker,
    ) -> Vec<PickerEntry> {
        entries
            .into_iter()
            .map(|mut base| {
                if base.error.is_some() {
                    return PickerEntry {
                        base,
                        frames: Vec::new(),
                    };
                }
                let Some(catalog_entry) = catalog.lookup(&base.id) else {
                    base.error = Some("Catalog entry missing for picker row".to_string());
                    return PickerEntry {
                        base,
                        frames: Vec::new(),
                    };
                };
                match build_preview_frames(catalog_entry, mtm) {
                    Ok(frames) => PickerEntry { base, frames },
                    Err(err) => {
                        base.error = Some(format!("Couldn't decode preview: {err}"));
                        PickerEntry {
                            base,
                            frames: Vec::new(),
                        }
                    }
                }
            })
            .collect()
    }
}

#[cfg(target_os = "macos")]
pub use macos::PickerWindowController;

#[cfg(target_os = "macos")]
pub use macos::{attach_preview_frames, PickerEntry, PreviewBuildError};

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
