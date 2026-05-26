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

impl WorkspaceSnapshot {
    pub fn is_busy(&self) -> bool {
        self.workspace_available
            && (self.frontmost_is_editor
                || self.typing_rate_per_sec > 1.0
                || self.seconds_idle < 2.0)
    }

    pub fn is_idle(&self) -> bool {
        self.workspace_available && self.seconds_idle >= 5.0 && !self.is_busy()
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
}
