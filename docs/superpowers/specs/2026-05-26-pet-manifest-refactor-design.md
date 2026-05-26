# Pet Manifest Refactor — Sub-project 1 Design

**Date:** 2026-05-26
**Status:** Draft for review
**Owner:** Tat Tran

## Context

Happy Cappy currently hard-codes a single capybara pet across three concerns: state machine (`src/pet.rs`), spritesheet layout (`src/sprite.rs`), and frame timing (constants in `src/pet.rs`). The `AnimationGroup` enum locks the set of animations to ten Rust variants, and adding a new animation requires modifying the enum, the spritesheet row mapping, and the behavior resolver in `Pet::refresh_behavior_mode()`.

We want to evolve Happy Cappy toward a multi-pet / customisable platform inspired by Codex CLI's `/pets` feature (see `~/Downloads/codex-pets-reference.md`). The long-term roadmap has four sub-projects:

1. **Sub-project 1 (this spec):** Animation engine refactor + pet manifest data-driven. Happy Cappy capybara loaded from an embedded JSON manifest. No behavior change visible to the user.
2. Sub-project 2: Catalog + custom pet loading from `~/Library/Application Support/Happy Cappy/pets/<id>/pet.json`.
3. Sub-project 3: Picker UI in the Settings panel with preview pane.
4. Sub-project 4: Notification system with external triggers, modelled on Codex `PetNotificationKind` + TTL.

This spec covers only sub-project 1. Sub-projects 2–4 are out of scope and will get their own spec/plan cycles.

## Goals

- Split `Pet` into a data-only `PetManifest` (parsed from JSON) and a runtime state machine `PetRuntime`.
- Replace `AnimationGroup` enum with string-keyed animation map plus a fallback chain resolver.
- Move frame timing from hard-coded constants to per-frame `ms` values inside the manifest.
- Bundle the existing capybara as `assets/manifests/happy_cappy.json`, loaded via `include_str!`.
- Keep all existing behavior (animation cycles, hover/drag/walk/sleep, workspace awareness, focus mode, micro-actions, drag persistence) byte-for-byte identical.

## Non-goals

- Multi-pet catalog, custom pet loading from disk, picker UI, notification system. Each deferred to its own sub-project.
- Changes to `workspace.rs`, `interaction.rs`, `settings.rs`, `settings_window_macos.rs`, `menu_bar.rs`, `renderer.rs`, `window_macos.rs`, `bundle.rs`, `command_target_macos.rs`. They keep their current API.
- Changing the sprite asset (`assets/happy_cappy_spritesheet.png` stays as-is, 256×640, 4×10 grid).
- Changes to `settings.json` schema. No `selected_pet_id` field added yet.

## Architecture

### Module layout

```
src/
├── pet/
│   ├── mod.rs          — public API, re-exports
│   ├── manifest.rs     — PetManifest, Animation, AnimationFrame, FrameGeometry,
│   │                     serde Deserialize, validation, load_embedded_happy_cappy()
│   ├── runtime.rs      — PetRuntime (state machine, renamed from Pet), BehaviorMode,
│   │                     BehaviorIntent, PetState, Direction, Personality, PetTick,
│   │                     AnimationCursor
│   └── resolver.rs     — resolve_animation_chain(), lookup_with_fallback()
├── sprite.rs           — trimmed; SpriteRow enum removed
├── ...                 — other files unchanged except for import renames
└── assets/
    └── manifests/
        └── happy_cappy.json   — bundled manifest, source of truth for animation data
```

`src/pet.rs` (current 867 LoC) is replaced by the `src/pet/` directory.

### Public API of the `pet` module

```rust
// pet/mod.rs
pub use manifest::{PetManifest, Animation, AnimationFrame, FrameGeometry};
pub use runtime::{
    PetRuntime, PetState, Direction, Personality,
    BehaviorMode, BehaviorIntent, PetTick,
};
pub use resolver::resolve_animation_chain;
```

The current `crate::pet::Pet` callers (`app.rs` is the main one) become `crate::pet::PetRuntime`. `AnimationGroup` is removed entirely from the public API.

## Data Model

### Manifest schema (JSON)

File: `assets/manifests/happy_cappy.json`.

