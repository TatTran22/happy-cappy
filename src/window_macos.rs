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
    window
        .set_cursor_hittest(false)
        .map_err(WindowTweaksError::CursorHitTest)?;
    apply_platform_window_behavior(window)
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
    ns_window.setIgnoresMouseEvents(true);
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary,
    );

    Ok(())
}
