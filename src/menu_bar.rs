//! macOS menu bar support.

use crate::app::AppCommand;

pub const MENU_TAG_SETTINGS: isize = 1001;
pub const MENU_TAG_SHOW_HIDE: isize = 1002;
pub const MENU_TAG_RESET: isize = 1003;
pub const MENU_TAG_QUIT: isize = 1004;
pub const MENU_TAG_FOCUS_MODE: isize = 1005;
pub const MENU_TAG_NAP: isize = 1006;
pub const MENU_TAG_CHEER_UP: isize = 1007;
pub const MENU_TAG_PERSONALITY: isize = 1101;
pub const MENU_TAG_SCALE: isize = 1102;
pub const MENU_TAG_MOVEMENT_SPEED: isize = 1103;
pub const MENU_TAG_HOVER_INTENSITY: isize = 1104;
pub const MENU_TAG_MONITOR_BEHAVIOR: isize = 1105;
pub const MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE: isize = 1106;
pub const MENU_TAG_AVOID_TEXT_CURSOR: isize = 1107;
pub const MENU_TAG_HIDE_ON_FULLSCREEN: isize = 1108;
pub const MENU_TAG_REREQUEST_ACCESSIBILITY: isize = 1109;
pub const MENU_TAG_AX_STATUS_LABEL: isize = 1110;

pub fn command_from_tag(tag: isize) -> Option<AppCommand> {
    match tag {
        MENU_TAG_SETTINGS => Some(AppCommand::OpenSettings),
        MENU_TAG_SHOW_HIDE => Some(AppCommand::TogglePetVisibility),
        MENU_TAG_RESET => Some(AppCommand::ResetPosition),
        MENU_TAG_QUIT => Some(AppCommand::Quit),
        MENU_TAG_FOCUS_MODE => Some(AppCommand::ToggleFocusMode),
        MENU_TAG_NAP => Some(AppCommand::Nap),
        MENU_TAG_CHEER_UP => Some(AppCommand::CheerUp),
        MENU_TAG_REREQUEST_ACCESSIBILITY => Some(AppCommand::RequestAccessibilityPermission),
        _ => None,
    }
}

pub fn settings_command_for_button(tag: isize, state_is_on: bool) -> Option<AppCommand> {
    match tag {
        MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE => Some(AppCommand::SetFollowCursorWhenIdle(state_is_on)),
        MENU_TAG_AVOID_TEXT_CURSOR => Some(AppCommand::SetAvoidTextCursor(state_is_on)),
        MENU_TAG_HIDE_ON_FULLSCREEN => Some(AppCommand::SetHideOnFullscreen(state_is_on)),
        _ => None,
    }
}

#[cfg(not(target_os = "macos"))]
pub struct MenuBarController;

#[cfg(not(target_os = "macos"))]
impl MenuBarController {
    pub fn new(_proxy: winit::event_loop::EventLoopProxy<AppCommand>) -> Option<Self> {
        None
    }

    pub fn sync_runtime_state(&self, _pet_visible: bool, _focus_mode: bool) {}
}

#[cfg(target_os = "macos")]
pub struct MenuBarController {
    _status_item: objc2::rc::Retained<objc2_app_kit::NSStatusItem>,
    _menu: objc2::rc::Retained<objc2_app_kit::NSMenu>,
    show_hide_item: objc2::rc::Retained<objc2_app_kit::NSMenuItem>,
    focus_mode_item: objc2::rc::Retained<objc2_app_kit::NSMenuItem>,
    _target: objc2::rc::Retained<crate::command_target_macos::CommandTarget>,
}

#[cfg(target_os = "macos")]
impl MenuBarController {
    pub fn new(proxy: winit::event_loop::EventLoopProxy<AppCommand>) -> Option<Self> {
        use objc2::{runtime::AnyObject, MainThreadOnly};
        use objc2_app_kit::{NSMenu, NSMenuItem, NSStatusBar, NSVariableStatusItemLength};
        use objc2_foundation::{ns_string, MainThreadMarker};

        let mtm = MainThreadMarker::new()?;
        let status_bar = NSStatusBar::systemStatusBar();
        let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);
        #[allow(deprecated)]
        status_item.setTitle(Some(ns_string!("HC")));

        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Happy Cappy"));
        let settings_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Settings..."),
                None,
                ns_string!(""),
            )
        };
        let show_hide_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                show_hide_ns_title(true),
                None,
                ns_string!(""),
            )
        };
        let focus_mode_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                focus_mode_ns_title(false),
                None,
                ns_string!(""),
            )
        };
        let nap_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Nap"),
                None,
                ns_string!(""),
            )
        };
        let cheer_up_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Cheer Up"),
                None,
                ns_string!(""),
            )
        };
        let reset_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Reset Position"),
                None,
                ns_string!(""),
            )
        };
        let quit_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Quit Happy Cappy"),
                None,
                ns_string!("q"),
            )
        };

        settings_item.setTag(MENU_TAG_SETTINGS);
        show_hide_item.setTag(MENU_TAG_SHOW_HIDE);
        focus_mode_item.setTag(MENU_TAG_FOCUS_MODE);
        nap_item.setTag(MENU_TAG_NAP);
        cheer_up_item.setTag(MENU_TAG_CHEER_UP);
        reset_item.setTag(MENU_TAG_RESET);
        quit_item.setTag(MENU_TAG_QUIT);

        let target = crate::command_target_macos::CommandTarget::new(mtm, proxy);
        let target_object: &AnyObject = target.as_ref();
        for item in [
            &settings_item,
            &show_hide_item,
            &focus_mode_item,
            &nap_item,
            &cheer_up_item,
            &reset_item,
            &quit_item,
        ] {
            unsafe {
                item.setTarget(Some(target_object));
                item.setAction(Some(
                    crate::command_target_macos::CommandTarget::command_selector(),
                ));
            }
        }

        menu.addItem(&settings_item);
        menu.addItem(&show_hide_item);
        menu.addItem(&focus_mode_item);
        menu.addItem(&nap_item);
        menu.addItem(&cheer_up_item);
        menu.addItem(&reset_item);
        menu.addItem(&quit_item);
        status_item.setMenu(Some(&menu));

        Some(Self {
            _status_item: status_item,
            _menu: menu,
            show_hide_item,
            focus_mode_item,
            _target: target,
        })
    }

    pub fn sync_runtime_state(&self, pet_visible: bool, focus_mode: bool) {
        self.show_hide_item
            .setTitle(show_hide_ns_title(pet_visible));
        self.focus_mode_item
            .setTitle(focus_mode_ns_title(focus_mode));
    }
}