```json
{
  "manifest_version": 1,
  "id": "happy-cappy",
  "displayName": "Happy Cappy",
  "spritesheetPath": "happy_cappy_spritesheet.png",
  "frame": { "width": 64, "height": 64, "columns": 4, "rows": 10 },
  "animations": {
    "idle":           { "frames": [{"index": 0,  "ms": 200}, {"index": 1,  "ms": 200}, {"index": 2,  "ms": 200}, {"index": 3,  "ms": 200}], "loop_start": 0 },
    "blink":          { "frames": [{"index": 4,  "ms": 200}, {"index": 5,  "ms": 200}, {"index": 6,  "ms": 200}, {"index": 7,  "ms": 200}], "loop_start": 0 },
    "happy":          { "frames": [{"index": 8,  "ms": 200}, {"index": 9,  "ms": 200}, {"index": 10, "ms": 200}, {"index": 11, "ms": 200}], "loop_start": 0 },
    "curious":        { "frames": [{"index": 12, "ms": 200}, {"index": 13, "ms": 200}, {"index": 14, "ms": 200}, {"index": 15, "ms": 200}], "loop_start": 0 },
    "sleepy":         { "frames": [{"index": 16, "ms": 500}, {"index": 17, "ms": 500}, {"index": 18, "ms": 500}, {"index": 19, "ms": 500}], "loop_start": 0 },
    "hover-calm":     { "frames": [{"index": 20, "ms": 220}, {"index": 21, "ms": 220}, {"index": 22, "ms": 220}, {"index": 23, "ms": 220}], "loop_start": 0 },
    "hover-cheerful": { "frames": [{"index": 24, "ms": 140}, {"index": 25, "ms": 140}, {"index": 26, "ms": 140}, {"index": 27, "ms": 140}], "loop_start": 0 },
    "hover-lively":   { "frames": [{"index": 28, "ms": 90},  {"index": 29, "ms": 90},  {"index": 30, "ms": 90},  {"index": 31, "ms": 90}],  "loop_start": 0 },
    "walk-right":     { "frames": [{"index": 32, "ms": 100}, {"index": 33, "ms": 100}, {"index": 34, "ms": 100}, {"index": 35, "ms": 100}], "loop_start": 0 },
    "drag":           { "frames": [{"index": 36, "ms": 200}, {"index": 37, "ms": 200}, {"index": 38, "ms": 200}, {"index": 39, "ms": 200}], "loop_start": 0 }
  }
}
```

Frame `ms` values preserve current behavior:

- Idle/Blink/Happy/Curious row durations: 200ms each (matches current `IDLE_FRAME_MS = 200`).
- Sleepy: 500ms (matches `SLEEP_FRAME_MS = 500`).
- Walk-right: 100ms (matches `WALK_FRAME_MS = 100`).
- Drag: 200ms (matches the idle default it currently falls into).
- Hover variants: 220/140/90 ms — these are the `hover_frame_duration()` values for Calm/Cheerful/Lively at `hover_intensity = 1.0` today. Hover intensity becomes a runtime multiplier (see Resolver section).

