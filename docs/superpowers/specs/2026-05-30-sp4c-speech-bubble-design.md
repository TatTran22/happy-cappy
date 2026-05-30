# SP4-C — Speech Bubble UI — Design

> Part of the multi-pet platform roadmap (umbrella: `2026-05-26-pet-manifest-refactor-design.md`).
> Sub-project 4, third of three specs. **Depends on SP4-B** (notification model + runtime `NotificationState`).
> - SP4-A: animation lifecycle engine (manifest v2, one-shot, fallback, loop_start, per-frame ms).
> - SP4-B: generic local notification model + Unix-socket transport + `notify` CLI. `label`/`body` carried + logged but **not rendered**.
> - **SP4-C (this spec):** render the active notification's `label`/`body` as a speech bubble above the pet, using a native AppKit child window.

## 1. Context & intent

SP4-B made the pet *react* to notifications by changing animation for the notification's TTL, and it already carries `label`/`body` on `NotificationState` (currently `#[allow(dead_code)]`). SP4-C makes that text **visible**: a small warm speech bubble above the pet showing `label` (title) and `body`.

The bubble is **notification-driven** in this sub-project, but its content type is defined as a **standalone interface** (`BubbleContent`) so a later producer (e.g. a Hermes agent message, or pet "chatter") can construct one directly without a rewrite. No general conversation/speech system is built here.

### Non-goals (SP4-C)

