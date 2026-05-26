# Workspace Awareness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three workspace-aware behaviors to Happy Cappy — cursor follow/avoid, text-caret avoidance, and fullscreen auto-hide — toggleable independently in Settings, with default-on values.

**Architecture:** A new `src/workspace.rs` polls macOS APIs (CGEventSource for idle/typing, NSWorkspace for frontmost app, CGWindowList for fullscreen, AX for caret) every ~500 ms and returns an owned `WorkspaceTick { snapshot, trust_changed }`. A new pure `decide_intent` function in `app.rs` maps the snapshot + settings to one of four `BehaviorIntent` variants (Idle, ChaseHorizontal, AvoidHorizontal, AvoidRectHorizontal — all carrying only a horizontal `Direction`). Pet consumes the intent; window auto-hides on fullscreen. Settings panel gains a Workspace Awareness section with three checkboxes, an AX permission status label, and a Re-request button.

**Tech Stack:** Rust 2021, winit 0.30, objc2 0.6, objc2-app-kit 0.3 (new features: NSWorkspace, NSRunningApplication, NSEvent, NSScreen), new `objc2-core-graphics = "0.3"` (CGEventSource, CGEventTypes, CGWindow, CGGeometry), new `objc2-application-services = "0.3"` (HIServices, AXUIElement, AXValue, AXError, AXAttributeConstants, AXValueConstants).

**Spec:** `docs/superpowers/specs/2026-05-25-workspace-awareness-design.md`

---

## File Structure

**New files:**
- `src/workspace.rs` — `WorkspaceObserver`, `WorkspaceSnapshot`, `WorkspaceTick`, AX helpers, coord normalization. macOS implementation behind `#[cfg(target_os = "macos")]`, stub for other platforms.

**Modified files:**
- `Cargo.toml` — new dependency rows, new AppKit features.
- `src/lib.rs` — register `workspace` module.
- `src/physics.rs` — add `Rect { min: Vec2, max: Vec2 }`.
- `src/pet.rs` — add `BehaviorIntent` enum + `Pet::set_intent` + intent storage and consumption.
- `src/settings.rs` — three new `bool` fields + `default_true` helper.
- `src/menu_bar.rs` — five new `MENU_TAG_*` constants, new `settings_command_for_button` pure helper, new `command_from_tag` arm.
- `src/command_target_macos.rs` — new arms in `dispatchSettingsValue:` + `read_button_state` helper.
- `src/app.rs` — `AppCommand` variants, `auto_hidden` field, `effective_window_visible`, `apply_window_visibility`, `set_auto_hidden`, `decide_intent`, observer wiring, new tick-interval branches, command handlers, startup AX prompt hook.
- `src/settings_window_macos.rs` — panel resize, new section + checkboxes + label + button + `is_visible()` accessor + `sync_settings` signature change.
- `src/window_macos.rs` — no changes (helper functions stay as-is per the spec).

---

## Phase 1 — Pure-Rust Foundations

These tasks introduce all the platform-independent types and pure functions. They can be developed and tested without any macOS runtime, and each commit leaves the build green.

### Task 1: Add `Rect` to `physics.rs`

**Files:**
- Modify: `src/physics.rs`
- Test: `src/physics.rs` (existing `#[cfg(test)] mod tests` block at bottom)

- [ ] **Step 1: Write the failing test**

Append to the existing `mod tests` in `src/physics.rs`:

```rust
#[test]
fn rect_intersects_returns_true_for_overlap() {
    let a = Rect { min: Vec2 { x: 0.0, y: 0.0 }, max: Vec2 { x: 10.0, y: 10.0 } };
    let b = Rect { min: Vec2 { x: 5.0, y: 5.0 }, max: Vec2 { x: 15.0, y: 15.0 } };
    assert!(a.intersects(&b));
    assert!(b.intersects(&a));
}

#[test]
fn rect_intersects_returns_false_for_disjoint() {
    let a = Rect { min: Vec2 { x: 0.0, y: 0.0 }, max: Vec2 { x: 10.0, y: 10.0 } };
    let b = Rect { min: Vec2 { x: 20.0, y: 20.0 }, max: Vec2 { x: 30.0, y: 30.0 } };
    assert!(!a.intersects(&b));
}

#[test]
fn rect_intersects_returns_false_for_touch_only() {
    let a = Rect { min: Vec2 { x: 0.0, y: 0.0 }, max: Vec2 { x: 10.0, y: 10.0 } };
    let b = Rect { min: Vec2 { x: 10.0, y: 10.0 }, max: Vec2 { x: 20.0, y: 20.0 } };
    assert!(!a.intersects(&b));
}

#[test]
fn rect_from_bounds_round_trips() {
    let bounds = Bounds { min_x: 0.0, min_y: 0.0, max_x: 100.0, max_y: 50.0 };
    let rect: Rect = bounds.into();
    assert_eq!(rect.min, Vec2 { x: 0.0, y: 0.0 });
    assert_eq!(rect.max, Vec2 { x: 100.0, y: 50.0 });
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib physics::tests::rect_ -- --nocapture`
Expected: FAIL — `cannot find type 'Rect'` and `the method 'intersects' is not in scope`.

- [ ] **Step 3: Implement `Rect`**

Add to `src/physics.rs` (above the existing `#[cfg(test)] mod tests`):

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn intersects(&self, other: &Rect) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
    }
}

