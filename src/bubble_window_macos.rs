//! Native AppKit speech-bubble window (SP4-C). A borderless, transparent,
//! click-through child window drawn above the pet. The placement math lives in
//! `crate::bubble_layout` (pure Rust, Y-down logical); this layer only draws,
//! measures text, and converts to AppKit (Y-up) at the boundary.

#[cfg(not(target_os = "macos"))]
pub struct BubbleWindow;

#[cfg(not(target_os = "macos"))]
impl BubbleWindow {
    pub fn new(_parent: &winit::window::Window) -> Option<Self> {
        None
    }
    pub fn update(
        &self,
        _content: &crate::bubble::BubbleContent,
        _pet_rect: crate::physics::Rect,
        _visible: crate::physics::Rect,
    ) {
    }
    pub fn hide(&self) {}
}

/// Outer bubble width cap (points). Text column = this minus padding/dot/gap.
pub const MAX_WIDTH: f64 = 240.0;

#[cfg(not(target_os = "macos"))]
pub fn active_visible_frame_y_down(
    _window: &winit::window::Window,
) -> Option<crate::physics::Rect> {
    None
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{
        define_class, msg_send, rc::Retained, runtime::NSObjectProtocol, DefinedClass,
        MainThreadOnly,
    };
    use objc2_app_kit::{
        NSBackingStoreType, NSBezierPath, NSColor, NSFont, NSLineBreakMode, NSPanel, NSScreen,
        NSScreenSaverWindowLevel, NSTextField, NSView, NSWindow, NSWindowOrderingMode,
        NSWindowStyleMask,
    };
    use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    // --- Render metrics (named for easy tuning at review time) -----------------
    const PAD_X: f64 = 12.0;
    const PAD_Y: f64 = 9.0;
    const DOT: f64 = 7.0;
    const DOT_EMPH: f64 = 9.0;
    const DOT_GAP: f64 = 6.0;
    const CORNER: f64 = 11.0;
    const TAIL_H: f64 = 9.0;
    const TAIL_HALF: f64 = 7.0;
    const TITLE_PT: f64 = 12.0;
    const BODY_PT: f64 = 11.0;
    const BODY_MAX_LINES: isize = 3;
    const ROW_GAP: f64 = 2.0;
    // #F5F2EC card, #23262E text, rgba(0,0,0,0.08) border.
    const CARD: (f64, f64, f64, f64) = (0.961, 0.949, 0.925, 1.0);
    const TEXT: (f64, f64, f64, f64) = (0.137, 0.149, 0.180, 1.0);
    const BORDER: (f64, f64, f64, f64) = (0.0, 0.0, 0.0, 0.08);

    /// Width of the text column, leaving room for paddings + the (emphasized)
    /// dot + its gap. The dot column is reserved at its max size so a normal dot
    /// never reflows the text.
    fn text_column_width() -> f64 {
        super::MAX_WIDTH - 2.0 * PAD_X - DOT_EMPH - DOT_GAP
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }

    fn srgb(c: (f64, f64, f64, f64)) -> Retained<NSColor> {
        NSColor::colorWithSRGBRed_green_blue_alpha(c.0, c.1, c.2, c.3)
    }

    // --- Custom drawing view ---------------------------------------------------

    struct BubbleViewIvars {
        /// When true the tail points UP (at the top edge of the view); when false
        /// it points DOWN (at the bottom edge). AppKit views are Y-up internally.
        tail_up: std::cell::Cell<bool>,
        /// Tail-tip X measured from the view's left edge, in points.
        tail_x: std::cell::Cell<f64>,
        /// Accent-dot color as straight-alpha sRGB in [0, 1].
        accent: std::cell::Cell<(f64, f64, f64, f64)>,
        /// Dot diameter (DOT or DOT_EMPH).
        dot_size: std::cell::Cell<f64>,
    }

    define_class!(
        #[unsafe(super(NSView))]
        #[name = "HappyCappyBubbleView"]
        #[thread_kind = MainThreadOnly]
        #[ivars = BubbleViewIvars]
        struct BubbleView;

        unsafe impl NSObjectProtocol for BubbleView {}

        impl BubbleView {
            #[unsafe(method(drawRect:))]
            fn draw_rect(&self, _dirty: NSRect) {
                draw_bubble(self);
            }
        }
    );

    fn make_bubble_view(mtm: MainThreadMarker) -> Retained<BubbleView> {
        let ivars = BubbleViewIvars {
            tail_up: std::cell::Cell::new(false),
            tail_x: std::cell::Cell::new(CORNER + TAIL_HALF),
            accent: std::cell::Cell::new((0.0, 0.0, 0.0, 1.0)),
            dot_size: std::cell::Cell::new(DOT),
        };
        let this = mtm.alloc().set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    /// Draw the rounded card + triangular tail + accent dot into the view's
    /// current bounds. AppKit's view origin is bottom-left (Y-up).
    fn draw_bubble(view: &BubbleView) {
        let bounds = view.bounds();
        let w = bounds.size.width;
        let h = bounds.size.height;
        if w <= 0.0 || h <= 0.0 {
            return;
        }

        let ivars = view.ivars();
        let tail_up = ivars.tail_up.get();
        let tail_x = ivars.tail_x.get().clamp(0.0, w);

        // The rounded body occupies everything except the tail strip. When the
        // tail points down it lives at the bottom (low Y), so the body sits at
        // the top; when it points up the body sits at the bottom.
        let (body_min_y, body_max_y) = if tail_up {
            (0.0, h - TAIL_H)
        } else {
            (TAIL_H, h)
        };
        let body_h = (body_max_y - body_min_y).max(0.0);

        // 1) Combined path: rounded body + tail triangle, filled with the card.
        let body_rect = rect(0.0, body_min_y, w, body_h);
        let path =
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(body_rect, CORNER, CORNER);

        // Tail tip points toward the pet: at the top edge when `tail_up`, else
        // at the bottom edge. Its base sits flush on the matching body edge.
        let (base_y, tip_y) = if tail_up {
            (body_max_y, h)
        } else {
            (body_min_y, 0.0)
        };
        let tail = NSBezierPath::bezierPath();
        tail.moveToPoint(NSPoint::new(tail_x - TAIL_HALF, base_y));
        tail.lineToPoint(NSPoint::new(tail_x, tip_y));
        tail.lineToPoint(NSPoint::new(tail_x + TAIL_HALF, base_y));
        tail.closePath();
        path.appendBezierPath(&tail);

        let card = srgb(CARD);
        card.set();
        path.fill();

        // 2) Border stroke on the same combined path.
        let border = srgb(BORDER);
        border.set();
        path.setLineWidth(1.0);
        path.stroke();

        // 3) Accent dot, top-left of the body interior.
        let dot = ivars.dot_size.get();
        let dot_x = PAD_X;
        // Place the dot near the body's top edge (top in screen space = high Y).
        let dot_y = body_max_y - PAD_Y - dot;
        let dot_rect = rect(dot_x, dot_y, dot, dot);
        let dot_path = NSBezierPath::bezierPathWithOvalInRect(dot_rect);
        let accent = srgb(ivars.accent.get());
        accent.set();
        dot_path.fill();
    }

    // --- Window facade ---------------------------------------------------------

    pub struct BubbleWindow {
        panel: Retained<NSPanel>,
        view: Retained<BubbleView>,
        title: Retained<NSTextField>,
        body: Retained<NSTextField>,
        mtm: MainThreadMarker,
    }

    impl BubbleWindow {
        pub fn new(parent: &winit::window::Window) -> Option<Self> {
            let mtm = MainThreadMarker::new()?;
            let parent_ns = parent_ns_window(parent)?;

            let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
                NSPanel::alloc(mtm),
                rect(0.0, 0.0, super::MAX_WIDTH, 80.0),
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            );
            unsafe {
                panel.setReleasedWhenClosed(false);
                panel.setOpaque(false);
                panel.setBackgroundColor(Some(&NSColor::clearColor()));
                panel.setHasShadow(true);
                panel.setIgnoresMouseEvents(true);
                // Sit just above the pet window (which is AlwaysOnTop) so the
                // bubble is never occluded by the pet.
                panel.setLevel(NSScreenSaverWindowLevel + 1);
                panel.setFloatingPanel(true);
                panel.setHidesOnDeactivate(false);
            }

            let view = make_bubble_view(mtm);
            panel.setContentView(Some(&view));

            let title = NSTextField::labelWithString(&NSString::from_str(""), mtm);
            let body = NSTextField::labelWithString(&NSString::from_str(""), mtm);
            configure_label(&title, TITLE_PT, true, 1);
            configure_label(&body, BODY_PT, false, BODY_MAX_LINES);
            view.addSubview(&title);
            view.addSubview(&body);

            unsafe { parent_ns.addChildWindow_ordered(&panel, NSWindowOrderingMode::Above) };

            Some(Self {
                panel,
                view,
                title,
                body,
                mtm,
            })
        }

        pub fn update(
            &self,
            content: &crate::bubble::BubbleContent,
            pet_rect: crate::physics::Rect,
            visible: crate::physics::Rect,
        ) {
            let col_w = text_column_width();

            // 1) Set label strings (empty + zero-height when absent) and measure.
            let title_h = match content.title.as_deref() {
                Some(text) => {
                    self.title.setStringValue(&NSString::from_str(text));
                    self.title.fittingSize().height
                }
                None => {
                    self.title.setStringValue(&NSString::from_str(""));
                    0.0
                }
            };
            let body_h = match content.body.as_deref() {
                Some(text) => {
                    self.body.setStringValue(&NSString::from_str(text));
                    self.body.fittingSize().height
                }
                None => {
                    self.body.setStringValue(&NSString::from_str(""));
                    0.0
                }
            };

            // Inter-row gap only when both rows are present.
            let inner_gap = if title_h > 0.0 && body_h > 0.0 {
                ROW_GAP
            } else {
                0.0
            };
            let content_h = title_h + body_h + inner_gap;

            // 2) Outer bubble size. Width is fixed at the cap (the dot column is
            //    reserved at DOT_EMPH so the text column never reflows); height
            //    is the measured text + vertical paddings + the tail strip.
            let accent = {
                let (r, g, b, a) = content.accent.rgba();
                (r as f64, g as f64, b as f64, a as f64)
            };
            let dot_size = if content.accent.emphasized() {
                DOT_EMPH
            } else {
                DOT
            };
            let w = super::MAX_WIDTH;
            let h = content_h + 2.0 * PAD_Y + TAIL_H;

            // 3) Pure-Rust placement with the REAL measured size.
            let placement =
                crate::bubble_layout::place_bubble(pet_rect, (w as f32, h as f32), visible);
            let tail_up = placement.tail == crate::bubble_layout::TailSide::Up;
            self.view.ivars().tail_up.set(tail_up);
            self.view.ivars().tail_x.set(placement.tail_x as f64);
            self.view.ivars().accent.set(accent);
            self.view.ivars().dot_size.set(dot_size);

            // Lay out the labels inside the body region. The body occupies the
            // top of the view when the tail points down, the bottom otherwise.
            let body_min_y = if tail_up { 0.0 } else { TAIL_H };
            let body_max_y = body_min_y + (h - TAIL_H);
            let text_x = PAD_X + DOT_EMPH + DOT_GAP;
            // Title sits at the top of the body interior; body below it.
            let title_top = body_max_y - PAD_Y;
            let title_y = title_top - title_h;
            self.title.setFrame(rect(text_x, title_y, col_w, title_h));
            let body_y = title_y - inner_gap - body_h;
            self.body.setFrame(rect(text_x, body_y, col_w, body_h));

            // 4) Convert Y-down logical top-left origin to AppKit (bottom-left).
            let primary_h = primary_display_height(self.mtm);
            let appkit_x = placement.origin.x as f64;
            let appkit_y = primary_h - (placement.origin.y as f64 + h);

            let frame = rect(appkit_x, appkit_y, w, h);
            self.panel.setFrame_display(frame, true);
            self.view.setNeedsDisplay(true);
            self.panel.orderFrontRegardless();
        }

        pub fn hide(&self) {
            self.panel.orderOut(None);
        }
    }

    fn configure_label(field: &NSTextField, pt: f64, bold: bool, max_lines: isize) {
        let font = if bold {
            NSFont::boldSystemFontOfSize(pt)
        } else {
            NSFont::systemFontOfSize(pt)
        };
        field.setFont(Some(&font));
        field.setTextColor(Some(&srgb(TEXT)));
        field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        field.setMaximumNumberOfLines(max_lines);
        field.setPreferredMaxLayoutWidth(text_column_width());
    }

    /// Reach the parent pet `NSWindow` from a winit window (verified route from
    /// `window_macos.rs`).
    fn parent_ns_window(window: &winit::window::Window) -> Option<Retained<NSWindow>> {
        let handle = window.window_handle().ok()?.as_raw();
        let RawWindowHandle::AppKit(h) = handle else {
            return None;
        };
        let ns_view = unsafe { h.ns_view.cast::<NSView>().as_ref() };
        ns_view.window()
    }

    /// Primary-display height in points, the pivot for Y-up↔Y-down conversion.
    fn primary_display_height(mtm: MainThreadMarker) -> f64 {
        let screens = NSScreen::screens(mtm);
        if let Some(first) = screens.iter().next() {
            return first.frame().size.height;
        }
        if let Some(main) = NSScreen::mainScreen(mtm) {
            return main.frame().size.height;
        }
        0.0
    }

    pub fn active_visible_frame_y_down(
        window: &winit::window::Window,
    ) -> Option<crate::physics::Rect> {
        let mtm = MainThreadMarker::new()?;
        let parent_ns = parent_ns_window(window)?;
        let screen = parent_ns.screen().or_else(|| NSScreen::mainScreen(mtm))?;
        let vf: NSRect = screen.visibleFrame();
        let primary_h = primary_display_height(mtm);
        Some(crate::physics::Rect {
            min: crate::physics::Vec2 {
                x: vf.origin.x as f32,
                y: (primary_h - (vf.origin.y + vf.size.height)) as f32,
            },
            max: crate::physics::Vec2 {
                x: (vf.origin.x + vf.size.width) as f32,
                y: (primary_h - vf.origin.y) as f32,
            },
        })
    }
}

#[cfg(target_os = "macos")]
pub use macos::{active_visible_frame_y_down, BubbleWindow};
