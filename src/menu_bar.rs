//! macOS menu bar support.

#[cfg(not(target_os = "macos"))]
pub struct MenuBarController;

#[cfg(not(target_os = "macos"))]
impl MenuBarController {
    pub fn new() -> Option<Self> {
        None
    }
}

#[cfg(target_os = "macos")]
pub struct MenuBarController {
    _status_item: objc2::rc::Retained<objc2_app_kit::NSStatusItem>,
    _menu: objc2::rc::Retained<objc2_app_kit::NSMenu>,
}

#[cfg(target_os = "macos")]
impl MenuBarController {
    pub fn new() -> Option<Self> {
        use objc2::{runtime::AnyObject, sel, MainThreadOnly};
        use objc2_app_kit::{
            NSApplication, NSMenu, NSMenuItem, NSStatusBar, NSVariableStatusItemLength,
        };
        use objc2_foundation::{ns_string, MainThreadMarker};

        let mtm = MainThreadMarker::new()?;
        let app = NSApplication::sharedApplication(mtm);
        let status_bar = NSStatusBar::systemStatusBar();
        let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);
        #[allow(deprecated)]
        status_item.setTitle(Some(ns_string!("DP")));

        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("DesktopPet"));
        let quit_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Quit DesktopPet"),
                Some(sel!(terminate:)),
                ns_string!("q"),
            )
        };
        let target: &AnyObject = app.as_ref();
        unsafe {
            quit_item.setTarget(Some(target));
        }
        menu.addItem(&quit_item);
        status_item.setMenu(Some(&menu));

        Some(Self {
            _status_item: status_item,
            _menu: menu,
        })
    }
}
