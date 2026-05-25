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
| `NSEvent.mouseLocation` | Cocoa screen, bottom-left origin, points | `logical_y = total_display_y_span - cocoa_y`; x unchanged |
| `CGWindowListCopyWindowInfo` bounds | Quartz, top-left origin, points | No flip needed; bounds already in points |
| `AXValue` rect from `kAXBoundsForRangeParameterizedAttribute` | Quartz, top-left origin, points | No flip needed |
| `NSScreen` frame (for fullscreen comparison) | Cocoa, bottom-left, points | Convert to Quartz top-left so it matches `CGWindowList` bounds |
| `winit::Monitor::size()` (physical pixels) | Top-left, physical | Divide by `scale_factor` to get logical points |

The active display for all per-display logic is the one the pet currently sits on, identified by `active_monitor_name` (already tracked in `DesktopPetApp`). The observer is told which display the pet is on via a `fn set_active_display(&mut self, name: Option<String>)` setter the app calls whenever it updates `active_monitor_name`. Multi-display correctness flows from "all comparisons happen in the active display's logical-point space".

For the fullscreen check specifically: a window from `CGWindowListCopyWindowInfo` is "fullscreen on the pet's display" iff its bounds equal the active display's logical bounds within 1 px on each side. Both sides are in points, top-left origin, no per-display origin adjustment needed because we compare against the active display's own `NSScreen.frame` converted to Quartz.

## Signals and how they're observed

| Signal | macOS API | Permission | Poll cadence |
|---|---|---|---|
| Seconds since last input | `CGEventSourceSecondsSinceLastEventType` | None | Every tick |
| Global key-event counter | `CGEventSourceCounterForEventType(kCGEventKeyDown)` | None | Every tick (delta → typing rate) |
| Frontmost app bundle ID | `NSWorkspace.frontmostApplication.bundleIdentifier` | None | 500 ms |
| Onscreen windows | `CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)` | None | 500 ms |
| Text caret rect | AX API: `AXFocusedUIElement` → `AXSelectedTextRange` → `kAXBoundsForRangeParameterizedAttribute` | Accessibility (prompt on first launch) | 250 ms |
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

### Module layout

- `src/physics.rs` (extended):
  - New `Rect { min: Vec2, max: Vec2 }` type alongside the existing `Vec2` and `Bounds`, used by `WorkspaceSnapshot.caret_rect` and `BehaviorIntent::AvoidRect`. Single canonical geometry type rather than re-inventing per module.

- `src/workspace.rs` (~250 lines, new):
  - `WorkspaceObserver` — owns last-known counter values, last-poll timestamps per source, AX permission state, cached editor-bundle-id list.
  - `fn tick(&mut self, now: Instant) -> &WorkspaceSnapshot` — polls all sources at their respective cadences, updates the snapshot, returns a reference. Called from the app's main loop.
  - `fn request_accessibility_if_needed(&mut self)` — calls `AXIsProcessTrustedWithOptions(@{kAXTrustedCheckOptionPrompt: @YES})` once on startup if `avoid_text_cursor` is enabled.
  - macOS-specific calls are behind `#[cfg(target_os = "macos")]`; other platforms get a stub `WorkspaceObserver` whose `tick` returns a default snapshot (idle = 0, no fullscreen, no caret, busy = false).

- `src/pet.rs` (extended):
  - New `BehaviorIntent` enum: `Idle | ChaseCursor { target_x: f32 } | AvoidCursor { from_x: f32 } | AvoidRect { rect: Rect }`.
  - **Motion is horizontal-only in v1.** This matches the existing model: `PetTick.speed_x` only (pet.rs:55), and `app.rs:262` only writes `physics.velocity.x`. `PetTick.speed_y` and any vertical pet motion are out of scope for this spec.
  - 2D inputs from the snapshot (cursor position, caret rect) are projected to 1D inside `decide_intent` before being placed into the intent:
    - **ChaseCursor:** `target_x = snapshot.cursor_pos.x`. The pet ambles in the cursor's general horizontal direction.
    - **AvoidCursor:** `from_x = snapshot.cursor_pos.x`. The pet ambles horizontally away.
    - **AvoidRect:** triggers only when the caret rect intersects the pet's frame in 2D. When triggered, the pet picks the horizontal direction that exits the rect with the shortest horizontal distance (computed in `decide_intent`, passed as `Direction` in the rect's accompanying intent payload — or equivalently as a signed `exit_dx: f32`).
  - `Pet::set_intent(&mut self, intent: BehaviorIntent)`.
  - The walk-cycle state machine consults the current intent at each cycle boundary to pick walk direction. `AvoidRect` is the one priority case that interrupts mid-walk; the others only take effect at the next boundary.
  - Existing personality animations (blink, happy, sleepy, curious) play during walk cycles unchanged.
  - `PetTick` is unchanged — still `{ state, frame_index, speed_x }`. The pet keeps emitting horizontal speed only; the intent only influences which horizontal direction the next walk picks.

