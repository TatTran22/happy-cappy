//! Native AppKit Pet Library picker window.
//!
//! Mirrors the structure of [`crate::settings_window_macos`]: an `NSPanel`
//! created lazily, populated synchronously from `DesktopPetApp`, and
//! dispatching user actions back through [`crate::app::AppCommand`] via
//! [`crate::command_target_macos::CommandTarget`].

#[cfg(not(target_os = "macos"))]
pub struct PickerWindowController;

#[cfg(not(target_os = "macos"))]
impl PickerWindowController {
    pub fn new(_proxy: winit::event_loop::EventLoopProxy<crate::app::AppCommand>) -> Option<Self> {
        None
    }

    pub fn show(&self) {}

    pub fn hide(&self) {}

    pub fn is_visible(&self) -> bool {
        false
    }

    pub fn sync_entries<T>(&self, _entries: Vec<T>, _active_id: &str) {}
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
    use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
    use objc2::AnyThread;
    use objc2::{sel, ClassType, DefinedClass, MainThreadOnly};
    use objc2_app_kit::NSImage;
    use objc2_app_kit::{
        NSBackingStoreType, NSBezelStyle, NSBorderType, NSButton, NSButtonType, NSColor,
        NSControlTextEditingDelegate, NSFloatingWindowLevel, NSFont, NSImageView, NSPanel,
        NSScrollView, NSTableColumn, NSTableView, NSTableViewDataSource, NSTableViewDelegate,
        NSTextField, NSView, NSWindowStyleMask,
    };
    use objc2_core_foundation::CFRetained;
    use objc2_core_foundation::{CFData, CFIndex, CGSize};
    use objc2_core_graphics::{
        CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage,
        CGImageAlphaInfo, CGImageByteOrderInfo,
    };
    use objc2_foundation::{
        ns_string, MainThreadMarker, NSIndexSet, NSInteger, NSObject, NSPoint, NSRect, NSSize,
        NSString, NSTimer, NSUInteger,
    };

    const PANEL_WIDTH: f64 = 480.0;
    const PANEL_HEIGHT: f64 = 420.0;
    const LIST_WIDTH: f64 = 200.0;
    const DETAIL_X: f64 = LIST_WIDTH;
    const DETAIL_WIDTH: f64 = PANEL_WIDTH - LIST_WIDTH;
    const PREVIEW_SIZE: f64 = 128.0;
    const ROW_HEIGHT: f64 = 44.0;

    pub struct PickerWindowController {
        panel: Retained<NSPanel>,
        source: Retained<PickerTableSource>,
        timer: std::cell::RefCell<Option<Retained<NSTimer>>>,
    }

    impl PickerWindowController {
        pub fn new(proxy: EventLoopProxy<AppCommand>) -> Option<Self> {
            let mtm = MainThreadMarker::new()?;
            let source = PickerTableSource::new(mtm, proxy);

            let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
                NSPanel::alloc(mtm),
                NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(PANEL_WIDTH, PANEL_HEIGHT),
                ),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::UtilityWindow,
                NSBackingStoreType::Buffered,
                false,
            );
            unsafe {
                panel.setReleasedWhenClosed(false);
            }
            panel.setTitle(ns_string!("Pet Library"));
            panel.setFloatingPanel(true);
            panel.setHidesOnDeactivate(false);
            panel.setLevel(NSFloatingWindowLevel);

