# Pet Manifest Refactor — Sub-project 1 Design

**Date:** 2026-05-26
**Status:** Draft for review (revision 2)
**Owner:** Tat Tran

## Context

Happy Cappy currently hard-codes a single capybara pet across three concerns: state machine (`src/pet.rs`), spritesheet row layout (`src/sprite.rs::SpriteRow`), and frame timing (state-based constants in `src/pet.rs`). The `AnimationGroup` enum locks the set of animations to ten Rust variants. Adding a new animation requires modifying the enum, the spritesheet row mapping, and the behavior resolver in `Pet::refresh_behavior_mode()`.

We want to evolve Happy Cappy toward a multi-pet / customisable platform inspired by Codex CLI's `/pets` feature (see `~/Downloads/codex-pets-reference.md`). The long-term roadmap has four sub-projects:

1. **Sub-project 1 (this spec):** Replace the `AnimationGroup`-enum + `SpriteRow`-enum coupling with a data-driven sprite-index table per animation name. Happy Cappy capybara loaded from an embedded JSON manifest. No behavior change visible to the user.
2. Sub-project 2: Catalog + custom pet loading from `~/Library/Application Support/Happy Cappy/pets/<id>/pet.json`.
3. Sub-project 3: Picker UI in the Settings panel with preview pane.
4. Sub-project 4: Notification system with external triggers. Likely introduces one-shot animations, per-frame `ms`, `loop_start`, and `fallback` — explicitly deferred from this spec.

This spec covers only sub-project 1. Sub-projects 2–4 get their own spec/plan cycles.

## Goals

- Split `Pet` into a data-only `PetManifest` (parsed from JSON) and a runtime state machine `PetRuntime`.
- Replace `AnimationGroup` enum with string-keyed animation map; each animation is just `{ frames: [u32] }` — an ordered list of sprite indices.
- Replace `SpriteRow` enum + row-index logic in `sprite.rs` with index-based sprite slicing using the manifest's `FrameGeometry`.
- Bundle the existing capybara as `assets/manifests/happy_cappy.json`, loaded via `include_str!`.
- Keep all existing behavior (animation cycles, frame timing, hover/drag/walk/sleep, workspace awareness, focus mode, micro-actions, drag persistence) byte-for-byte identical.

## Non-goals

- Per-frame `ms`, `loop_start`, `fallback`, one-shot animations. Deferred to sub-project 4 along with notifications.
- State-aware timing in the manifest. Frame duration stays a pure runtime concern of `PetRuntime`, computed from `behavior_mode`, `state`, `personality`, and `hover_intensity` exactly as today.
- Multi-pet catalog, custom pet loading from disk, picker UI, notification system.
- Changes to `workspace.rs`, `interaction.rs`, `settings.rs`, `settings_window_macos.rs`, `menu_bar.rs`, `renderer.rs`, `window_macos.rs`, `bundle.rs`, `command_target_macos.rs`. They keep their current API.
- Changing the sprite asset (`assets/happy_cappy_spritesheet.png` stays as-is, 256×640, 4×10 grid).
- Changes to `settings.json` schema. No `selected_pet_id` field added.

## Design Principle

**Manifest = animation structure. Runtime = animation timing and state.**

- Manifest only says *which sprite frames make up animation X*. It does not declare how fast they play, whether they loop, or what plays after them.
- Runtime owns frame duration (state-based: 200/100/500 ms + hover intensity formula), frame advancement, cursor reset rules, and the resolver chain from behavior mode → animation name.

This boundary is the smallest viable refactor that gets us a data-driven sprite table without changing observable behavior. Per-frame timing and lifecycle features land later, when notifications actually need them.

## Architecture

### Module layout

```
src/
├── pet/
│   ├── mod.rs          — public API, re-exports
│   ├── manifest.rs     — PetManifest, Animation, FrameGeometry,
│   │                     serde Deserialize, validation,
│   │                     load_embedded_happy_cappy()
│   ├── runtime.rs      — PetRuntime (state machine, renamed from Pet),
│   │                     BehaviorMode, BehaviorIntent, PetState,
│   │                     Direction, Personality, PetTick,
│   │                     frame_duration(), frame_position/frame_elapsed
│   └── resolver.rs     — resolve_animation_chain(),
│                         lookup_with_fallback() — runtime-side fallback
│                         from a chain of candidate names to a manifest entry
├── sprite.rs           — trimmed; SpriteRow enum removed
├── ...                 — other files unchanged except for import renames
└── assets/
    └── manifests/
        └── happy_cappy.json
```

`src/pet.rs` (current 867 LoC) is replaced by the `src/pet/` directory.

