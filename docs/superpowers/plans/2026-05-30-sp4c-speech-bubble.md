# SP4-C Speech Bubble Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render the active notification's `label`/`body` as a warm speech bubble above the pet, using a native AppKit child window.

**Architecture:** Pure-Rust layers own all testable logic — `bubble.rs` (content + per-kind accent, with the visibility rule) and `bubble_layout.rs` (placement geometry in winit/Quartz **Y-down logical** points). A thin macOS layer `bubble_window_macos.rs` draws a borderless, click-through child `NSWindow` and measures text via `NSTextField`. A required TTL-correctness fix makes notifications expire on wall-clock time even while hidden (split TTL countdown into `tick_notification(true_elapsed)` + a scheduler wake-bound), without changing the existing `tick(dt)` / `set_notification(event)` signatures.

**Tech Stack:** Rust 2021, objc2 / objc2-app-kit (AppKit), winit, the existing `physics`/`workspace`/`notification`/`pet::runtime` modules.

**Spec:** `docs/superpowers/specs/2026-05-30-sp4c-speech-bubble-design.md`

---

## File Structure

- **Create** `src/bubble.rs` — `BubbleContent`, `BubbleAccent`, the trim/visibility rule, kind→accent + RGBA. Pure Rust, fully unit-tested.
- **Create** `src/bubble_layout.rs` — `place_bubble()`, `BubblePlacement`, `TailSide`, geometry constants. Pure Rust, fully unit-tested.
- **Create** `src/bubble_window_macos.rs` — `BubbleWindow` (stub + `mod macos`), `active_visible_frame_y_down` helper. Platform layer; build + manual smoke.
- **Modify** `src/pet/runtime.rs` — add `kind` to `NotificationState`; add `bubble_content()`, `tick_notification()`, `notification_remaining()`; remove the dt-driven TTL countdown from `tick()`; update two TTL tests.
- **Modify** `src/app.rs` — feed true elapsed to `tick_notification`; clamp the frame scheduler to the notification's remaining; own + drive an `Option<BubbleWindow>`.
- **Modify** `src/lib.rs` — declare `bubble`, `bubble_layout`, `bubble_window_macos`.
- **Modify** `Cargo.toml` — add the `NSBezierPath` feature to `objc2-app-kit`.
- **Modify** `scripts/smoke_app.sh` — add a manual bubble checklist item.

---

## Task 1: Bubble content + accent (`src/bubble.rs`)

**Files:**
- Create: `src/bubble.rs`
- Modify: `src/lib.rs` (add `pub mod bubble;`)

- [ ] **Step 1: Declare the module**

In `src/lib.rs`, add `pub mod bubble;` in alphabetical position (immediately after `pub mod bundle;` is fine — keep the list sorted: it goes before `bundle`). Final ordering near the top:

```rust
pub mod app;
pub mod bubble;
pub mod bundle;
pub mod command_target_macos;
```

- [ ] **Step 2: Write `src/bubble.rs` with the failing tests first**

Create `src/bubble.rs` with ONLY the test module + empty type stubs so it fails to compile/assert:

```rust
//! Pure-Rust speech-bubble content model (SP4-C). No platform dependencies.

/// Per-kind accent for the bubble's dot, derived from a notification `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleAccent {
    Running,
    Message,
    Succeeded,
    NeedsReview,
    Failed,
}