impl From<Bounds> for Rect {
    fn from(bounds: Bounds) -> Self {
        Rect {
            min: Vec2 { x: bounds.min_x, y: bounds.min_y },
            max: Vec2 { x: bounds.max_x, y: bounds.max_y },
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib physics::tests::rect_ -- --nocapture`
Expected: PASS, all 4 cases green.

- [ ] **Step 5: Commit**

```bash
git add src/physics.rs
git commit -m "feat(physics): add Rect type with intersects + Bounds conversion"
```

---

### Task 2: Add three workspace-awareness fields to `AppSettings`

**Files:**
- Modify: `src/settings.rs`
- Test: `src/settings.rs` (existing tests module if present, else add new `#[cfg(test)] mod tests` at the bottom)

- [ ] **Step 1: Write the failing test**

Append to `src/settings.rs` (inside or create a `#[cfg(test)] mod tests` block):

```rust
#[cfg(test)]
mod workspace_awareness_settings_tests {
    use super::*;

    #[test]
    fn missing_workspace_keys_default_to_true() {
        let json = r#"{"personality":"calm","scale":2.0,"movement_speed":1.0,"hover_intensity":1.0,"monitor_behavior":"current_display","pet_visible":true,"focus_mode":false}"#;
        let settings: AppSettings = serde_json::from_str(json).expect("parse");
        assert!(settings.follow_cursor_when_idle);
        assert!(settings.avoid_text_cursor);
        assert!(settings.hide_on_fullscreen);
    }

    #[test]
    fn explicit_workspace_keys_round_trip() {
        let json = r#"{"personality":"calm","scale":2.0,"movement_speed":1.0,"hover_intensity":1.0,"monitor_behavior":"current_display","pet_visible":true,"focus_mode":false,"follow_cursor_when_idle":false,"avoid_text_cursor":false,"hide_on_fullscreen":false}"#;
        let settings: AppSettings = serde_json::from_str(json).expect("parse");
        assert!(!settings.follow_cursor_when_idle);
        assert!(!settings.avoid_text_cursor);
        assert!(!settings.hide_on_fullscreen);
    }

    #[test]
    fn default_settings_have_workspace_features_enabled() {
        let s = AppSettings::default();
        assert!(s.follow_cursor_when_idle);
        assert!(s.avoid_text_cursor);
        assert!(s.hide_on_fullscreen);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib workspace_awareness_settings_tests -- --nocapture`
Expected: FAIL — `no field 'follow_cursor_when_idle' on type 'AppSettings'`.

- [ ] **Step 3: Add fields, default function, and Default impl entries**

Edit `src/settings.rs`:

1. Add to the `AppSettings` struct (after `pub focus_mode: bool,`):

```rust
    #[serde(default = "default_true")]
    pub follow_cursor_when_idle: bool,
    #[serde(default = "default_true")]
    pub avoid_text_cursor: bool,
    #[serde(default = "default_true")]
    pub hide_on_fullscreen: bool,
```

2. Add to the `Default for AppSettings` impl (inside the `Self { ... }`, after `focus_mode: false,`):

```rust
            follow_cursor_when_idle: true,
            avoid_text_cursor: true,
            hide_on_fullscreen: true,
```

3. Add the `default_true` helper function (next to the existing `default_pet_visible`, etc.):

```rust
fn default_true() -> bool {
    true
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib workspace_awareness_settings_tests -- --nocapture && cargo test --lib settings`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/settings.rs
git commit -m "feat(settings): add follow_cursor_when_idle, avoid_text_cursor, hide_on_fullscreen"
```

---

### Task 3: Add `BehaviorIntent` enum + `Pet::set_intent` (no behavior yet)

**Files:**
- Modify: `src/pet.rs`
- Test: `src/pet.rs` (existing `#[cfg(test)] mod tests` block)

- [ ] **Step 1: Write the failing test**

Append to `src/pet.rs` `mod tests`:

```rust
#[test]
fn set_intent_stores_intent() {
    let mut pet = Pet::new_with_seed(0);
    pet.set_intent(BehaviorIntent::ChaseHorizontal { direction: Direction::Right });
    assert_eq!(pet.intent(), BehaviorIntent::ChaseHorizontal { direction: Direction::Right });
}

#[test]
fn default_intent_is_idle() {
    let pet = Pet::new_with_seed(0);
    assert_eq!(pet.intent(), BehaviorIntent::Idle);
}

#[test]
fn set_intent_avoid_rect_interrupts_idle_into_walk() {
    let mut pet = Pet::new_with_seed(0);
    // Force pet into Idle state via a complete tick cycle.
    pet.tick(std::time::Duration::from_millis(0));
    assert_eq!(pet.state(), PetState::Idle);

    pet.set_intent(BehaviorIntent::AvoidRectHorizontal { direction: Direction::Left });

    assert_eq!(pet.state(), PetState::Walk);
    assert!(pet.tick(std::time::Duration::ZERO).speed_x < 0.0);
}

#[test]
fn set_intent_chase_does_not_interrupt_mid_walk() {
    let mut pet = Pet::new_with_seed(0);
    pet.tick(std::time::Duration::from_millis(0));
    // Drive the pet into a walk segment.
    while pet.state() != PetState::Walk {
        pet.tick(std::time::Duration::from_millis(200));
    }
    let direction_before = pet.tick(std::time::Duration::ZERO).speed_x.signum();

    pet.set_intent(BehaviorIntent::ChaseHorizontal { direction: Direction::Left });

    // Within the same walk segment, the direction is preserved.
    let direction_after = pet.tick(std::time::Duration::ZERO).speed_x.signum();
    assert_eq!(direction_before, direction_after);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib pet::tests::set_intent -- --nocapture`
Expected: FAIL — `cannot find type 'BehaviorIntent'`.

- [ ] **Step 3: Implement `BehaviorIntent` + storage + `set_intent` + `intent`**

Edit `src/pet.rs`:

1. Add the enum (above the `pub struct Pet`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorIntent {
    Idle,
    ChaseHorizontal { direction: Direction },
    AvoidHorizontal { direction: Direction },
    AvoidRectHorizontal { direction: Direction },
}
```

2. Add a field to `Pet` (next to `hidden: bool`):

```rust
    intent: BehaviorIntent,
```

3. Initialize it in `Pet::new_with_seed` (next to `hidden: false,` or the field list end):

```rust
            intent: BehaviorIntent::Idle,
```

4. Add the accessors + mutator (anywhere in the `impl Pet` block, near `set_hidden`):

```rust
    pub fn intent(&self) -> BehaviorIntent {
        self.intent
    }

    pub fn set_intent(&mut self, intent: BehaviorIntent) {
        self.intent = intent;
        if let BehaviorIntent::AvoidRectHorizontal { direction } = intent {
            self.direction = direction;
            self.walk_distance_remaining = WALK_DISTANCE;
            if matches!(self.state, PetState::Idle | PetState::Sleep) {
                self.enter_walk();
            }
        }
    }
```

5. Consume the intent in `enter_walk` so chase/avoid take effect on the next walk-cycle boundary. Find `fn enter_walk(&mut self)` (around pet.rs:291) and add this just before the existing direction-pick logic, so that if the stored intent is `Chase/Avoid Horizontal`, we override the random/seeded direction:

```rust
        match self.intent {
            BehaviorIntent::ChaseHorizontal { direction }
            | BehaviorIntent::AvoidHorizontal { direction }
            | BehaviorIntent::AvoidRectHorizontal { direction } => {
                self.direction = direction;
            }
            BehaviorIntent::Idle => {}
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib pet -- --nocapture`
Expected: all PASS, including the four new tests and all pre-existing pet tests still green.

- [ ] **Step 5: Commit**

```bash
git add src/pet.rs
git commit -m "feat(pet): add BehaviorIntent with AvoidRect priority interrupt"
```

---

### Task 4: Add `WorkspaceSnapshot`, `WorkspaceTick`, `is_busy`, `is_idle`

**Files:**
- Create: `src/workspace.rs` (initial stub — only types and pure logic, no observer yet)
- Modify: `src/lib.rs` (register the module)
- Test: `src/workspace.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Register the module**

Edit `src/lib.rs`, add (alphabetically):

```rust
pub mod workspace;
```

- [ ] **Step 2: Create `src/workspace.rs` with types and tests**

```rust
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
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --lib workspace -- --nocapture`
Expected: all 8 cases PASS.

- [ ] **Step 4: Commit**

```bash
git add src/lib.rs src/workspace.rs
git commit -m "feat(workspace): add WorkspaceSnapshot/Tick with is_busy/is_idle policy"
```

---

### Task 5: Add pure `decide_intent` function

**Files:**
- Modify: `src/app.rs` (top-level free function, not on `DesktopPetApp`)
- Test: `src/app.rs` (existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Append to `src/app.rs` `mod tests`:

```rust
#[cfg(test)]
mod decide_intent_tests {
    use super::*;
    use crate::pet::{BehaviorIntent, Direction};
    use crate::physics::{Rect, Vec2};
    use crate::settings::AppSettings;
    use crate::workspace::WorkspaceSnapshot;

    fn settings_all_on() -> AppSettings {
        let mut s = AppSettings::default();
        s.follow_cursor_when_idle = true;
        s.avoid_text_cursor = true;
        s.hide_on_fullscreen = true;
        s
    }

    fn snap(idle: f32, cursor_x: f32, caret: Option<Rect>) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            workspace_available: true,
            seconds_idle: idle,
            typing_rate_per_sec: 0.0,
            frontmost_bundle_id: None,
            frontmost_is_editor: false,
            caret_rect: caret,
            fullscreen_active: false,
            cursor_pos: Vec2 { x: cursor_x, y: 0.0 },
        }
    }

    fn pet_frame_at(x: f32) -> Rect {
        Rect {
            min: Vec2 { x, y: 0.0 },
            max: Vec2 { x: x + 100.0, y: 100.0 },
        }
    }

    #[test]
    fn idle_when_no_signals() {
        let settings = settings_all_on();
        let intent = decide_intent(&snap(3.5, 50.0, None), &settings, pet_frame_at(0.0));
        assert_eq!(intent, BehaviorIntent::Idle);
    }

    #[test]
    fn chase_right_when_idle_and_cursor_to_right() {
        let intent = decide_intent(&snap(6.0, 1000.0, None), &settings_all_on(), pet_frame_at(0.0));
        assert_eq!(intent, BehaviorIntent::ChaseHorizontal { direction: Direction::Right });
    }

    #[test]
    fn chase_left_when_idle_and_cursor_to_left() {
        let intent = decide_intent(&snap(6.0, -50.0, None), &settings_all_on(), pet_frame_at(0.0));
        assert_eq!(intent, BehaviorIntent::ChaseHorizontal { direction: Direction::Left });
    }

    #[test]
    fn avoid_horizontal_when_busy_and_cursor_to_right() {
        // seconds_idle < 2.0 → busy
        let intent = decide_intent(&snap(0.5, 1000.0, None), &settings_all_on(), pet_frame_at(0.0));
        assert_eq!(intent, BehaviorIntent::AvoidHorizontal { direction: Direction::Left });
    }

    #[test]
    fn avoid_rect_overrides_chase_when_caret_intersects_pet() {
        let caret = Rect {
            min: Vec2 { x: 60.0, y: 40.0 },
            max: Vec2 { x: 120.0, y: 60.0 },
        };
        let intent = decide_intent(&snap(6.0, 1000.0, Some(caret)), &settings_all_on(), pet_frame_at(50.0));
        match intent {
            BehaviorIntent::AvoidRectHorizontal { .. } => {}
            other => panic!("expected AvoidRectHorizontal, got {other:?}"),
        }
    }

    #[test]
    fn caret_rect_not_intersecting_does_not_trigger_avoid_rect() {
        let caret = Rect {
            min: Vec2 { x: 500.0, y: 40.0 },
            max: Vec2 { x: 560.0, y: 60.0 },
        };
        // Idle and cursor to right; AvoidRect should NOT fire since rect is far away.
        let intent = decide_intent(&snap(6.0, 1000.0, Some(caret)), &settings_all_on(), pet_frame_at(0.0));
        assert_eq!(intent, BehaviorIntent::ChaseHorizontal { direction: Direction::Right });
    }

    #[test]
    fn all_gates_off_returns_idle() {
        let mut settings = settings_all_on();
        settings.follow_cursor_when_idle = false;
        settings.avoid_text_cursor = false;
        let caret = Rect {
            min: Vec2 { x: 60.0, y: 40.0 },
            max: Vec2 { x: 120.0, y: 60.0 },
        };
        let intent = decide_intent(&snap(6.0, 1000.0, Some(caret)), &settings, pet_frame_at(50.0));
        assert_eq!(intent, BehaviorIntent::Idle);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib decide_intent_tests -- --nocapture`
Expected: FAIL — `cannot find function 'decide_intent'`.

- [ ] **Step 3: Implement `decide_intent`**

Add to `src/app.rs` (as a free function, near the bottom of the file but above any `#[cfg(test)] mod tests`):

```rust
pub fn decide_intent(
    snapshot: &crate::workspace::WorkspaceSnapshot,
    settings: &crate::settings::AppSettings,
    pet_frame: crate::physics::Rect,
) -> crate::pet::BehaviorIntent {
    use crate::pet::{BehaviorIntent, Direction};

    let pet_center_x = (pet_frame.min.x + pet_frame.max.x) * 0.5;

    if settings.avoid_text_cursor {
        if let Some(caret) = snapshot.caret_rect {
            if caret.intersects(&pet_frame) {
                // Pick the side of the caret rect that's closer to the pet center.
                let exit_left_dx = (pet_center_x - caret.min.x).abs();
                let exit_right_dx = (caret.max.x - pet_center_x).abs();
                let direction = if exit_left_dx < exit_right_dx {
                    Direction::Left
                } else {
                    Direction::Right
                };
                return BehaviorIntent::AvoidRectHorizontal { direction };
            }
        }
    }

    if settings.follow_cursor_when_idle {
        if snapshot.is_idle() {
            let direction = if snapshot.cursor_pos.x > pet_center_x {
                Direction::Right
            } else {
                Direction::Left
            };
            return BehaviorIntent::ChaseHorizontal { direction };
        }
        if snapshot.is_busy() {
            let direction = if snapshot.cursor_pos.x > pet_center_x {
                Direction::Left
            } else {
                Direction::Right
            };
            return BehaviorIntent::AvoidHorizontal { direction };
        }
    }

    BehaviorIntent::Idle
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib decide_intent_tests -- --nocapture`
Expected: all 7 cases PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): add pure decide_intent function for workspace awareness"
```

---

### Task 6: Add pure `cocoa_to_quartz_y` coord helper to `workspace.rs`

**Files:**
- Modify: `src/workspace.rs`

- [ ] **Step 1: Write the failing test**

Append to `src/workspace.rs` `mod tests`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib workspace::tests::cocoa_to_quartz_y -- --nocapture`
Expected: FAIL — `cannot find function 'cocoa_to_quartz_y'`.

- [ ] **Step 3: Implement the helper**

Add to `src/workspace.rs` (above `#[cfg(test)] mod tests`):

```rust
/// Convert a y coordinate from Cocoa global space (origin = primary display bottom-left, Y up)
/// to Quartz space (origin = primary display top-left, Y down). x is unchanged across the
/// two spaces. The pivot is the primary display's logical height, so this is correct for
/// points on any secondary display, including displays with negative coordinates or
/// vertical layouts — both spaces share the primary display as their anchor.
pub fn cocoa_to_quartz_y(cocoa_y: f32, primary_display_height: f32) -> f32 {
    primary_display_height - cocoa_y
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib workspace::tests::cocoa_to_quartz_y -- --nocapture`
Expected: all 3 cases PASS.

- [ ] **Step 5: Commit**

```bash
git add src/workspace.rs
git commit -m "feat(workspace): add cocoa_to_quartz_y coordinate flip"
```

---

### Task 7: Add 5 new `MENU_TAG_*` constants + `settings_command_for_button` + REREQUEST arm

**Files:**
- Modify: `src/menu_bar.rs`
- Test: `src/menu_bar.rs` (existing `#[cfg(test)] mod tests` block at bottom)

- [ ] **Step 1: Write the failing test**

Append to `src/menu_bar.rs` `mod tests`:

```rust
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
    fn settings_command_for_button_returns_none_for_slider_tag() {
        assert_eq!(settings_command_for_button(MENU_TAG_SCALE, true), None);
    }

    #[test]
    fn command_from_tag_maps_rerequest_accessibility() {
        assert_eq!(
            command_from_tag(MENU_TAG_REREQUEST_ACCESSIBILITY),
            Some(AppCommand::RequestAccessibilityPermission)
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib menu_bar -- --nocapture`
Expected: FAIL — missing constants and missing helper and missing `AppCommand` variants.

- [ ] **Step 3: Add constants and helper** (the `AppCommand` variants are added in Task 8; this step pulls them in but compilation will fail until both this task and Task 8 are done — that's expected; we'll batch the commit after Task 8 passes)

Add to `src/menu_bar.rs` (next to the existing `MENU_TAG_*` constants):

```rust
pub const MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE: isize = 1106;
pub const MENU_TAG_AVOID_TEXT_CURSOR: isize = 1107;
pub const MENU_TAG_HIDE_ON_FULLSCREEN: isize = 1108;
pub const MENU_TAG_REREQUEST_ACCESSIBILITY: isize = 1109;
pub const MENU_TAG_AX_STATUS_LABEL: isize = 1110;
```

Add to the existing `command_from_tag` match (any position; suggest alphabetical):

```rust
        MENU_TAG_REREQUEST_ACCESSIBILITY => Some(AppCommand::RequestAccessibilityPermission),
```

Add the new pure helper (just below `command_from_tag`):

```rust
pub fn settings_command_for_button(tag: isize, state_is_on: bool) -> Option<AppCommand> {
    match tag {
        MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE => Some(AppCommand::SetFollowCursorWhenIdle(state_is_on)),
        MENU_TAG_AVOID_TEXT_CURSOR => Some(AppCommand::SetAvoidTextCursor(state_is_on)),
        MENU_TAG_HIDE_ON_FULLSCREEN => Some(AppCommand::SetHideOnFullscreen(state_is_on)),
        _ => None,
    }
}
```

- [ ] **Step 4: Build deferred — continue to Task 8**

The crate will not compile until Task 8 adds the `AppCommand` variants. Do not commit yet.

---

### Task 8: Add 4 new `AppCommand` variants

**Files:**
- Modify: `src/app.rs`
- Test: covered by Task 7's tests + a new test below

- [ ] **Step 1: Add `AppCommand` variants**

In `src/app.rs`, extend the `AppCommand` enum (between `ToggleFocusMode` and `Nap`):

```rust
    SetFollowCursorWhenIdle(bool),
    SetAvoidTextCursor(bool),
    SetHideOnFullscreen(bool),
    RequestAccessibilityPermission,
```

- [ ] **Step 2: Verify Task 7 tests now compile and pass**

Run: `cargo test --lib menu_bar -- --nocapture && cargo test --lib decide_intent_tests`
Expected: all PASS, including Task 7's 5 new `menu_bar` tests.

- [ ] **Step 3: Commit Tasks 7 + 8 together**

```bash
git add src/menu_bar.rs src/app.rs
git commit -m "feat(menu_bar,app): add Workspace Awareness AppCommands + tag helpers"
```

---

## Phase 2 — Workspace Observer (macOS impl + stub)

### Task 9: Update `Cargo.toml` with new dependencies and features

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Edit `Cargo.toml`**

Extend the `[target.'cfg(target_os = "macos")'.dependencies]` block:

1. Add to the `features = [ ... ]` array of `objc2-app-kit`:

```toml
  "NSEvent",
  "NSRunningApplication",
  "NSScreen",
  "NSWorkspace",
```

2. Add two new dependency rows below `objc2-foundation = "0.3"`:

```toml
objc2-core-graphics = { version = "0.3", features = [
  "CGEventSource",
  "CGEventTypes",
  "CGGeometry",
  "CGWindow",
] }
objc2-application-services = { version = "0.3", features = [
  "AXAttributeConstants",
  "AXError",
  "AXUIElement",
  "AXValue",
  "AXValueConstants",
  "HIServices",
] }
```

- [ ] **Step 2: Verify the project still compiles (no usages yet)**

Run: `cargo check`
Expected: clean build, no errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "build: add objc2-core-graphics and objc2-application-services deps"
```

---

### Task 10: `workspace.rs` skeleton — `WorkspaceObserver` struct + `tick()` returning default snapshot

This task adds the observer type with stub bodies, with the `cfg(target_os = "macos")` real impl and a non-macOS stub. No real polling yet — that comes in subsequent tasks. This lets the rest of the app integration land before the macOS code is fully wired.

**Files:**
- Modify: `src/workspace.rs`

- [ ] **Step 1: Write the failing test**

Append to `src/workspace.rs` `mod tests`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib workspace -- --nocapture`
Expected: FAIL — `cannot find type 'WorkspaceObserver'`.

- [ ] **Step 3: Add the observer skeleton**

Append to `src/workspace.rs`:

```rust
use std::time::Instant;

pub struct WorkspaceObserver {
    last_known_ax_trusted: Option<bool>,
    prompted_for_accessibility_at_startup: bool,
    active_display: Option<DisplayInfo>,
    last_snapshot: WorkspaceSnapshot,
    // Real impl will add: last_tick_at, last_key_counter, last_frontmost_poll_at, etc.
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
        }
    }

    pub fn set_active_display(&mut self, info: Option<DisplayInfo>) {
        self.active_display = info;
    }

    pub fn tick(&mut self, _now: Instant) -> WorkspaceTick {
        // Real polling lands in subsequent tasks. For now, return the last
        // known snapshot (default at startup) and report no trust change.
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib workspace -- --nocapture && cargo check`
Expected: all PASS, clean build.

- [ ] **Step 5: Commit**

```bash
git add src/workspace.rs
git commit -m "feat(workspace): add WorkspaceObserver skeleton with trust-change reporting"
```

---

### Task 11: Implement idle-time + typing-rate polling (macOS) — `tick()` populates `seconds_idle`, `typing_rate_per_sec`, sets `workspace_available = true`

**Files:**
- Modify: `src/workspace.rs`

- [ ] **Step 1: Add per-tick polling state**

Inside the `WorkspaceObserver` struct (in the existing field list), add:

```rust
    last_tick_at: Option<Instant>,
    last_key_counter: Option<i64>,
```

Initialize them in `WorkspaceObserver::new`:

```rust
            last_tick_at: None,
            last_key_counter: None,
```

- [ ] **Step 2: Implement the polling helpers behind cfg(macos)**

Add a private module at the end of `src/workspace.rs`:

```rust
#[cfg(target_os = "macos")]
mod macos_polling {
    use objc2_core_graphics::{
        CGEventSourceCounterForEventType, CGEventSourceSecondsSinceLastEventType,
        CGEventSourceStateID, CGEventType,
    };

    /// Seconds since the most recent input event (mouse or keyboard) globally.
    pub fn seconds_since_last_input() -> f32 {
        unsafe {
            CGEventSourceSecondsSinceLastEventType(
                CGEventSourceStateID::CombinedSessionState,
                CGEventType::Null,  // Any input event
            )
        }
        .max(0.0) as f32
    }

    /// Cumulative count of key-down events since the session started.
    pub fn key_down_counter() -> i64 {
        unsafe {
            CGEventSourceCounterForEventType(
                CGEventSourceStateID::CombinedSessionState,
                CGEventType::KeyDown,
            )
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod macos_polling {
    pub fn seconds_since_last_input() -> f32 {
        0.0
    }
    pub fn key_down_counter() -> i64 {
        0
    }
}
```

> Note: The exact `CGEventType` discriminant name for "any event" may need adjustment after running `cargo check`. If the binding exposes `CGEventType::Null` differently (e.g., `kCGAnyInputEventType` as a raw integer), use `CGEventType::from_raw(0)` or the equivalent. Plan-stage hedge: the polling fn name and arg shape are stable; only the enum variant identifier may need a rename.

- [ ] **Step 3: Update `tick()` to populate idle + typing-rate**

Replace the existing `tick()` body in `src/workspace.rs` with:

```rust
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
```

- [ ] **Step 4: Build**

Run: `cargo check`
Expected: clean.

If `CGEventType::Null` does not exist as named, replace with whichever variant the crate exports for "any input event" — check `objc2_core_graphics::CGEventType` docs or `cargo doc --open -p objc2-core-graphics` and pick the variant whose discriminant is `0xFFFFFFFF` (matches `kCGAnyInputEventType` semantically).

- [ ] **Step 5: Smoke test by hand**

Run: `cargo test --lib workspace -- --nocapture`
Expected: existing tests still PASS. On macOS, the new fields populate; on Linux/Windows they're zeros (which is fine since `workspace_available = false`).

- [ ] **Step 6: Commit**

```bash
git add src/workspace.rs
git commit -m "feat(workspace): poll idle-time and key-down counter on macOS"
```

---

### Task 12: Add frontmost-app polling + editor-bundle-id matcher

**Files:**
- Modify: `src/workspace.rs`

- [ ] **Step 1: Add editor list constant and matcher with tests**

Append to `src/workspace.rs` (above `#[cfg(test)] mod tests`):

```rust
/// Bundle identifiers we consider "editors" for the purpose of marking the user busy.
/// Prefix match supported via the trailing `*` convention; matcher handles it.
const EDITOR_BUNDLE_IDS: &[&str] = &[
    "com.apple.dt.Xcode",
    "com.microsoft.VSCode",
    "com.todesktop.230313mzl4w4u92", // Cursor
    "com.sublimetext.4",
    "com.googlecode.iterm2",
    "com.apple.Terminal",
    "com.mitchellh.ghostty",
    "com.jetbrains.*",
];

pub fn is_editor_bundle_id(bundle_id: &str) -> bool {
    EDITOR_BUNDLE_IDS.iter().any(|pattern| {
        if let Some(prefix) = pattern.strip_suffix('*') {
            bundle_id.starts_with(prefix)
        } else {
            *pattern == bundle_id
        }
    })
}
```

Append to `mod tests`:

```rust
    #[test]
    fn editor_matcher_exact() {
        assert!(is_editor_bundle_id("com.apple.dt.Xcode"));
        assert!(is_editor_bundle_id("com.microsoft.VSCode"));
    }

    #[test]
    fn editor_matcher_jetbrains_prefix() {
        assert!(is_editor_bundle_id("com.jetbrains.intellij"));
        assert!(is_editor_bundle_id("com.jetbrains.RustRover"));
    }

    #[test]
    fn editor_matcher_rejects_unrelated_substring() {
        assert!(!is_editor_bundle_id("com.example.notxcode"));
        assert!(!is_editor_bundle_id(""));
    }
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib workspace::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Add frontmost-app polling helper**

Extend the `macos_polling` module (`#[cfg(target_os = "macos")]` half):

```rust
    use objc2::msg_send;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    pub fn frontmost_bundle_id() -> Option<String> {
        unsafe {
            let workspace: &NSWorkspace = &NSWorkspace::sharedWorkspace();
            let app = workspace.frontmostApplication()?;
            let id: Option<objc2::rc::Retained<NSString>> = msg_send![&*app, bundleIdentifier];
            id.map(|s| s.to_string())
        }
    }
```

And the stub (`#[cfg(not(target_os = "macos"))]` half):

```rust
    pub fn frontmost_bundle_id() -> Option<String> {
        None
    }
```

- [ ] **Step 4: Poll frontmost only every 500 ms (cadence gate)**

Inside `WorkspaceObserver`, add:

```rust
    last_frontmost_poll_at: Option<Instant>,
```

and in `WorkspaceObserver::new`:

```rust
            last_frontmost_poll_at: None,
```

Inside `tick()`, before the `self.last_snapshot = ...` assignment, add:

```rust
        let (frontmost_bundle_id, frontmost_is_editor) = {
            let due = self
                .last_frontmost_poll_at
                .map_or(true, |t| now.saturating_duration_since(t) >= std::time::Duration::from_millis(500));
            if due {
                let id = macos_polling::frontmost_bundle_id();
                let is_editor = id.as_deref().map_or(false, is_editor_bundle_id);
                self.last_frontmost_poll_at = Some(now);
                (id, is_editor)
            } else {
                (
                    self.last_snapshot.frontmost_bundle_id.clone(),
                    self.last_snapshot.frontmost_is_editor,
                )
            }
        };
```

Change the `WorkspaceSnapshot { ... }` block in `tick` to use the new values:

```rust
            frontmost_bundle_id,
            frontmost_is_editor,
```

(Remove the old `.clone()` lines for these two fields.)

- [ ] **Step 5: Build + run tests**

Run: `cargo check && cargo test --lib workspace -- --nocapture`
Expected: clean build, all tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/workspace.rs
git commit -m "feat(workspace): poll frontmost-app and gate editor detection at 500ms"
```

---

### Task 13: Implement fullscreen detection via `CGWindowListCopyWindowInfo`

**Files:**
- Modify: `src/workspace.rs`

- [ ] **Step 1: Add the fullscreen-check helper (macOS)**

Extend `mod macos_polling` (`#[cfg(target_os = "macos")]` half):

```rust
    use crate::physics::Rect as PetRect;
    use objc2_core_foundation::{CFArrayRef, CFDictionaryRef};
    use objc2_core_graphics::{CGRectMakeWithDictionaryRepresentation, CGWindowID, CGWindowListCopyWindowInfo, CGWindowListOption};

    /// Returns true if any window meeting the spec's filter rules covers
    /// `active_bounds` within 1 px on each side.
    pub fn any_fullscreen_on(active_bounds: PetRect, our_pid: i32) -> bool {
        unsafe {
            let info: Option<objc2::rc::Retained<objc2_core_foundation::CFArray>> =
                CGWindowListCopyWindowInfo(
                    CGWindowListOption::OptionOnScreenOnly,
                    CGWindowID(0),
                );
            let Some(info) = info else { return false; };
            for i in 0..info.count() {
                let dict_ptr = info.value_at_index(i) as CFDictionaryRef;
                if dict_ptr.is_null() {
                    continue;
                }
                // Filter: kCGWindowLayer != 0 → skip
                let layer = dict_get_i64(dict_ptr, "kCGWindowLayer").unwrap_or(0);
                if layer != 0 {
                    continue;
                }
                // Filter: own process → skip
                let owner_pid = dict_get_i64(dict_ptr, "kCGWindowOwnerPID").unwrap_or(0) as i32;
                if owner_pid == our_pid {
                    continue;
                }
                // Read the window bounds (CGRect) from the dict
                let Some(bounds_dict) = dict_get_dict(dict_ptr, "kCGWindowBounds") else {
                    continue;
                };
                let mut rect = objc2_core_graphics::CGRect::default();
                let ok = CGRectMakeWithDictionaryRepresentation(bounds_dict, &mut rect);
                if !ok {
                    continue;
                }
                let win = PetRect {
                    min: crate::physics::Vec2 { x: rect.origin.x as f32, y: rect.origin.y as f32 },
                    max: crate::physics::Vec2 {
                        x: (rect.origin.x + rect.size.width) as f32,
                        y: (rect.origin.y + rect.size.height) as f32,
                    },
                };
                if rects_equal_within(&win, &active_bounds, 1.0) {
                    return true;
                }
            }
            false
        }
    }

    fn rects_equal_within(a: &PetRect, b: &PetRect, tol: f32) -> bool {
        (a.min.x - b.min.x).abs() <= tol
            && (a.min.y - b.min.y).abs() <= tol
            && (a.max.x - b.max.x).abs() <= tol
            && (a.max.y - b.max.y).abs() <= tol
    }

    // Helper: read an i64 from a CFDictionary by string key
    unsafe fn dict_get_i64(dict: CFDictionaryRef, key: &str) -> Option<i64> {
        use objc2_core_foundation::{CFNumber, CFNumberGetValue, CFNumberType, CFString};
        let key_cfstr = CFString::from_str(key);
        let value: *const std::ffi::c_void = std::ptr::null();
        let mut value = value;
        let found = objc2_core_foundation::CFDictionaryGetValueIfPresent(
            dict,
            key_cfstr.as_concrete_TypeRef() as *const _,
            &mut value,
        );
        if !found || value.is_null() {
            return None;
        }
        let mut out: i64 = 0;
        let ok = CFNumberGetValue(value as *const CFNumber, CFNumberType::SInt64Type, &mut out as *mut i64 as *mut _);
        if ok { Some(out) } else { None }
    }

    unsafe fn dict_get_dict(dict: CFDictionaryRef, key: &str) -> Option<CFDictionaryRef> {
        use objc2_core_foundation::CFString;
        let key_cfstr = CFString::from_str(key);
        let value: *const std::ffi::c_void = std::ptr::null();
        let mut value = value;
        let found = objc2_core_foundation::CFDictionaryGetValueIfPresent(
            dict,
            key_cfstr.as_concrete_TypeRef() as *const _,
            &mut value,
        );
        if !found || value.is_null() {
            None
        } else {
            Some(value as CFDictionaryRef)
        }
    }
```

> **Note for the implementer:** The exact APIs for unwrapping `CFArray`/`CFDictionary` in `objc2-core-foundation` 0.3 vary by minor version. If `info.count()` and `info.value_at_index(i)` don't exist on `Retained<CFArray>`, fall back to `CFArrayGetCount` and `CFArrayGetValueAtIndex` extern calls. If `CFString::from_str` and `as_concrete_TypeRef` don't exist with those exact names, use `CFString::new(key)` and `as_ref() as *const _`. The functional contract is: iterate windows, filter by layer + own-PID, parse bounds dict, compare against active display bounds.

Stub (`#[cfg(not(target_os = "macos"))]`):

```rust
    use crate::physics::Rect as PetRect;
    pub fn any_fullscreen_on(_active_bounds: PetRect, _our_pid: i32) -> bool {
        false
    }
```

- [ ] **Step 2: Add cadence gate + plumb into `tick()`**

Add to `WorkspaceObserver` struct:

```rust
    last_fullscreen_poll_at: Option<Instant>,
```

Initialize in `new`:

```rust
            last_fullscreen_poll_at: None,
```

Inside `tick()`, before the `self.last_snapshot = ...` assignment:

```rust
        let fullscreen_active = {
            let due = self
                .last_fullscreen_poll_at
                .map_or(true, |t| now.saturating_duration_since(t) >= std::time::Duration::from_millis(500));
            match (due, self.active_display.as_ref()) {
                (true, Some(display)) => {
                    self.last_fullscreen_poll_at = Some(now);
                    let pid = std::process::id() as i32;
                    macos_polling::any_fullscreen_on(display.bounds_logical, pid)
                }
                _ => self.last_snapshot.fullscreen_active,
            }
        };
```

Update the snapshot construction:

```rust
            fullscreen_active,
```

(remove the existing line that carries it forward.)

- [ ] **Step 3: Build**

Run: `cargo check`
Expected: clean. If symbols don't match, follow the implementer note above and adjust.

- [ ] **Step 4: Commit**

```bash
git add src/workspace.rs
git commit -m "feat(workspace): detect fullscreen on the active display via CGWindowList"
```

---

### Task 14: Implement AX caret rect polling (macOS) — `caret_rect` populated when permission granted

**Files:**
- Modify: `src/workspace.rs`

- [ ] **Step 1: Add the AX helpers (macOS)**

Extend `mod macos_polling`:

```rust
    use objc2_application_services::{
        AXIsProcessTrusted, AXUIElement, AXUIElementCopyAttributeValue, AXUIElementCopyParameterizedAttributeValue,
        AXUIElementCreateSystemWide, AXUIElementSetMessagingTimeout, AXValue, AXValueGetValue,
        kAXBoundsForRangeParameterizedAttribute, kAXFocusedUIElementAttribute,
        kAXSelectedTextRangeAttribute, kAXValueCGRectType,
    };

    pub fn is_ax_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    pub fn caret_rect_quartz() -> Option<PetRect> {
        unsafe {
            let systemwide: objc2::rc::Retained<AXUIElement> = AXUIElementCreateSystemWide();
            AXUIElementSetMessagingTimeout(&systemwide, 0.1);

            // Focused element
            let focused_value: Option<objc2::rc::Retained<objc2_core_foundation::CFType>> =
                AXUIElementCopyAttributeValue(&systemwide, kAXFocusedUIElementAttribute).ok()?;
            let focused: objc2::rc::Retained<AXUIElement> = focused_value.cast()?;
            AXUIElementSetMessagingTimeout(&focused, 0.1);

            // Selected text range
            let range_value: objc2::rc::Retained<objc2_core_foundation::CFType> =
                AXUIElementCopyAttributeValue(&focused, kAXSelectedTextRangeAttribute).ok()?;

            // Parameterized bounds-for-range
            let bounds_value: objc2::rc::Retained<objc2_core_foundation::CFType> =
                AXUIElementCopyParameterizedAttributeValue(
                    &focused,
                    kAXBoundsForRangeParameterizedAttribute,
                    &range_value,
                )
                .ok()?;
            let ax_value: objc2::rc::Retained<AXValue> = bounds_value.cast()?;
            let mut rect = objc2_core_graphics::CGRect::default();
            let ok = AXValueGetValue(
                &ax_value,
                kAXValueCGRectType,
                &mut rect as *mut _ as *mut std::ffi::c_void,
            );
            if !ok {
                return None;
            }
            Some(PetRect {
                min: crate::physics::Vec2 { x: rect.origin.x as f32, y: rect.origin.y as f32 },
                max: crate::physics::Vec2 {
                    x: (rect.origin.x + rect.size.width) as f32,
                    y: (rect.origin.y + rect.size.height) as f32,
                },
            })
        }
    }
```

Stub:

```rust
    pub fn is_ax_trusted() -> bool { true }
    pub fn caret_rect_quartz() -> Option<PetRect> { None }
```

> **Implementer note:** The exact `.cast()` / `.ok()?` shapes depend on whether `objc2-application-services` returns `Result<Retained<CFType>, AXError>` or `Option<Retained<CFType>>` at the pinned version. If `Result`, use `.ok()?`; if `Option`, use `?` directly. The cast from `CFType` to `AXUIElement` / `AXValue` may need `Retained::downcast` (returns Result), `Retained::cast_unchecked`, or a manual `unsafe` pointer cast — pick whichever the version exposes safely. The functional contract is unchanged: focused → range → parameterized bounds → CGRect.

- [ ] **Step 2: Add cadence gate + plumb into `tick()`**

Add to `WorkspaceObserver` struct:

```rust
    last_caret_poll_at: Option<Instant>,
```

Initialize:

```rust
            last_caret_poll_at: None,
```

Inside `tick()`, before snapshot construction:

```rust
        let caret_rect = {
            let due = self
                .last_caret_poll_at
                .map_or(true, |t| now.saturating_duration_since(t) >= std::time::Duration::from_millis(250));
            if due && self.is_accessibility_trusted() {
                self.last_caret_poll_at = Some(now);
                macos_polling::caret_rect_quartz()
            } else if due {
                self.last_caret_poll_at = Some(now);
                None
            } else {
                self.last_snapshot.caret_rect
            }
        };
```

Update snapshot:

```rust
            caret_rect,
```

- [ ] **Step 3: Update `is_accessibility_trusted` to use the real call**

Replace the stub body inside `is_accessibility_trusted`:

```rust
    pub fn is_accessibility_trusted(&self) -> bool {
        macos_polling::is_ax_trusted()
    }
```

- [ ] **Step 4: Build**

Run: `cargo check`
Expected: clean. Apply implementer-note adjustments as needed.

- [ ] **Step 5: Commit**

```bash
git add src/workspace.rs
git commit -m "feat(workspace): poll AX caret rect at 250ms when permission granted"
```

---

### Task 15: Wire AX prompt entry points (`request_accessibility_on_startup_if_enabled`, `request_accessibility_now`)

**Files:**
- Modify: `src/workspace.rs`

- [ ] **Step 1: Add the prompting helper (macOS)**

Extend `mod macos_polling`:

```rust
    use objc2_application_services::{AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt};
    use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};

    pub fn ax_request_prompt() -> bool {
        unsafe {
            let key: objc2::rc::Retained<CFString> = kAXTrustedCheckOptionPrompt.clone();
            let value: objc2::rc::Retained<CFBoolean> = CFBoolean::true_value();
            let dict = CFDictionary::from_pairs(&[(key.as_ref() as &_, value.as_ref() as &_)]);
            AXIsProcessTrustedWithOptions(Some(&dict))
        }
    }
```

Stub:

```rust
    pub fn ax_request_prompt() -> bool { true }
```

> **Implementer note on the constant import:** If `kAXTrustedCheckOptionPrompt` is exported as a `CFString` constant by `objc2-application-services`, the `.clone()` works. If it's exported as a raw pointer or static, replace the borrow accordingly (e.g., `CFString::from_static_ptr(kAXTrustedCheckOptionPrompt)`). If the crate version pinned in `Cargo.toml` does NOT export it at all, declare an extern block at the top of `mod macos_polling`:
>
> ```rust
> #[link(name = "ApplicationServices", kind = "framework")]
> extern "C" {
>     pub static kAXTrustedCheckOptionPrompt: *const std::ffi::c_void;
> }
> ```
>
> and convert to a `CFString` via `CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt as _)` or equivalent. The crate path is preferred; the extern is the documented fallback.

- [ ] **Step 2: Wire the helper into the two prompt methods**

Replace the bodies in `src/workspace.rs`:

```rust
    pub fn request_accessibility_on_startup_if_enabled(&mut self, avoid_text_cursor: bool) {
        if !avoid_text_cursor || self.prompted_for_accessibility_at_startup {
            return;
        }
        self.prompted_for_accessibility_at_startup = true;
        let _ = macos_polling::ax_request_prompt();
    }

    pub fn request_accessibility_now(&mut self) {
        let _ = macos_polling::ax_request_prompt();
    }
```

- [ ] **Step 3: Add tests for the prompt-flag semantics**

Append to `mod tests`:

```rust
    #[test]
    fn startup_prompt_is_no_op_when_disabled() {
        let mut o = WorkspaceObserver::new();
        o.request_accessibility_on_startup_if_enabled(false);
        // No way to assert "prompt was not called" directly without injection; verify
        // the gate flag does not flip.
        assert!(!o.prompted_for_accessibility_at_startup);
    }

    #[test]
    fn startup_prompt_flips_flag_after_first_call_when_enabled() {
        let mut o = WorkspaceObserver::new();
        o.request_accessibility_on_startup_if_enabled(true);
        assert!(o.prompted_for_accessibility_at_startup);
        // Second call is a no-op (re-running does not re-prompt; can't observe directly,
        // but the flag stays set and the method returns immediately).
        o.request_accessibility_on_startup_if_enabled(true);
        assert!(o.prompted_for_accessibility_at_startup);
    }
```

> **Note:** `prompted_for_accessibility_at_startup` needs to be `pub(crate)` (or accessed via a `#[cfg(test)] pub(crate) fn was_prompted_at_startup(&self) -> bool { self.prompted_for_accessibility_at_startup }` accessor) for the test to see it. Add either change to `src/workspace.rs`.

- [ ] **Step 4: Build + test**

Run: `cargo check && cargo test --lib workspace -- --nocapture`
Expected: clean, all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/workspace.rs
git commit -m "feat(workspace): wire AX startup-once and always-prompt entry points"
```

---

### Task 16: Wire `set_active_display` and Y-flip (NSEvent.mouseLocation → pet space)

**Files:**
- Modify: `src/workspace.rs`

- [ ] **Step 1: Add NSEvent mouse-location polling helper (macOS)**

Extend `mod macos_polling`:

```rust
    use objc2_app_kit::NSEvent;
    use objc2_foundation::NSPoint;

    pub fn cursor_cocoa_location() -> (f32, f32) {
        unsafe {
            let p: NSPoint = NSEvent::mouseLocation();
            (p.x as f32, p.y as f32)
        }
    }
```

Stub:

```rust
    pub fn cursor_cocoa_location() -> (f32, f32) { (0.0, 0.0) }
```

- [ ] **Step 2: Plumb cursor + Y-flip into `tick()`**

Inside `tick()`, before snapshot construction:

```rust
        let cursor_pos = if let Some(display) = self.active_display.as_ref() {
            let (cx, cy_cocoa) = macos_polling::cursor_cocoa_location();
            let cy_quartz = cocoa_to_quartz_y(cy_cocoa, display.primary_display_height);
            crate::physics::Vec2 { x: cx, y: cy_quartz }
        } else {
            self.last_snapshot.cursor_pos
        };
```

Update snapshot:

```rust
            cursor_pos,
```

- [ ] **Step 3: Build + commit**

Run: `cargo check`
Expected: clean.

```bash
git add src/workspace.rs
git commit -m "feat(workspace): poll cursor location and Y-flip into pet space"
```

---

## Phase 3 — DesktopPetApp Integration

### Task 17: Add `auto_hidden` field + `effective_window_visible` + `apply_window_visibility`

**Files:**
- Modify: `src/app.rs`
- Test: `src/app.rs` `mod tests`

- [ ] **Step 1: Write the failing test**

Append to `src/app.rs` `mod tests`:

```rust
    #[test]
    fn effective_window_visible_truth_table() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        app.pet_visible = true;
        app.auto_hidden = false;
        assert!(app.effective_window_visible());

        app.auto_hidden = true;
        assert!(!app.effective_window_visible());

        app.pet_visible = false;
        app.auto_hidden = false;
        assert!(!app.effective_window_visible());

        app.auto_hidden = true;
        assert!(!app.effective_window_visible());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib effective_window_visible_truth_table -- --nocapture`
Expected: FAIL — `no field 'auto_hidden'` and `no method 'effective_window_visible'`.

- [ ] **Step 3: Add the field and accessors**

In `src/app.rs`:

1. Add `auto_hidden: bool,` to the `DesktopPetApp` struct (next to `pet_visible: bool,`).
2. Initialize `auto_hidden: false,` in BOTH `DesktopPetApp::new` and `#[cfg(test)] new_with_event_proxy`.
3. Add the methods (inside `impl DesktopPetApp`):

```rust
    fn effective_window_visible(&self) -> bool {
        self.pet_visible && !self.auto_hidden
    }

    fn apply_window_visibility(&mut self) {
        let visible = self.effective_window_visible();
        if let Some(window) = &self.window {
            window.set_visible(visible);
            if visible {
                window.request_redraw();
            }
        }
        if visible {
            self.next_tick_at = Instant::now();
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib effective_window_visible_truth_table -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): add auto_hidden + effective_window_visible + apply_window_visibility"
```

---

### Task 18: Refactor `set_pet_visible` to use `apply_window_visibility`, add `set_auto_hidden` with drag termination

**Files:**
- Modify: `src/app.rs`
- Test: `src/app.rs` `mod tests`

- [ ] **Step 1: Write the failing test**

Append to `src/app.rs` `mod tests`:

```rust
    #[test]
    fn set_auto_hidden_does_not_modify_settings() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        app.settings.pet_visible = true;
        app.set_auto_hidden(true);
        assert!(app.settings.pet_visible, "auto-hide must not touch the persisted setting");
        assert!(app.auto_hidden);
    }

    #[test]
    fn set_auto_hidden_persistence_sequence() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        app.pet_visible = true;
        app.auto_hidden = false;

        app.set_auto_hidden(true);
        assert!(!app.effective_window_visible());

        app.set_pet_visible(false);
        assert!(!app.effective_window_visible());

        app.set_auto_hidden(false);
        assert!(!app.effective_window_visible(), "pet_visible drives the final result");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib set_auto_hidden -- --nocapture`
Expected: FAIL — `no method 'set_auto_hidden'`.

- [ ] **Step 3: Implement `set_auto_hidden` and refactor `set_pet_visible`**

In `src/app.rs`, add:

```rust
    pub fn set_auto_hidden(&mut self, hidden: bool) {
        if hidden && self.interaction.is_dragging() {
            let last_pointer = self
                .last_cursor_screen_position
                .unwrap_or(self.physics.position);
            let events = self.interaction.mouse_released(
                last_pointer,
                MouseButtonKind::Left,
                /*hit_visible_pixel=*/ false,
            );
            self.handle_interaction_events(events);
        }
        self.auto_hidden = hidden;
        self.apply_window_visibility();
    }
```

Replace the body of `set_pet_visible` (currently app.rs:363-380) with:

```rust
    #[allow(dead_code)]
    fn set_pet_visible(&mut self, visible: bool) {
        self.settings.pet_visible = visible;
        self.pet_visible = visible;
        self.pet.set_hidden(!visible);
        self.apply_window_visibility();
        self.sync_settings_window();
        self.sync_menu_bar();
        self.save_settings();
    }
```

- [ ] **Step 4: Run tests + verify pre-existing pet-visibility tests still pass**

Run: `cargo test --lib -- --nocapture`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): add set_auto_hidden with drag termination; refactor set_pet_visible"
```

---

### Task 19: Update `next_tick_interval` precedence (settings-visible > !pet_visible > auto_hidden > existing)

**Files:**
- Modify: `src/app.rs`
- Modify: `src/settings_window_macos.rs` (add `is_visible()` accessor; non-macOS stub too)

- [ ] **Step 1: Add `is_visible()` to `SettingsWindowController`**

In `src/settings_window_macos.rs`, inside the macOS `impl SettingsWindowController`:

```rust
        pub fn is_visible(&self) -> bool {
            unsafe { self.panel.isVisible() }
        }
```

In the non-macOS stub for `SettingsWindowController` (if it exists in the file):

```rust
        pub fn is_visible(&self) -> bool {
            false
        }
```

- [ ] **Step 2: Update `next_tick_interval` precedence**

In `src/app.rs`, replace the body of `fn next_tick_interval(&self) -> Duration` (currently app.rs:583-600) with:

```rust
    fn next_tick_interval(&self) -> Duration {
        let settings_visible = self
            .settings_window
            .as_ref()
            .map_or(false, |w| w.is_visible());
        if settings_visible {
            return Duration::from_millis(500);
        }
        if !self.pet_visible {
            return Duration::from_secs(5);
        }
        if self.auto_hidden {
            return Duration::from_millis(500);
        }
        match self.pet.behavior_mode() {
            crate::pet::BehaviorMode::Hovered
            | crate::pet::BehaviorMode::Dragging
            | crate::pet::BehaviorMode::Action
            | crate::pet::BehaviorMode::Walking => TARGET_FRAME_TIME,
            crate::pet::BehaviorMode::Hidden => Duration::from_secs(5),
            crate::pet::BehaviorMode::Default => match self.pet.state() {
                PetState::Walk => TARGET_FRAME_TIME,
                PetState::Idle => IDLE_FRAME_TIME,
                PetState::Sleep => SLEEP_FRAME_TIME,
            },
        }
    }
```

- [ ] **Step 3: Make `open_settings_window` wake the loop immediately after `show()`**

Find `fn open_settings_window` in `src/app.rs` (around line 504-526). After the `settings_window.show();` call (around line 522), add:

```rust
            self.next_tick_at = Instant::now();
```

- [ ] **Step 4: Build**

Run: `cargo check`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs src/settings_window_macos.rs
git commit -m "feat(app): tick precedence for settings-visible and auto_hidden"
```

---

### Task 20: Wire `WorkspaceObserver` into `DesktopPetApp` + intent dispatch in `tick`

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add the observer field**

In `src/app.rs`, add to the `DesktopPetApp` struct (next to other state):

```rust
    workspace_observer: crate::workspace::WorkspaceObserver,
```

Initialize in BOTH constructors (`new` and `#[cfg(test)] new_with_event_proxy`):

```rust
            workspace_observer: crate::workspace::WorkspaceObserver::new(),
```

- [ ] **Step 2: Push `DisplayInfo` into the observer when bounds update**

In `src/app.rs`, find `update_bounds_from_window` (around line 213). After the existing block that sets `self.physics.bounds = Bounds { ... }`, add:

```rust
        // Keep the workspace observer's active-display info in lockstep with physics bounds.
        if let Some(monitor) = self
            .window
            .as_ref()
            .and_then(|w| w.current_monitor())
        {
            let scale = monitor.scale_factor() as f32;
            let primary_height = event_loop
                .primary_monitor()
                .map(|m| (m.size().height as f32) / (m.scale_factor() as f32))
                .unwrap_or(0.0);
            self.workspace_observer
                .set_active_display(Some(crate::workspace::DisplayInfo {
                    name: monitor.name(),
                    bounds_logical: self.physics.bounds.into(),
                    scale_factor: scale,
                    primary_display_height: primary_height,
                }));
        }
```

- [ ] **Step 3: Wire the observer into `tick`, dispatch intent, drive auto-hide**

In `src/app.rs`, modify `fn tick(&mut self, now: Instant)` (around line 252). After the existing `let dt = ...; self.last_tick = now;` block and BEFORE `let tick = self.pet.tick(dt);`, insert:

```rust
        // 1) Poll workspace state (owned tick releases the &mut observer borrow).
        let workspace_tick = self.workspace_observer.tick(now);
        let snapshot = workspace_tick.snapshot;

        // 2) If AX trust changed, refresh the Settings UI.
        if workspace_tick.trust_changed {
            self.sync_settings_window();
        }

        // 3) Decide intent and push to pet.
        let pet_frame = crate::physics::Rect {
            min: self.physics.position,
            max: crate::physics::Vec2 {
                x: self.physics.position.x + self.physics.size.x,
                y: self.physics.position.y + self.physics.size.y,
            },
        };
        let intent = decide_intent(&snapshot, &self.settings, pet_frame);
        self.pet.set_intent(intent);

        // 4) Drive auto-hide based on fullscreen.
        let should_auto_hide = self.settings.hide_on_fullscreen && snapshot.fullscreen_active;
        if should_auto_hide != self.auto_hidden {
            self.set_auto_hidden(should_auto_hide);
        }
```

Then gate the existing `window.request_redraw();` at the end of `tick` on visibility:

```rust
        self.move_window_to_pet();
        if self.effective_window_visible() {
            window.request_redraw();
        }
```

- [ ] **Step 4: Build**

Run: `cargo check`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): wire WorkspaceObserver into tick with intent + auto-hide dispatch"
```

---

### Task 21: Run startup AX prompt after `apply_settings` in `create_window`

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add the call**

In `src/app.rs`, find the existing `self.apply_settings(settings);` line inside the window-creation path (around line 162). Immediately after it (and before `if !self.pet_visible {` at line 163), add:

```rust
        self.workspace_observer
            .request_accessibility_on_startup_if_enabled(self.settings.avoid_text_cursor);
```

- [ ] **Step 2: Build**

Run: `cargo check`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): prompt for Accessibility once after settings load"
```

---

### Task 22: Add command handlers for the 4 new `AppCommand` variants

**Files:**
- Modify: `src/app.rs`
- Test: `src/app.rs` `mod tests`

- [ ] **Step 1: Write the failing test**

Append to `src/app.rs` `mod tests`:

```rust
    #[test]
    fn handle_command_set_follow_cursor_updates_settings() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        app.settings.follow_cursor_when_idle = true;
        app.handle_app_command(AppCommand::SetFollowCursorWhenIdle(false));
        assert!(!app.settings.follow_cursor_when_idle);
    }

    #[test]
    fn handle_command_set_hide_on_fullscreen_updates_settings() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        app.settings.hide_on_fullscreen = true;
        app.handle_app_command(AppCommand::SetHideOnFullscreen(false));
        assert!(!app.settings.hide_on_fullscreen);
    }

    #[test]
    fn handle_command_set_avoid_text_cursor_updates_settings() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        app.settings.avoid_text_cursor = false;
        app.handle_app_command(AppCommand::SetAvoidTextCursor(true));
        assert!(app.settings.avoid_text_cursor);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib handle_command -- --nocapture`
Expected: FAIL.

- [ ] **Step 3: Add the handler arms**

In `src/app.rs`, find the existing match in `handle_app_command` (or whatever the command-dispatch fn is named — look near where `AppCommand::SetPersonality` is handled). Add these arms:

```rust
            AppCommand::SetFollowCursorWhenIdle(value) => {
                self.settings.follow_cursor_when_idle = value;
                self.sync_settings_window();
                self.save_settings();
            }
            AppCommand::SetAvoidTextCursor(value) => {
                self.settings.avoid_text_cursor = value;
                self.sync_settings_window();
                self.save_settings();
                if value && !self.workspace_observer.is_accessibility_trusted() {
                    self.workspace_observer.request_accessibility_now();
                }
            }
            AppCommand::SetHideOnFullscreen(value) => {
                self.settings.hide_on_fullscreen = value;
                self.sync_settings_window();
                self.save_settings();
            }
            AppCommand::RequestAccessibilityPermission => {
                self.workspace_observer.request_accessibility_now();
            }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib handle_command -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): handle SetFollowCursorWhenIdle/AvoidTextCursor/HideOnFullscreen/RequestAccessibility"
```

---

### Task 23: Change `sync_settings` signature to carry `ax_trusted: bool`, update all callsites

**Files:**
- Modify: `src/settings_window_macos.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Change `SettingsWindowController::sync_settings` signature**

In `src/settings_window_macos.rs`, change:

```rust
        pub fn sync_settings(&self, settings: &AppSettings) {
            set_show_hide_title(&self.show_hide_button, settings.pet_visible);
            set_focus_mode_title(&self.focus_mode_button, settings.focus_mode);
        }
```

to:

```rust
        pub fn sync_settings(&self, settings: &AppSettings, ax_trusted: bool) {
            set_show_hide_title(&self.show_hide_button, settings.pet_visible);
            set_focus_mode_title(&self.focus_mode_button, settings.focus_mode);
            // Workspace Awareness controls + AX label are wired in later tasks;
            // this signature change lands first so all callsites pass `ax_trusted`.
            let _ = ax_trusted;
        }
```

Apply the same signature change to the non-macOS stub `SettingsWindowController::sync_settings` (it ignores both args).

- [ ] **Step 2: Update all four callsites in `src/app.rs`**

Find both direct `settings_window.sync_settings(&self.settings)` invocations (around `open_settings_window` near line 521 AND inside the wrapper `sync_settings_window` near line 530).

Update each to:

```rust
            settings_window.sync_settings(&self.settings, self.workspace_observer.is_accessibility_trusted());
```

The two indirect callers (`apply_settings` at line 327 and `set_pet_visible` at line 377) call `self.sync_settings_window()` which now forwards `ax_trusted` internally — no change at those sites.

- [ ] **Step 3: Build + test**

Run: `cargo check && cargo test --lib -- --nocapture`
Expected: clean, all existing tests still PASS.

- [ ] **Step 4: Commit**

```bash
git add src/settings_window_macos.rs src/app.rs
git commit -m "refactor(settings): sync_settings(&self, settings, ax_trusted)"
```

---

## Phase 4 — Settings UI

### Task 24: Resize Settings panel and shift all existing controls by +190

**Files:**
- Modify: `src/settings_window_macos.rs`

- [ ] **Step 1: Bump `PANEL_HEIGHT` and shift content view**

In `src/settings_window_macos.rs`:

1. Change `const PANEL_HEIGHT: f64 = 370.0;` to `const PANEL_HEIGHT: f64 = 560.0;`.

2. Find every `setFrame(rect(_, y_value, _, _))` call in the file (search for `setFrame`). Add `190.0` to every `y_value`:
    - `add_title`: `322.0 → 512.0`
    - `add_personality_control`: label `264.0 → 454.0`, control `262.0 → 452.0`
    - `add_monitor_control`: label `230.0 → 420.0`, control `228.0 → 418.0`
    - `add_slider` callers: 198.0 → 388.0, 156.0 → 346.0, 114.0 → 304.0 (these are the slider y args passed in by `Self::new`; bump each call site)
    - `add_buttons`: each of the bottom buttons (28.0 → 218.0, 64.0 → 254.0). Look for `rect(MARGIN_X, 28.0, ...)`, `rect(168.0, 28.0, ...)`, `rect(168.0, 64.0, ...)`, `rect(292.0, 28.0, ...)`.

3. Inside `add_label` (the helper function), no change needed — y is a parameter.

4. The label-helper internal y is unchanged because callers pass shifted values.

- [ ] **Step 2: Build + open the app once manually to verify layout**

Run: `cargo run` (on macOS), open Happy Cappy → menu bar `HC` → Settings.

Expected: panel is taller; existing controls (title, personality, display, sliders, bottom buttons) are positioned in the top portion; the lower portion (y=22..212) is empty awaiting Task 25.

- [ ] **Step 3: Commit**

```bash
git add src/settings_window_macos.rs
git commit -m "feat(settings_window): resize panel to 560 and shift existing controls +190"
```

---

### Task 25: Add the Workspace Awareness section heading + 3 checkboxes

**Files:**
- Modify: `src/settings_window_macos.rs`

- [ ] **Step 1: Add controller fields**

In `src/settings_window_macos.rs`, extend the `SettingsWindowController` struct:

```rust
        follow_cursor_when_idle_button: Retained<NSButton>,
        avoid_text_cursor_button: Retained<NSButton>,
        hide_on_fullscreen_button: Retained<NSButton>,
```

- [ ] **Step 2: Add a `add_workspace_section` helper + checkbox factory**

Add at the end of the file (above `#[cfg(test)] mod tests` if present):

```rust
    fn add_workspace_section(
        content_view: &NSView,
        mtm: MainThreadMarker,
        target_object: &AnyObject,
        settings: &AppSettings,
    ) -> (Retained<NSButton>, Retained<NSButton>, Retained<NSButton>) {
        let heading = NSTextField::labelWithString(ns_string!("Workspace Awareness"), mtm);
        heading.setFrame(rect(
            MARGIN_X,
            190.0,
            PANEL_WIDTH - (MARGIN_X * 2.0),
            22.0,
        ));
        content_view.addSubview(&heading);

        let follow = add_checkbox(
            content_view,
            mtm,
            ns_string!("Follow cursor when idle"),
            158.0,
            MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE,
            settings.follow_cursor_when_idle,
            target_object,
        );
        let avoid = add_checkbox(
            content_view,
            mtm,
            ns_string!("Avoid text-cursor area"),
            128.0,
            MENU_TAG_AVOID_TEXT_CURSOR,
            settings.avoid_text_cursor,
            target_object,
        );
        let hide = add_checkbox(
            content_view,
            mtm,
            ns_string!("Auto-hide when any app is fullscreen"),
            98.0,
            MENU_TAG_HIDE_ON_FULLSCREEN,
            settings.hide_on_fullscreen,
            target_object,
        );
        (follow, avoid, hide)
    }

    fn add_checkbox(
        content_view: &NSView,
        mtm: MainThreadMarker,
        title: &objc2_foundation::NSString,
        y: f64,
        tag: isize,
        initial_state: bool,
        target_object: &AnyObject,
    ) -> Retained<NSButton> {
        let button = NSButton::initWithFrame(
            NSButton::alloc(mtm),
            rect(MARGIN_X, y, PANEL_WIDTH - (MARGIN_X * 2.0), 22.0),
        );
        unsafe {
            button.setButtonType(objc2_app_kit::NSButtonType::Switch);
            button.setTitle(title);
            button.setState(if initial_state {
                objc2_app_kit::NSControlStateValueOn
            } else {
                objc2_app_kit::NSControlStateValueOff
            });
            button.setTag(tag as NSInteger);
            button.setTarget(Some(target_object));
            button.setAction(Some(CommandTarget::settings_value_selector()));
        }
        content_view.addSubview(&button);
        button
    }
```

Add to the top imports next to other `MENU_TAG_*`:

```rust
        menu_bar::{
            MENU_TAG_AVOID_TEXT_CURSOR,
            MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE,
            MENU_TAG_HIDE_ON_FULLSCREEN,
            MENU_TAG_HOVER_INTENSITY, MENU_TAG_MONITOR_BEHAVIOR,
            MENU_TAG_MOVEMENT_SPEED, MENU_TAG_PERSONALITY, MENU_TAG_SCALE,
        },
```

- [ ] **Step 3: Call the helper from `Self::new` and store the buttons**

Inside `Self::new`, after `let (show_hide_button, focus_mode_button) = add_buttons(...);`, add:

```rust
            let (follow_cursor_when_idle_button, avoid_text_cursor_button, hide_on_fullscreen_button) =
                add_workspace_section(&content_view, mtm, target_object, settings);
```

Extend the final `Some(Self { ... })` to include the new fields.

- [ ] **Step 4: Build + visually verify**

Run: `cargo run`, open Settings. Expect the three new checkboxes labeled correctly, all checked by default (matching settings defaults).

- [ ] **Step 5: Commit**

```bash
git add src/settings_window_macos.rs
git commit -m "feat(settings_window): add Workspace Awareness section with 3 checkboxes"
```

---

### Task 26: Add AX status label + Re-request button + sync logic

**Files:**
- Modify: `src/settings_window_macos.rs`

- [ ] **Step 1: Extend controller fields**

Add to the `SettingsWindowController` struct:

```rust
        ax_status_label: Retained<NSTextField>,
        _rerequest_accessibility_button: Retained<NSButton>,
```

- [ ] **Step 2: Add the label + button helpers**

Append to the macOS impl:

```rust
    fn add_ax_status_label(content_view: &NSView, mtm: MainThreadMarker) -> Retained<NSTextField> {
        let label = NSTextField::labelWithString(ns_string!(""), mtm);
        label.setFrame(rect(MARGIN_X, 58.0, PANEL_WIDTH - (MARGIN_X * 2.0), 36.0));
        label.setTag(MENU_TAG_AX_STATUS_LABEL as NSInteger);
        unsafe {
            label.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
            label.setMaximumNumberOfLines(2);
        }
        content_view.addSubview(&label);
        label
    }

    fn add_rerequest_button(
        content_view: &NSView,
        mtm: MainThreadMarker,
        target_object: &AnyObject,
    ) -> Retained<NSButton> {
        let button = NSButton::initWithFrame(
            NSButton::alloc(mtm),
            rect(MARGIN_X, 22.0, 280.0, 28.0),
        );
        unsafe {
            button.setTitle(ns_string!("Re-request Accessibility permission"));
            button.setBezelStyle(objc2_app_kit::NSBezelStyle::Rounded);
            button.setTag(MENU_TAG_REREQUEST_ACCESSIBILITY as NSInteger);
            button.setTarget(Some(target_object));
            button.setAction(Some(CommandTarget::command_selector()));
        }
        content_view.addSubview(&button);
        button
    }
```

Add `MENU_TAG_AX_STATUS_LABEL` and `MENU_TAG_REREQUEST_ACCESSIBILITY` to the top imports.

- [ ] **Step 3: Wire into `Self::new`**

After `add_workspace_section(...)`, add:

```rust
            let ax_status_label = add_ax_status_label(&content_view, mtm);
            let rerequest_accessibility_button = add_rerequest_button(&content_view, mtm, target_object);
```

Extend the final `Some(Self { ... })` to include both. Use `_rerequest_accessibility_button: rerequest_accessibility_button,`.

- [ ] **Step 4: Implement label sync in `sync_settings`**

Replace the body of `sync_settings`:

```rust
        pub fn sync_settings(&self, settings: &AppSettings, ax_trusted: bool) {
            set_show_hide_title(&self.show_hide_button, settings.pet_visible);
            set_focus_mode_title(&self.focus_mode_button, settings.focus_mode);
            self.follow_cursor_when_idle_button.setState(if settings.follow_cursor_when_idle {
                objc2_app_kit::NSControlStateValueOn
            } else {
                objc2_app_kit::NSControlStateValueOff
            });
            self.avoid_text_cursor_button.setState(if settings.avoid_text_cursor {
                objc2_app_kit::NSControlStateValueOn
            } else {
                objc2_app_kit::NSControlStateValueOff
            });
            self.hide_on_fullscreen_button.setState(if settings.hide_on_fullscreen {
                objc2_app_kit::NSControlStateValueOn
            } else {
                objc2_app_kit::NSControlStateValueOff
            });
            let label_text: &objc2_foundation::NSString = if settings.avoid_text_cursor && !ax_trusted {
                ns_string!("Permission needed. If no dialog appears, click Re-request or open System Settings → Privacy & Security → Accessibility.")
            } else {
                ns_string!("")
            };
            self.ax_status_label.setStringValue(label_text);
        }
```

- [ ] **Step 5: Build + visually verify**

Run: `cargo run`. Open Settings. The label area should be blank (assuming AX is trusted) or show the guidance text when `avoid_text_cursor` is on and AX is denied. The Re-request button should be clickable.

- [ ] **Step 6: Commit**

```bash
git add src/settings_window_macos.rs
git commit -m "feat(settings_window): add AX status label and Re-request button"
```

---

### Task 27: Wire `dispatchSettingsValue:` arms for the new checkbox tags

**Files:**
- Modify: `src/command_target_macos.rs`

- [ ] **Step 1: Add the `read_button_state` helper + the new arms**

In `src/command_target_macos.rs`, inside `mod macos`, add this free function at the bottom:

```rust
    fn read_button_state(sender: &AnyObject) -> bool {
        let state: NSInteger = unsafe { msg_send![sender, state] };
        state != 0
    }
```

In `dispatch_settings_value`, extend the `match tag as isize { ... }` block. Add these arms (any position; suggest alphabetical):

```rust
                    crate::menu_bar::MENU_TAG_AVOID_TEXT_CURSOR
                    | crate::menu_bar::MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE
                    | crate::menu_bar::MENU_TAG_HIDE_ON_FULLSCREEN => {
                        let state_is_on = read_button_state(sender);
                        crate::menu_bar::settings_command_for_button(tag as isize, state_is_on)
                    }
```

(Move it into the existing `match` block, not as a stand-alone arm; or use `_` to bind the unhandled fall-through and check for the three tags first.)

- [ ] **Step 2: Build**

Run: `cargo check`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/command_target_macos.rs
git commit -m "feat(command_target): bridge new checkbox tags through dispatchSettingsValue:"
```

---

## Phase 5 — Verification

### Task 28: Run the full test suite and `verify.sh`

**Files:**
- (no edits; runs the existing verification scripts)

- [ ] **Step 1: Run unit tests**

Run: `cargo test --lib -- --nocapture`
Expected: all PASS, including:
- `physics::tests::rect_*` (Task 1)
- `workspace_awareness_settings_tests` (Task 2)
- `pet::tests::set_intent*` (Task 3)
- `workspace::tests` (Tasks 4, 6, 10, 12, 15)
- `decide_intent_tests` (Task 5)
- `menu_bar::tests::settings_command_for_button*` and `command_from_tag_maps_rerequest_accessibility` (Task 7)
- `effective_window_visible_truth_table`, `set_auto_hidden_*` (Tasks 17, 18)
- `handle_command_*` (Task 22)

- [ ] **Step 2: Run `verify.sh`**

Run: `./scripts/verify.sh`
Expected: format, tests, clippy, release build, bundle, codesign verification all PASS.

- [ ] **Step 3: If any failure, fix in place and re-run before continuing**

---

### Task 29: Smoke verification (semi-manual; macOS only)

**Files:**
- (no edits; semi-manual run-through)

- [ ] **Step 1: Build and launch**

Run:
```bash
./scripts/build_app.sh
open "dist/Happy Cappy.app"
```

- [ ] **Step 2: Run through the manual checklist from the spec**

For each item below, verify in order. Report any failure as a follow-up task.

1. [ ] Open Settings; toggle each of the 3 new checkboxes; reopen Settings and confirm state persists across panel reopens.
2. [ ] Idle the machine for ~6 seconds with no app focused; pet should walk toward the cursor at the next walk-cycle boundary.
3. [ ] Bring Xcode (or any editor in the list) to front; pet should walk away from the cursor.
4. [ ] Open YouTube in Safari and go fullscreen; pet should hide within ~1 second. Exit fullscreen; pet returns with previous visibility.
5. [ ] Open a text field (Notes, browser address bar). Move the pet so its frame intersects the caret. Pet should walk away within one tick.
6. [ ] Two-display setup: place pet on display 1, fullscreen Safari on display 2. Pet should stay visible.
7. [ ] Start dragging the pet, then begin fullscreen on the same display. Pet should hide cleanly; position should be persisted; subsequent clicks should not behave as if a drag is still in progress.
8. [ ] In Settings, turn off "Avoid text-cursor area" then back on while AX is denied. Checkbox stays checked; AX status label shows the neutral guidance string.
9. [ ] With Settings still open and pet hidden, grant AX permission in System Settings. Label should clear within ~500 ms.
10. [ ] Click "Re-request Accessibility permission" after denying. The call should land every time (no internal no-op); macOS may or may not show a dialog.

- [ ] **Step 3: Commit any fix-ups discovered during smoke testing**

If issues are found, add a new task (or sub-step) to address them and re-run the affected portions of the checklist.

---

## Self-Review Summary

Run after writing this plan:

1. **Spec coverage:**
   - Goals 1–3: Tasks 5 (decide_intent for chase/avoid + caret avoidance) + 20 (intent dispatch) + 23 (auto-hide on fullscreen).
   - Three independent toggles: Task 2 (settings) + Tasks 25, 26 (UI) + Task 22 (handlers).
   - Coordinate system normalization: Tasks 6 (Y-flip), 16 (cursor), 14 (caret rect), 13 (fullscreen).
   - DisplayInfo + active-display lifecycle: Task 20 (set_active_display alongside physics bounds).
   - Three AX entry points (startup-once, runtime-toggle, Re-request): Tasks 15 (impl) + 21 (startup hook) + 22 (handlers).
   - Cargo deps with all required features: Task 9.
   - Module layout (workspace.rs, Rect, BehaviorIntent, AppCommand variants, MENU_TAGs, settings_command_for_button): Tasks 1, 3, 4, 7, 8.
   - Borrow-checker-safe owned WorkspaceTick: Task 4 (type) + Task 10 (`tick()` returns owned + observer_owns_borrow_releases test).
   - workspace_available stub gate: Task 4 (test + impl) + Task 11 (set true on macOS).
   - Drag termination on auto-hide: Task 18.
   - Tick-interval precedence (settings-visible > !pet_visible > auto_hidden > existing): Task 19.
   - Settings panel resize + new controls + AX label + Re-request button: Tasks 24, 25, 26.
   - dispatchSettingsValue: bridge with read_button_state helper: Task 27.
   - sync_settings(&self, settings, ax_trusted) signature + all callsites: Task 23.
   - trust-transition refresh: Task 20 (`if workspace_tick.trust_changed { sync_settings_window() }`).
   - Manual verification checklist: Task 29.
   - Pre-existing scale_factor latent bug: deferred per spec — no task in this plan.

2. **Placeholder scan:** All TBDs are inside `> Implementer note:` callouts that explicitly identify *what to verify locally* (exact symbol names from a particular crate version) — these are not "implement later" placeholders. Each note pins the functional contract; only the import name may need tweaking.

3. **Type consistency:**
   - `Rect { min: Vec2, max: Vec2 }` — used identically in tasks 1, 4, 5, 13, 14.
   - `DisplayInfo { name, bounds_logical, scale_factor, primary_display_height }` — Task 10 defines, Task 20 constructs.
   - `BehaviorIntent` variants — Task 3 defines, Task 5 constructs, Task 20 dispatches.
   - `WorkspaceTick { snapshot, trust_changed }` — Task 4 defines, Tasks 10, 11, 12, 13, 14, 16 produce, Task 20 consumes.
   - `AppCommand::Set*` variants — Task 8 adds, Task 22 handles.
   - `MENU_TAG_*` constants (1106-1110) — Task 7 declares, Tasks 25, 26 use, Task 27 routes.