            let content_view = NSView::initWithFrame(
                NSView::alloc(mtm),
                NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(PANEL_WIDTH, PANEL_HEIGHT),
                ),
            );
            panel.setContentView(Some(&content_view));

            // ── Left: scroll view + table view ──────────────────────────────
            let scroll = NSScrollView::initWithFrame(
                NSScrollView::alloc(mtm),
                NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(LIST_WIDTH, PANEL_HEIGHT),
                ),
            );
            scroll.setHasVerticalScroller(true);
            scroll.setBorderType(NSBorderType::NoBorder);

            let table = NSTableView::initWithFrame(
                NSTableView::alloc(mtm),
                NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(LIST_WIDTH, PANEL_HEIGHT),
                ),
            );
            table.setRowHeight(ROW_HEIGHT);
            table.setHeaderView(None);

            let column = NSTableColumn::initWithIdentifier(
                NSTableColumn::alloc(mtm),
                &NSString::from_str("pet"),
            );
            unsafe {
                column.setWidth(LIST_WIDTH - 4.0);
                table.addTableColumn(&column);
                let delegate: &ProtocolObject<dyn NSTableViewDelegate> =
                    ProtocolObject::from_ref(&*source);
                table.setDelegate(Some(delegate));
                let data_source: &ProtocolObject<dyn NSTableViewDataSource> =
                    ProtocolObject::from_ref(&*source);
                table.setDataSource(Some(data_source));
            }
            scroll.setDocumentView(Some(table.as_ref()));
            content_view.addSubview(&scroll);
            *source.ivars().table_view.borrow_mut() = Some(table);

            // ── Right: detail pane ─────────────────────────────────────────
            let detail = NSView::initWithFrame(
                NSView::alloc(mtm),
                NSRect::new(
                    NSPoint::new(DETAIL_X, 0.0),
                    NSSize::new(DETAIL_WIDTH, PANEL_HEIGHT),
                ),
            );
            content_view.addSubview(&detail);

            let preview = NSImageView::initWithFrame(
                NSImageView::alloc(mtm),
                NSRect::new(
                    NSPoint::new(
                        (DETAIL_WIDTH - PREVIEW_SIZE) / 2.0,
                        PANEL_HEIGHT - PREVIEW_SIZE - 24.0,
                    ),
                    NSSize::new(PREVIEW_SIZE, PREVIEW_SIZE),
                ),
            );
            detail.addSubview(&preview);
            *source.ivars().detail_image.borrow_mut() = Some(preview);

            let mut next_y = PANEL_HEIGHT - PREVIEW_SIZE - 60.0;
            let name_field = make_detail_label(mtm, "", &detail, next_y, 20.0, true);
            *source.ivars().detail_name.borrow_mut() = Some(name_field);
            next_y -= 24.0;
            let id_field = make_detail_label(mtm, "", &detail, next_y, 16.0, false);
            *source.ivars().detail_id.borrow_mut() = Some(id_field);
            next_y -= 20.0;
            let source_field = make_detail_label(mtm, "", &detail, next_y, 16.0, false);
            *source.ivars().detail_source.borrow_mut() = Some(source_field);
            next_y -= 20.0;
            let anim_field = make_detail_label(mtm, "", &detail, next_y, 16.0, false);
            *source.ivars().detail_anim.borrow_mut() = Some(anim_field);
            next_y -= 28.0;
            let error_field = make_detail_label(mtm, "", &detail, next_y, 16.0, false);
            {
                let red: Retained<NSColor> = NSColor::redColor();
                error_field.setTextColor(Some(&red));
            }
            *source.ivars().detail_error.borrow_mut() = Some(error_field);

            // Bottom buttons
            let apply = NSButton::initWithFrame(
                NSButton::alloc(mtm),
                NSRect::new(
                    NSPoint::new(DETAIL_WIDTH - 92.0 - 12.0, 12.0),
                    NSSize::new(92.0, 28.0),
                ),
            );
            apply.setTitle(ns_string!("Apply"));
            unsafe {
                apply.setBezelStyle(NSBezelStyle::Push);
                apply.setButtonType(NSButtonType::MomentaryPushIn);
                let target: &AnyObject = source.as_ref();
                apply.setTarget(Some(target));
                apply.setAction(Some(PickerTableSource::apply_selector()));
            }
            detail.addSubview(&apply);
            *source.ivars().apply_button.borrow_mut() = Some(apply);

            let reveal = NSButton::initWithFrame(
                NSButton::alloc(mtm),
                NSRect::new(NSPoint::new(12.0, 12.0), NSSize::new(140.0, 28.0)),
            );
            reveal.setTitle(ns_string!("Reveal in Finder"));
            unsafe {
                reveal.setBezelStyle(NSBezelStyle::Push);
                reveal.setButtonType(NSButtonType::MomentaryPushIn);
                let target: &AnyObject = source.as_ref();
                reveal.setTarget(Some(target));
                reveal.setAction(Some(PickerTableSource::reveal_selector()));
            }
            detail.addSubview(&reveal);
            *source.ivars().reveal_button.borrow_mut() = Some(reveal);

            panel.center();

            *source.ivars().panel.borrow_mut() = Some(panel.clone());

            Some(Self {
                panel,
                source,
                timer: std::cell::RefCell::new(None),
            })
        }

        pub fn show(&self) {
            self.panel.makeKeyAndOrderFront(None);
            self.panel.orderFrontRegardless();
            self.start_animation_timer();
        }

        pub fn hide(&self) {
            self.stop_animation_timer();
            self.panel.orderOut(None);
        }

        fn start_animation_timer(&self) {
            if self.timer.borrow().is_some() {
                return;
            }
            let interval = 0.1_f64; // 10 fps
            let target_obj: &AnyObject = self.source.as_ref();
            let timer: Retained<NSTimer> = unsafe {
                NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                    interval,
                    target_obj,
                    PickerTableSource::tick_selector(),
                    None,
                    true,
                )
            };
            *self.timer.borrow_mut() = Some(timer);
        }

        fn stop_animation_timer(&self) {
            if let Some(timer) = self.timer.borrow_mut().take() {
                timer.invalidate();
            }
        }

        pub fn is_visible(&self) -> bool {
            self.panel.isVisible()
        }

        pub fn sync_entries(&self, entries: Vec<PickerEntry>, active_id: &str) {
            let ivars = self.source.ivars();
            *ivars.active_id.borrow_mut() = active_id.to_string();
            *ivars.entries.borrow_mut() = entries;
            let new_selection = {
                let entries = ivars.entries.borrow();
                if entries.is_empty() {
                    None
                } else {
                    entries
                        .iter()
                        .position(|e| e.base.id == *active_id)
                        .or(Some(0))
                }
            };
            *ivars.selected_index.borrow_mut() = new_selection;
            if let Some(table) = ivars.table_view.borrow().as_ref().cloned() {
                table.reloadData();
                if let Some(row) = new_selection {
                    let index_set = NSIndexSet::indexSetWithIndex(row as NSUInteger);
                    table.selectRowIndexes_byExtendingSelection(&index_set, false);
                }
            }
            self.source.refresh_detail_pane();
        }
    }

    fn make_detail_label(
        mtm: MainThreadMarker,
        text: &str,
        parent: &NSView,
        y: f64,
        height: f64,
        bold: bool,
    ) -> Retained<NSTextField> {
        let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
        field.setFrame(NSRect::new(
            NSPoint::new(16.0, y),
            NSSize::new(DETAIL_WIDTH - 32.0, height),
        ));
        if bold {
            let bold_font = NSFont::boldSystemFontOfSize(18.0);
            field.setFont(Some(&bold_font));
        }
        parent.addSubview(&field);
        field
    }

    #[allow(dead_code)]
    pub(super) struct PickerTableSourceIvars {
        pub proxy: EventLoopProxy<AppCommand>,
        pub entries: RefCell<Vec<PickerEntry>>,
        pub active_id: RefCell<String>,
        pub selected_index: RefCell<Option<usize>>,
        pub frame_counter: RefCell<usize>,
        pub panel: RefCell<Option<Retained<NSPanel>>>,
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
                if let Some(panel) = self.ivars().panel.borrow().clone() {
                    panel.orderOut(None);
                }
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
                panel: RefCell::new(None),
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

        pub(super) fn refresh_detail_pane(&self) {
            let ivars = self.ivars();
            let entries = ivars.entries.borrow();
            let selected = *ivars.selected_index.borrow();
            let active_id = ivars.active_id.borrow().clone();
            let entry = selected.and_then(|i| entries.get(i));
            let Some(entry) = entry else {
                Self::clear_detail_pane(ivars);
                return;
            };
            if let Some(label) = ivars.detail_name.borrow().as_ref() {
                label.setStringValue(&NSString::from_str(&entry.base.display_name));
            }
            if let Some(label) = ivars.detail_id.borrow().as_ref() {
                label.setStringValue(&NSString::from_str(&format!("id: {}", entry.base.id)));
            }
            if let Some(label) = ivars.detail_source.borrow().as_ref() {
                let source = match entry.base.source {
                    PickerSource::Bundled => "bundled".to_string(),
                    PickerSource::Custom => "custom".to_string(),
                };
                let dimensions = if entry.base.frame_width == 0 {
                    "—".to_string()
                } else {
                    format!("{}×{}", entry.base.frame_width, entry.base.frame_height)
                };
                label.setStringValue(&NSString::from_str(&format!("{source} · {dimensions}")));
            }
            if let Some(label) = ivars.detail_anim.borrow().as_ref() {
                let text = if entry.base.animations.is_empty() {
                    "anims: —".to_string()
                } else {
                    format!("anims: {}", entry.base.animations.join(", "))
                };
                label.setStringValue(&NSString::from_str(&text));
            }
            if let Some(label) = ivars.detail_error.borrow().as_ref() {
                let text = entry.base.error.clone().unwrap_or_default();
                label.setStringValue(&NSString::from_str(&text));
            }
            if let Some(button) = ivars.apply_button.borrow().as_ref() {
                let can_apply = entry.base.error.is_none() && entry.base.id != active_id;
                button.setEnabled(can_apply);
            }
            if let Some(button) = ivars.reveal_button.borrow().as_ref() {
                let visible = entry.base.error.is_some();
                button.setHidden(!visible);
            }
            self.refresh_detail_image();
        }

        pub(super) fn refresh_detail_image(&self) {
            let ivars = self.ivars();
            let entries = ivars.entries.borrow();
            let selected = match *ivars.selected_index.borrow() {
                Some(i) => i,
                None => return,
            };
            let Some(entry) = entries.get(selected) else {
                return;
            };
            let Some(image_view) = ivars.detail_image.borrow().as_ref().cloned() else {
                return;
            };
            if entry.frames.is_empty() {
                image_view.setImage(None);
                return;
            }
            let counter = *ivars.frame_counter.borrow();
            let idx = counter % entry.frames.len();
            image_view.setImage(Some(&entry.frames[idx]));
        }

        pub(super) fn refresh_visible_row_images(&self) {
            let ivars = self.ivars();
            let Some(table) = ivars.table_view.borrow().clone() else {
                return;
            };
            let entries = ivars.entries.borrow();
            let counter = *ivars.frame_counter.borrow();
            let visible_range: objc2_foundation::NSRange = unsafe {
                let visible_rect: NSRect = msg_send![&*table, visibleRect];
                msg_send![&*table, rowsInRect: visible_rect]
            };
            for offset in 0..visible_range.length {
                let row = visible_range.location + offset;
                let Some(entry) = entries.get(row as usize) else {
                    continue;
                };
                if entry.frames.is_empty() {
                    continue;
                }
                let idx = counter % entry.frames.len();
                let row_view: Option<Retained<objc2_app_kit::NSView>> = unsafe {
                    msg_send![
                        &*table,
                        viewAtColumn: 0_i64,
                        row: row as i64,
                        makeIfNecessary: false
                    ]
                };
                let Some(row_view) = row_view else { continue };
                // The row view's first NSImageView subview is the thumbnail (see make_row_view).
                let subviews: Retained<objc2_foundation::NSArray<objc2_app_kit::NSView>> =
                    unsafe { msg_send![&*row_view, subviews] };
                if subviews.is_empty() {
                    continue;
                }
                let first = subviews.objectAtIndex(0);
                let is_image: bool = unsafe {
                    msg_send![
                        &*first,
                        isKindOfClass: NSImageView::class()
                    ]
                };
                if !is_image {
                    continue;
                }
                // Safe because `isKindOfClass` confirmed `NSImageView`.
                let image_view: &NSImageView =
                    unsafe { &*(&*first as *const NSView as *const NSImageView) };
                image_view.setImage(Some(&entry.frames[idx]));
            }
        }

        fn clear_detail_pane(ivars: &PickerTableSourceIvars) {
            let set_blank = |field: Option<&Retained<NSTextField>>| {
                if let Some(f) = field {
                    f.setStringValue(&NSString::from_str(""));
                }
            };
            set_blank(ivars.detail_name.borrow().as_ref());
            set_blank(ivars.detail_id.borrow().as_ref());
            set_blank(ivars.detail_source.borrow().as_ref());
            set_blank(ivars.detail_anim.borrow().as_ref());
            set_blank(ivars.detail_error.borrow().as_ref());
            if let Some(image_view) = ivars.detail_image.borrow().as_ref() {
                image_view.setImage(None);
            }
            if let Some(button) = ivars.apply_button.borrow().as_ref() {
                button.setEnabled(false);
            }
            if let Some(button) = ivars.reveal_button.borrow().as_ref() {
                button.setHidden(true);
            }
        }
    }

    fn make_row_view_for_index(entries: &[PickerEntry], idx: usize) -> Option<Retained<NSView>> {
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

    /// Convert a packed RGBA byte buffer into a `CGImage` that owns its pixels.
    ///
    /// `rgba` must have exactly `width * height * 4` bytes (row stride is
    /// `width * 4`). The returned `CGImage` keeps its own copy of the pixel
    /// data, so the caller's `rgba` slice may be freed immediately afterward.
    pub(super) fn rgba_to_cgimage(rgba: &[u8], width: u32, height: u32) -> CFRetained<CGImage> {
        let row_bytes = (width as usize) * 4;
        debug_assert_eq!(rgba.len(), row_bytes * height as usize);

        // Copy the pixels into a `CFData` so the `CGImage` owns its own backing
        // store. The resulting `CGImage` keeps the provider (and therefore the
        // `CFData`) alive for as long as the image lives, so `rgba` is free to
        // be dropped the instant this function returns. Using a borrowed
        // pointer here (e.g. `CGDataProvider::with_data` with no release
        // callback) is a use-after-free: `CGImage` reads pixels lazily at draw
        // time, long after the caller's buffer is gone.
        //
        // SAFETY: `rgba.as_ptr()` is valid for `rgba.len()` bytes, and
        // `CFDataCreate` copies those bytes before returning.
        let data: CFRetained<CFData> =
            unsafe { CFData::new(None, rgba.as_ptr(), rgba.len() as CFIndex) }
                .expect("CFData::new returned null");

        let provider: CFRetained<CGDataProvider> = CGDataProvider::with_cf_data(Some(&data))
            .expect("CGDataProvider::with_cf_data returned null");

        let color_space: CFRetained<CGColorSpace> =
            CGColorSpace::new_device_rgb().expect("CGColorSpace::new_device_rgb returned null");

        // ByteOrderDefault (0) | PremultipliedLast (1) = 1
        let bitmap_info = CGBitmapInfo(
            CGImageByteOrderInfo::OrderDefault.0 | CGImageAlphaInfo::PremultipliedLast.0,
        );

        // SAFETY: `decode` is null (use default), which is explicitly allowed
        // per the CG API contract.
        unsafe {
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
        .expect("CGImage::new returned null")
    }

    /// Convert a packed RGBA byte buffer into a `Retained<NSImage>`.
    ///
    /// `rgba` must have exactly `width * height * 4` bytes (row stride is
    /// `width * 4`). The pixel data is copied into the image, so `rgba` may be
    /// freed as soon as this call returns.
    pub(super) fn rgba_to_nsimage(
        rgba: &[u8],
        width: u32,
        height: u32,
        _mtm: MainThreadMarker,
    ) -> Retained<NSImage> {
        let cg_image = rgba_to_cgimage(rgba, width, height);

        let size = CGSize {
            width: width as f64,
            height: height as f64,
        };

        // NSImage is AnyThread, so alloc() does not require mtm.
        NSImage::initWithCGImage_size(NSImage::alloc(), &cg_image, size)
    }

    use crate::pet::catalog::{CatalogEntry, PetCatalog};
    use crate::picker_entries::{PickerEntryBase, PickerSource};
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
        let mut frames = Vec::with_capacity(animation.frame_count());
        for frame in &animation.frames {
            let rgba = crop_frame_rgba(&sheet, frame.index as usize);
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
    use super::macos::{crop_frame_rgba, rgba_to_cgimage};
    use crate::pet::manifest::FrameGeometry;
    use crate::sprite::SpriteSheet;
    use image::{Rgba, RgbaImage};
    use objc2_core_graphics::{CGDataProvider, CGImage};

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

    /// Read the pixel bytes back out of a `CGImage` via its data provider.
    fn cgimage_bytes(image: &CGImage) -> Vec<u8> {
        let provider = CGImage::data_provider(Some(image)).expect("image has a data provider");
        let data = CGDataProvider::data(Some(&provider)).expect("provider yields data");
        let len = data.length() as usize;
        let ptr = data.byte_ptr();
        assert!(!ptr.is_null());
        // SAFETY: `ptr` points to `len` valid bytes owned by `data`, which is
        // alive for the duration of this borrow.
        unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
    }

    #[test]
    fn cgimage_retains_pixels_after_source_buffer_freed() {
        // A 2×1 RGBA image with four distinct, recognizable pixels.
        let original: Vec<u8> = vec![11, 22, 33, 255, 44, 55, 66, 255];

        // Build the image from a buffer that is dropped before we read back.
        let image = {
            let src = original.clone();
            rgba_to_cgimage(&src, 2, 1)
            // `src` is dropped here — its heap allocation is freed.
        };

        // Aggressively reuse the just-freed allocation so that a use-after-free
        // surfaces as a byte mismatch rather than silently reading stale data.
        let mut clobber: Vec<Vec<u8>> = Vec::new();
        for _ in 0..256 {
            clobber.push(vec![0xABu8; original.len()]);
        }
        std::hint::black_box(&clobber);

        let read_back = cgimage_bytes(&image);
        assert_eq!(
            &read_back[..original.len()],
            &original[..],
            "CGImage must own a copy of the pixels, independent of the source buffer"
        );
    }
}
