//! Native settings window facade.

#[cfg(not(target_os = "macos"))]
pub struct SettingsWindowController;

#[cfg(not(target_os = "macos"))]
impl SettingsWindowController {
    pub fn new(
        _settings: &crate::settings::AppSettings,
        _proxy: winit::event_loop::EventLoopProxy<crate::app::AppCommand>,
    ) -> Option<Self> {
        None
    }

    pub fn show(&self) {}

    pub fn sync_settings(&self, _settings: &crate::settings::AppSettings, _ax_trusted: bool) {}

    pub fn is_visible(&self) -> bool {
        false
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{rc::Retained, runtime::AnyObject, MainThreadOnly};
    use objc2_app_kit::{
        NSBackingStoreType, NSButton, NSButtonType, NSControlStateValueOff, NSControlStateValueOn,
        NSFloatingWindowLevel, NSPanel, NSSegmentSwitchTracking, NSSegmentedControl, NSSlider,
        NSTextField, NSView, NSWindowStyleMask,
    };
    use objc2_foundation::{
        ns_string, MainThreadMarker, NSInteger, NSPoint, NSRect, NSSize, NSString,
    };
    use winit::event_loop::EventLoopProxy;

    use crate::{
        app::AppCommand,
        command_target_macos::CommandTarget,
        menu_bar::{
            MENU_TAG_AVOID_TEXT_CURSOR, MENU_TAG_AX_STATUS_LABEL, MENU_TAG_FOCUS_MODE,
            MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE, MENU_TAG_HIDE_ON_FULLSCREEN, MENU_TAG_HOVER_INTENSITY,
            MENU_TAG_MONITOR_BEHAVIOR, MENU_TAG_MOVEMENT_SPEED, MENU_TAG_PERSONALITY,
            MENU_TAG_QUIT, MENU_TAG_REREQUEST_ACCESSIBILITY, MENU_TAG_RESET, MENU_TAG_SCALE,
            MENU_TAG_SHOW_HIDE,
        },
        pet::Personality,
        settings::{AppSettings, MonitorBehavior},
    };

    const PANEL_WIDTH: f64 = 420.0;
    const PANEL_HEIGHT: f64 = 560.0;
    const MARGIN_X: f64 = 24.0;
    const LABEL_WIDTH: f64 = 126.0;
    const CONTROL_X: f64 = 154.0;
    const CONTROL_WIDTH: f64 = 232.0;
    const ROW_HEIGHT: f64 = 24.0;

    pub struct SettingsWindowController {
        panel: Retained<NSPanel>,
        show_hide_button: Retained<NSButton>,
        focus_mode_button: Retained<NSButton>,
        follow_cursor_when_idle_button: Retained<NSButton>,
        avoid_text_cursor_button: Retained<NSButton>,
        hide_on_fullscreen_button: Retained<NSButton>,
        ax_status_label: Retained<NSTextField>,
        _rerequest_accessibility_button: Retained<NSButton>,
        _target: Retained<CommandTarget>,
    }

    impl SettingsWindowController {
        pub fn new(settings: &AppSettings, proxy: EventLoopProxy<AppCommand>) -> Option<Self> {
            let mtm = MainThreadMarker::new()?;
            let target = CommandTarget::new(mtm, proxy);
            let target_object: &AnyObject = target.as_ref();

            let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
                NSPanel::alloc(mtm),
                rect(0.0, 0.0, PANEL_WIDTH, PANEL_HEIGHT),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::UtilityWindow,
                NSBackingStoreType::Buffered,
                false,
            );
            unsafe {
                panel.setReleasedWhenClosed(false);
            }
            panel.setTitle(ns_string!("Happy Cappy Settings"));
            panel.setFloatingPanel(true);
            panel.setHidesOnDeactivate(false);
            panel.setLevel(NSFloatingWindowLevel);

            let content_view = NSView::initWithFrame(
                NSView::alloc(mtm),
                rect(0.0, 0.0, PANEL_WIDTH, PANEL_HEIGHT),
            );
            panel.setContentView(Some(&content_view));

            add_title(&content_view, mtm);
            add_personality_control(&content_view, mtm, target_object, settings.personality);
            add_monitor_control(&content_view, mtm, target_object, settings.monitor_behavior);
            add_slider(
                &content_view,
                mtm,
                "Scale",
                388.0,
                MENU_TAG_SCALE,
                settings.scale,
                AppSettings::MIN_SCALE,
                AppSettings::MAX_SCALE,
                target_object,
            );
            add_slider(
                &content_view,
                mtm,
                "Movement",
                346.0,
                MENU_TAG_MOVEMENT_SPEED,
                settings.movement_speed,
                AppSettings::MIN_MOVEMENT_SPEED,
                AppSettings::MAX_MOVEMENT_SPEED,
                target_object,
            );
            add_slider(
                &content_view,
                mtm,
                "Hover",
                304.0,
                MENU_TAG_HOVER_INTENSITY,
                settings.hover_intensity,
                AppSettings::MIN_HOVER_INTENSITY,
                AppSettings::MAX_HOVER_INTENSITY,
                target_object,
            );
            let (show_hide_button, focus_mode_button) = add_buttons(
                &content_view,
                mtm,
                target_object,
                settings.pet_visible,
                settings.focus_mode,
            );
            let (
                follow_cursor_when_idle_button,
                avoid_text_cursor_button,
                hide_on_fullscreen_button,
            ) = add_workspace_section(&content_view, mtm, target_object, settings);
            let ax_status_label = add_ax_status_label(&content_view, mtm);
            let rerequest_accessibility_button =
                add_rerequest_button(&content_view, mtm, target_object);

            panel.center();

            Some(Self {
                panel,
                show_hide_button,
                focus_mode_button,
                follow_cursor_when_idle_button,
                avoid_text_cursor_button,
                hide_on_fullscreen_button,
                ax_status_label,
                _rerequest_accessibility_button: rerequest_accessibility_button,
                _target: target,
            })
        }

        pub fn show(&self) {
            self.panel.makeKeyAndOrderFront(None);
            self.panel.orderFrontRegardless();
        }

        pub fn sync_settings(&self, settings: &AppSettings, ax_trusted: bool) {
            set_show_hide_title(&self.show_hide_button, settings.pet_visible);
            set_focus_mode_title(&self.focus_mode_button, settings.focus_mode);
            self.follow_cursor_when_idle_button
                .setState(if settings.follow_cursor_when_idle {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            self.avoid_text_cursor_button
                .setState(if settings.avoid_text_cursor {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            self.hide_on_fullscreen_button
                .setState(if settings.hide_on_fullscreen {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            let label_text = if settings.avoid_text_cursor && !ax_trusted {
                ns_string!(
                    "Permission needed. If no dialog appears, click Re-request or open System Settings → Privacy & Security → Accessibility."
                )
            } else {
                ns_string!("")
            };
            self.ax_status_label.setStringValue(label_text);
        }

        pub fn is_visible(&self) -> bool {
            self.panel.isVisible()
        }
    }

    fn add_title(content_view: &NSView, mtm: MainThreadMarker) {
        let title = NSTextField::labelWithString(ns_string!("Happy Cappy"), mtm);
        title.setFrame(rect(MARGIN_X, 512.0, PANEL_WIDTH - (MARGIN_X * 2.0), 28.0));
        content_view.addSubview(&title);
    }

    fn add_personality_control(
        content_view: &NSView,
        mtm: MainThreadMarker,
        target_object: &AnyObject,
        personality: Personality,
    ) {
        add_label(content_view, mtm, ns_string!("Personality"), 454.0);

        let control = NSSegmentedControl::initWithFrame(
            NSSegmentedControl::alloc(mtm),
            rect(CONTROL_X, 452.0, CONTROL_WIDTH, ROW_HEIGHT),
        );
        control.setSegmentCount(3);
        control.setLabel_forSegment(ns_string!("Calm"), 0);
        control.setLabel_forSegment(ns_string!("Cheerful"), 1);
        control.setLabel_forSegment(ns_string!("Lively"), 2);
        control.setTrackingMode(NSSegmentSwitchTracking::SelectOne);
        control.setSelectedSegment(match personality {
            Personality::Calm => 0,
            Personality::Cheerful => 1,
            Personality::Lively => 2,
        });
        control.setTag(MENU_TAG_PERSONALITY as NSInteger);
        unsafe {
            control.setTarget(Some(target_object));
            control.setAction(Some(CommandTarget::settings_value_selector()));
        }
        content_view.addSubview(&control);
    }

    fn add_monitor_control(
        content_view: &NSView,
        mtm: MainThreadMarker,
        target_object: &AnyObject,
        monitor_behavior: MonitorBehavior,
    ) {
        add_label(content_view, mtm, ns_string!("Display"), 420.0);

        let control = NSSegmentedControl::initWithFrame(
            NSSegmentedControl::alloc(mtm),
            rect(CONTROL_X, 418.0, CONTROL_WIDTH, ROW_HEIGHT),
        );
        control.setSegmentCount(2);
        control.setLabel_forSegment(ns_string!("Current Display"), 0);
        control.setLabel_forSegment(ns_string!("Primary Display"), 1);
        control.setTrackingMode(NSSegmentSwitchTracking::SelectOne);
        control.setSelectedSegment(match monitor_behavior {
            MonitorBehavior::CurrentDisplay => 0,
            MonitorBehavior::PrimaryDisplay => 1,
        });
        control.setTag(MENU_TAG_MONITOR_BEHAVIOR as NSInteger);
        unsafe {
            control.setTarget(Some(target_object));
            control.setAction(Some(CommandTarget::settings_value_selector()));
        }
        content_view.addSubview(&control);
    }

    #[allow(clippy::too_many_arguments)]
    fn add_slider(
        content_view: &NSView,
        mtm: MainThreadMarker,
        label: &'static str,
        y: f64,
        tag: isize,
        value: f32,
        min: f32,
        max: f32,
        target_object: &AnyObject,
    ) {
        let label = match label {
            "Scale" => ns_string!("Scale"),
            "Movement" => ns_string!("Movement"),
            "Hover" => ns_string!("Hover"),
            _ => return,
        };
        add_label(content_view, mtm, label, y + 2.0);

        let slider = NSSlider::initWithFrame(
            NSSlider::alloc(mtm),
            rect(CONTROL_X, y, CONTROL_WIDTH, ROW_HEIGHT),
        );
        slider.setMinValue(min as f64);
        slider.setMaxValue(max as f64);
        slider.setDoubleValue(value.clamp(min, max) as f64);
        slider.setTag(tag as NSInteger);
        unsafe {
            slider.setTarget(Some(target_object));
            slider.setAction(Some(CommandTarget::settings_value_selector()));
        }
        content_view.addSubview(&slider);
    }

    fn add_buttons(
        content_view: &NSView,
        mtm: MainThreadMarker,
        target_object: &AnyObject,
        pet_visible: bool,
        focus_mode: bool,
    ) -> (Retained<NSButton>, Retained<NSButton>) {
        let quit = unsafe {
            NSButton::buttonWithTitle_target_action(
                ns_string!("Quit Happy Cappy"),
                Some(target_object),
                Some(CommandTarget::command_selector()),
                mtm,
            )
        };
        quit.setFrame(rect(MARGIN_X, 218.0, 132.0, 30.0));
        quit.setTag(MENU_TAG_QUIT as NSInteger);
        content_view.addSubview(&quit);

        let reset = unsafe {
            NSButton::buttonWithTitle_target_action(
                ns_string!("Reset Position"),
                Some(target_object),
                Some(CommandTarget::command_selector()),
                mtm,
            )
        };
        reset.setFrame(rect(168.0, 218.0, 112.0, 30.0));
        reset.setTag(MENU_TAG_RESET as NSInteger);
        content_view.addSubview(&reset);

        let focus_mode = unsafe {
            NSButton::buttonWithTitle_target_action(
                focus_mode_title(focus_mode),
                Some(target_object),
                Some(CommandTarget::command_selector()),
                mtm,
            )
        };
        focus_mode.setFrame(rect(168.0, 254.0, 112.0, 30.0));
        focus_mode.setTag(MENU_TAG_FOCUS_MODE as NSInteger);
        content_view.addSubview(&focus_mode);

        let show_hide = unsafe {
            NSButton::buttonWithTitle_target_action(
                show_hide_title(pet_visible),
                Some(target_object),
                Some(CommandTarget::command_selector()),
                mtm,
            )
        };
        show_hide.setFrame(rect(292.0, 218.0, 108.0, 30.0));
        show_hide.setTag(MENU_TAG_SHOW_HIDE as NSInteger);
        content_view.addSubview(&show_hide);
        (show_hide, focus_mode)
    }

    fn set_show_hide_title(button: &NSButton, pet_visible: bool) {
        button.setTitle(show_hide_title(pet_visible));
    }

    fn show_hide_title(pet_visible: bool) -> &'static NSString {
        if pet_visible {
            ns_string!("Hide Pet")
        } else {
            ns_string!("Show Pet")
        }
    }

    fn set_focus_mode_title(button: &NSButton, focus_mode: bool) {
        button.setTitle(focus_mode_title(focus_mode));
    }

    fn focus_mode_title(focus_mode: bool) -> &'static NSString {
        if focus_mode {
            ns_string!("Disable Focus")
        } else {
            ns_string!("Enable Focus")
        }
    }

    fn add_label(content_view: &NSView, mtm: MainThreadMarker, text: &NSString, y: f64) {
        let label = NSTextField::labelWithString(text, mtm);
        label.setFrame(rect(MARGIN_X, y, LABEL_WIDTH, ROW_HEIGHT));
        content_view.addSubview(&label);
    }

    fn add_workspace_section(
        content_view: &NSView,
        mtm: MainThreadMarker,
        target_object: &AnyObject,
        settings: &AppSettings,
    ) -> (Retained<NSButton>, Retained<NSButton>, Retained<NSButton>) {
        let heading = NSTextField::labelWithString(ns_string!("Workspace Awareness"), mtm);
        heading.setFrame(rect(
            MARGIN_X,
            190.0,
            PANEL_WIDTH - (MARGIN_X * 2.0),
            22.0,
        ));
        content_view.addSubview(&heading);

        let follow = add_checkbox(
            content_view,
            mtm,
            ns_string!("Follow cursor when idle"),
            158.0,
            MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE,
            settings.follow_cursor_when_idle,
            target_object,
        );
        let avoid = add_checkbox(
            content_view,
            mtm,
            ns_string!("Avoid text-cursor area"),
            128.0,
            MENU_TAG_AVOID_TEXT_CURSOR,
            settings.avoid_text_cursor,
            target_object,
        );
        let hide = add_checkbox(
            content_view,
            mtm,
            ns_string!("Auto-hide when any app is fullscreen"),
            98.0,
            MENU_TAG_HIDE_ON_FULLSCREEN,
            settings.hide_on_fullscreen,
            target_object,
        );
        (follow, avoid, hide)
    }

    fn add_checkbox(
        content_view: &NSView,
        mtm: MainThreadMarker,
        title: &NSString,
        y: f64,
        tag: isize,
        initial_state: bool,
        target_object: &AnyObject,
    ) -> Retained<NSButton> {
        let button = NSButton::initWithFrame(
            NSButton::alloc(mtm),
            rect(MARGIN_X, y, PANEL_WIDTH - (MARGIN_X * 2.0), 22.0),
        );
        button.setTitle(title);
        button.setTag(tag as NSInteger);
        unsafe {
            button.setButtonType(NSButtonType::Switch);
            button.setState(if initial_state {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
            button.setTarget(Some(target_object));
            button.setAction(Some(CommandTarget::settings_value_selector()));
        }
        content_view.addSubview(&button);
        button
    }

    fn add_ax_status_label(content_view: &NSView, mtm: MainThreadMarker) -> Retained<NSTextField> {
        let label = NSTextField::labelWithString(ns_string!(""), mtm);
        label.setFrame(rect(MARGIN_X, 58.0, PANEL_WIDTH - (MARGIN_X * 2.0), 36.0));
        label.setTag(MENU_TAG_AX_STATUS_LABEL as NSInteger);
        label.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
        label.setMaximumNumberOfLines(2);
        content_view.addSubview(&label);
        label
    }

    fn add_rerequest_button(
        content_view: &NSView,
        mtm: MainThreadMarker,
        target_object: &AnyObject,
    ) -> Retained<NSButton> {
        let button = NSButton::initWithFrame(
            NSButton::alloc(mtm),
            rect(MARGIN_X, 22.0, 280.0, 28.0),
        );
        button.setTitle(ns_string!("Re-request Accessibility permission"));
        button.setTag(MENU_TAG_REREQUEST_ACCESSIBILITY as NSInteger);
        unsafe {
            button.setBezelStyle(objc2_app_kit::NSBezelStyle::Push);
            button.setTarget(Some(target_object));
            button.setAction(Some(CommandTarget::command_selector()));
        }
        content_view.addSubview(&button);
        button
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
    }
}

#[cfg(target_os = "macos")]
pub use macos::SettingsWindowController;
