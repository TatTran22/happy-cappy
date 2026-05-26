//! Workspace observation: idle/typing detection, frontmost app, fullscreen, text caret.
//!
//! macOS-specific polling is added in later commits; this file currently defines
//! only the platform-independent snapshot/tick types and the is_busy/is_idle policy.

use crate::physics::{Rect, Vec2};

#[derive(Clone, Debug)]
pub struct WorkspaceSnapshot {
    /// false on non-macOS stub builds or before the first successful poll on macOS.
    /// Gates is_busy()/is_idle() so the pet falls through to Idle on stub builds.
    pub workspace_available: bool,
    pub seconds_idle: f32,
    pub typing_rate_per_sec: f32,
    pub frontmost_bundle_id: Option<String>,
    pub frontmost_is_editor: bool,
    /// Pet-space points, top-left origin (see spec §Coordinate system).
    pub caret_rect: Option<Rect>,
    /// On the pet's active display only.
    pub fullscreen_active: bool,
    /// Pet-space points, top-left origin.
    pub cursor_pos: Vec2,
}

impl Default for WorkspaceSnapshot {
    fn default() -> Self {
        Self {
            workspace_available: false,
            seconds_idle: 0.0,
            typing_rate_per_sec: 0.0,
            frontmost_bundle_id: None,
            frontmost_is_editor: false,
            caret_rect: None,
            fullscreen_active: false,
            cursor_pos: Vec2 { x: 0.0, y: 0.0 },
        }
    }
}

/// Keys-per-second rate above which the user is considered to be typing.
const TYPING_BUSY_THRESHOLD: f32 = 1.0;
/// Seconds of inactivity below which the user is still considered recently active (busy).
const BUSY_IDLE_SECS: f32 = 2.0;
/// Seconds of inactivity at or above which the user is considered idle.
const IDLE_SECS: f32 = 5.0;

impl WorkspaceSnapshot {
    pub fn is_busy(&self) -> bool {
        self.workspace_available
            && (self.frontmost_is_editor
                || self.typing_rate_per_sec > TYPING_BUSY_THRESHOLD
                || self.seconds_idle < BUSY_IDLE_SECS)
    }

    pub fn is_idle(&self) -> bool {
        self.workspace_available && self.seconds_idle >= IDLE_SECS && !self.is_busy()
    }
}

/// Owned result of one observer tick. Returning an owned value (not `&WorkspaceSnapshot`)
/// releases the `&mut WorkspaceObserver` borrow immediately, so the app can then call
/// `is_accessibility_trusted()` or `sync_settings_window()` without fighting the borrow
/// checker.
#[derive(Clone, Debug)]
pub struct WorkspaceTick {
    pub snapshot: WorkspaceSnapshot,
    /// True if `is_accessibility_trusted()` flipped during this tick vs the previous one.
    pub trust_changed: bool,
}

/// Convert a y coordinate from Cocoa global space (origin = primary display bottom-left, Y up)
/// to Quartz space (origin = primary display top-left, Y down). x is unchanged across the
/// two spaces. The pivot is the primary display's logical height, so this is correct for
/// points on any secondary display, including displays with negative coordinates or
/// vertical layouts — both spaces share the primary display as their anchor.
pub fn cocoa_to_quartz_y(cocoa_y: f32, primary_display_height: f32) -> f32 {
    primary_display_height - cocoa_y
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_with(
        editor: bool,
        typing_rate: f32,
        idle: f32,
    ) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            workspace_available: true,
            seconds_idle: idle,
            typing_rate_per_sec: typing_rate,
            frontmost_bundle_id: None,
            frontmost_is_editor: editor,
            caret_rect: None,
            fullscreen_active: false,
            cursor_pos: Vec2 { x: 0.0, y: 0.0 },
        }
    }

    #[test]
    fn busy_when_editor_frontmost() {
        assert!(snapshot_with(true, 0.0, 10.0).is_busy());
    }

    #[test]
    fn busy_when_typing_fast() {
        assert!(snapshot_with(false, 2.0, 10.0).is_busy());
    }

    #[test]
    fn busy_when_recently_active() {
        assert!(snapshot_with(false, 0.0, 1.0).is_busy());
    }

    #[test]
    fn idle_when_long_quiet_and_not_editor() {
        let s = snapshot_with(false, 0.0, 6.0);
        assert!(!s.is_busy());
        assert!(s.is_idle());
    }

    #[test]
    fn between_2_and_5_seconds_is_neither_busy_nor_idle() {
        let s = snapshot_with(false, 0.0, 3.5);
        assert!(!s.is_busy());
        assert!(!s.is_idle());
    }

    #[test]
    fn boundary_at_2s_idle_is_neither_busy_nor_idle() {
        let s = snapshot_with(false, 0.0, 2.0);
        assert!(!s.is_busy(), "seconds_idle == 2.0 should not be busy (condition is < 2.0)");
        assert!(!s.is_idle(), "seconds_idle == 2.0 should not be idle (condition is >= 5.0)");
    }

    #[test]
    fn boundary_at_5s_idle_is_idle() {
        let s = snapshot_with(false, 0.0, 5.0);
        assert!(!s.is_busy());
        assert!(s.is_idle(), "seconds_idle == 5.0 should be idle (condition is >= 5.0)");
    }

    #[test]
    fn is_busy_and_is_idle_never_both_true() {
        for editor in [false, true] {
            for &typing in &[0.0_f32, 0.5, 2.0] {
                for &idle in &[0.0_f32, 1.5, 3.0, 6.0] {
                    let s = snapshot_with(editor, typing, idle);
                    assert!(
                        !(s.is_busy() && s.is_idle()),
                        "both busy and idle for editor={editor} typing={typing} idle={idle}"
                    );
                }
            }
        }
    }

    #[test]
    fn workspace_unavailable_blocks_both_busy_and_idle() {
        let mut s = snapshot_with(true, 10.0, 10.0);
        s.workspace_available = false;
        assert!(!s.is_busy());
        assert!(!s.is_idle());
    }

    #[test]
    fn default_snapshot_is_inert() {
        let s = WorkspaceSnapshot::default();
        assert!(!s.workspace_available);
        assert!(!s.is_busy());
        assert!(!s.is_idle());
        assert!(!s.fullscreen_active);
        assert!(s.caret_rect.is_none());
    }

    #[test]
    fn cocoa_to_quartz_y_on_primary_display() {
        // primary display 900 pt tall, point at cocoa y=800 → quartz y=100.
        assert_eq!(cocoa_to_quartz_y(800.0, 900.0), 100.0);
    }

    #[test]
    fn cocoa_to_quartz_y_on_display_above_primary() {
        // Cocoa y > primary_height means above primary; quartz y is negative.
        assert_eq!(cocoa_to_quartz_y(1400.0, 900.0), -500.0);
    }

    #[test]
    fn cocoa_to_quartz_y_on_display_below_primary() {
        // Negative cocoa y means below primary; quartz y > primary_height.
        assert_eq!(cocoa_to_quartz_y(-300.0, 900.0), 1200.0);
    }
}
