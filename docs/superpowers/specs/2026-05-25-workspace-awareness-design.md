# Happy Cappy — Workspace Awareness Design

**Date:** 2026-05-25
**Status:** Approved for planning
**Scope:** Add three workspace-aware behaviors to Happy Cappy: cursor follow/avoid, text-caret avoidance, and fullscreen auto-hide.

## Goals

Make the pet feel aware of the user's workspace context:

1. **Follow cursor when idle, avoid when busy.** When the user is idle, the pet ambles toward the mouse cursor. When the user is busy (typing, in an editor, recently active), the pet ambles away from the cursor.
2. **Never block the text caret.** When the system text cursor (caret) is visible and inside the pet's frame, the pet moves away.
3. **Auto-hide on fullscreen.** When any window on the pet's display enters native macOS fullscreen or covers the full display, the pet hides itself; it reappears when fullscreen ends.

Each behavior is independently toggleable in Settings; defaults are all on.

## Non-goals

- Detecting "video specifically" (vs. any fullscreen app). Treating all fullscreen as hide-worthy is the explicit policy.
- Real-time per-frame cursor steering. The pet stays calm; intent is re-evaluated at walk-cycle boundaries (~every 1–2 s).
- Cross-platform parity. Workspace observation is macOS-only; other platforms get a no-op stub that produces an empty snapshot.
- Surfacing the new toggles in the menu bar. They are set-and-forget preferences and live in the Settings panel only.
- Vertical pet motion. The pet stays horizontal-only (`PetTick.speed_x` only, matching today's model). 2D inputs are projected to a horizontal direction before being applied. Adding vertical motion would touch sprite animation, walk states, and physics y-velocity — out of scope for this spec.

## Coordinate system

The app's pet and physics work in **winit logical points, top-left origin** — see `app.rs:239-244`, where `physics.bounds` is computed by dividing physical monitor coords by `window.scale_factor()`. `physics.position`, `physics.size`, and `physics.bounds` are all in that space. The pet's window position is set via `LogicalPosition` (`app.rs:271-277`).

All workspace observations are normalized into this space **inside `workspace.rs`** before they appear in `WorkspaceSnapshot`. The observer is the only place that talks to macOS coordinate conventions; downstream code (`decide_intent`, pet, window) only sees pet-space coords.

Normalization contract per signal:

| Source | Native space | Conversion to pet space |
|---|---|---|
| `NSEvent.mouseLocation` | Cocoa global, primary-display bottom-left origin, Y up, points | `quartz_y = primary_display_height - cocoa_y`; x unchanged. The Cocoa and Quartz spaces share the same origin column (primary display's left edge); only the Y axis flips, pivoting around the primary display's height. This formula is correct for points on any secondary display, including displays with negative coordinates or vertical layouts, because both coord systems are anchored to the same primary display. |
| `CGWindowListCopyWindowInfo` bounds | Quartz, top-left origin, points | No flip needed; bounds already in points |
| `AXValue` rect from `kAXBoundsForRangeParameterizedAttribute` | Quartz, top-left origin, points | No flip needed |
| `NSScreen` frame (for fullscreen comparison) | Cocoa, bottom-left, points | Convert to Quartz top-left so it matches `CGWindowList` bounds |
| `winit::Monitor::size()` (physical pixels) | Top-left, physical | Divide by `scale_factor` to get logical points |

The active display for all per-display logic is the one the pet currently sits on. The observer is told about it via:

```rust
pub struct DisplayInfo {
    pub name: Option<String>,         // monitor.name(); diagnostic only, not unique
    pub bounds_logical: Rect,         // pet-space, top-left origin, points
    pub scale_factor: f32,            // window.scale_factor()
    pub primary_display_height: f32,  // height in points of the primary display;
                                      // used as the Y-flip pivot. Same for all
                                      // DisplayInfo updates within a session unless
                                      // displays are reconfigured.
}

impl WorkspaceObserver {
    pub fn set_active_display(&mut self, info: Option<DisplayInfo>);
}
```

The app calls `set_active_display(Some(DisplayInfo { ... }))` from the same block at `app.rs:220-247` that already builds `self.physics.bounds`. Sourcing rules — chosen so observer bounds and physics bounds can never diverge:
- `bounds_logical = Rect::from(self.physics.bounds)` — taken directly from the `Bounds` the app just assigned, NOT recomputed from `monitor.position()`/`size()`/scale. Any future change to how physics bounds are computed automatically flows through to the observer. (The existing computation at `app.rs:234-243` uses `window.scale_factor()`; whether that's correct for mixed-DPI is a separate, pre-existing question — see deferred item below.)
- `scale_factor` from `window.scale_factor()` — kept as the pet display's scale because everything downstream that uses `DisplayInfo.scale_factor` (e.g., converting `monitor.size()` to logical points for sanity checks) needs to match the scale that was actually used to derive `bounds_logical`.
- `primary_display_height` from `event_loop.primary_monitor().size().height / event_loop.primary_monitor().scale_factor()` — using the **primary monitor's own scale factor**, NOT the pet display's, because the Y-flip uses the primary display's logical height in its own native scale. On mixed-DPI setups this can differ from the pet display's scale. This value is cached on the observer; recomputed only on display reconfiguration.

Passing only `name` is not enough because monitor names are not unique and don't recover origin/size/scale.

For the fullscreen check specifically: a window from `CGWindowListCopyWindowInfo` is "fullscreen on the pet's display" iff its bounds equal `active_display.bounds_logical` within 1 px on each side. Both sides are in points, top-left origin (Quartz), no per-display origin adjustment needed because `bounds_logical` already encodes the display's global origin and the `CGWindowList` bounds are in the same space.

**Deferred (open item):** the existing `app.rs:234-243` block sources `scale_factor` from `window.scale_factor()` even when computing bounds for a display the window is not on (e.g., `MonitorBehavior::PrimaryDisplay` while the window has been moved). This is a pre-existing latent bug, not one this spec introduces. Workspace-awareness inherits it. Plan stage should call it out separately; if it's fixed in the same plan, the observer's `bounds_logical` source remains correct by construction because it still tracks `self.physics.bounds`.

## Signals and how they're observed

| Signal | macOS API | Permission | Poll cadence |
|---|---|---|---|
| Seconds since last input | `CGEventSourceSecondsSinceLastEventType` | None | Every tick |
| Global key-event counter | `CGEventSourceCounterForEventType(kCGEventKeyDown)` | None | Every tick (delta → typing rate) |
| Frontmost app bundle ID | `NSWorkspace.frontmostApplication.bundleIdentifier` | None | 500 ms |
| Onscreen windows | `CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)` | None | 500 ms |
| Text caret rect | AX API: `AXUIElementCopyAttributeValue(systemwide, kAXFocusedUIElementAttribute)` → `AXUIElementCopyAttributeValue(focused, kAXSelectedTextRangeAttribute)` → **`AXUIElementCopyParameterizedAttributeValue(focused, kAXBoundsForRangeParameterizedAttribute, range)`** → `AXValueGetValue(..., kAXValueCGRectType, ...)`. The bounds attribute is parameterized (takes the selected-text range as a parameter), so it must be fetched with the *Parameterized* variant — `AXUIElementCopyAttributeValue` alone returns `kAXErrorAttributeUnsupported` for it. | Accessibility (prompt on first launch) | 250 ms |
| Mouse cursor position | `NSEvent.mouseLocation` | None | Every tick |

No global event taps. No Screen Recording permission. Only Accessibility, only for caret avoidance, prompted once.

## Architecture

A new module `src/workspace.rs` owns a `WorkspaceObserver` that polls the signals above and produces a `WorkspaceSnapshot`:

```rust
pub struct WorkspaceSnapshot {
    pub seconds_idle: f32,
    pub typing_rate_per_sec: f32,
    pub frontmost_bundle_id: Option<String>,
    pub frontmost_is_editor: bool,
    pub caret_rect: Option<Rect>,         // pet-space points, top-left origin (per §Coordinate system)
    pub fullscreen_active: bool,          // on the pet's active display only
    pub cursor_pos: Vec2,                 // pet-space points, top-left origin
}

impl WorkspaceSnapshot {
    pub fn is_busy(&self) -> bool {
        self.frontmost_is_editor
            || self.typing_rate_per_sec > 1.0
            || self.seconds_idle < 2.0
    }
    pub fn is_idle(&self) -> bool {
        self.seconds_idle >= 5.0 && !self.is_busy()
    }
}
```

`is_busy` and `is_idle` are mutually exclusive but not exhaustive (the "in between" window from 2 s to 5 s of idleness is neither — the pet stays in its existing autonomous mode there).

The editor-detection list is a compile-time `&[&str]` of bundle IDs:

```text
com.apple.dt.Xcode
com.microsoft.VSCode
com.todesktop.230313mzl4w4u92  // Cursor
com.sublimetext.4
com.googlecode.iterm2
com.apple.Terminal
com.mitchellh.ghostty
com.jetbrains.*                  // matched by prefix
```

(Final list maintained in code; plan stage will confirm exact bundle IDs.)

### Cargo / macOS dependencies

The current `Cargo.toml` (the macOS target block, lines 18–34) only enables AppKit menu / panel / view features. The new APIs need additional bindings:

| API used in spec | Crate | Feature to enable (or crate to add) |
|---|---|---|
| `NSWorkspace`, `NSWorkspace.frontmostApplication` | existing `objc2-app-kit = "0.3"` | add features `"NSWorkspace"` AND `"NSRunningApplication"` (the latter is required for the return type of `frontmostApplication()` and for the `bundleIdentifier` accessor; without it the import does not compile) |
| `NSEvent.mouseLocation` | existing `objc2-app-kit = "0.3"` | add feature `"NSEvent"` |
| `NSScreen.frame`, `NSScreen.mainScreen` | existing `objc2-app-kit = "0.3"` | add feature `"NSScreen"` |
| `NSControlStateValueOn/Off` | existing `objc2-app-kit = "0.3"` | covered by existing `"NSControl"` |
| `CGEventSourceSecondsSinceLastEventType`, `CGEventSourceCounterForEventType`, `CGWindowListCopyWindowInfo`, `CGWindowID`, `CGRect`, `CGPoint` | new `objc2-core-graphics = "0.3"` | add to the `cfg(target_os = "macos")` dependencies block |
| `AXUIElementCreateSystemWide`, `AXUIElementCopyAttributeValue`, **`AXUIElementCopyParameterizedAttributeValue`** (required for the caret-bounds fetch — `kAXBoundsForRangeParameterizedAttribute` is a parameterized attribute and cannot be retrieved via the non-parameterized call), `AXValueGetValue`, `AXIsProcessTrusted`, `AXIsProcessTrustedWithOptions`, `AXUIElementSetMessagingTimeout`, `kAXFocusedUIElementAttribute`, `kAXSelectedTextRangeAttribute`, `kAXBoundsForRangeParameterizedAttribute`, `kAXValueCGRectType`, `kAXTrustedCheckOptionPrompt` | new `objc2-application-services = "0.3"` (provides the HIServices / AX bindings under one umbrella crate) | add to the `cfg(target_os = "macos")` dependencies block |

Plan stage will pin exact versions consistent with `objc2 = "0.6"`. The `objc2-core-graphics` and `objc2-application-services` crates are gated behind `#[cfg(target_os = "macos")]` so non-macOS builds remain dependency-free (the stub `WorkspaceObserver` referenced below does not pull these crates in).

If any specific AX symbol turns out not to be exposed by `objc2-application-services` at the chosen version, the fallback is a small `extern "C"` block in `workspace.rs` declaring the C signatures directly and linking against `ApplicationServices` (`#[link(name = "ApplicationServices", kind = "framework")]`). Plan stage decides; the spec just commits to "AX bindings come from a single source, either the crate or a direct extern block".

### Module layout

- `src/physics.rs` (extended):
  - New `Rect { min: Vec2, max: Vec2 }` type alongside the existing `Vec2` and `Bounds`, used by `WorkspaceSnapshot.caret_rect`, `DisplayInfo.bounds_logical`, and inside `decide_intent` for the pet-frame ∩ caret-rect test. Single canonical geometry type rather than re-inventing per module. Note: `Rect` does NOT appear in any `BehaviorIntent` variant — those carry only `Direction` (see pet.rs section below).

- `src/workspace.rs` (~250 lines, new):
  - `WorkspaceObserver` — owns last-known counter values, last-poll timestamps per source, AX permission state, cached editor-bundle-id list, a `prompted_for_accessibility_at_startup: bool` flag.
  - `fn tick(&mut self, now: Instant) -> &WorkspaceSnapshot` — polls all sources at their respective cadences, updates the snapshot, returns a reference. Called from the app's main loop.
  - **Two distinct AX-prompt entry points**, deliberately not unified:
    - `fn request_accessibility_on_startup_if_enabled(&mut self)` — called once during `DesktopPetApp` init. No-op if `prompted_for_accessibility_at_startup == true` or `avoid_text_cursor == false`. On the first call where both gates pass, calls `AXIsProcessTrustedWithOptions(@{kAXTrustedCheckOptionPrompt: @YES})` and flips the flag. This is the "polite once on startup" semantics.
    - `fn request_accessibility_now(&mut self)` — called in response to `AppCommand::RequestAccessibilityPermission`. Always calls `AXIsProcessTrustedWithOptions(@{kAXTrustedCheckOptionPrompt: @YES})` regardless of the `prompted_at_startup` flag. macOS itself decides whether to actually display the system dialog: if the user previously denied, macOS shows the dialog again only if the user has since removed the entry from System Settings → Privacy → Accessibility. If macOS suppresses the dialog because the choice is sticky, we surface a one-shot log line and (plan-stage) optionally open System Settings to the Accessibility pane via `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`. The user-initiated path never silently no-ops on the Rust side.
  - macOS-specific calls are behind `#[cfg(target_os = "macos")]`; other platforms get a stub `WorkspaceObserver` whose `tick` returns a default snapshot (idle = 0, no fullscreen, no caret, busy = false) and whose two prompt entry points are no-ops.

- `src/pet.rs` (extended):
  - New `BehaviorIntent` enum, **all variants pre-resolved to 1D**:
    ```rust
    pub enum BehaviorIntent {
        Idle,
        ChaseHorizontal { direction: Direction },
        AvoidHorizontal { direction: Direction },
        AvoidRectHorizontal { direction: Direction },
    }
    ```
    `Direction` is the existing `pet::Direction { Left, Right }` enum (pet.rs:46-48). The intent carries only the resolved horizontal direction; pet does not need the original 2D inputs.
  - **Motion is horizontal-only in v1.** This matches the existing model: `PetTick.speed_x` only (pet.rs:55), and `app.rs:262` only writes `physics.velocity.x`. `PetTick.speed_y` and any vertical pet motion are out of scope for this spec.
  - All 2D resolution happens in `decide_intent`:
    - **ChaseHorizontal:** `direction = if snapshot.cursor_pos.x > pet_center_x { Right } else { Left }`. Pet ambles toward the cursor's horizontal position.
    - **AvoidHorizontal:** `direction = opposite of ChaseHorizontal` for the cursor case. Pet ambles away.
    - **AvoidRectHorizontal:** triggers only when the caret rect intersects the pet's frame in 2D. When triggered, `direction = the side of the caret rect that is closer to the pet`, so the pet exits with the shortest horizontal travel.
  - `Pet::set_intent(&mut self, intent: BehaviorIntent)`.
  - The walk-cycle state machine consults the current intent at each cycle boundary to pick walk direction. `AvoidRectHorizontal` is the one priority case that interrupts mid-walk (it implies a danger zone the pet is currently inside); the others only take effect at the next boundary.
  - Existing personality animations (blink, happy, sleepy, curious) play during walk cycles unchanged.
  - `PetTick` is unchanged — still `{ state, frame_index, speed_x }`. The pet keeps emitting horizontal speed only; the intent only influences which horizontal direction the next walk picks.

  This shape removes the earlier ambiguity (was the rect in the payload? was the direction?). Answer: only the direction. `decide_intent` is the single owner of 2D→1D resolution, and the pet is a pure consumer of resolved direction. The intersection test that triggers `AvoidRectHorizontal` still uses the full 2D `caret_rect` and `pet_frame` — but that's inside `decide_intent`, not in the intent payload.

- `src/app.rs` window-visibility composition (no new controller — `window_macos.rs` stays a collection of helper functions):
  - `DesktopPetApp` already owns `pet_visible: bool` (app.rs:72). Add a sibling field `auto_hidden: bool` (default false, runtime-only, never persisted to settings).
  - New private helper: `fn effective_window_visible(&self) -> bool { self.pet_visible && !self.auto_hidden }`.
  - New private helper: `fn apply_window_visibility(&mut self)` — `&mut self` because it mutates `self.next_tick_at = Instant::now()` when transitioning to visible (matching the existing logic at `app.rs:374-376`). Calls `window.set_visible(self.effective_window_visible())`, `window.request_redraw()` when becoming visible, and resets `next_tick_at` when becoming visible. Encapsulates the redraw/tick-resume block currently inlined inside `set_pet_visible`.
  - The existing `set_pet_visible(&mut self, visible: bool)` is refactored: updates `self.pet_visible`, calls `self.pet.set_hidden(!visible)`, then calls `self.apply_window_visibility()` instead of calling `window.set_visible` and `next_tick_at` directly.
  - New method `set_auto_hidden(&mut self, hidden: bool)` on `DesktopPetApp`: updates `self.auto_hidden`, then calls `self.apply_window_visibility()`. Does NOT touch settings or save to disk. Does NOT call `sync_settings_window()` / `sync_menu_bar()` — auto-hide is invisible to the user-facing controls.
  - Tick cadence while auto-hidden: the tick loop continues to run so the workspace observer and pet state machine stay live. The redraw call inside `tick` is gated on `effective_window_visible()` — when hidden, no redraw is requested. This matches the existing pattern around `IDLE_FRAME_TIME` / `SLEEP_FRAME_TIME` (app.rs:33-35); we add a similar gate.
  - Drag termination on auto-hide entry: before `set_auto_hidden(true)` flips the flag, the app inspects `self.interaction.is_dragging()`. If dragging, it routes a synthesized `InteractionEvent::DragEnded { pointer: last_known_pointer }` through the existing `handle_interaction_events` path (app.rs:680-685). That handler already does the full drag-end work: clears the pet drag flag, clamps physics, moves the window, and `persist_current_position()` saves the dropped location. Just clearing `InteractionState` without going through this path would lose the dragged position from settings — which is the bug the reviewer flagged.

- `src/app.rs` (orchestration):
  - Owns a `WorkspaceObserver` and a `Settings`.
  - On each tick: call `observer.tick()`, run `decide_intent(...)` (a pure function), push intent to pet and auto-hide flag to window.

- `src/settings.rs` (extended):
  - Three new `bool` fields: `follow_cursor_when_idle`, `avoid_text_cursor`, `hide_on_fullscreen`. Each has `#[serde(default = "default_true")]`. Existing settings files load with all three defaulting to `true`.

- `src/settings_window_macos.rs` (extended):
  - Three new `NSButton` checkboxes (`setButtonType:NSButtonTypeSwitch`) under a "Workspace Awareness" section heading. Each is tagged with a new `MENU_TAG_*` constant, has its action set to `CommandTarget::settings_value_selector()` (= `dispatchSettingsValue:`), and its target set to the shared `CommandTarget` — same wiring as the existing scale/movement-speed sliders.
  - A separate "Re-request Accessibility permission" `NSButton` (push button, not a checkbox) tagged with its own constant, with action set to `CommandTarget::command_selector()` (= `dispatchCommand:`). It carries no payload — it just emits an `AppCommand`.

- `src/menu_bar.rs` — extended with four new tag constants in the existing 11xx range (settings-control tags, not menu-item tags):
  ```rust
  pub const MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE: isize = 1106;
  pub const MENU_TAG_AVOID_TEXT_CURSOR: isize = 1107;
  pub const MENU_TAG_HIDE_ON_FULLSCREEN: isize = 1108;
  pub const MENU_TAG_REREQUEST_ACCESSIBILITY: isize = 1109;
  ```
  `command_from_tag` gets a new arm for `MENU_TAG_REREQUEST_ACCESSIBILITY` returning `Some(AppCommand::RequestAccessibilityPermission)`. The three checkbox tags do NOT go in `command_from_tag` because they carry state. Instead, a new pure-Rust helper alongside `command_from_tag` is added:
  ```rust
  pub fn settings_command_for_button(tag: isize, state_is_on: bool) -> Option<AppCommand> {
      match tag {
          MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE => Some(AppCommand::SetFollowCursorWhenIdle(state_is_on)),
          MENU_TAG_AVOID_TEXT_CURSOR        => Some(AppCommand::SetAvoidTextCursor(state_is_on)),
          MENU_TAG_HIDE_ON_FULLSCREEN       => Some(AppCommand::SetHideOnFullscreen(state_is_on)),
          _ => None,
      }
  }
  ```
  `dispatchSettingsValue:` reads `NSButton.state` into a `bool` and forwards to this helper, keeping the Obj-C bridge thin and the testable logic pure. No menu-bar menu items are added; these are settings-window tags only, matching how the existing `MENU_TAG_SCALE` etc. are used.

- `src/command_target_macos.rs::dispatchSettingsValue:` (extended) — for the three boolean tags, read the sender's `state` into a `bool` via a new local helper, then delegate to the pure `settings_command_for_button(tag, state_is_on)` defined in `menu_bar.rs`. The Obj-C-touching helper is a thin wrapper:
  ```rust
  fn read_button_state(sender: &AnyObject) -> bool {
      let state: NSInteger = unsafe { msg_send![sender, state] };
      state != 0  // NSControlStateValueOff = 0, On = 1, Mixed = -1 (treat any nonzero as on)
  }
  ```
  This keeps the Obj-C surface minimal and lets unit tests exercise `settings_command_for_button` directly without an AppKit runtime.

- `src/app.rs::AppCommand` (extended) — new variants:
  - `SetFollowCursorWhenIdle(bool)`
  - `SetAvoidTextCursor(bool)`
  - `SetHideOnFullscreen(bool)`
  - `RequestAccessibilityPermission`

  These are handled in `DesktopPetApp`'s command dispatch (alongside `SetPersonality`, `SetScale`, etc.). The three `Set*` variants update `self.settings.<field>`, call `save_settings()`, and on the next tick the new value gates `decide_intent` / `set_auto_hidden`. `RequestAccessibilityPermission` calls `WorkspaceObserver::request_accessibility_now()` — the "always prompt regardless of startup-flag" path, not the startup-once path. The startup-once path (`request_accessibility_on_startup_if_enabled`) runs from `DesktopPetApp::init` instead.

## Data flow per tick

```
app.rs main loop
   │
   ├─► observer.tick(now)  ──► WorkspaceSnapshot
   │       │
   │       ├─ poll CGEventSourceSecondsSinceLastEventType         (every tick, ~µs)
   │       ├─ poll CGEventSourceCounterForEventType + delta       (every tick)
   │       ├─ poll NSWorkspace.frontmostApplication.bundleId      (every 500ms)
   │       ├─ poll CGWindowListCopyWindowInfo → fullscreen check  (every 500ms)
   │       └─ poll AX caret rect (if permission granted)          (every 250ms)
   │
   ├─► decide_intent(&snapshot, &settings, pet_frame)  -> BehaviorIntent
   │       ┌─────────────────────────────────────────────────────┐
   │       │ if avoid_text_cursor && caret rect intersects       │
   │       │       pet_frame in 2D:                              │
   │       │     dir = side of caret_rect closer to pet center   │
   │       │     intent = AvoidRectHorizontal { direction: dir } │
   │       │ elif follow_cursor_when_idle && snapshot.is_idle(): │
   │       │     dir = sign(cursor.x - pet_center_x) as Direction│
   │       │     intent = ChaseHorizontal { direction: dir }     │
   │       │ elif follow_cursor_when_idle && snapshot.is_busy(): │
   │       │     dir = sign(pet_center_x - cursor.x) as Direction│
   │       │     intent = AvoidHorizontal { direction: dir }     │
   │       │ else:                                               │
   │       │     intent = Idle                                   │
   │       └─────────────────────────────────────────────────────┘
   │
   ├─► pet.set_intent(intent)        // 1D horizontal direction only
   │
   └─► self.set_auto_hidden(
           settings.hide_on_fullscreen && snapshot.fullscreen_active
       )                              // updates self.auto_hidden, then
                                      // apply_window_visibility()
```

The snapshot is consumed and dropped per tick — no shared mutable state between modules. `pet.rs` and `window_macos.rs` see only the resolved intent / boolean. Policy lives in `app.rs::decide_intent`, observation lives in `workspace.rs`. Each is testable in isolation.

## Error handling and degradation

**Accessibility permission.** Three entry points cover the realistic UX flows:

1. **Startup with `avoid_text_cursor` already on** (returning user with feature enabled): `request_accessibility_on_startup_if_enabled` runs during `DesktopPetApp::init`, prompts once, sets `prompted_for_accessibility_at_startup`. Idempotent within a session.
2. **User toggles `avoid_text_cursor` OFF → ON at runtime**: `SetAvoidTextCursor(true)` first calls `AXIsProcessTrusted()`; if not trusted, calls `request_accessibility_now()` to surface the dialog immediately. The toggle never appears enabled while permission is missing without the user seeing a dialog in the same interaction.
3. **User explicitly clicks "Re-request Accessibility permission"**: `request_accessibility_now()` always calls `AXIsProcessTrustedWithOptions` with the prompt option, regardless of `prompted_at_startup`. This is the escape hatch when macOS suppresses repeat prompts after a sticky denial — the user must clear the entry in System Settings → Privacy & Security → Accessibility before macOS will show the dialog again.

If denied, `caret_rect` is always `None`, the `AvoidRectHorizontal` arm of the decision tree never fires, and the rest of the features work normally. If the user grants permission later via System Settings without touching any in-app button, the next AX poll picks it up automatically.

**Caret query failures.** If the focused element doesn't expose `kAXSelectedTextRangeAttribute` (canvas-based editors, some Catalyst apps, non-AX-friendly Electron builds), `caret_rect = None` for that poll. No error surfaced.

**AX query slowness.** Bound each AX call with `AXUIElementSetMessagingTimeout(element, 0.1)` (100 ms). On timeout, treat as `caret_rect = None`.

**Fullscreen detection filtering.** From `CGWindowListCopyWindowInfo` results:
- Skip windows with `kCGWindowLayer != 0`.
- Skip windows owned by our own process (`kCGWindowOwnerPID == getpid()`).
- A window is "fullscreen" iff its bounds equal a connected display's bounds within 1 px on each side, AND the window is on the pet's current display.

If `CGWindowListCopyWindowInfo` returns an error (rare, can happen during display reconfiguration), reuse the previous snapshot's `fullscreen_active` for one cycle.

**Multi-display.**
- Fullscreen on a display other than the pet's → `fullscreen_active = false`. The pet stays.
- Cursor on another display → chase/avoid still computes against global coords; the pet effectively walks to the edge of its display nearest the cursor, clamped by existing physics bounds.
- Caret rect on another display → no intersection with the pet's frame, no avoidance triggered.

**Drag-in-progress + fullscreen.** If the pet is being dragged when fullscreen begins, `set_auto_hidden(true)` first routes a synthesized `InteractionEvent::DragEnded { pointer: last_known }` through `handle_interaction_events` (the same path real drag-release uses, app.rs:680-685). That clears the pet's drag flag, clamps physics, moves the window, and persists the position via `persist_current_position()`. Only after the synthesized drag-end completes does the app flip `auto_hidden` and call `apply_window_visibility()`. Avoids both the leaked held-mouse state AND the loss of the dragged position.

**Pet stuck near screen edge.** The existing physics module clamps to the configured bounds. New behavior only sets target direction; final position is always clamped by physics. No change needed.

**Settings backward compatibility.** Existing `~/Library/Application Support/Happy Cappy/settings.json` files lack the three new keys. `#[serde(default = ...)]` on each field fills them with `true`. No migration code.

## Testing

### Unit tests — platform-independent (no macOS runtime required)

`workspace.rs` — platform-independent logic:
- `WorkspaceSnapshot::is_busy` truth table across (editor frontmost) × (typing rate above/below) × (idle above/below). 8 cases.
- `is_busy` and `is_idle` are never both true.
- Editor-bundle-id matcher: exact match, prefix match for `com.jetbrains.*`, no false positives on substring.

`app.rs::decide_intent` (extracted as a pure function):
- Caret-rect avoidance overrides chase when both apply.
- Chase fires only when `follow_cursor_when_idle && is_idle`.
- Avoid-cursor fires only when `follow_cursor_when_idle && is_busy`.
- All three gates off → always `Idle`.
- Caret rect on a different display from the pet → no avoidance.
- Caret rect that doesn't intersect the pet frame → no avoidance even though caret exists.
- **Direction resolution:** cursor to pet's right → `ChaseHorizontal { Right }`; cursor to pet's left → `ChaseHorizontal { Left }`. Busy: directions flipped. AvoidRect: caret rect to pet's right → pet exits `Left` (and vice versa).

`pet.rs` intent handling:
- `set_intent(ChaseHorizontal { Right })` mid-walk does not interrupt; takes effect at next walk-cycle boundary.
- `set_intent(AvoidRectHorizontal { Left })` interrupts immediately.
- Repeated `set_intent(Idle)` is idempotent and preserves walk progress.

`DesktopPetApp` visibility composition (table-driven test against `effective_window_visible`):
- `(pet_visible, auto_hidden) = (T,F)` → shown.
- `(T,T)` → hidden.
- `(F,F)` → hidden.
- `(F,T)` → hidden.
- Sequence: visible → `set_auto_hidden(true)` → `set_pet_visible(false)` → `set_auto_hidden(false)` → still hidden (pet_visible drives it).
- `set_auto_hidden(true)` does not modify `self.settings.pet_visible` and does not call `save_settings()`.

`settings.rs` deserialization:
- Load fixture JSON missing the three new keys → all three default to `true`.

`workspace.rs` coord normalization (pure functions, no macOS runtime):
- `cocoa_to_quartz_y(cocoa_y, primary_height)` returns `primary_height - cocoa_y`. Test cases:
  - Primary-display point: `cocoa=(100, 800)`, `primary_height=900` → `quartz=(100, 100)`.
  - Secondary display above primary (Cocoa y > primary_height): `cocoa=(50, 1400)`, `primary_height=900` → `quartz=(50, -500)`. Negative Quartz y is expected and represents a point above the primary display.
  - Secondary display below primary (Cocoa y negative): `cocoa=(50, -300)`, `primary_height=900` → `quartz=(50, 1200)`.
- `quartz_rect_to_pet_space(rect)` is identity (round-trip equal).
- `monitor_size_physical_to_logical(size, scale_factor)` divides correctly for `scale_factor = 1.0, 2.0, 1.5`.
- `set_active_display(DisplayInfo { ... })` cached value is used by the next `tick`'s fullscreen comparison and Y-flip.

Command dispatch (in `app.rs`):
- `AppCommand::SetFollowCursorWhenIdle(false)` updates `self.settings.follow_cursor_when_idle` and calls `save_settings()`.
- `AppCommand::SetAvoidTextCursor(true)` checks AX trust state first via `AXIsProcessTrusted()` (a non-prompting query). If trusted, no further action. If NOT trusted, calls `request_accessibility_now()` immediately — so flipping the toggle from OFF to ON in Settings always either confirms permission silently or surfaces the macOS prompt then and there. The user never sees a toggle that appears enabled while the feature silently degrades. The Re-request button remains available for the macOS-suppressed-dialog case (post-denial), but is no longer the only path from "enable the feature" to "see a dialog".
- `AppCommand::SetHideOnFullscreen(false)` immediately allows next-tick `set_auto_hidden(false)` even if fullscreen is currently true.
- `AppCommand::RequestAccessibilityPermission` always calls `request_accessibility_now()` regardless of `prompted_for_accessibility_at_startup`. Fake observer test verifies the call lands every time it is dispatched, even after the startup-once path has already run.
- `request_accessibility_on_startup_if_enabled()` is a no-op on the second call (idempotent), and a no-op when `avoid_text_cursor` is false at startup.

Pure tag → command logic (extracted out of `command_target_macos.rs` for testability):
- A new helper `pub(crate) fn settings_command_for_button(tag: isize, state_is_on: bool) -> Option<AppCommand>` lives in `menu_bar.rs` (alongside the existing `command_from_tag`). It is pure Rust — no Obj-C, no `unsafe`. The `dispatchSettingsValue:` arms for boolean checkboxes call this helper after reading `NSButton.state` into a plain `bool`. Unit tests target this pure helper:
  - `settings_command_for_button(MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE, true)` → `Some(AppCommand::SetFollowCursorWhenIdle(true))`.
  - `settings_command_for_button(MENU_TAG_AVOID_TEXT_CURSOR, false)` → `Some(AppCommand::SetAvoidTextCursor(false))`.
  - `settings_command_for_button(MENU_TAG_HIDE_ON_FULLSCREEN, true)` → `Some(AppCommand::SetHideOnFullscreen(true))`.
  - `settings_command_for_button(MENU_TAG_SCALE, true)` → `None` (this tag is handled by a different `dispatchSettingsValue:` arm that reads a slider, not a button).
- `command_from_tag(MENU_TAG_REREQUEST_ACCESSIBILITY)` returns `Some(AppCommand::RequestAccessibilityPermission)` — also pure Rust, also tested here.

### Unit tests — macOS-only (require AppKit / Obj-C runtime, gated `#[cfg(target_os = "macos")]`)

`command_target_macos.rs::dispatchSettingsValue:` end-to-end (these need a real Obj-C runtime because `msg_send![sender, state]` is real Obj-C dispatch):
- Construct an `NSButton` with `setState:NSControlStateValueOn` and the appropriate tag, invoke `dispatchSettingsValue:`, observe that the right `AppCommand` is sent through a fake `EventLoopProxy`.
- Same with `NSControlStateValueOff`.

These tests live behind `#[cfg(target_os = "macos")]` next to the existing `command_target_macos` test scaffolding. The pure tag → command logic above is the primary coverage; these end-to-end tests only confirm the Obj-C bridge wiring works against a real `NSButton`. If running the harness in CI without a macOS runtime, only the pure tests run; the macOS-only tests are skipped via `cfg`.

### Smoke test (extend `scripts/`)

Inject synthetic snapshots into a `WorkspaceObserver` trait + `FakeObserver` impl behind a small dispatch enum on `App`. Decision deferred to plan stage on whether to gate this with `#[cfg(test)]` or a `test-fixtures` feature; the trait/fake split itself is the load-bearing part.

Smoke scenarios:
- Boot, idle 6 s → assert pet enters chase intent.
- Boot, frontmost = Xcode → assert pet enters avoid-cursor intent within 1 s.
- Synthesize a fullscreen-active snapshot → assert `effective_window_visible()` becomes `false` within 1 s and `window.set_visible(false)` is observed.

### Manual verification checklist (run during implementation)

- Toggle each setting off/on; effect within 1 poll cycle (~500 ms).
- Grant Accessibility permission, then deny via System Settings → graceful degradation (no crashes, no log spam).
- YouTube fullscreen in Safari → pet hides; exit → pet returns with last user-visibility.
- Type rapidly in Notes (non-editor) → pet recognizes busy via typing rate.
- Two displays, pet on display 1, fullscreen on display 2 → pet stays visible.
- Drag pet while entering fullscreen → drag terminates cleanly, pet hides.

### What we don't test

- The macOS AX API itself; trust the SDK.
- Real polling cadence under CPU load; if 500 ms proves wrong, adjust the constant.

## Open items deferred to plan stage

- Exact list of editor bundle IDs (need verification of current Cursor / JetBrains IDs).
- Whether the `FakeObserver` is gated by `#[cfg(test)]` or a `test-fixtures` Cargo feature.
- Whether to add a brief "fading" animation before `orderOut:` rather than instant hide. Punted unless it looks bad in manual testing.