#[cfg(target_os = "macos")]
fn show_hide_ns_title(pet_visible: bool) -> &'static objc2_foundation::NSString {
    use objc2_foundation::ns_string;

    if pet_visible {
        ns_string!("Hide Pet")
    } else {
        ns_string!("Show Pet")
    }
}

#[cfg(target_os = "macos")]
fn focus_mode_ns_title(focus_mode: bool) -> &'static objc2_foundation::NSString {
    use objc2_foundation::ns_string;

    if focus_mode {
        ns_string!("Disable Focus Mode")
    } else {
        ns_string!("Enable Focus Mode")
    }
}

pub fn focus_mode_title(focus_mode: bool) -> &'static str {
    if focus_mode {
        "Disable Focus Mode"
    } else {
        "Enable Focus Mode"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppCommand;

    #[test]
    fn command_tags_map_to_app_commands() {
        assert_eq!(
            command_from_tag(MENU_TAG_SETTINGS),
            Some(AppCommand::OpenSettings)
        );
        assert_eq!(
            command_from_tag(MENU_TAG_SHOW_HIDE),
            Some(AppCommand::TogglePetVisibility)
        );
        assert_eq!(
            command_from_tag(MENU_TAG_RESET),
            Some(AppCommand::ResetPosition)
        );
        assert_eq!(
            command_from_tag(MENU_TAG_FOCUS_MODE),
            Some(AppCommand::ToggleFocusMode)
        );
        assert_eq!(command_from_tag(MENU_TAG_NAP), Some(AppCommand::Nap));
        assert_eq!(
            command_from_tag(MENU_TAG_CHEER_UP),
            Some(AppCommand::CheerUp)
        );
        assert_eq!(command_from_tag(MENU_TAG_QUIT), Some(AppCommand::Quit));
        assert_eq!(command_from_tag(999), None);
    }

    #[test]
    fn focus_mode_titles_match_runtime_state() {
        assert_eq!(focus_mode_title(false), "Enable Focus Mode");
        assert_eq!(focus_mode_title(true), "Disable Focus Mode");
    }

    #[test]
    fn settings_command_for_button_maps_follow_cursor() {
        assert_eq!(
            settings_command_for_button(MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE, true),
            Some(AppCommand::SetFollowCursorWhenIdle(true))
        );
        assert_eq!(
            settings_command_for_button(MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE, false),
            Some(AppCommand::SetFollowCursorWhenIdle(false))
        );
    }

    #[test]
    fn settings_command_for_button_maps_avoid_text_cursor() {
        assert_eq!(
            settings_command_for_button(MENU_TAG_AVOID_TEXT_CURSOR, true),
            Some(AppCommand::SetAvoidTextCursor(true))
        );
    }

    #[test]
    fn settings_command_for_button_maps_hide_on_fullscreen() {
        assert_eq!(
            settings_command_for_button(MENU_TAG_HIDE_ON_FULLSCREEN, false),
            Some(AppCommand::SetHideOnFullscreen(false))
        );
    }

    #[test]
    fn settings_command_for_button_returns_none_for_non_checkbox_tags() {
        // Sliders, push buttons, and labels all route through other paths — never this helper.
        assert_eq!(settings_command_for_button(MENU_TAG_SCALE, true), None);
        assert_eq!(settings_command_for_button(MENU_TAG_AX_STATUS_LABEL, true), None);
        assert_eq!(settings_command_for_button(MENU_TAG_REREQUEST_ACCESSIBILITY, true), None);
    }

    #[test]
    fn command_from_tag_maps_rerequest_accessibility() {
        assert_eq!(
            command_from_tag(MENU_TAG_REREQUEST_ACCESSIBILITY),
            Some(AppCommand::RequestAccessibilityPermission)
        );
    }
}
