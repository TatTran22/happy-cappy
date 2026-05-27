//! Native AppKit Pet Library picker window.
//!
//! Mirrors the structure of [`crate::settings_window_macos`]: an `NSPanel`
//! created lazily, populated synchronously from `DesktopPetApp`, and
//! dispatching user actions back through [`crate::app::AppCommand`] via
//! [`crate::command_target_macos::CommandTarget`].

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
    use super::*;
    use winit::event_loop::EventLoopProxy;

    use crate::app::AppCommand;

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
}

#[cfg(target_os = "macos")]
pub use macos::PickerWindowController;