### Public API of the `pet` module

```rust
// pet/mod.rs
pub use manifest::{PetManifest, Animation, FrameGeometry, ManifestError};
pub use runtime::{
    PetRuntime, PetState, Direction, Personality,
    BehaviorMode, BehaviorIntent, PetTick,
};
pub use resolver::resolve_animation_chain;
```

The current `crate::pet::Pet` callers (`app.rs` is the main one) become `crate::pet::PetRuntime`. `AnimationGroup` is removed from the public API.

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
    "idle":           { "frames": [0,  1,  2,  3]  },
    "blink":          { "frames": [4,  5,  6,  7]  },
    "happy":          { "frames": [8,  9, 10, 11] },
    "curious":        { "frames": [12, 13, 14, 15] },
    "sleepy":         { "frames": [16, 17, 18, 19] },
    "hover-calm":     { "frames": [20, 21, 22, 23] },
    "hover-cheerful": { "frames": [24, 25, 26, 27] },
    "hover-lively":   { "frames": [28, 29, 30, 31] },
    "walk-right":     { "frames": [32, 33, 34, 35] },
    "drag":           { "frames": [36, 37, 38, 39] }
  }
}
```

Sprite indices are flat: `index = row * columns + column`. So index 32 = row 8 col 0 = the WalkRight row's first frame, matching today's mapping in `sprite.rs::frame_rect`.

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
    pub frames: Vec<u32>,   // sprite indices into row*columns+column grid
}

fn default_manifest_version() -> u32 { 1 }
```

`BTreeMap` (not `HashMap`) for deterministic iteration in tests and Debug output. `Animation` carries no `ms`, `loop_start`, or `fallback` — those are sub-project 4 territory.

### Validation rules

`PetManifest::validate()` returns `Result<(), ManifestError>` enforcing:

