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
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{rc::Retained, runtime::AnyObject, MainThreadOnly};
    use objc2_app_kit::{
        NSBackingStoreType, NSButton, NSFloatingWindowLevel, NSPanel, NSSegmentSwitchTracking,
        NSSegmentedControl, NSSlider, NSTextField, NSView, NSWindowStyleMask,
    };
    use objc2_foundation::{
        ns_string, MainThreadMarker, NSInteger, NSPoint, NSRect, NSSize, NSString,
    };
    use winit::event_loop::EventLoopProxy;

    use crate::{
        app::AppCommand,
        command_target_macos::CommandTarget,
        menu_bar::{
            MENU_TAG_HOVER_INTENSITY, MENU_TAG_MONITOR_BEHAVIOR, MENU_TAG_MOVEMENT_SPEED,
            MENU_TAG_PERSONALITY, MENU_TAG_RESET, MENU_TAG_SCALE, MENU_TAG_SHOW_HIDE,
        },
        pet::Personality,
        settings::{AppSettings, MonitorBehavior},
    };

    const PANEL_WIDTH: f64 = 420.0;
    const PANEL_HEIGHT: f64 = 330.0;
    const MARGIN_X: f64 = 24.0;
    const LABEL_WIDTH: f64 = 126.0;
    const CONTROL_X: f64 = 154.0;
    const CONTROL_WIDTH: f64 = 232.0;
    const ROW_HEIGHT: f64 = 24.0;

    pub struct SettingsWindowController {
        panel: Retained<NSPanel>,
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
                158.0,
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
                116.0,
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
                74.0,
                MENU_TAG_HOVER_INTENSITY,
                settings.hover_intensity,
                AppSettings::MIN_HOVER_INTENSITY,
                AppSettings::MAX_HOVER_INTENSITY,
                target_object,
            );
            add_buttons(&content_view, mtm, target_object, settings.pet_visible);

            panel.center();

            Some(Self {
                panel,
                _target: target,
            })
        }

        pub fn show(&self) {
            self.panel.makeKeyAndOrderFront(None);
            self.panel.orderFrontRegardless();
        }
    }

    fn add_title(content_view: &NSView, mtm: MainThreadMarker) {
        let title = NSTextField::labelWithString(ns_string!("Happy Cappy"), mtm);
        title.setFrame(rect(MARGIN_X, 282.0, PANEL_WIDTH - (MARGIN_X * 2.0), 28.0));
        content_view.addSubview(&title);
    }

    fn add_personality_control(
        content_view: &NSView,
        mtm: MainThreadMarker,
        target_object: &AnyObject,
        personality: Personality,
    ) {
        add_label(content_view, mtm, ns_string!("Personality"), 224.0);

        let control = NSSegmentedControl::initWithFrame(
            NSSegmentedControl::alloc(mtm),
            rect(CONTROL_X, 222.0, CONTROL_WIDTH, ROW_HEIGHT),
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
        add_label(content_view, mtm, ns_string!("Display"), 190.0);

        let control = NSSegmentedControl::initWithFrame(
            NSSegmentedControl::alloc(mtm),
            rect(CONTROL_X, 188.0, CONTROL_WIDTH, ROW_HEIGHT),
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
    ) {
        let reset = unsafe {
            NSButton::buttonWithTitle_target_action(
                ns_string!("Reset Position"),
                Some(target_object),
                Some(CommandTarget::command_selector()),
                mtm,
            )
        };
        reset.setFrame(rect(CONTROL_X, 28.0, 112.0, 30.0));
        reset.setTag(MENU_TAG_RESET as NSInteger);
        content_view.addSubview(&reset);

        let show_hide_title = if pet_visible {
            ns_string!("Hide Pet")
        } else {
            ns_string!("Show Pet")
        };
        let show_hide = unsafe {
            NSButton::buttonWithTitle_target_action(
                show_hide_title,
                Some(target_object),
                Some(CommandTarget::command_selector()),
                mtm,
            )
        };
        show_hide.setFrame(rect(CONTROL_X + 124.0, 28.0, 108.0, 30.0));
        show_hide.setTag(MENU_TAG_SHOW_HIDE as NSInteger);
        content_view.addSubview(&show_hide);
    }

    fn add_label(content_view: &NSView, mtm: MainThreadMarker, text: &NSString, y: f64) {
        let label = NSTextField::labelWithString(text, mtm);
        label.setFrame(rect(MARGIN_X, y, LABEL_WIDTH, ROW_HEIGHT));
        content_view.addSubview(&label);
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
    }
}

#[cfg(target_os = "macos")]
pub use macos::SettingsWindowController;
