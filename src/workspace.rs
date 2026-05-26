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

use std::time::Instant;

pub struct WorkspaceObserver {
    last_known_ax_trusted: Option<bool>,
    pub(crate) prompted_for_accessibility_at_startup: bool,
    active_display: Option<DisplayInfo>,
    last_snapshot: WorkspaceSnapshot,
    last_tick_at: Option<Instant>,
    last_key_counter: Option<i64>,
    // Real impl will add: last_frontmost_poll_at, etc.
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayInfo {
    /// monitor.name(); diagnostic only, not unique
    pub name: Option<String>,
    /// pet-space, top-left origin, points
    pub bounds_logical: Rect,
    /// window.scale_factor()
    pub scale_factor: f32,
    /// height in points of the primary display, used as Y-flip pivot
    pub primary_display_height: f32,
}

impl WorkspaceObserver {
    pub fn new() -> Self {
        Self {
            last_known_ax_trusted: None,
            prompted_for_accessibility_at_startup: false,
            active_display: None,
            last_snapshot: WorkspaceSnapshot::default(),
            last_tick_at: None,
            last_key_counter: None,
        }
    }

    pub fn set_active_display(&mut self, info: Option<DisplayInfo>) {
        self.active_display = info;
    }

    pub fn tick(&mut self, now: Instant) -> WorkspaceTick {
        let seconds_idle = macos_polling::seconds_since_last_input();
        let key_counter = macos_polling::key_down_counter();

        let typing_rate_per_sec = match (self.last_tick_at, self.last_key_counter) {
            (Some(prev_at), Some(prev_counter)) => {
                let elapsed = now.saturating_duration_since(prev_at).as_secs_f32();
                if elapsed > 0.0 {
                    ((key_counter - prev_counter).max(0) as f32) / elapsed
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };

        self.last_tick_at = Some(now);
        self.last_key_counter = Some(key_counter);

        self.last_snapshot = WorkspaceSnapshot {
            workspace_available: cfg!(target_os = "macos"),
            seconds_idle,
            typing_rate_per_sec,
            // The remaining fields stay at their previous values; later tasks
            // populate them.
            frontmost_bundle_id: self.last_snapshot.frontmost_bundle_id.clone(),
            frontmost_is_editor: self.last_snapshot.frontmost_is_editor,
            caret_rect: self.last_snapshot.caret_rect,
            fullscreen_active: self.last_snapshot.fullscreen_active,
            cursor_pos: self.last_snapshot.cursor_pos,
        };

        let now_trusted = self.is_accessibility_trusted();
        let trust_changed = match self.last_known_ax_trusted {
            Some(prev) => prev != now_trusted,
            None => false,
        };
        self.last_known_ax_trusted = Some(now_trusted);

        WorkspaceTick {
            snapshot: self.last_snapshot.clone(),
            trust_changed,
        }
    }

    pub fn is_accessibility_trusted(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            // Real wrapper in a later task. For now: not trusted, so the AX flow
            // degrades to caret_rect = None — which is the safe default.
            false
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }

    pub fn request_accessibility_on_startup_if_enabled(&mut self, avoid_text_cursor: bool) {
        if !avoid_text_cursor || self.prompted_for_accessibility_at_startup {
            return;
        }
        self.prompted_for_accessibility_at_startup = true;
        #[cfg(target_os = "macos")]
        {
            // Real AXIsProcessTrustedWithOptions call lands in Task 15.
        }
    }

    pub fn request_accessibility_now(&mut self) {
        #[cfg(target_os = "macos")]
        {
            // Real AXIsProcessTrustedWithOptions call lands in Task 15.
        }
    }
}

impl Default for WorkspaceObserver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
mod macos_polling {
    use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGEventType};

    /// kCGAnyInputEventType in C is `~0u32` (0xFFFFFFFF). The objc2-core-graphics
    /// crate doesn't expose a named constant for it (the same raw value is reused
    /// in `CGEventType::TapDisabledByUserInput`), so we construct it explicitly.
    /// Passing this sentinel to `seconds_since_last_event_type` returns the
    /// seconds since the most recent event of ANY input type (mouse + keyboard).
    const ANY_INPUT_EVENT: CGEventType = CGEventType(!0u32);

    /// Seconds since the most recent input event (mouse or keyboard) globally.
    pub fn seconds_since_last_input() -> f32 {
        let secs = CGEventSource::seconds_since_last_event_type(
            CGEventSourceStateID::CombinedSessionState,
            ANY_INPUT_EVENT,
        );
        if secs.is_finite() && secs > 0.0 { secs as f32 } else { 0.0 }
    }

    /// Cumulative count of key-down events since the session started.
    /// `counter_for_event_type` returns u32; we widen to i64 so a u32
    /// wraparound is still representable when subtracting two samples.
    pub fn key_down_counter() -> i64 {
        let count = CGEventSource::counter_for_event_type(
            CGEventSourceStateID::CombinedSessionState,
            CGEventType::KeyDown, // matches kCGEventKeyDown = 10
        );
        count as i64
    }
}

#[cfg(not(target_os = "macos"))]
mod macos_polling {
    pub fn seconds_since_last_input() -> f32 { 0.0 }
    pub fn key_down_counter() -> i64 { 0 }
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

    #[test]
    fn observer_tick_returns_workspace_tick() {
        let mut observer = WorkspaceObserver::new();
        let tick = observer.tick(std::time::Instant::now());
        assert!(!tick.trust_changed, "first tick has no prior state to compare");
        // On the stub (non-macOS) build, workspace_available is false.
        // On macOS, this test runs but real polling hasn't been wired yet,
        // so workspace_available is also false. Both are acceptable.
        let _ = tick.snapshot;
    }

    #[test]
    fn observer_owns_borrow_releases_after_tick() {
        // Compile-fence test: if tick() returned &WorkspaceSnapshot, the second
        // borrow below would fail.
        let mut observer = WorkspaceObserver::new();
        let tick = observer.tick(std::time::Instant::now());
        let _trusted = observer.is_accessibility_trusted();
        let _ = tick;
    }
}