- `manifest_version >= 1` (reject 0; warn but accept values > 1 for forward compat).
- `id` non-empty, contains no `/`, `\`, or null bytes.
- `display_name` non-empty.
- `frame.width`, `frame.height`, `frame.columns`, `frame.rows` all > 0.
- For every animation: `frames.len() >= 1` and `frames.len() <= MAX_FRAMES_PER_ANIMATION (= 64)`.
- For every sprite index in every animation: `index < frame.columns * frame.rows`.
- `animations.contains_key("idle")` — guarantees the terminal fallback in the runtime resolver chain always resolves.
- The bundled Happy Cappy manifest must additionally contain all of: `"idle"`, `"blink"`, `"happy"`, `"curious"`, `"sleepy"`, `"hover-calm"`, `"hover-cheerful"`, `"hover-lively"`, `"walk-right"`, `"drag"`. Enforced via a separate `validate_happy_cappy_required_keys()` check called from `load_embedded_happy_cappy()`, not from generic `validate()` (so future custom pets only need `"idle"`).

`ManifestError` is a structured enum implementing `std::error::Error`:

```rust
pub enum ManifestError {
    Json(serde_json::Error),
    InvalidVersion(u32),
    EmptyField(&'static str),
    InvalidIdChars,
    ZeroGeometry,
    EmptyAnimation { name: String },
    TooManyFrames { name: String, count: usize },
    SpriteIndexOutOfBounds { animation: String, frame_pos: usize, index: u32, max: u32 },
    MissingRequiredAnimation { name: &'static str },
}
```

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
        let manifest = Self::from_json_str(JSON)
            .expect("bundled happy_cappy.json must parse and validate");
        manifest.validate_happy_cappy_required_keys()
            .expect("bundled happy_cappy.json must declare all required animations");
        manifest
    }
}
```

`from_json_str` is the public core; `load_embedded_happy_cappy` is a thin wrapper for sub-project 1. Sub-project 2 will add `from_path` reusing `from_json_str`.

## Animation Resolver (runtime-side, in Rust)

The resolver lives in `src/pet/resolver.rs`. It is pure Rust logic, not data-driven by the manifest. Custom pets in future sub-projects will be expected to declare these standard animation names.

### Behavior-to-name mapping

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

The "second-tier" fallback names (`"hover"`, `"walk"`) are present so future custom pets that omit the personality-specific variants still degrade gracefully.

### Lookup with fallback (runtime, not manifest)

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

The "fallback" here is runtime fallback over the candidate chain — distinct from per-animation `fallback` field (deferred to sub-project 4).

## State Machine Internals

`PetRuntime` retains every current field except:

- Remove `animation_group: AnimationGroup`.
- Add `manifest: PetManifest` (owned).
- Add `current_animation_name: String` (replaces `animation_group` semantically).

`frame_index: usize` and `frame_elapsed: Duration` are **kept exactly as today**. Frame advancement logic in `advance_animation()` is **unchanged** in behavior; only `FRAME_COUNT` is replaced by `manifest.animations[name].frames.len()`.

`frame_duration()` is **unchanged**:

```rust
fn frame_duration(&self) -> Duration {
    if self.behavior_mode == BehaviorMode::Hovered {
        return self.hover_frame_duration();  // unchanged, still rounds
    }
    match self.state {
        PetState::Idle => Duration::from_millis(IDLE_STATE_MS),
        PetState::Walk => Duration::from_millis(WALK_STATE_MS),
        PetState::Sleep => Duration::from_millis(SLEEP_STATE_MS),
    }
}
```

Constants stay in `pet/runtime.rs`:

```rust
const IDLE_STATE_MS:  u64 = 200;   // was IDLE_FRAME_MS
const WALK_STATE_MS:  u64 = 100;   // was WALK_FRAME_MS
const SLEEP_STATE_MS: u64 = 500;   // was SLEEP_FRAME_MS
```

`hover_frame_duration()` keeps its current implementation including `(base_ms / divisor).round()` to preserve fractional-intensity behavior:

```rust
fn hover_frame_duration(&self) -> Duration {
    let base_ms = match self.personality {
        Personality::Calm => 220.0,
        Personality::Cheerful => 140.0,
        Personality::Lively => 90.0,
    };
    let divisor = self.hover_intensity.max(0.5);
    Duration::from_millis((base_ms / divisor).round() as u64)
}
```

### Frame advancement

```rust
fn advance_animation(&mut self) {
    let anim = self.manifest.animations.get(&self.current_animation_name)
        .or_else(|| self.manifest.animations.get("idle"))
        .expect("validation guarantees idle exists");
    let frame_count = anim.frames.len();
    let frame_duration = self.frame_duration();
    while self.frame_elapsed >= frame_duration {
        self.frame_elapsed -= frame_duration;
        self.frame_index = (self.frame_index + 1) % frame_count;
    }
}
```

### Animation transition (replaces `refresh_behavior_mode`'s group-mapping arm)

```rust
fn refresh_behavior_mode(&mut self) {
    // Compute behavior_mode exactly as today (unchanged).
    self.behavior_mode = if self.hidden { ... } else if self.dragging { ... } ... ;

    // Resolve animation name from chain — replaces the AnimationGroup match.
    let chain = resolve_animation_chain(
        self.behavior_mode,
        self.personality,
        self.expression_index,
        self.action_override.map(|a| a.action()),
    );
    let (name, _) = lookup_with_fallback(&self.manifest, chain);
    self.current_animation_name = name.to_string();
    // Note: NO frame_index reset here. Matches current behavior.
}
```

### Cursor reset

Cursor reset (frame_index = 0, frame_elapsed = 0) happens only inside the three existing state transitions: `enter_idle()`, `enter_walk()`, `enter_sleep()`, plus the `force_state_for_test()` test helper. Identical to current code.

### `current_sprite_index()`

```rust
pub fn current_sprite_index(&self) -> u32 {
    let anim = self.manifest.animations.get(&self.current_animation_name)
        .or_else(|| self.manifest.animations.get("idle"))
        .expect("validation guarantees idle exists");
    anim.frames[self.frame_index % anim.frames.len()]
}
```

### `current_animation_name()`

```rust
pub fn current_animation_name(&self) -> &str {
    &self.current_animation_name
}
```

### `micro_action.rs` adjustments

- Add accessor: `pub fn action(&self) -> MicroAction { self.action }` on `ActionOverride`. Needed by `PetRuntime::refresh_behavior_mode()` to feed the resolver.
- Remove `pub fn animation_group(&self) -> AnimationGroup` — no longer referenced.
- Keep `disables_movement()`, `tick()`, `remaining()`, `new()`.

`MicroAction` enum is unchanged.

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
- `FRAME_SIZE: u32 = 64` constant removed; window sizing reads from `runtime.manifest().frame.width`.
- Per-frame draw:
  ```rust
  let geometry = runtime.manifest().frame;
  let sprite_index = runtime.current_sprite_index();
  let rect = sprite_sheet.frame_rect(sprite_index, &geometry);
  let flip_x = runtime.current_animation_name() == "walk-right"
      && runtime.direction() == Direction::Left;
  renderer.draw(sprite_sheet.image(), rect, flip_x)?;
  ```
- Hit-test code in `interaction.rs` already uses the same `flip_x` logic chain — the `walk-right` name check replaces the `AnimationGroup::WalkRight` check, byte-for-byte equivalent.

## Testing

### New tests

`pet/manifest.rs`:

- `parses_bundled_manifest()` — load `include_str!`, validate, no panic.
- `bundled_manifest_declares_all_required_happy_cappy_keys()`.
- `rejects_manifest_missing_idle()`.
- `rejects_frame_index_out_of_bounds()`.
- `rejects_empty_animation()`.
- `rejects_zero_frame_geometry()`.
- `rejects_manifest_version_zero()`.
- `accepts_unknown_future_manifest_version()`.
- `rejects_too_many_frames_in_animation()` — `MAX_FRAMES_PER_ANIMATION + 1`.
- `from_json_str_round_trips_minimal_manifest()` — manifest with only "idle" passes generic validate, fails happy-cappy-required-keys validate.

`pet/resolver.rs`:

- `chain_for_hovered_uses_personality_variant()` — three sub-cases.
- `chain_for_default_cycles_through_5_expressions()`.
- `chain_for_action_uses_micro_action_animation()` — Nap and CheerUp sub-cases.
- `chain_for_walking_uses_walk_right_then_walk_then_idle()`.
- `lookup_falls_back_when_specific_missing()` — fixture manifest without `hover-lively` resolves Lively to `idle` (no `hover` present either).
- `lookup_uses_second_tier_when_specific_missing()` — fixture with `hover` but not `hover-lively` resolves Lively to `hover`.

### Rewritten existing tests

Existing tests in `src/pet.rs` (about 30 total) split:

**Keep verbatim (assert state machine, not animation enum):**
- `starts_idle_on_frame_zero` — also asserts `current_sprite_index() == 0`.
- `dragging_overrides_hover_and_movement` — drop AnimationGroup assertion, keep state/behavior_mode + tick.speed_x assertions.
- `dragging_pauses_*`, `movement_speed_*`, `idle_transitions_to_walk_*`, `walk_*`, `sleep_*`, `turn_around_*`, `set_intent_*` — keep, only rename `Pet` → `PetRuntime`.
- `cheerful_is_default_personality` — keep.

**Rewrite to assert animation name string:**
- `personality_changes_hover_group` — assert `current_animation_name() == "hover-calm"` / "hover-cheerful" / "hover-lively".
- `expression_loop_advances_without_requiring_walk` — assert `current_animation_name()` changes across ticks.
- `nap_micro_action_uses_sleepy_group_and_stops_movement` — assert `current_animation_name() == "sleepy"`.
- `cheer_up_micro_action_uses_happy_group_temporarily` — assert names transition `"happy"` → `"walk-right"`.
- `hover_overrides_micro_action_until_hover_ends` — assert name transitions across hover toggle.

**Frame timing tests — must still pass with identical numeric boundaries:**
- `idle_animation_advances_every_200ms` — assert `current_sprite_index()` changes between idle frames 0 and 1 after 200ms tick.
- `sleep_uses_slow_animation_rate` — sprite_index does not change at 499ms but does at 500ms during Sleep state.

### New regression test for hover intensity fractional behavior

`hover_intensity_fractional_value_preserves_rounding_boundary()`:

- Set personality Cheerful (base 140ms), hover_intensity = 1.3.
- Expected frame_duration = round(140 / 1.3) = round(107.69) = 108ms.
- Assert: after exactly 107ms total elapsed, sprite_index unchanged. After 108ms, advanced to next frame.

This pins the rounding semantics so any future cursor refactor can't silently shift boundaries.

### Cursor-reset boundary test

`animation_name_change_does_not_reset_frame_index()`:

- Force PetRuntime into Walk state.
- Tick 250ms → frame_index = 2 (at WALK_STATE_MS = 100ms each, 250ms → 2 transitions).
- Set hovered = true → behavior_mode = Hovered → animation name changes to "hover-cheerful".
- Without ticking, assert frame_index still == 2 (no reset).
- Assert current_sprite_index() now points into the "hover-cheerful" frames at index 2 (sprite 26).

This pins the spec's most subtle parity requirement.

### Sprite-row mapping tests in `sprite.rs`

- `frame_rect_for_sprite_index_0_returns_top_left()`.
- `frame_rect_for_sprite_index_32_returns_walk_row_first_column()` — explicit parity with old `SpriteRow::WalkRight, frame_index=0`.
- `frame_rect_for_sprite_index_39_returns_drag_row_last_column()`.
- `from_image_rejects_dimensions_not_matching_geometry()`.
- `from_image_accepts_geometry_matching_actual_image()`.

Expected total test count: ~30 existing rewritten/kept + ~17 new ≈ 47 tests across `pet/` and `sprite.rs`.

## Migration Steps

1. **Add `pet/manifest.rs` + `assets/manifests/happy_cappy.json` + manifest tests.** Not wired into runtime yet. `cargo build` passes, manifest tests pass.

2. **Add `pet/resolver.rs` + resolver tests.** Pure functions, no runtime wiring. Tests pass.

3. **Rename `Pet` → `PetRuntime`. Move into `pet/runtime.rs`. Add `manifest: PetManifest` and `current_animation_name: String` fields. Replace the `animation_group` match in `refresh_behavior_mode()` with a call to `resolve_animation_chain` + `lookup_with_fallback`. Delete `AnimationGroup` enum and `current_animation_group()`. Add `current_animation_name()` and `current_sprite_index()`.** Rewrite/migrate affected tests in this step.

4. **Update `src/sprite.rs`:** delete `SpriteRow` and `impl From<AnimationGroup>`, change `frame_rect` to `(sprite_index, &FrameGeometry)`, update `SpriteSheet::load`/`from_image` signature, rewrite sprite tests.

5. **Update `src/app.rs`:** import `PetRuntime`, construct with manifest, read sprite_index from runtime, update flip_x check to use animation name, drop `FRAME_SIZE` constant.

6. **Update `src/micro_action.rs`:** add `ActionOverride::action()`, remove `animation_group()`.

7. **`cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test`.**

8. **`./scripts/verify.sh`.** Confirms fmt + tests + clippy + release build + bundle assembly + codesign (if available).

9. **Manual smoke test:** open the built app, verify capybara appears, idle/walk/sleep cycle, hover reaction across all three personalities at multiple fractional intensities, drag-to-move + persist position, Nap and Cheer Up from menu bar, Focus Mode toggle, workspace awareness (caret avoidance, fullscreen auto-hide) — everything matches behavior before the refactor.

Each step is a commit. Step 3 is the largest single commit and can be subdivided further during implementation planning if review reveals risk.

## Risks

| Risk | Mitigation |
|---|---|
| Frame timing accidentally shifted by the refactor | State-based `frame_duration()` formula copied verbatim. `idle_animation_advances_every_200ms`, `sleep_uses_slow_animation_rate`, and the new `hover_intensity_fractional_value_preserves_rounding_boundary()` pin numeric boundaries. |
| Cursor inadvertently resets on animation-name change | Explicit `animation_name_change_does_not_reset_frame_index()` test prevents regression. `refresh_behavior_mode()` deliberately does NOT touch `frame_index` / `frame_elapsed`. |
| Flip applied to non-walk animations after a leftward walk | `flip_x = name == "walk-right" && direction == Left` in `app.rs` and `interaction.rs` hit-test path. Old code's `matches!(group, AnimationGroup::WalkRight)` becomes a name-string check — byte-equivalent. |
| Bundled JSON malformed | `parses_bundled_manifest()` + `bundled_manifest_declares_all_required_happy_cappy_keys()` tests run in CI before merge. End users never hit malformed bundle. |
| Subtle behavior change between Nap (Action mode) and Sleep state — both resolve to "sleepy" | They go through different code paths (`BehaviorMode::Action` vs `BehaviorMode::Default + expression_index==4`). State machine logic (`completed_walk_cycles`, sleep duration 12s) is untouched. Animation name coincidence is intentional. |
| Future sub-project 2 disk loading requires API symmetry | `from_json_str()` is the parser core; `load_embedded_happy_cappy` is a wrapper. Sub-project 2 adds `from_path` reusing `from_json_str` without disturbing existing API. |

## Exit Criteria

- `cargo build --release` succeeds.
- `cargo test` passes with all ~47 expected tests in `pet/` + `sprite.rs`.
- `cargo clippy --all-targets --all-features -- -D warnings` clean.
- `./scripts/verify.sh` passes.
- No `#[allow(dead_code)]` annotations added.
- `AnimationGroup` and `SpriteRow` have zero references in the codebase.
- Manual smoke test confirms behavior parity with `main` before the refactor.

## Open Questions Deferred to Later Sub-projects

- Sub-project 2 — manifest discovery rules: directory scan order, custom-pet manifest version compatibility, asset bundling for custom pets.
- Sub-project 3 — picker UI placement: standalone tab in Settings, or row inline?
- Sub-project 4 — animation lifecycle: per-frame `ms`, `loop_start`, `fallback`, one-shot semantics, notification → animation name mapping, namespacing (e.g. `notify-running` to avoid colliding with `happy`/`sleepy`).

These are captured here only so they don't get lost; decisions belong to the respective sub-project specs.
