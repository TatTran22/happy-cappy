//! macOS window behavior.

use std::{error::Error, fmt};

use winit::window::{Window, WindowLevel};

#[derive(Debug)]
pub enum WindowTweaksError {
    CursorHitTest(winit::error::ExternalError),
    #[cfg(target_os = "macos")]
    WindowHandle(winit::raw_window_handle::HandleError),
    #[cfg(target_os = "macos")]
    MissingAppKitWindow,
}

impl fmt::Display for WindowTweaksError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CursorHitTest(error) => write!(f, "failed to update cursor hit testing: {error}"),
            #[cfg(target_os = "macos")]
            Self::WindowHandle(error) => {
                write!(f, "failed to get raw AppKit window handle: {error}")
            }
            #[cfg(target_os = "macos")]
            Self::MissingAppKitWindow => {
                write!(f, "raw AppKit view is not attached to an NSWindow")
            }
        }
    }
}

impl Error for WindowTweaksError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CursorHitTest(error) => Some(error),
            #[cfg(target_os = "macos")]
            Self::WindowHandle(error) => Some(error),
            #[cfg(target_os = "macos")]
            Self::MissingAppKitWindow => None,
        }
    }
}

pub fn apply_desktop_pet_window_behavior(window: &Window) -> Result<(), WindowTweaksError> {
    window.set_window_level(WindowLevel::AlwaysOnTop);
    set_pet_window_mouse_passthrough(window, false)?;
    apply_platform_window_behavior(window)
}

pub fn set_pet_window_mouse_passthrough(
    window: &Window,
    passthrough: bool,
) -> Result<(), WindowTweaksError> {
    window
        .set_cursor_hittest(!passthrough)
        .map_err(WindowTweaksError::CursorHitTest)?;

    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSView;
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let handle = window
            .window_handle()
            .map_err(WindowTweaksError::WindowHandle)?
            .as_raw();
        let RawWindowHandle::AppKit(handle) = handle else {
            return Ok(());
        };
        let ns_view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let ns_window = ns_view
            .window()
            .ok_or(WindowTweaksError::MissingAppKitWindow)?;
        ns_window.setIgnoresMouseEvents(passthrough);
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn apply_platform_window_behavior(_window: &Window) -> Result<(), WindowTweaksError> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn apply_platform_window_behavior(window: &Window) -> Result<(), WindowTweaksError> {
    use objc2_app_kit::{NSView, NSWindowCollectionBehavior};
    use winit::{
        platform::macos::WindowExtMacOS,
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
    };

    window.set_has_shadow(false);

    let handle = window
        .window_handle()
        .map_err(WindowTweaksError::WindowHandle)?
        .as_raw();

    let RawWindowHandle::AppKit(handle) = handle else {
        return Ok(());
    };

    let ns_view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let ns_window = ns_view
        .window()
        .ok_or(WindowTweaksError::MissingAppKitWindow)?;

    ns_window.setHasShadow(false);
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary,
    );

    Ok(())
}

pub fn show_pet_context_menu(
    window: &Window,
    proxy: winit::event_loop::EventLoopProxy<crate::app::AppCommand>,
    pet_visible: bool,
    focus_mode: bool,
    local_position: Option<crate::physics::Vec2>,
) {
    #[cfg(target_os = "macos")]
    {
        use objc2::{runtime::AnyObject, MainThreadOnly};
        use objc2_app_kit::{NSMenu, NSMenuItem, NSView};
        use objc2_foundation::{ns_string, MainThreadMarker, NSPoint};
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let target = crate::command_target_macos::CommandTarget::new(mtm, proxy);
        let target_object: &AnyObject = target.as_ref();
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Happy Cappy"));
        let settings = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Settings..."),
                Some(crate::command_target_macos::CommandTarget::command_selector()),
                ns_string!(""),
            )
        };
        let hide = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                if pet_visible {
                    ns_string!("Hide Pet")
                } else {
                    ns_string!("Show Pet")
                },
                Some(crate::command_target_macos::CommandTarget::command_selector()),
                ns_string!(""),
            )
        };
        let focus_mode = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                context_focus_mode_ns_title(focus_mode),
                Some(crate::command_target_macos::CommandTarget::command_selector()),
                ns_string!(""),
            )
        };
        let nap = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Nap"),
                Some(crate::command_target_macos::CommandTarget::command_selector()),
                ns_string!(""),
            )
        };
        let cheer_up = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Cheer Up"),
                Some(crate::command_target_macos::CommandTarget::command_selector()),
                ns_string!(""),
            )
        };
        let reset = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Reset Position"),
                Some(crate::command_target_macos::CommandTarget::command_selector()),
                ns_string!(""),
            )
        };
        settings.setTag(crate::menu_bar::MENU_TAG_SETTINGS);
        hide.setTag(crate::menu_bar::MENU_TAG_SHOW_HIDE);
        focus_mode.setTag(crate::menu_bar::MENU_TAG_FOCUS_MODE);
        nap.setTag(crate::menu_bar::MENU_TAG_NAP);
        cheer_up.setTag(crate::menu_bar::MENU_TAG_CHEER_UP);
        reset.setTag(crate::menu_bar::MENU_TAG_RESET);
        unsafe {
            settings.setTarget(Some(target_object));
            hide.setTarget(Some(target_object));
            focus_mode.setTarget(Some(target_object));
            nap.setTarget(Some(target_object));
            cheer_up.setTarget(Some(target_object));
            reset.setTarget(Some(target_object));
        }
        menu.addItem(&settings);
        menu.addItem(&hide);
        menu.addItem(&focus_mode);
        menu.addItem(&nap);
        menu.addItem(&cheer_up);
        menu.addItem(&reset);
        let local_position = local_position.unwrap_or(crate::physics::Vec2 { x: 0.0, y: 0.0 });
        let ns_view = match window.window_handle().ok().map(|handle| handle.as_raw()) {
            Some(RawWindowHandle::AppKit(handle)) => {
                Some(unsafe { handle.ns_view.cast::<NSView>().as_ref() })
            }
            _ => None,
        };
        menu.popUpMenuPositioningItem_atLocation_inView(
            None,
            NSPoint::new(local_position.x as f64, local_position.y as f64),
            ns_view,
        );
        let _keep_target_alive_until_menu_returns = target;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, proxy, pet_visible, focus_mode, local_position);
    }
}

#[cfg(target_os = "macos")]
fn context_focus_mode_ns_title(focus_mode: bool) -> &'static objc2_foundation::NSString {
    use objc2_foundation::ns_string;

    if focus_mode {
        ns_string!("Disable Focus Mode")
    } else {
        ns_string!("Enable Focus Mode")
    }
}

pub fn context_focus_mode_title(focus_mode: bool) -> &'static str {
    if focus_mode {
        "Disable Focus Mode"
    } else {
        "Enable Focus Mode"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_focus_mode_title_tracks_current_focus_mode() {
        assert_eq!(context_focus_mode_title(false), "Enable Focus Mode");
        assert_eq!(context_focus_mode_title(true), "Disable Focus Mode");
    }
}