/// What the bubble renders. Constructible directly by any producer; SP4-C
/// derives it from the active notification, but a future producer (e.g. a
/// Hermes agent message) may build one too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BubbleContent {
    pub title: Option<String>,
    pub body: Option<String>,
    pub accent: BubbleAccent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_empty_yields_none() {
        assert_eq!(BubbleContent::from_parts("running", None, None), None);
        assert_eq!(BubbleContent::from_parts("running", Some("   "), Some("\t")), None);
    }

    #[test]
    fn title_only_keeps_title_drops_body() {
        let c = BubbleContent::from_parts("succeeded", Some("  Done  "), Some("   ")).unwrap();
        assert_eq!(c.title.as_deref(), Some("Done"));
        assert_eq!(c.body, None);
        assert_eq!(c.accent, BubbleAccent::Succeeded);
    }

    #[test]
    fn body_only_keeps_body_drops_title() {
        let c = BubbleContent::from_parts("message", None, Some("hello")).unwrap();
        assert_eq!(c.title, None);
        assert_eq!(c.body.as_deref(), Some("hello"));
        assert_eq!(c.accent, BubbleAccent::Message);
    }

    #[test]
    fn accent_maps_known_kinds_and_falls_back_to_message() {
        assert_eq!(BubbleAccent::for_kind("running"), BubbleAccent::Running);
        assert_eq!(BubbleAccent::for_kind("message"), BubbleAccent::Message);
        assert_eq!(BubbleAccent::for_kind("succeeded"), BubbleAccent::Succeeded);
        assert_eq!(BubbleAccent::for_kind("needs-review"), BubbleAccent::NeedsReview);
        assert_eq!(BubbleAccent::for_kind("failed"), BubbleAccent::Failed);
        assert_eq!(BubbleAccent::for_kind("deploy"), BubbleAccent::Message);
    }

    #[test]
    fn needs_review_and_failed_are_emphasized() {
        assert!(BubbleAccent::NeedsReview.emphasized());
        assert!(BubbleAccent::Failed.emphasized());
        assert!(!BubbleAccent::Running.emphasized());
        assert!(!BubbleAccent::Message.emphasized());
        assert!(!BubbleAccent::Succeeded.emphasized());
    }

    #[test]
    fn rgba_is_in_unit_range_and_opaque() {
        for accent in [
            BubbleAccent::Running,
            BubbleAccent::Message,
            BubbleAccent::Succeeded,
            BubbleAccent::NeedsReview,
            BubbleAccent::Failed,
        ] {
            let (r, g, b, a) = accent.rgba();
            for ch in [r, g, b, a] {
                assert!((0.0..=1.0).contains(&ch));
            }
            assert_eq!(a, 1.0);
        }
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --manifest-path Cargo.toml bubble::tests`
Expected: FAIL — `no function or associated item named 'from_parts'` / `'for_kind'` / `'emphasized'` / `'rgba'`.

- [ ] **Step 4: Implement the methods**

Add these `impl` blocks to `src/bubble.rs` (above the `#[cfg(test)]` module):

```rust
impl BubbleAccent {
    /// Map a notification `kind` to an accent. Unknown kinds borrow `Message`
    /// (mirrors `notification::preset_for`'s default).
    pub fn for_kind(kind: &str) -> Self {
        match kind {
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "needs-review" => Self::NeedsReview,
            "failed" => Self::Failed,
            _ => Self::Message,
        }
    }

    /// Dot color as straight-alpha sRGB components in `[0, 1]`.
    pub fn rgba(self) -> (f32, f32, f32, f32) {
        match self {
            Self::Running => (0.243, 0.482, 0.839, 1.0),     // #3E7BD6
            Self::Message => (0.541, 0.565, 0.612, 1.0),     // #8A909C
            Self::Succeeded => (0.243, 0.608, 0.310, 1.0),   // #3E9B4F
            Self::NeedsReview => (0.878, 0.639, 0.180, 1.0), // #E0A32E
            Self::Failed => (0.898, 0.282, 0.302, 1.0),      // #E5484D
        }
    }

    /// `needs-review` / `failed` use a larger dot to draw the eye.
    pub fn emphasized(self) -> bool {
        matches!(self, Self::NeedsReview | Self::Failed)
    }
}

impl BubbleContent {
    /// Build content from a notification's `kind` + raw `label`/`body`. Each
    /// text field is trimmed; empty/whitespace-only becomes `None`. Returns
    /// `None` when BOTH title and body are empty (no bubble is shown).
    pub fn from_parts(kind: &str, label: Option<&str>, body: Option<&str>) -> Option<Self> {
        let clean = |s: Option<&str>| -> Option<String> {
            s.map(str::trim).filter(|t| !t.is_empty()).map(str::to_string)
        };
        let title = clean(label);
        let body = clean(body);
        if title.is_none() && body.is_none() {
            return None;
        }
        Some(Self {
            title,
            body,
            accent: BubbleAccent::for_kind(kind),
        })
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --manifest-path Cargo.toml bubble::tests`
Expected: PASS (6 tests).

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/bubble.rs
git commit -m "feat(sp4c): add pure-Rust BubbleContent + per-kind accent"
```

---

## Task 2: Placement geometry (`src/bubble_layout.rs`)

**Files:**
- Create: `src/bubble_layout.rs`
- Modify: `src/lib.rs` (add `pub mod bubble_layout;`)

- [ ] **Step 1: Declare the module**

In `src/lib.rs`, add `pub mod bubble_layout;` right after `pub mod bubble;`.

- [ ] **Step 2: Write `src/bubble_layout.rs` with failing tests first**

Create `src/bubble_layout.rs`:

```rust
//! Pure-Rust speech-bubble placement geometry (SP4-C). Works entirely in the
//! app's winit/Quartz logical space: primary-display top-left origin, **Y-DOWN**,
//! points — the same space as `physics`, `workspace`, and `move_window_to_pet`.

use crate::physics::{Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailSide {
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BubblePlacement {
    /// Top-left of the bubble, in Y-down logical points.
    pub origin: Vec2,
    pub tail: TailSide,
    /// Tail-tip X measured from `origin.x`, in points.
    pub tail_x: f32,
}

/// Gap between the bubble and the pet.
pub const GAP: f32 = 6.0;
/// Inset kept from the visible-frame edges.
pub const INSET: f32 = 8.0;
/// Corner radius reserved so the tail never overruns a rounded corner.
pub const CORNER_RADIUS: f32 = 11.0;
/// Half-width of the tail-triangle base.
pub const TAIL_HALF: f32 = 7.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Rect {
        Rect {
            min: Vec2 { x: min_x, y: min_y },
            max: Vec2 { x: max_x, y: max_y },
        }
    }

    // A roomy screen with the pet comfortably in the middle.
    fn screen() -> Rect {
        rect(0.0, 0.0, 1000.0, 800.0)
    }

    #[test]
    fn defaults_above_with_down_tail() {
        // pet 64x64 at (468,400); bubble 200x80.
        let pet = rect(468.0, 400.0, 532.0, 464.0);
        let p = place_bubble(pet, (200.0, 80.0), screen());
        assert_eq!(p.tail, TailSide::Down);
        // bottom edge = pet.min.y - GAP => origin.y = 400 - 6 - 80 = 314
        assert_eq!(p.origin.y, 314.0);
        // centered: pet center x = 500 => origin.x = 500 - 100 = 400
        assert_eq!(p.origin.x, 400.0);
        // tail points at pet center: 500 - 400 = 100
        assert_eq!(p.tail_x, 100.0);
    }

    #[test]
    fn flips_below_when_no_room_above() {
        // pet near the top edge: above would land at negative y.
        let pet = rect(468.0, 10.0, 532.0, 74.0);
        let p = place_bubble(pet, (200.0, 80.0), screen());
        assert_eq!(p.tail, TailSide::Up);
        // below: origin.y = pet.max.y + GAP = 74 + 6 = 80
        assert_eq!(p.origin.y, 80.0);
    }

    #[test]
    fn clamps_to_left_edge_with_inset() {
        let pet = rect(0.0, 400.0, 64.0, 464.0); // pet center x = 32
        let p = place_bubble(pet, (200.0, 80.0), screen());
        // origin.x clamped to INSET (8), not 32 - 100 = -68
        assert_eq!(p.origin.x, INSET);
        // tail still points at pet center 32 => 32 - 8 = 24, within body
        assert_eq!(p.tail_x, 24.0);
    }

    #[test]
    fn clamps_to_right_edge_with_inset() {
        let pet = rect(936.0, 400.0, 1000.0, 464.0); // center x = 968
        let p = place_bubble(pet, (200.0, 80.0), screen());
        // max origin.x = 1000 - 8 - 200 = 792
        assert_eq!(p.origin.x, 792.0);
        // tail wants 968 - 792 = 176, clamped to max = 200 - 11 - 7 = 182 -> 176 < 182 ok
        assert_eq!(p.tail_x, 176.0);
    }

    #[test]
    fn tail_x_is_clamped_within_rounded_body() {
        // Pet far right so the tail target would exceed the body; expect clamp.
        let pet = rect(990.0, 400.0, 1000.0, 464.0); // center x = 995
        let p = place_bubble(pet, (200.0, 80.0), screen());
        // origin.x clamped to 792; raw tail = 995 - 792 = 203; max = 200-11-7 = 182
        assert_eq!(p.tail_x, 182.0);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --manifest-path Cargo.toml bubble_layout::tests`
Expected: FAIL — `cannot find function 'place_bubble' in this scope`.

- [ ] **Step 4: Implement `place_bubble`**

Add to `src/bubble_layout.rs` (above the `#[cfg(test)]` module):

```rust
/// Place a bubble of `size` = `(width, height)` above `pet`, within the
/// `visible` frame. Flips below when there isn't room above, and clamps
/// horizontally to the visible frame with `INSET`. All inputs/outputs are in
/// Y-down logical points.
pub fn place_bubble(pet: Rect, size: (f32, f32), visible: Rect) -> BubblePlacement {
    let (w, h) = size;
    let pet_center_x = (pet.min.x + pet.max.x) * 0.5;

    // Horizontal: center on the pet, then clamp into [min+inset, max-inset-w].
    let min_x = visible.min.x + INSET;
    let max_x = (visible.max.x - INSET - w).max(min_x);
    let origin_x = (pet_center_x - w * 0.5).clamp(min_x, max_x);

    // Vertical: default above (smaller Y); flip below if it crosses the top.
    let above_y = pet.min.y - GAP - h;
    let (origin_y, tail) = if above_y >= visible.min.y + INSET {
        (above_y, TailSide::Down)
    } else {
        let below_y = pet.max.y + GAP;
        if below_y + h <= visible.max.y - INSET {
            (below_y, TailSide::Up)
        } else {
            // Neither above nor below fully fits: prefer above, clamp inward.
            (above_y.max(visible.min.y + INSET), TailSide::Down)
        }
    };

    // Tail points at the pet center, clamped within the rounded body.
    let tail_min = CORNER_RADIUS + TAIL_HALF;
    let tail_max = (w - CORNER_RADIUS - TAIL_HALF).max(tail_min);
    let tail_x = (pet_center_x - origin_x).clamp(tail_min, tail_max);

    BubblePlacement {
        origin: Vec2 { x: origin_x, y: origin_y },
        tail,
        tail_x,
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --manifest-path Cargo.toml bubble_layout::tests`
Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/bubble_layout.rs
git commit -m "feat(sp4c): add pure-Rust bubble placement geometry"
```

---

## Task 3: Runtime wiring + TTL split (`src/pet/runtime.rs`)

**Files:**
- Modify: `src/pet/runtime.rs` (`NotificationState`, `set_notification`, `tick`, new methods, tests)

- [ ] **Step 1: Add `kind` to `NotificationState` and drop the dead_code attrs**

Replace the struct at `src/pet/runtime.rs:90-99`:

```rust
#[derive(Debug, Clone)]
struct NotificationState {
    animation_name: String,
    remaining: Duration,
    priority: i32,
    #[allow(dead_code)] // carried for SP4-C (not rendered in SP4-B)
    label: Option<String>,
    #[allow(dead_code)]
    body: Option<String>,
}
```

with (adds `kind`, removes both `#[allow(dead_code)]` — these are now read by `bubble_content`):

```rust
#[derive(Debug, Clone)]
struct NotificationState {
    animation_name: String,
    remaining: Duration,
    priority: i32,
    kind: String,
    label: Option<String>,
    body: Option<String>,
}
```

- [ ] **Step 2: Populate `kind` in `set_notification`**

In `set_notification` (`src/pet/runtime.rs:249-255`), add `kind` to the struct literal:

```rust
        self.notification = Some(NotificationState {
            animation_name: resolved,
            remaining: Duration::from_millis(ttl_ms),
            priority,
            kind: event.kind.clone(),
            label: event.label.clone(),
            body: event.body.clone(),
        });
```

- [ ] **Step 3: Remove the dt-driven TTL countdown from `tick`**

In `tick` (`src/pet/runtime.rs:288-300`), delete this block entirely (the countdown moves to `tick_notification`; the one-shot clear later in `tick` stays):

```rust
        // Notification TTL counts down in every state (hidden / drag / hover included),
        // so a stale notification never lingers behind an obscuring state.
        if let Some(n) = self.notification.as_mut() {
            n.remaining = n.remaining.saturating_sub(dt);
        }
        if self
            .notification
            .as_ref()
            .is_some_and(|n| n.remaining.is_zero())
        {
            self.notification = None;
            self.refresh_behavior_mode();
        }
```

Leave everything else in `tick` unchanged (including the one-shot clear at lines ~320-325).

- [ ] **Step 4: Write the failing tests for the new methods**

In the `#[cfg(test)]` module of `runtime.rs`, add a label/body event helper next to `fn event` (`src/pet/runtime.rs:1523`):

```rust
    fn event_text(kind: &str, label: Option<&str>, body: Option<&str>) -> crate::notification::NotificationEvent {
        crate::notification::NotificationEvent {
            kind: kind.to_string(),
            animation_name: None,
            label: label.map(str::to_string),
            body: body.map(str::to_string),
            ttl_ms: None,
            priority: None,
        }
    }
```

Replace the body of `ttl_expires_and_clears_notification` (`src/pet/runtime.rs:1610-1619`) to drive the new method:

```rust
    #[test]
    fn ttl_expires_and_clears_notification() {
        let mut pet = notify_fixture();
        let mut ev = event("running");
        ev.ttl_ms = Some(100);
        pet.set_notification(&ev);
        pet.tick_notification(Duration::from_millis(60));
        assert!(pet.notification_animation().is_some());
        pet.tick_notification(Duration::from_millis(60)); // total 120 > 100
        assert_eq!(pet.notification_animation(), None);
    }
```

Replace the body of `ttl_counts_down_while_hidden` (`src/pet/runtime.rs:1621-1634`):

```rust
    #[test]
    fn ttl_counts_down_while_hidden() {
        let mut pet = notify_fixture();
        let mut ev = event("running");
        ev.ttl_ms = Some(100);
        pet.set_notification(&ev);
        pet.set_hidden(true);
        pet.tick_notification(Duration::from_millis(120));
        assert_eq!(
            pet.notification_animation(),
            None,
            "TTL must keep counting while hidden"
        );
    }
```

Add new tests at the end of the notify test block (before the closing `}` at `src/pet/runtime.rs:1686`):

```rust
    #[test]
    fn notification_remaining_reports_time_left() {
        let mut pet = notify_fixture();
        let mut ev = event("running");
        ev.ttl_ms = Some(100);
        pet.set_notification(&ev);
        assert_eq!(pet.notification_remaining(), Some(Duration::from_millis(100)));
        pet.tick_notification(Duration::from_millis(40));
        assert_eq!(pet.notification_remaining(), Some(Duration::from_millis(60)));
    }

    #[test]
    fn no_notification_has_no_remaining_and_no_bubble() {
        let pet = notify_fixture();
        assert_eq!(pet.notification_remaining(), None);
        assert_eq!(pet.bubble_content(), None);
    }

    #[test]
    fn bubble_content_none_when_notification_has_no_text() {
        let mut pet = notify_fixture();
        pet.set_notification(&event("running")); // label/body both None
        assert_eq!(pet.bubble_content(), None);
    }

    #[test]
    fn bubble_content_carries_text_and_accent() {
        let mut pet = notify_fixture();
        pet.set_notification(&event_text("failed", Some("Build failed"), Some("3 errors")));
        let c = pet.bubble_content().expect("text present -> Some");
        assert_eq!(c.title.as_deref(), Some("Build failed"));
        assert_eq!(c.body.as_deref(), Some("3 errors"));
        assert_eq!(c.accent, crate::bubble::BubbleAccent::Failed);
    }
```

- [ ] **Step 5: Run the new tests to verify they fail**

Run: `cargo test --manifest-path Cargo.toml -p happy-cappy runtime`
Expected: FAIL — `no method named 'tick_notification'` / `'notification_remaining'` / `'bubble_content'`.

- [ ] **Step 6: Implement the three methods**

Add to the `impl PetRuntime` block, right after `clear_notification` (`src/pet/runtime.rs:262`):

```rust
    /// Count the active notification's TTL down by `elapsed` (the TRUE
    /// wall-clock elapsed since the last frame, NOT the animation-capped `dt`
    /// passed to `tick`). Clears + refreshes when it reaches zero. SP4-C: this
    /// keeps a notification's lifetime wall-clock-accurate even while the pet is
    /// hidden and the frame scheduler is throttled to a coarse interval.
    pub fn tick_notification(&mut self, elapsed: Duration) {
        if let Some(n) = self.notification.as_mut() {
            n.remaining = n.remaining.saturating_sub(elapsed);
        }
        if self
            .notification
            .as_ref()
            .is_some_and(|n| n.remaining.is_zero())
        {
            self.notification = None;
            self.refresh_behavior_mode();
        }
    }

    /// Time left on the active notification, if any. The frame scheduler clamps
    /// its next wake to this so a hidden notification expires on time (SP4-C).
    pub fn notification_remaining(&self) -> Option<Duration> {
        self.notification.as_ref().map(|n| n.remaining)
    }

    /// The active notification rendered as bubble content, or `None` when there
    /// is no notification or it carries no displayable text (SP4-C). Read-only:
    /// it does not touch animation, the countdown, or preemption.
    pub fn bubble_content(&self) -> Option<crate::bubble::BubbleContent> {
        let n = self.notification.as_ref()?;
        crate::bubble::BubbleContent::from_parts(&n.kind, n.label.as_deref(), n.body.as_deref())
    }
```

- [ ] **Step 7: Run the runtime tests to verify they pass**

Run: `cargo test --manifest-path Cargo.toml -p happy-cappy runtime`
Expected: PASS (all runtime tests, including the updated TTL tests and the new ones).

- [ ] **Step 8: Commit**

```bash
git add src/pet/runtime.rs
git commit -m "feat(sp4c): runtime bubble_content + wall-clock TTL split"
```

---

## Task 4: App TTL wiring + scheduler wake-bound (`src/app.rs`)

**Files:**
- Modify: `src/app.rs` (`tick`, `next_tick_interval`, a new pure helper + its test)

- [ ] **Step 1: Write the failing test for the wake-bound helper**

In the `#[cfg(test)]` module of `src/app.rs`, add:

```rust
    #[test]
    fn bounded_tick_interval_clamps_to_notification_remaining() {
        use std::time::Duration;
        let base = Duration::from_secs(5);
        // No notification -> unchanged.
        assert_eq!(bounded_tick_interval(base, None), base);
        // Remaining shorter than base -> clamp to remaining (wake on time).
        assert_eq!(
            bounded_tick_interval(base, Some(Duration::from_millis(800))),
            Duration::from_millis(800)
        );
        // Remaining longer than base -> keep base.
        assert_eq!(
            bounded_tick_interval(base, Some(Duration::from_secs(60))),
            base
        );
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --manifest-path Cargo.toml -p happy-cappy bounded_tick_interval`
Expected: FAIL — `cannot find function 'bounded_tick_interval'`.

- [ ] **Step 3: Add the helper (free function)**

Add near the other module-level constants/helpers in `src/app.rs` (e.g. just below `fn inner_size_for(...)` around line 94):

```rust
/// Clamp a base frame-scheduler interval so the loop never sleeps past an
/// active notification's expiry (SP4-C). Without this, a hidden pet (which
/// ticks every 5s) would notice expiry up to 5s late.
fn bounded_tick_interval(base: Duration, notification_remaining: Option<Duration>) -> Duration {
    match notification_remaining {
        Some(remaining) => base.min(remaining),
        None => base,
    }
}
```

- [ ] **Step 4: Apply the wake-bound at the single exit of `next_tick_interval`**

`next_tick_interval` currently has multiple `return`s and a final `match`. Restructure it so every path computes a `base` and the function clamps once at the end. Replace the whole body of `next_tick_interval` (`src/app.rs:981-1008` — from `let settings_visible` through the closing brace of the function) with:

```rust
    fn next_tick_interval(&self) -> Duration {
        let settings_visible = self
            .settings_window
            .as_ref()
            .is_some_and(|w| w.is_visible());
        let base = if settings_visible {
            Duration::from_millis(500)
        } else if !self.pet_visible {
            Duration::from_secs(5)
        } else if self.auto_hidden {
            Duration::from_millis(500)
        } else {
            match self.pet.behavior_mode() {
                crate::pet::BehaviorMode::Hovered
                | crate::pet::BehaviorMode::Dragging
                | crate::pet::BehaviorMode::Action
                | crate::pet::BehaviorMode::Walking
                | crate::pet::BehaviorMode::Notifying => TARGET_FRAME_TIME,
                crate::pet::BehaviorMode::Hidden => Duration::from_secs(5),
                crate::pet::BehaviorMode::Default => match self.pet.state() {
                    PetState::Walk => TARGET_FRAME_TIME,
                    PetState::Idle => IDLE_FRAME_TIME,
                    PetState::Sleep => SLEEP_FRAME_TIME,
                },
            }
        };
        bounded_tick_interval(base, self.pet.notification_remaining())
    }
```

> NOTE: verify the `PetState` arms match the original `match` you are replacing (the original spans `src/app.rs:1002-1006`). Keep them identical — only the surrounding structure and the trailing `bounded_tick_interval` call are new.

- [ ] **Step 5: Feed the TRUE elapsed to the notification countdown in `tick`**

In `tick` (`src/app.rs:385-392`), the current code is:

```rust
        let dt = now.duration_since(self.last_tick).min(MAX_TICK_DELTA);
        self.last_tick = now;
```

Replace it with (compute the uncapped elapsed first, cap only the animation `dt`):

```rust
        let true_elapsed = now.duration_since(self.last_tick);
        let dt = true_elapsed.min(MAX_TICK_DELTA);
        self.last_tick = now;
```

Then, immediately after the existing `let tick = self.pet.tick(dt);` line (`src/app.rs:421`), add the notification countdown with the uncapped elapsed:

```rust
        self.pet.tick_notification(true_elapsed);
```

> Ordering note: the old code counted the TTL down at the *top* of `PetRuntime::tick`. Calling `tick_notification` right after `pet.tick(dt)` here is equivalent for behavior (both run once per frame before the next render); the one-shot clear still lives inside `pet.tick`. Placing it after keeps `true_elapsed` adjacent to its use.

- [ ] **Step 6: Run the full test suite**

Run: `cargo test --manifest-path Cargo.toml`
Expected: PASS — all tests green, including `bounded_tick_interval_clamps_to_notification_remaining` and the unchanged SP4-B notification tests.

- [ ] **Step 7: Verify clippy + fmt**

Run: `cargo fmt --manifest-path Cargo.toml --check && cargo clippy --manifest-path Cargo.toml --all-targets -- -D warnings`
Expected: clean (exit 0).

- [ ] **Step 8: Commit**

```bash
git add src/app.rs
git commit -m "feat(sp4c): wall-clock TTL countdown + scheduler wake-bound"
```

---

## Task 5: macOS bubble window (`src/bubble_window_macos.rs`)

This is the platform render layer. There is no unit test for objc2 UI; it is verified by `cargo build` + the manual smoke step in Task 7. Mirror the patterns in `src/picker_window_macos.rs` / `src/settings_window_macos.rs` / `src/window_macos.rs`.

**Files:**
- Create: `src/bubble_window_macos.rs`
- Modify: `src/lib.rs` (add `pub mod bubble_window_macos;`)
- Modify: `Cargo.toml` (add `"NSBezierPath"` to `objc2-app-kit` features)

- [ ] **Step 1: Add the module declaration**

In `src/lib.rs`, add `pub mod bubble_window_macos;` right after `pub mod bubble_layout;`.

- [ ] **Step 2: Add the `NSBezierPath` AppKit feature**

In `Cargo.toml`, inside the `objc2-app-kit` `features = [ ... ]` list, add `"NSBezierPath"` (keep the list alphabetical — it goes just before `"NSButton"`):

```toml
objc2-app-kit = { version = "0.3", features = [
  "NSApplication",
  "NSBezierPath",
  "NSButton",
  ...
] }
```

- [ ] **Step 3: Write the non-macOS stub + the public API**

Create `src/bubble_window_macos.rs`. Start with the stub so the crate builds on non-macOS (mirrors `settings_window_macos.rs:1-22`). The public API the app depends on is: `new(parent: &winit::window::Window) -> Option<Self>`, `update(&self, content: &BubbleContent, pet_rect: Rect, visible: Rect)` (measures text, calls the pure `place_bubble`, converts to AppKit, sets the frame), and `hide(&self)`.

```rust
//! Native AppKit speech-bubble window (SP4-C). A borderless, transparent,
//! click-through child window drawn above the pet. The placement math lives in
//! `crate::bubble_layout` (pure Rust, Y-down logical); this layer only draws,
//! measures text, and converts to AppKit (Y-up) at the boundary.

#[cfg(not(target_os = "macos"))]
pub struct BubbleWindow;

#[cfg(not(target_os = "macos"))]
impl BubbleWindow {
    pub fn new(_parent: &winit::window::Window) -> Option<Self> {
        None
    }
    pub fn update(
        &self,
        _content: &crate::bubble::BubbleContent,
        _pet_rect: crate::physics::Rect,
        _visible: crate::physics::Rect,
    ) {
    }
    pub fn hide(&self) {}
}

/// Outer bubble width cap (points). Text column = this minus padding/dot/gap.
pub const MAX_WIDTH: f64 = 240.0;

#[cfg(not(target_os = "macos"))]
pub fn active_visible_frame_y_down(_window: &winit::window::Window) -> Option<crate::physics::Rect> {
    None
}
```

- [ ] **Step 4: Write the macOS implementation**

Append the `#[cfg(target_os = "macos")] mod macos { ... }` block and re-exports. Use these concrete objc2 calls (verified against the existing window modules):

- Imports (mirror `picker_window_macos.rs:37-52` + add the bubble-specific ones):
  `use objc2::{define_class, msg_send, rc::Retained, runtime::NSObjectProtocol, sel, ClassType, DefinedClass, MainThreadOnly, AnyThread};`
  `use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};`
  `use objc2_app_kit::{NSBackingStoreType, NSBezierPath, NSColor, NSFont, NSPanel, NSScreen, NSTextField, NSView, NSWindow, NSWindowOrderingMode, NSWindowStyleMask, NSWindowLevel, NSLineBreakMode};`
  `use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};`

- Constants (render metrics; the warm card from the spec):

```rust
    const PAD_X: f64 = 12.0;
    const PAD_Y: f64 = 9.0;
    const DOT: f64 = 7.0;
    const DOT_EMPH: f64 = 9.0;
    const DOT_GAP: f64 = 6.0;
    const CORNER: f64 = 11.0;
    const TAIL_H: f64 = 9.0;
    const TAIL_HALF: f64 = 7.0;
    const TITLE_PT: f64 = 12.0;
    const BODY_PT: f64 = 11.0;
    const BODY_MAX_LINES: isize = 3;
    // #F5F2EC card, #23262E text, rgba(0,0,0,0.08) border.
    const CARD: (f64, f64, f64, f64) = (0.961, 0.949, 0.925, 1.0);
    const TEXT: (f64, f64, f64, f64) = (0.137, 0.149, 0.180, 1.0);
    const BORDER: (f64, f64, f64, f64) = (0.0, 0.0, 0.0, 0.08);
```

- A custom drawing view via `define_class!` (the only `NSView` subclass in the repo — model it on the `define_class!` skeleton at `picker_window_macos.rs:342-356`, but with `#[unsafe(super(NSView))]`). Ivars hold the current tail side, tail-x, card/accent colors, dot size. Override `drawRect:`:

```rust
    struct BubbleViewIvars {
        tail_up: std::cell::Cell<bool>,
        tail_x: std::cell::Cell<f64>,
        accent: std::cell::Cell<(f64, f64, f64, f64)>,
        dot_size: std::cell::Cell<f64>,
    }

    define_class!(
        #[unsafe(super(NSView))]
        #[name = "HappyCappyBubbleView"]
        #[thread_kind = MainThreadOnly]
        #[ivars = BubbleViewIvars]
        struct BubbleView;

        impl BubbleView {
            #[unsafe(method(drawRect:))]
            fn draw_rect(&self, _dirty: NSRect) {
                unsafe { draw_bubble(self) };
            }
        }
    );
```

  `draw_bubble` builds an `NSBezierPath` for the rounded-rect body plus the triangular tail (using `bezierPathWithRoundedRect_xRadius_yRadius` for the body and `moveToPoint`/`lineToPoint`/`closePath` lines for the tail), fills it with the `CARD` color (`NSColor::colorWithSRGBRed_green_blue_alpha`), strokes the `BORDER`, then fills the accent dot with `bezierPathWithOvalInRect`. AppKit views are Y-up internally, so the tail goes at the bottom when `tail_up == false` (tail points down toward the pet) and at the top when `tail_up == true`.

- The `BubbleWindow` struct + `new`:

```rust
    pub struct BubbleWindow {
        panel: Retained<NSPanel>,
        view: Retained<BubbleView>,
        title: Retained<NSTextField>,
        body: Retained<NSTextField>,
        mtm: MainThreadMarker,
    }

    impl BubbleWindow {
        pub fn new(parent: &winit::window::Window) -> Option<Self> {
            let mtm = MainThreadMarker::new()?;
            let parent_ns = parent_ns_window(parent)?;

            let panel = unsafe {
                NSPanel::initWithContentRect_styleMask_backing_defer(
                    NSPanel::alloc(mtm),
                    NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(super::MAX_WIDTH, 80.0)),
                    NSWindowStyleMask::Borderless,
                    NSBackingStoreType::Buffered,
                    false,
                )
            };
            unsafe {
                panel.setReleasedWhenClosed(false);
                panel.setOpaque(false);
                panel.setBackgroundColor(Some(&NSColor::clearColor()));
                panel.setHasShadow(true);
                panel.setIgnoresMouseEvents(true);
                panel.setLevel(NSWindowLevel::from(super::status_level()));
                panel.setFloatingPanel(true);
                panel.setHidesOnDeactivate(false);
            }

            let view = make_bubble_view(mtm);
            panel.setContentView(Some(&view));

            let title = NSTextField::labelWithString(&NSString::from_str(""), mtm);
            let body = NSTextField::labelWithString(&NSString::from_str(""), mtm);
            configure_label(&title, TITLE_PT, true, 1);
            configure_label(&body, BODY_PT, false, BODY_MAX_LINES);
            view.addSubview(&title);
            view.addSubview(&body);

            unsafe { parent_ns.addChildWindow_ordered(&panel, NSWindowOrderingMode::Above) };

            Some(Self { panel, view, title, body, mtm })
        }
        // update() + hide() below
    }
```

  `parent_ns_window(parent)` reuses the route from `window_macos.rs:60-73`: `parent.window_handle().ok()?.as_raw()` → match `RawWindowHandle::AppKit(h)` → `h.ns_view.cast::<NSView>().as_ref().window()`.

  `configure_label(field, pt, bold, max_lines)` sets font (`NSFont::boldSystemFontOfSize(pt)` / `systemFontOfSize(pt)`), `setTextColor(Some(&srgb(TEXT)))`, `setLineBreakMode(NSLineBreakMode::ByTruncatingTail)`, `setMaximumNumberOfLines(max_lines)`, and `setPreferredMaxLayoutWidth(text_column_width())`. `text_column_width()` = `MAX_WIDTH - 2*PAD_X - DOT_EMPH - DOT_GAP`.

- `update`: set `stringValue` on each label (hide the unused one with an empty string + zero height), read `fittingSize` to compute the content height, compute the outer bubble size (`width` = text column + paddings + dot, capped to `MAX_WIDTH`; `height` = title height + body height + paddings + tail), set the view ivars (`tail_up`, `tail_x`, `accent` from `content.accent.rgba()` cast to f64, `dot_size` from `content.accent.emphasized()`), lay out the labels with `setFrame`, convert the placement origin to AppKit, set the panel frame, mark the view `setNeedsDisplay(true)`, and order it on screen:

```rust
        pub fn update(
            &self,
            content: &crate::bubble::BubbleContent,
            pet_rect: crate::physics::Rect,
            visible: crate::physics::Rect,
        ) {
            // 1) Set label strings (empty string + zero-height frame when None),
            //    set the accent ivars, and read each label's `fittingSize` at the
            //    fixed text-column width.
            // 2) Compute the OUTER bubble size from the measured text + paddings
            //    + dot + tail, with width capped to MAX_WIDTH.
            let (w, h): (f64, f64) = /* measured outer size */;

            // 3) Placement is pure Rust, called with the REAL measured size so the
            //    above/flip/clamp + tail decisions are correct.
            let placement = crate::bubble_layout::place_bubble(
                pet_rect,
                (w as f32, h as f32),
                visible,
            );
            self.view.ivars().tail_up.set(placement.tail == crate::bubble_layout::TailSide::Up);
            self.view.ivars().tail_x.set(placement.tail_x as f64);
            // ... lay out the title/body NSTextFields with setFrame inside the body ...

            // 4) Convert the Y-down logical top-left origin to AppKit global
            //    (bottom-left). Primary display height comes from NSScreen.
            let primary_h = primary_display_height(self.mtm);
            let appkit_x = placement.origin.x as f64;
            let appkit_y = primary_h - (placement.origin.y as f64 + h);

            let frame = NSRect::new(NSPoint::new(appkit_x, appkit_y), NSSize::new(w, h));
            unsafe { self.panel.setFrame_display(frame, true) };
            self.view.setNeedsDisplay(true);
            self.panel.orderFrontRegardless();
        }

        pub fn hide(&self) {
            self.panel.orderOut(None);
        }
```

- `status_level()` returns the same window level constant the pet uses (screen-saver level) so the bubble sits above the pet; if a numeric level is needed, mirror whatever `window_macos.rs` uses for the pet window level and add `+ 1`.

- `primary_display_height(mtm: MainThreadMarker) -> f64` (shared pivot helper, used by both `update` and `active_visible_frame_y_down`): first `NSScreen::screens(mtm)` element `.frame().size.height`; fallback `NSScreen::mainScreen(mtm)` height; fallback `0.0`.
- `active_visible_frame_y_down(window)` (the §4 helper): get the parent `NSWindow` via `parent_ns_window`, then `ns_window.screen().or_else(|| NSScreen::mainScreen(mtm))`; read `screen.visibleFrame()` (NSRect, bottom-left). Convert to a Y-down `crate::physics::Rect` with `primary_display_height(mtm)` as the pivot: `min.x = vf.origin.x`, `max.x = vf.origin.x + vf.size.width`, `min.y = primary_h - (vf.origin.y + vf.size.height)`, `max.y = primary_h - vf.origin.y`. Return `None` if no screen resolves — the caller falls back to `physics.bounds`.

- Re-exports at the bottom:

```rust
#[cfg(target_os = "macos")]
pub use macos::{active_visible_frame_y_down, BubbleWindow};
```

(`pub const MAX_WIDTH` stays at module top, shared by both cfgs.)

- [ ] **Step 5: Build for the host (macOS)**

Run: `cargo build --manifest-path Cargo.toml`
Expected: compiles. Fix any objc2 signature mismatches by checking the method names against `picker_window_macos.rs` / `settings_window_macos.rs` (e.g. `initWithContentRect_styleMask_backing_defer`, `setFrame_display`, `addChildWindow_ordered`).

- [ ] **Step 6: clippy + fmt**

Run: `cargo fmt --manifest-path Cargo.toml && cargo clippy --manifest-path Cargo.toml --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/lib.rs src/bubble_window_macos.rs
git commit -m "feat(sp4c): native AppKit speech-bubble window"
```

---

## Task 6: App integration — drive the bubble each frame (`src/app.rs`)

**Files:**
- Modify: `src/app.rs` (struct field, init, `tick`/`sync_bubble`, Notify handler)

- [ ] **Step 1: Add the field to `DesktopPetApp`**

In the struct (`src/app.rs:133-159`), add after `picker`:

```rust
    bubble_window: Option<crate::bubble_window_macos::BubbleWindow>,
```

Initialize it to `None` in BOTH constructors (the production one near `src/app.rs:176` and the `#[cfg(test)]` one near `src/app.rs:218`) — add `bubble_window: None,` alongside `renderer: None,`.

- [ ] **Step 2: Create the bubble window once the pet window exists**

The pet `Window` is created during resume/window-init (where `self.renderer` is built, around `src/app.rs:287-296`). Right after the renderer is stored, create the bubble window from the same `window`:

```rust
        self.bubble_window = crate::bubble_window_macos::BubbleWindow::new(&window);
```

(`new` returns `None` on non-macOS or if the AppKit window can't be reached; that's fine — `sync_bubble` no-ops.)

- [ ] **Step 3: Add `sync_bubble` and call it at the end of `tick`**

Add this method to `impl DesktopPetApp`:

```rust
    fn sync_bubble(&mut self) {
        let Some(bubble) = self.bubble_window.as_ref() else {
            return;
        };
        // Visible only when the pet window is visible (Hide / auto-hide hide it;
        // Focus Mode does NOT — it only toggles passthrough) AND there is text.
        let content = if self.effective_window_visible() {
            self.pet.bubble_content()
        } else {
            None
        };
        let Some(content) = content else {
            bubble.hide();
            return;
        };

        let pet_rect = crate::physics::Rect {
            min: self.physics.position,
            max: crate::physics::Vec2 {
                x: self.physics.position.x + self.physics.size.x,
                y: self.physics.position.y + self.physics.size.y,
            },
        };
        let visible = self
            .window
            .as_ref()
            .and_then(|w| crate::bubble_window_macos::active_visible_frame_y_down(w))
            .unwrap_or_else(|| self.physics.bounds.into());

        bubble.update(&content, pet_rect, visible);
    }
```

> `BubbleWindow::update` measures the real text via `fittingSize`, calls the pure `bubble_layout::place_bubble` with the true size (so the above/flip/clamp + tail decisions are correct), then converts to AppKit and sets the frame. The app passes only the pet rect + visible frame, both in Y-down logical points — no AppKit coordinates leak into `app.rs`.

Call it at the very end of `tick` (after the existing body, before `tick` returns):

```rust
        self.sync_bubble();
```

- [ ] **Step 4: Trigger a redraw + immediate bubble refresh on Notify**

The Notify handler already calls `self.pet.set_notification(&event)` and `window.request_redraw()` (`src/app.rs:817-826`). Add an immediate tick wake so the bubble appears without waiting for the next scheduled frame — after `set_notification`, set:

```rust
                self.next_tick_at = Instant::now();
```

(Leave the existing `set_notification` + `request_redraw` lines as-is; this just schedules an immediate `about_to_wait` tick which runs `sync_bubble`.)

- [ ] **Step 5: Build + full test suite**

Run: `cargo build --manifest-path Cargo.toml && cargo test --manifest-path Cargo.toml`
Expected: compiles; all tests pass (no behavior change to existing tests; `sync_bubble` is a no-op when `bubble_window` is `None`, which is always the case in `#[cfg(test)]` builds since no real AppKit window exists).

- [ ] **Step 6: clippy + fmt**

Run: `cargo fmt --manifest-path Cargo.toml && cargo clippy --manifest-path Cargo.toml --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs
git commit -m "feat(sp4c): drive the speech bubble from the app frame loop"
```

---

## Task 7: Smoke checklist + full verify

**Files:**
- Modify: `scripts/smoke_app.sh` (append a manual checklist item)

- [ ] **Step 1: Add the bubble smoke checklist item**

In `scripts/smoke_app.sh`, under the existing "Manual smoke checklist:" section (`scripts/smoke_app.sh:87`), add a line:

```
  - Run: ./'dist/Happy Cappy.app/Contents/MacOS/happy-cappy' notify --kind needs-review --label "Cần bạn duyệt" --body "2 file đang chờ review trong nhánh main"
    Expect: a warm light speech bubble appears ABOVE the pet (tail pointing down), within the screen, with an amber dot. Drag the pet near the top edge -> bubble flips below (tail up). Drag near a side edge -> bubble clamps inside the screen. Enable Focus Mode -> bubble stays visible. Hide the pet (menu) -> bubble disappears.
```

- [ ] **Step 2: Run the full automated gate**

Run: `./scripts/verify.sh`
Expected: green — `cargo fmt --check`, `cargo test` (all unit tests incl. the new SP4-C tests), `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, bundle assembly, and `codesign --verify`. (`verify.sh` does NOT run `smoke_app.sh`.)

- [ ] **Step 3: Run the manual smoke**

Run: `./scripts/smoke_app.sh` and walk the checklist, focusing on the new bubble item from Step 1.
Expected: the bubble behaves as described (appears above the pet, flips/clamps at edges, follows while dragging, stays in Focus Mode, hides on Hide).

- [ ] **Step 4: Commit**

```bash
git add scripts/smoke_app.sh
git commit -m "test(sp4c): add speech-bubble manual smoke checklist item"
```

---

## Self-Review (filled in)

**Spec coverage:**
- §2 modules → Tasks 1 (`bubble.rs`), 2 (`bubble_layout.rs`), 3 (runtime), 5 (`bubble_window_macos.rs`), 6 (`app.rs`), lib/Cargo edits in 1/2/5. ✓
- §3 content model + visibility rule (trim, both-empty→None, title/body-only, accent incl. unknown→Message) → Task 1 tests + `bubble_content` in Task 3. ✓
- §4 placement (above default, flip, clamp, tail, Y-down coords, `active_visible_frame_y_down` + fallback) → Task 2 + the helper in Task 5 + use in Task 6. ✓
- §5 render layer (borderless transparent click-through child window, warm card, accent dot incl. emphasized, NSTextField truncation, fitting-size measurement, MAX_WIDTH outer cap) → Task 5. ✓
- §6 lifecycle + visibility gate (`effective_window_visible`, Focus Mode stays visible) + wall-clock TTL fix (true-elapsed countdown) + scheduler wake-bound → Tasks 3, 4, 6. ✓
- §7 edge cases → covered by Task 2 (clamp/flip), Task 1 (no-text), Task 6 (hidden gate). ✓
- §8 tests (bubble unit, layout unit, parity + wake-bound app-level, manual smoke) → Tasks 1, 2, 3, 4, 7. ✓
- §9 exit criteria (`verify.sh` green; wall-clock TTL; Focus Mode visible) → Task 7 + Tasks 3/4/6. ✓

**Placeholder scan:** the only intentionally-narrative parts are inside Task 5 (the objc2 UI body, which has no in-repo precedent and is verified by build + smoke, per the spec's "smoke" test column). All pure-Rust tasks (1–4, 6 wiring) carry complete code. The `update` width/height computation in Task 5 is described rather than fully spelled (it depends on `fittingSize` returned at runtime); this is the one place where exact pixel math is finalized against the live `NSTextField`.

**Type consistency:** `BubbleContent::from_parts`, `BubbleAccent::{for_kind,rgba,emphasized}`, `place_bubble(pet, size, visible) -> BubblePlacement{origin,tail,tail_x}`, `tick_notification`, `notification_remaining`, `bubble_content`, `bounded_tick_interval`, `BubbleWindow::{new,update,hide}`, `active_visible_frame_y_down`, `MAX_WIDTH` are used identically across tasks. ✓