### Rust structs

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetManifest {
    #[serde(default = "default_manifest_version")]
    pub manifest_version: u32,
    pub id: String,
    pub display_name: String,
    pub spritesheet_path: String,
    pub frame: FrameGeometry,
    pub animations: BTreeMap<String, Animation>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FrameGeometry {
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub rows: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Animation {
    pub frames: Vec<AnimationFrame>,
    #[serde(default)]
    pub loop_start: Option<usize>,
    #[serde(default)]
    pub fallback: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct AnimationFrame {
    pub index: u32,
    pub ms: u64,
}

fn default_manifest_version() -> u32 { 1 }
```

`BTreeMap` (not `HashMap`) for deterministic iteration in tests and Debug output.

### Validation rules

`PetManifest::validate()` returns `Result<(), ManifestError>` enforcing:

- `manifest_version >= 1` (reject 0; warn but accept values > 1 for forward compat).
- `id` non-empty, contains no `/`, `\`, or null bytes.
- `display_name` non-empty.
- `frame.width`, `frame.height`, `frame.columns`, `frame.rows` all > 0.
- For every animation: `frames.len() >= 1`.
- For every frame: `index < frame.columns * frame.rows`.
- For every frame: `ms >= 1` and `ms <= MAX_FRAME_DURATION_MS (= 10_000)`.
- For every animation: `frames.len() <= MAX_FRAMES_PER_ANIMATION (= 64)`.
- If `loop_start = Some(i)`: `i < frames.len()`.
- If `fallback = Some(name)`: `animations.contains_key(name)`.
- `animations.contains_key("idle")` — guarantees the terminal fallback always resolves.

`ManifestError` is a structured enum (one variant per failure category) implementing `std::error::Error`.

### Embedded loading

```rust
impl PetManifest {
    pub fn from_json_str(json: &str) -> Result<Self, ManifestError> {
        let raw: PetManifest = serde_json::from_str(json)
            .map_err(ManifestError::Json)?;
        raw.validate()?;
        Ok(raw)
    }

    pub fn load_embedded_happy_cappy() -> Self {
        const JSON: &str = include_str!("../../assets/manifests/happy_cappy.json");
        Self::from_json_str(JSON)
            .expect("bundled happy_cappy.json must be valid (caught by CI test)")
    }
}
```

`from_json_str` is the public core; `load_embedded_happy_cappy` is a thin wrapper for sub-project 1. Sub-project 2 will add `from_path` reusing `from_json_str`.

## Animation Resolver

### Behavior-to-name mapping (`resolve_animation_chain`)

```rust
pub fn resolve_animation_chain(
    mode: BehaviorMode,
    personality: Personality,
    expression_index: usize,
    action: Option<MicroAction>,
) -> &'static [&'static str] {
    match mode {
        BehaviorMode::Hidden => &["idle"],
        BehaviorMode::Dragging => &["drag", "idle"],
        BehaviorMode::Hovered => match personality {
            Personality::Calm     => &["hover-calm",     "hover", "idle"],
            Personality::Cheerful => &["hover-cheerful", "hover", "idle"],
            Personality::Lively   => &["hover-lively",   "hover", "idle"],
        },
        BehaviorMode::Action => match action {
            Some(MicroAction::Nap)     => &["sleepy", "idle"],
            Some(MicroAction::CheerUp) => &["happy",  "idle"],
            None => &["idle"],
        },
        BehaviorMode::Walking => &["walk-right", "walk", "idle"],
        BehaviorMode::Default => match expression_index % 5 {
            0 => &["idle"],
            1 => &["blink",   "idle"],
            2 => &["happy",   "idle"],
            3 => &["curious", "idle"],
            _ => &["sleepy",  "idle"],
        },
    }
}
```

### Lookup with fallback

```rust
pub fn lookup_with_fallback<'a>(
    manifest: &'a PetManifest,
    chain: &[&str],
) -> (&'a str, &'a Animation) {
    for name in chain {
        if let Some(anim) = manifest.animations.get(*name) {
            return (name, anim);
        }
    }
    let idle = manifest.animations.get("idle")
        .expect("manifest validation guarantees 'idle' exists");
    ("idle", idle)
}
```

### `AnimationCursor` — frame from elapsed time

```rust
pub struct AnimationCursor {
    name: String,
    elapsed_in_anim: Duration,
}

impl AnimationCursor {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), elapsed_in_anim: Duration::ZERO }
    }

    pub fn name(&self) -> &str { &self.name }

    pub fn tick(&mut self, dt: Duration, time_multiplier: f32) {
        let scaled = Duration::from_secs_f32(dt.as_secs_f32() * time_multiplier.max(0.0));
        self.elapsed_in_anim += scaled;
    }

    pub fn current_frame<'a>(&self, anim: &'a Animation) -> &'a AnimationFrame {
        let total_ms: u64 = anim.frames.iter().map(|f| f.ms).sum();
        let elapsed_ms = self.elapsed_in_anim.as_millis() as u64;
        let pos_ms = match anim.loop_start {
            None => elapsed_ms.min(total_ms.saturating_sub(1)),
            Some(start) => {
                if elapsed_ms < total_ms {
                    elapsed_ms
                } else {
                    let head_ms: u64 = anim.frames[..start].iter().map(|f| f.ms).sum();
                    let loop_len = total_ms - head_ms;
                    head_ms + ((elapsed_ms - head_ms) % loop_len.max(1))
                }
            }
        };
        let mut acc = 0u64;
        for frame in &anim.frames {
            acc += frame.ms;
            if pos_ms < acc { return frame; }
        }
        anim.frames.last().expect("validation guarantees non-empty frames")
    }
}
```

### Hover intensity

`hover_intensity` is no longer baked into per-mode frame duration. The runtime computes a `time_multiplier`:

```rust
fn time_multiplier(&self) -> f32 {
    if self.behavior_mode == BehaviorMode::Hovered {
        self.hover_intensity.max(0.5)
    } else {
        1.0
    }
}
```

Applied in `cursor.tick(dt, multiplier)`. Behavior is mathematically equivalent to the current `base_ms / divisor` formula when every frame in the hover animation has the same `ms`, which is true in the bundled manifest.

### Animation transition

```rust
fn refresh_animation(&mut self) {
    let chain = resolve_animation_chain(
        self.behavior_mode,
        self.personality,
        self.expression_index,
        self.action_override.map(|a| a.action()),  // ActionOverride is Copy
    );
    let (name, _) = lookup_with_fallback(&self.manifest, chain);
    if self.cursor.name() != name {
        self.cursor = AnimationCursor::new(name);
    }
}
```

Called from `refresh_behavior_mode()` (rename: this method now also refreshes animation). Cursor resets to elapsed=0 only on name change, matching today's "reset frame_index to 0 when entering a new state".

## Sprite & Renderer Changes

### `sprite.rs`

- Delete: `SpriteRow` enum, `EXPECTED_COLUMNS`, `EXPECTED_ROWS`, `impl From<AnimationGroup> for SpriteRow`.
- Replace `SpriteSheet::frame_rect(SpriteRow, frame_index)` with `frame_rect(sprite_index: u32, geometry: &FrameGeometry) -> FrameRect`.
- `SpriteSheet::load`/`from_image` takes `&FrameGeometry` instead of `frame_size: u32`. Validation: image width == `geometry.columns * geometry.width`, height == `geometry.rows * geometry.height`.

```rust
pub fn frame_rect(&self, sprite_index: u32, geometry: &FrameGeometry) -> FrameRect {
    let row = sprite_index / geometry.columns;
    let col = sprite_index % geometry.columns;
    FrameRect {
        x: col * geometry.width,
        y: row * geometry.height,
        width: geometry.width,
        height: geometry.height,
    }
}
```

### `app.rs`

- `pet: Pet` → `pet: PetRuntime`.
- `Pet::new_with_seed(seed)` → `PetRuntime::new(manifest, seed)` where `manifest = PetManifest::load_embedded_happy_cappy()`.
- `FRAME_SIZE: u32 = 64` constant removed; window sizing reads from `runtime.manifest().frame.width` instead.
- Per-frame draw: replace `sprite_sheet.frame_rect(SpriteRow::from(group), frame_index)` with `sprite_sheet.frame_rect(runtime.current_sprite_index(), runtime.manifest().frame)`.
- Direction-based flip stays as-is: `runtime.direction() == Direction::Left` ⇒ `flip_x = true`.

`PetRuntime` exposes a small accessor surface used by `app.rs`:

```rust
impl PetRuntime {
    pub fn manifest(&self) -> &PetManifest { &self.manifest }
    pub fn current_sprite_index(&self) -> u32 { /* cursor.current_frame(...).index */ }
    pub fn current_animation_name(&self) -> &str { self.cursor.name() }
    // unchanged: state(), direction(), behavior_mode(), personality(),
    //            tick(), set_hovered(), set_dragging(), set_hidden(),
    //            set_intent(), start_micro_action(), clear_micro_action(),
    //            apply_personality(), set_movement_speed_multiplier(),
    //            set_hover_intensity(), turn_around()
}
```

`current_animation_group()` is removed.

## State Machine Internals

`PetRuntime` retains all current fields except:

- Remove `animation_group: AnimationGroup`.
- Remove `frame_index: usize` and `frame_elapsed: Duration`.
- Add `manifest: PetManifest` (owned, cheap clone — small).
- Add `cursor: AnimationCursor`.

Constants removed from `pet/runtime.rs`:

- `IDLE_FRAME_MS`, `WALK_FRAME_MS`, `SLEEP_FRAME_MS` — frame durations now live in the manifest.
- `FRAME_COUNT = 4` — animations declare their own frame count.
- `hover_frame_duration()` private method — replaced by `time_multiplier()` + per-frame `ms`.
- `default_expression_group()` private method — replaced by `resolve_animation_chain` Default arm.

Kept (these describe behavior, not animation): `WALK_SPEED`, `WALK_DISTANCE`, expression intervals (`Personality` → 5s/3s/2s), idle→walk threshold (5s), sleep duration (12s), walk-cycles-before-sleep (2).

### `micro_action.rs` adjustments

- Add accessor: `pub fn action(&self) -> MicroAction { self.action }` on `ActionOverride`. Needed by `PetRuntime::refresh_animation()` to feed the resolver.
- Remove `pub fn animation_group(&self) -> AnimationGroup` — no longer referenced after the resolver moves into `pet/resolver.rs`.
- Keep `disables_movement()`, `tick()`, `remaining()`, `new()`.

`MicroAction` enum itself stays in `micro_action.rs` and is unchanged. The resolver in `pet/resolver.rs` matches on it directly.

`tick(dt)` flow:

1. Tick `action_override`, expire if done.
2. If hidden: refresh behavior + return early.
3. Accumulate `state_elapsed`, `expression_elapsed` (unchanged, gated by `!dragging`).
4. `cursor.tick(dt, self.time_multiplier())` — replaces `advance_animation()` + `frame_duration()` lookups.
5. `advance_state(dt)` — unchanged (idle→walk→sleep cycle, micro-action duration).
6. Expression index bump on `expression_elapsed >= expression_interval()` — unchanged.
7. `refresh_behavior_mode()` — now also calls `refresh_animation()` at the end, resetting cursor if name changed.

State machine constants (`WALK_DISTANCE`, `WALK_SPEED`, expression intervals, sleep duration) stay as-is. These describe **behavior**, not **animation**, and belong in runtime.

## Testing

### New tests

`pet/manifest.rs`:

- `parses_bundled_manifest()` — load `include_str!`, validate, no panic.
- `rejects_manifest_missing_idle()`.
- `rejects_frame_index_out_of_bounds()`.
- `rejects_loop_start_past_end()`.
- `rejects_fallback_to_unknown_animation()`.
- `rejects_empty_animation()`.
- `rejects_zero_frame_geometry()`.
- `rejects_manifest_version_zero()`.
- `accepts_unknown_manifest_version_with_warning()`.

`pet/resolver.rs`:

- `chain_for_hovered_uses_personality_variant()` — three sub-cases.
- `chain_for_default_cycles_through_5_expressions()`.
- `chain_for_action_uses_micro_action_animation()`.
- `lookup_falls_back_when_specific_missing()` — fixture manifest without `hover-lively` resolves Lively to `idle`.
- `lookup_returns_idle_when_chain_completely_misses()`.

`pet/runtime.rs` (AnimationCursor):

- `cursor_advances_through_frames_using_per_frame_ms()` — frame 0 at 0-99ms, frame 1 at 100-199ms.
- `one_shot_animation_stays_on_final_frame()` — `loop_start = None`, elapsed > total → last frame.
- `looping_animation_wraps_using_loop_start()` — `loop_start = Some(1)`, wraps from frame N-1 back to frame 1.
- `animation_name_change_resets_cursor()`.
- `time_multiplier_speeds_up_hover()` — hover intensity 2.0 advances frames in half real time.

### Rewritten tests

Existing tests in `src/pet.rs` (about 30 total) split into two groups:

**Keep (assert state machine, not animation enum):**
- `starts_idle_on_frame_zero` (assert via `state()` + `current_sprite_index() == 0`).
- `dragging_overrides_hover_and_movement` (assert `behavior_mode == Dragging`, drop AnimationGroup check).
- `dragging_pauses_*`, `movement_speed_*`, `idle_transitions_to_walk_after_threshold`, `walk_*`, `sleep_*`, `turn_around_*`, `set_intent_*` — all keep, only rename `Pet` → `PetRuntime`.

**Rewrite to use animation name:**
- `cheerful_is_default_personality` — drop `behavior_mode == Default` assertion if needed.
- `personality_changes_hover_group` — assert `pet.current_animation_name() == "hover-calm"` / "hover-cheerful" / "hover-lively".
- `expression_loop_advances_without_requiring_walk` — assert `current_animation_name()` changes across ticks.
- `nap_micro_action_uses_sleepy_group_and_stops_movement` — assert `current_animation_name() == "sleepy"`.
- `cheer_up_micro_action_uses_happy_group_temporarily` — assert `current_animation_name() == "happy"`, then `"walk-right"`.
- `hover_overrides_micro_action_until_hover_ends` — assert name transitions.
- `idle_animation_advances_every_200ms` — assert `current_sprite_index()` changes from 0 to 1 after 200ms.
- `sleep_uses_slow_animation_rate` — assert sprite_index does not change at 499ms but does at 500ms.

Expected total test count: ~30 existing rewritten/kept + 15 new ≈ 45 tests in `pet/` module.

### Other test files

`src/sprite.rs` tests: rewrite for new `frame_rect(sprite_index, &geometry)` signature.

## Migration Steps

1. **Add `pet/manifest.rs` + `assets/manifests/happy_cappy.json` + manifest tests.** Not wired into runtime yet. `cargo build` passes, `cargo test` runs new manifest tests in isolation.

2. **Add `pet/resolver.rs` + resolver tests.** Pure functions, no runtime wiring. Tests pass.

3. **Add `pet/runtime.rs` with `AnimationCursor` only (no PetRuntime yet) + cursor tests.** Existing `src/pet.rs` (with `Pet`) coexists. Tests pass.

4. **Rename `Pet` → `PetRuntime`. Move it into `pet/runtime.rs`. Wire up `manifest`, `cursor`, `refresh_animation()`. Delete `AnimationGroup` enum and `current_animation_group()`. Add `current_animation_name()` and `current_sprite_index()`.** Rewrite/migrate the affected tests in this step. `src/pet.rs` deleted, `src/pet/mod.rs` re-exports. The biggest single commit; can be split further if review reveals risks.

5. **Update `src/sprite.rs`:** delete `SpriteRow` and `impl From<AnimationGroup>`, change `frame_rect` to `(sprite_index, &FrameGeometry)`, update `SpriteSheet::load`/`from_image` signature, rewrite sprite tests.

6. **Update `src/app.rs`:** import `PetRuntime`, construct with manifest, read sprite_index from runtime, drop `FRAME_SIZE` constant.

7. **`cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test`.**

8. **`./scripts/verify.sh`.** Confirms fmt + tests + clippy + release build + bundle assembly + codesign (if available).

9. **Manual smoke test:** open the built app, verify capybara appears, idle/walk/sleep cycle, hover reaction across all three personalities, drag-to-move + persist position, Nap and Cheer Up from menu bar, Focus Mode toggle, workspace awareness (caret avoidance, fullscreen auto-hide) — everything matches behavior before the refactor.

## Risks

| Risk | Mitigation |
|---|---|
| Frame timing drift across the refactor | Bundled manifest `ms` values match current constants exactly. Tests like `idle_animation_advances_every_200ms` re-pass with same boundaries. |
| Hover intensity semantics change at edges | Mathematically equivalent when frames in an animation share the same `ms`, which is true for all hover variants today. |
| Confusion between Nap micro-action and natural `PetState::Sleep` (both resolve to `"sleepy"`) | They take different code paths (`BehaviorMode::Action` vs `BehaviorMode::Default` with expression_index=4). State logic untouched, only the animation name happens to match. |
| Bundled JSON malformed | `parses_bundled_manifest()` test runs in CI before merge. End users never hit a malformed bundle because JSON is committed alongside the binary. |
| Future sub-project 2 disk loading requires API symmetry | `from_json_str()` is the parser core; `load_embedded_happy_cappy` is a wrapper. Sub-project 2 adds `from_path` without disturbing existing API. |

## Exit Criteria

- `cargo build --release` succeeds.
- `cargo test` passes, ≥45 tests in `pet/` module across `manifest.rs`, `resolver.rs`, `runtime.rs`.
- `cargo clippy --all-targets --all-features -- -D warnings` clean.
- `./scripts/verify.sh` passes.
- No `#[allow(dead_code)]` annotations added.
- Manual smoke test confirms behavior parity with `main` before the refactor.
- `AnimationGroup` enum has zero references in the codebase.

## Open Questions Deferred to Later Sub-projects

- Sub-project 2 — manifest discovery rules: directory scan order, custom-pet manifest version compatibility, asset bundling for custom pets.
- Sub-project 3 — picker UI placement: standalone tab in Settings, or row inline?
- Sub-project 4 — notification animation namespace: prefix with `"notify-"` to avoid collision with `"happy"`, `"sleepy"` etc.

These are captured here only so they don't get lost; decisions belong to the respective sub-project specs.