- No general/dynamic dialogue system, no chat history, no scrollable/long-form reader.
- No direction-based or walking-aware offset (the bubble anchors to the pet's top-center only). Deferred as polish.
- No vibrancy/HUD appearance, no automatic dark/light-mode switching. Deferred as a future appearance option.
- No kind-fallback copy (e.g. "Done!") and no i18n/default strings.
- No changes to SP4-B's animation selection or preemption semantics. (One required correctness fix to TTL *expiry timing* — wall-clock accuracy while hidden — is in scope; see §6.)
- The bubble is **display-only** and click-through; not interactive (no dismiss-on-click, no expand).

## 2. Architecture & module boundaries

The hard split is: **measure = macOS** (`NSTextField` fitting size) → **place = pure Rust** → **draw + set frame = macOS**.

| File | Responsibility | Tested |
|---|---|---|
| `src/bubble.rs` *(new)* | `BubbleContent`, `BubbleAccent`, and derivation from notification state (visibility rule + `kind`→accent). Standalone interface. | unit |
| `src/bubble_layout.rs` *(new)* | Placement geometry: `place_bubble(...) -> BubblePlacement`. Pure Rust. | unit |
| `src/bubble_window_macos.rs` *(new)* | `BubbleWindow`: transparent borderless NSWindow + custom `NSView` for background/border/shadow/tail/dot, hosting child `NSTextField`s for title/body. Measures fitting size via `NSTextField` (no Core Text). | smoke |
| `src/pet/runtime.rs` *(modified)* | Store `kind` on `NotificationState`; add `pub fn bubble_content(&self) -> Option<BubbleContent>`; drop `#[allow(dead_code)]` on `label`/`body`. | unit |
| `src/app.rs` *(modified)* | Own `Option<BubbleWindow>`; each frame query `bubble_content()` → show/update/hide and set frame via `place_bubble`. | — |
| `src/lib.rs` *(modified)* | Declare `bubble`, `bubble_layout`, and (cfg macOS) `bubble_window_macos`. | — |
| `scripts/smoke_app.sh` *(modified)* | Add a bubble smoke step. | — |

`bubble_window_macos` mirrors the existing `settings_window_macos.rs` / `picker_window_macos.rs` naming and `#[cfg(target_os = "macos")]` pattern.

## 3. Content model & visibility rule (`src/bubble.rs`, pure Rust)

```rust
pub struct BubbleContent {
    pub title: Option<String>,   // trimmed label; None if empty after trim
    pub body:  Option<String>,   // trimmed body;  None if empty after trim
    pub accent: BubbleAccent,    // derived from notification kind
}

pub enum BubbleAccent { Running, Message, Succeeded, NeedsReview, Failed }
```

- `BubbleContent` is constructible directly by any producer. SP4-C provides one constructor that derives it from the active notification.
- **Trim** `label`/`body`. A field that is empty/whitespace-only after trim becomes `None`.
- **Visibility rule:** if *both* `title` and `body` are `None` → no bubble (`bubble_content()` returns `None`). Title-only and body-only are both valid; body-only reserves no title space.
- **Accent mapping** from `kind`: `running→Running`, `message→Message`, `succeeded→Succeeded`, `needs-review→NeedsReview`, `failed→Failed`; any unknown kind → `Message` (mirrors `preset_for`'s default).
- Accent → RGBA is owned by the pure layer so the macOS view just reads a color:

  | accent | kind | hex | dot |
  |---|---|---|---|
  | Running | `running` | `#3E7BD6` | normal |
  | Message | `message` / unknown | `#8A909C` | normal |
  | Succeeded | `succeeded` | `#3E9B4F` | normal |
  | NeedsReview | `needs-review` | `#E0A32E` | **emphasized** |
  | Failed | `failed` | `#E5484D` | **emphasized** |

- Content tracks the notification in lockstep: when SP4-B preempts (higher priority) or replaces (latest-wins at equal priority), the next `bubble_content()` call returns the new content. The bubble does **not** own a TTL.

### Runtime wiring (`src/pet/runtime.rs`)

`NotificationState` gains a `kind: String` field (set in `set_notification`). Add:

```rust
pub fn bubble_content(&self) -> Option<BubbleContent>
```

which returns `None` when there is no active notification *or* when the visibility rule yields no text, else `Some(BubbleContent)`. `label`/`body` lose their `#[allow(dead_code)]`. `bubble_content()` is **read-only** — it does not touch animation resolution, the countdown, or preemption. (The separate wall-clock TTL-expiry fix lives in §6.)

## 4. Placement geometry (`src/bubble_layout.rs`, pure Rust)

**Coordinate system.** All placement math is done in the app's existing **winit/Quartz logical coordinate system** (primary-display top-left origin, **Y-down**, points) — the same space as `physics`, `workspace` (see `workspace.rs`: "primary display top-left origin, Y-down, points"), and `move_window_to_pet`'s `LogicalPosition`. The pure layer never sees AppKit coordinates. The single Y-up↔Y-down flip reuses the **existing** `workspace::cocoa_to_quartz_y(y, primary_display_height)` helper (`workspace.rs`) at the `bubble_window_macos` boundary when setting the child window's frame.

**Visible-frame source (new macOS helper).** Note that `physics.bounds` is the **full** monitor rect (winit `monitor.size()`, `app.rs`) and does *not* exclude the menu bar / Dock — so it is not a correct usable area on its own. SP4-C adds a macOS helper `active_visible_frame_y_down(window) -> Rect` (in `bubble_window_macos`, or `window_macos`) that reads the pet screen's `NSScreen.visibleFrame` and converts it to Y-down logical points via `cocoa_to_quartz_y`. If a screen can't be resolved (or on stub/non-macOS builds), it **falls back to `physics.bounds`** (full monitor rect). This helper — not `physics.bounds` directly — supplies the visible-frame input below.

Inputs (all in Y-down logical points): the pet window rect, the measured bubble size, the active display's visible frame (from `active_visible_frame_y_down`), and an inset. Output:

```rust
pub struct BubblePlacement { pub origin: (f64, f64), pub tail: TailSide, pub tail_x: f64 }
pub enum TailSide { Down, Up }
```

- **Anchor:** pet window **top-center**.
- **Default (above):** bubble sits `gap ≈ 6pt` above the pet (smaller Y), horizontally centered on the pet center, then **clamped** to `[screen.minX + inset, screen.maxX − inset − width]` (`inset ≈ 8pt`). Tail points **down**.
- **Tail x:** `petCenterX − bubbleOriginX`, clamped to stay within the bubble body (leaving the corner radius), so it always points at the pet but never overruns a rounded corner.
- **Flip (below):** if there isn't room above — i.e. the bubble's top edge would fall above `screen.minY + inset` (Y-down) — place it `gap` below the pet instead (larger Y); tail points **up**.
- **Screen:** the active display containing the pet; its visible frame (menu bar / Dock excluded) is supplied in Y-down logical points. Multi-monitor safe.
- **Degenerate case:** if neither above nor below fully fits (tiny screen), prefer above and clamp vertically into the visible frame.

The macOS layer recomputes placement **every frame while the bubble is visible** (cheap), which uniformly handles dragging, walking, content size changes, and edge flips.

## 5. macOS render layer (`src/bubble_window_macos.rs`)

- **Window:** borderless `NSWindow`, transparent (`opaque = false`, clear background), `ignoresMouseEvents = true` (**click-through**; never intercepts the pet's drag/hover/click), non-activating. Added as a **child window** of the pet window (ordered above), so it shares the pet's space and hides when the app hides; its frame is still set explicitly each frame from `place_bubble`.
- **Drawing** (custom layer-backed `NSView`): rounded rect `r ≈ 11`, padding `≈ 9×12`, background `#F5F2EC`, border `1px rgba(0,0,0,0.08)`, soft drop shadow; a kind **accent dot** (`Ø 7px`, enlarged to `Ø 9px` for `NeedsReview`/`Failed`); a downward/upward **tail** triangle (~14px base) at `tail_x`.
- **Text (native, `NSTextField`):** title `12pt` semibold, body `11pt` regular, color `#23262E`. Title is a single-line `NSTextField` with `lineBreakMode = .byTruncatingTail`. Body is an `NSTextField` with `maximumNumberOfLines = 3` and `.byTruncatingTail` (last-line `…`). `NSTextField` is already an enabled `objc2-app-kit` feature and is used in `picker_window_macos.rs` — **no new dependency, no Core Text**.
- **Sizing:** **max outer bubble width = 240pt** (logical). The text column width = `240 − horizontalPadding − dotDiameter − dotGap`; each `NSTextField`'s `preferredMaxLayoutWidth` is set to that column width.
- **Measurement:** the bubble size is derived from the title/body `NSTextField` `fittingSize` (height) at the fixed text-column width, plus paddings; this size is the input to `place_bubble`.
- **Metrics** (radius, paddings, fonts, colors, line caps) are named constants for easy tuning at review time.

## 6. App integration & lifecycle (`src/app.rs`)

- `DesktopPetApp` owns `Option<BubbleWindow>`, created lazily the first time a bubble needs to show (and reused thereafter).
- Each frame:
  - Compute `want = self.pet.bubble_content()` **and** `self.effective_window_visible()`.
  - `Some(content)` & window visible → show/update the bubble with `content`, measure size, compute `place_bubble` from the pet window rect + the active display's visible frame (Y-down logical, §4), set the frame.
  - `None`, **or** the pet window is not visible (`effective_window_visible() == pet_visible && !auto_hidden` is false — menu **Hide**, or auto-hide on fullscreen/avoid) → hide the bubble.
  - **Focus Mode does NOT hide the bubble.** `set_focus_mode` only toggles mouse passthrough (`sync_window_passthrough`), not visibility — so in Focus Mode the pet and bubble both stay visible and click-through.
- **Lifecycle = active notification lifecycle.** The bubble shows / updates / hides in lockstep with the active notification (clear / expire / preempt / latest-wins); its *visibility* additionally requires `effective_window_visible()`.
- **Required TTL-correctness fix (carried into SP4-C).** Today the TTL is decremented by the per-tick `dt`, which `app.rs` caps at `MAX_TICK_DELTA = 1s` and only delivers every `5s` while the pet is hidden (`next_tick_interval`) — so a 10s notification can take ~50s of wall-clock to expire when hidden, then resurface as a stale bubble on unhide. SP4-C requires the notification lifetime to be **wall-clock accurate regardless of tick cadence or pet visibility** — e.g. store an absolute `expires_at: Instant` in `set_notification` and expire when `now >= expires_at`, rather than accumulating capped `dt`.
  - **Scheduler contract (required, not just `expires_at`).** `expires_at` alone is insufficient: while hidden, `next_tick_interval()` returns `5s`, so expiry would only be *noticed* at the next tick (up to 5s late). So whenever a notification is active, `next_tick_interval()` **must return no more than the time remaining until `expires_at`**, so the runtime wakes to expire/clear it on time even when hidden. (Accepted relaxation *only if explicitly chosen later*: drop the wake-bound and document "expires on the first tick after `expires_at`" — i.e. up to ~5s late. This is **not** the chosen behavior; the wake-bound is the requirement.)
  - These two together (absolute `expires_at` + wake-bound) are the one intentional change to SP4-B's TTL behavior.
- **Fade:** fade-in on show / fade-out on hide via `alphaValue` (~120ms) as the default. A `remaining`-driven fade (dimming near end of TTL) is optional polish — `BubbleContent` may carry `remaining`/`expires_at` for this but never decides its own lifetime.

## 7. Edge cases

- Both fields empty/whitespace → no window shown.
- Notification with no text but with an animation → animation reacts (SP4-B), no bubble (SP4-C).
- Content changes while visible (preempt / latest-wins) → text + size + placement update in place.
- Pet hidden (menu **Hide** / auto-hide) while a notification is active → bubble hidden; notification animation unaffected; TTL now expires on wall-clock (§6), so the notification does not resurface as a stale bubble on unhide.
- **Focus Mode** active → pet and bubble both remain visible and click-through (Focus Mode ≠ hidden).
- Pet dragged/walking near a screen edge → per-frame recompute clamps horizontally and flips above/below as needed.
- Very long unbroken token in body → `NSTextField` character-wraps within the text-column width, then truncates at 3 lines with `…`.
- Multi-monitor → clamps to the display currently containing the pet.

## 8. Testing strategy

- **Unit — `src/bubble.rs`:** both-empty→`None`; whitespace-only trim→`None`; title-only; body-only; `kind`→accent for each known kind and an unknown kind (→`Message`); content reflects the active notification after preempt/replace.
- **Unit — `src/bubble_layout.rs`:** default-above; flip-when-near-top; horizontal clamp left/right with inset; `tail_x` correctness and clamping at corners; varying bubble widths; degenerate tiny-screen fallback.
- **Parity + TTL fix:** SP4-B animation-selection / preemption tests stay unchanged. TTL tests are updated for wall-clock expiry. Add **app-level tests** that (a) `next_tick_interval()` is bounded by the time remaining until `expires_at` whenever a notification is active (the wake-bound contract, §6), and (b) a short-TTL notification expires on wall-clock even when the pet is hidden — so it does not resurface as a stale bubble on unhide. Also assert `bubble_content()` is read-only (no lifecycle side effects).
- **Smoke (manual checklist):** `scripts/smoke_app.sh` is a **manual** checklist and is **not** run by `verify.sh`. Add a checklist item that fires `notify --kind needs-review --label "…" --body "…"` and visually confirms the bubble appears above the pet with the correct style, position, edge-flip, and follow-while-dragged.

## 9. Exit criteria

- `scripts/verify.sh` green (the automated gate; it does **not** run `smoke_app.sh`): `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, `build_app.sh` bundle assembly, and `codesign --verify`.
- A `notify` event with `label`/`body` shows a warm light bubble above the pet that flips/clamps at screen edges, follows the pet while dragged/walking, hides when the pet is hidden (but stays visible & click-through in Focus Mode), and clears/swaps in lockstep with the notification.
- A notification's TTL expires on wall-clock time even while the pet is hidden (covered by an app-level test); no stale bubble resurfaces on unhide.
- An event with no text shows no bubble while the animation still reacts (SP4-B parity).
- No `#[allow(dead_code)]` remains on `NotificationState.label`/`body`.

## 10. File summary

**New:** `src/bubble.rs`, `src/bubble_layout.rs`, `src/bubble_window_macos.rs`.
**Modified:** `src/pet/runtime.rs` (add `kind`, `bubble_content()`, drop dead_code), `src/app.rs` (own + drive `BubbleWindow`), `src/lib.rs` (module decls), `scripts/smoke_app.sh` (bubble smoke step).