- `src/app.rs` window-visibility composition (no new controller — `window_macos.rs` stays a collection of helper functions):
  - `DesktopPetApp` already owns `pet_visible: bool` (app.rs:72). Add a sibling field `auto_hidden: bool` (default false, runtime-only, never persisted to settings).
  - New private helper: `fn effective_window_visible(&self) -> bool { self.pet_visible && !self.auto_hidden }`.
  - New private helper: `fn apply_window_visibility(&self)` — calls `window.set_visible(self.effective_window_visible())` plus the existing redraw/tick-resume logic that lives today inside `set_pet_visible` (app.rs:363-380).
  - The existing `set_pet_visible(&mut self, visible: bool)` is refactored: updates `self.pet_visible`, then calls `apply_window_visibility()` instead of calling `window.set_visible` directly with the raw `visible` argument.
  - New method `set_auto_hidden(&mut self, hidden: bool)` on `DesktopPetApp`: updates `self.auto_hidden`, then calls `apply_window_visibility()`. Does NOT touch settings or save to disk. Does NOT call `sync_settings_window()` / `sync_menu_bar()` — auto-hide is invisible to the user-facing controls.
  - Tick cadence while auto-hidden: the tick loop continues to run so the workspace observer and pet state machine stay live. The redraw call inside `tick` is gated on `effective_window_visible()` — when hidden, no redraw is requested. This matches the existing pattern around `IDLE_FRAME_TIME` / `SLEEP_FRAME_TIME` (app.rs:33-35); we add a similar gate.
  - Drag termination on auto-hide entry: before `set_auto_hidden(true)` flips the flag, the app inspects `self.interaction.is_dragging()` and synthesizes a drag-end if needed, to avoid the leaked held-mouse state called out in §"Error handling and degradation".

- `src/app.rs` (orchestration):
  - Owns a `WorkspaceObserver` and a `Settings`.
  - On each tick: call `observer.tick()`, run `decide_intent(...)` (a pure function), push intent to pet and auto-hide flag to window.

- `src/settings.rs` (extended):
  - Three new `bool` fields: `follow_cursor_when_idle`, `avoid_text_cursor`, `hide_on_fullscreen`. Each has `#[serde(default = "default_true")]`. Existing settings files load with all three defaulting to `true`.

- `src/settings_window_macos.rs` (extended):
  - Three new `NSButton` checkboxes under a "Workspace Awareness" section heading. A "Re-request Accessibility permission" button next to `avoid_text_cursor`.
  - Each control is wired through the existing settings-window pattern: control change → `SettingsWindowController` emits an `AppCommand` via the `EventLoopProxy<AppCommand>` (same path personality/scale/movement-speed/etc. use today).

- `src/app.rs::AppCommand` (extended) — new variants:
  - `SetFollowCursorWhenIdle(bool)`
  - `SetAvoidTextCursor(bool)`
  - `SetHideOnFullscreen(bool)`
  - `RequestAccessibilityPermission`

  These are handled in `DesktopPetApp`'s command dispatch (alongside `SetPersonality`, `SetScale`, etc.). The three `Set*` variants update `self.settings.<field>`, call `save_settings()`, and on the next tick the new value gates `decide_intent` / `set_auto_hidden`. `RequestAccessibilityPermission` calls into `WorkspaceObserver::request_accessibility_if_needed()`.

- `src/menu_bar.rs` — no changes. The three toggles are settings-panel-only (decided in §3); no new `MENU_TAG_*` constants and no new `command_from_tag` arms. The existing menu bar tag namespace stays clean.

- `src/command_target_macos.rs` — no changes required. The new `AppCommand` variants flow through the same `EventLoopProxy` channel that the settings window already uses; the `CommandTarget` Objective-C bridge is only invoked for menu items, which we are not adding.

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
   │       │     intent = AvoidRect(caret_rect)                  │
   │       │ elif follow_cursor_when_idle && snapshot.is_idle(): │
   │       │     intent = ChaseCursor(target_x=cursor.x)         │
   │       │ elif follow_cursor_when_idle && snapshot.is_busy(): │
   │       │     intent = AvoidCursor(from_x=cursor.x)           │
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

**Accessibility permission.** First launch with `avoid_text_cursor` on: prompt once. If denied, `caret_rect` is always `None`, the `AvoidRect` arm of the decision tree never fires, and the rest of the features work normally. Settings exposes a "Re-request Accessibility permission" button to re-trigger the prompt. If the user grants it later via System Settings, the next poll picks it up automatically.

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

**Drag-in-progress + fullscreen.** If the pet is being dragged when fullscreen begins, `set_auto_hidden(true)` first terminates the drag synchronously by clearing the `InteractionState` (drop dragging + hover flags) before flipping `auto_hidden` and calling `apply_window_visibility()`. Avoids leaked held-mouse state.

**Pet stuck near screen edge.** The existing physics module clamps to the configured bounds. New behavior only sets target direction; final position is always clamped by physics. No change needed.

**Settings backward compatibility.** Existing `~/Library/Application Support/Happy Cappy/settings.json` files lack the three new keys. `#[serde(default = ...)]` on each field fills them with `true`. No migration code.

## Testing

### Unit tests (Rust, no macOS runtime required)

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

`pet.rs` intent handling:
- `set_intent(ChaseCursor)` mid-walk does not interrupt; takes effect at next walk-cycle boundary.
- `set_intent(AvoidRect)` interrupts immediately.
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
- `cocoa_screen_to_pet_space((x, y), display_y_span)` flips Y correctly for known inputs.
- `quartz_rect_to_pet_space(rect)` is identity (round-trip equal).
- `monitor_size_physical_to_logical(size, scale_factor)` divides correctly for `scale_factor = 1.0, 2.0, 1.5`.

Command dispatch (in `app.rs`):
- `AppCommand::SetFollowCursorWhenIdle(false)` updates `self.settings.follow_cursor_when_idle` and calls `save_settings()`.
- `AppCommand::SetAvoidTextCursor(true)` triggers `request_accessibility_if_needed()` on the observer.
- `AppCommand::SetHideOnFullscreen(false)` immediately allows next-tick `set_auto_hidden(false)` even if fullscreen is currently true.
- `AppCommand::RequestAccessibilityPermission` calls into the observer's prompt path.

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
