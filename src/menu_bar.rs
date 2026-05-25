//! macOS menu bar support.

use crate::app::AppCommand;

pub const MENU_TAG_SETTINGS: isize = 1001;
pub const MENU_TAG_SHOW_HIDE: isize = 1002;
pub const MENU_TAG_RESET: isize = 1003;
pub const MENU_TAG_QUIT: isize = 1004;
pub const MENU_TAG_PERSONALITY: isize = 1101;
pub const MENU_TAG_SCALE: isize = 1102;
pub const MENU_TAG_MOVEMENT_SPEED: isize = 1103;
pub const MENU_TAG_HOVER_INTENSITY: isize = 1104;
pub const MENU_TAG_MONITOR_BEHAVIOR: isize = 1105;

pub fn command_from_tag(tag: isize) -> Option<AppCommand> {
    match tag {
        MENU_TAG_SETTINGS => Some(AppCommand::OpenSettings),
        MENU_TAG_SHOW_HIDE => Some(AppCommand::TogglePetVisibility),
        MENU_TAG_RESET => Some(AppCommand::ResetPosition),
        MENU_TAG_QUIT => Some(AppCommand::Quit),
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
}

#[cfg(target_os = "macos")]
pub struct MenuBarController {
    _status_item: objc2::rc::Retained<objc2_app_kit::NSStatusItem>,
    _menu: objc2::rc::Retained<objc2_app_kit::NSMenu>,
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
                ns_string!("Show/Hide Pet"),
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
        reset_item.setTag(MENU_TAG_RESET);
        quit_item.setTag(MENU_TAG_QUIT);

        let target = crate::command_target_macos::CommandTarget::new(mtm, proxy);
        let target_object: &AnyObject = target.as_ref();
        for item in [&settings_item, &show_hide_item, &reset_item, &quit_item] {
            unsafe {
                item.setTarget(Some(target_object));
                item.setAction(Some(
                    crate::command_target_macos::CommandTarget::command_selector(),
                ));
            }
        }

        menu.addItem(&settings_item);
        menu.addItem(&show_hide_item);
        menu.addItem(&reset_item);
        menu.addItem(&quit_item);
        status_item.setMenu(Some(&menu));

        Some(Self {
            _status_item: status_item,
            _menu: menu,
            _target: target,
        })
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
        assert_eq!(command_from_tag(MENU_TAG_QUIT), Some(AppCommand::Quit));
        assert_eq!(command_from_tag(999), None);
    }
}
