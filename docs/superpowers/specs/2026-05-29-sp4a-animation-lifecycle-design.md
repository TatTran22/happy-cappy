# SP4-A — Animation Lifecycle Engine (manifest v2) — Design

> Part of the multi-pet platform roadmap (umbrella: `2026-05-26-pet-manifest-refactor-design.md`).
> Sub-project 4 is split into three sequential specs:
> - **SP4-A (this spec):** animation lifecycle engine — manifest v2, per-frame timing, loop point, one-shot + fallback. Pure engine; no notifications.
> - **SP4-B:** notification model + Unix socket + CLI subcommand (animation-only reaction).
> - **SP4-C:** speech-bubble UI for `label`/`body` (deferred; separate spec).

## 1. Context

SP1 replaced the `AnimationGroup`/`SpriteRow` enums with a data-driven manifest where each animation is a flat list of sprite indices (`{ "frames": [u32] }`). Frame timing is computed entirely at runtime from `state`/`behavior_mode`/`personality`/`hover_intensity`. SP1 explicitly deferred per-frame `ms`, `loop_start`, `fallback`, and one-shot animations to sub-project 4.

SP4-A delivers that animation engine. It is the foundation SP4-B builds on (notifications use one-shot/looping animations with a fallback), but it has standalone value: richer, manifest-authored animation timing for any pet.

## 2. Goals

- Extend the manifest schema to v2 with **optional** per-frame `ms`, `loopStart`, `fallback`, and `oneShot`.
- Keep **100% backward compatibility** with v1 manifests: the bundled `assets/manifests/happy_cappy.json` and any existing custom pet manifest parse and animate byte-for-byte identically.
- Add runtime support for: per-frame timing override, loop-from-point, and one-shot animations that transition to a `fallback` animation on completion.
- Expose a "one-shot completed" signal from the runtime so SP4-B can end a notification when its one-shot animation finishes.

## 3. Non-goals

- Notifications, sockets, CLI, external triggers — all SP4-B.
- Text rendering / speech bubbles — SP4-C.
- New sprite art. The spritesheet is unchanged (256×640, 4×10 grid, 40 frames).
- Changing v1 manifest behavior in any observable way.

## 4. Manifest schema v2 (`src/pet/manifest.rs`)

A single `frame` deserializes from **either** a bare integer (v1) or an object (v2), via serde untagged. This means existing v1 manifests need no change.

```jsonc
// v1 animation — unchanged, still valid:
"idle": { "frames": [0, 1, 2, 3] },

// v2 one-shot — per-frame ms, plays once, then enters `fallback`:
"notify-success": {
  "frames": [
    { "index": 8,  "ms": 80 },
    { "index": 9,  "ms": 80 },
    { "index": 10, "ms": 120 },
    { "index": 11, "ms": 300 }
  ],
  "oneShot": true,        // play once, then switch to `fallback`
  "fallback": "idle"      // animation to enter when a one-shot completes (default: "idle")
},

// v2 looping with intro — frames 0..2 play once, then loop 2..len forever:
"notify-running": {
  "frames": [{ "index": 12, "ms": 120 }, { "index": 13, "ms": 120 },
             { "index": 14, "ms": 120 }, { "index": 15, "ms": 120 }],
  "loopStart": 2          // mutually exclusive with `oneShot` (see validation)
}
```

Rust types:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]   // REQUIRED: parent PetManifest's rename_all does NOT
pub struct Animation {                // propagate to nested structs, so loopStart/oneShot
    pub frames: Vec<Frame>,           // must be mapped here explicitly.
    #[serde(default)]
    pub loop_start: Option<usize>,
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub one_shot: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub index: u32,
    pub ms: Option<u32>,   // None -> use runtime-computed frame duration
}
```

- `Frame` custom/untagged `Deserialize`: a JSON number → `Frame { index, ms: None }`; a JSON object `{ "index", "ms"? }` → `Frame { index, ms }`.
- `loop_start`, `fallback`, `one_shot` are all optional with defaults that preserve current behavior (`None`, `None`, `false`).
- `one_shot` and `loop_start` are **mutually exclusive** (a one-shot does not loop) — setting both is a validation error (see below).
- **Version policy (decided — lenient):** v2 fields (object frames, `loopStart`, `oneShot`, `fallback`) are accepted on **any** `manifest_version`. They are optional and additive, so no version bump is required and existing v1 manifests need no change. `manifest_version` is reserved for future *breaking* changes; the bundled `happy_cappy.json` stays at version 1 even after `notify-*` v2 animations are added in SP4-B. (Chosen over requiring `manifest_version >= 2`.)
- The existing `Animation { frames: Vec<u32> }` callers (`current_sprite_index`, `advance_animation`, resolver fixtures, picker preview) switch from `frames: Vec<u32>` to `frames: Vec<Frame>`; sprite index is read via `frame.index`.

### Validation (extends existing `validate()`)

Keep all current checks (non-empty, `MAX_FRAMES_PER_ANIMATION`, sprite index bounds via `frame.index`, `idle` present). Add:

- `loop_start`, when present, must be `< frames.len()`.
- `fallback`, when present, must name an animation that exists in the manifest **or** be `"idle"` (which is guaranteed present). An unresolved `fallback` is a validation error.
- `ms`, when present, must be `> 0`.
- `one_shot == true` together with a present `loop_start` is a validation error (mutually exclusive).

## 5. Runtime semantics (`src/pet/runtime.rs`)

### 5.1 Per-frame timing

In `advance_animation`, the duration for the current frame is:

- `frame.ms` (as `Duration::from_millis`) if present, **else**
- the existing `self.frame_duration()` (state/mode/personality/hover-derived).

Because v1 manifests carry no `ms`, every existing animation keeps runtime-driven timing → **exact parity** with SP1. `advance_animation` must consult the *current frame's* `ms` rather than a single per-animation duration, so the `while self.frame_elapsed >= frame_duration` loop reads the duration of the frame it is leaving.

### 5.2 loop_start

When the animation index passes the last frame, it wraps to `loop_start` instead of `0`: frames `0..loop_start` are a one-time intro and `loop_start..len` is the steady-state loop. `loop_start` defaults to `0`, which preserves current full-cycle looping. The intro only plays when the cursor starts at 0 — guaranteed by the entry-reset policy in §5.4.

### 5.3 one_shot + fallback

A `one_shot` animation **completes when its final frame has been displayed for its full duration** — not the instant the index reaches the last frame. On completion the runtime:

- surfaces a **completion signal** on the value `tick()` returns (a `oneshot_completed: bool` on `PetTick`), and
- exposes the active animation's `fallback` via an accessor so the owner can transition (to `fallback`, or `"idle"` when `None`).

`advance_animation` **must not itself overwrite** `current_animation_name`: `refresh_behavior_mode` recomputes the animation every tick (§5.4) and would clobber it. Instead the animation's *owner* reacts to the signal. Ordering within `tick` is fixed and explicit:

```
advance_animation()      // detect + set oneshot_completed flag (does NOT change the name)
→ owner consumes flag     // e.g. SP4-B notification clears itself before refresh
→ refresh_behavior_mode() // re-selects the next animation
```

For a **pinned** animation (selected by an override owner, not the behavior chain — introduced by SP4-B's notification), the **owner decides** what completion means: it MAY transition to the animation's `fallback`, or take another action entirely. (SP4-B's notification chooses to *clear itself* on completion and does **not** consult `fallback` — see SP4-B §3.1. So `fallback` is the SP4-A engine primitive — exposed and tested here — but has no consumer that applies it within SP4; it remains available for future pinned uses such as an intro-then-loop hand-off.) For **chain-driven** animations (Default/hover/walk), one-shot/fallback has no effect — the chain re-selects each tick, so only non-one-shot animations are ever chosen that way. SP4-A ships the primitive + signal and unit-tests it in isolation.

### 5.4 Behavior-mode selection & entry-reset policy

`resolve_animation_chain` is unchanged. `refresh_behavior_mode` gains exactly **one** change: an *entry-reset* rule.

Today the runtime preserves `frame_index` when the selected animation **name** changes (protected by the `animation_name_change_does_not_reset_frame_index` test) — this keeps hover/walk transitions smooth and is parity behavior. But a `loop_start` intro or a `one_shot` must start at frame 0 to play correctly. So:

- When the selected animation name changes **to a lifecycle animation** (`loop_start.is_some()` or `one_shot == true`), reset the cursor (`frame_index = 0`, `frame_elapsed = 0`).
- When it changes to any other animation, **preserve** the cursor (unchanged parity behavior).

The bundled v1 manifest has no lifecycle animations, so this rule is inert for it (full parity). It first takes effect when SP4-B adds `notify-*` lifecycle animations. (SP4-B additionally adds a `Notifying` selector branch — see SP4-B §3.3 — that pins the notification's resolved animation instead of consulting `resolve_animation_chain`.)

## 6. Error handling

- Manifest parse/validation errors use the existing `ManifestError` enum, extended with variants for: `LoopStartOutOfBounds`, `UnresolvedFallback { animation, fallback }`, `ZeroFrameDuration { animation, frame_pos }`, `OneShotWithLoopStart { animation }`.
- The bundled manifest is validated at load (`load_embedded_happy_cappy` already `expect`s a valid manifest); the new checks apply there too.
- Custom-pet manifests that fail v2 validation flow through the existing SP2 `CatalogLoadError` path (skipped + surfaced), no new error surface.

## 7. Testing (pure Rust, no AppKit)

- **Parse:** bare-int frames; object frames; mixed bare+object in one animation; `loopStart`/`fallback`/`oneShot` present and absent; defaults applied.
- **Validation failures:** `loop_start >= len`; unresolved `fallback`; `ms == 0`; `oneShot` + `loopStart` together; plus all existing checks still pass.
- **Runtime timing:** an animation with per-frame `ms` advances at exactly those durations; an animation without `ms` uses runtime-computed durations.
- **loop_start:** after the last frame, index returns to `loop_start`, not 0; the `0..loop_start` intro is not replayed on subsequent loops.
- **Entry-reset:** selecting a lifecycle animation (has `loop_start`/`one_shot`) resets the cursor to frame 0; selecting a non-lifecycle animation preserves the cursor (the existing `animation_name_change_does_not_reset_frame_index` behavior still holds).
- **one_shot completion:** the completion signal fires **only after the final frame has been shown for its full duration** (a one-frame-early or one-tick-late assertion), fires exactly once, and the animation's `fallback` is exposed for the owner to consume. `advance_animation` does not mutate `current_animation_name`.
- **Version policy:** a `manifest_version: 1` manifest carrying `loopStart`/`oneShot`/object frames parses and validates (lenient policy).
- **Parity test:** load the bundled v1 manifest and assert the full frame-index sequence and per-frame timing match the pre-SP4 behavior for `idle`/`walk-right`/`hover-*`/`sleepy` over a representative tick window.

## 8. Exit criteria

- All new unit tests pass under `cargo test`.
- `cargo clippy --all-targets --all-features -- -D warnings` clean.
- `cargo fmt --check` clean.
- `./scripts/verify.sh` passes.
- Manual smoke: bundled pet looks and animates identically to `main` (no observable change) — SP4-A ships no user-visible behavior on its own.

## 9. Dependencies on prior sub-projects

| SP4-A needs | From |
|---|---|
| `PetManifest`, `Animation`, `FrameGeometry`, `from_json_str`, validation harness | SP1 |
| `PetRuntime::advance_animation`, `current_sprite_index`, `frame_duration`, `current_animation_name` | SP1 |
| `resolve_animation_chain`, `lookup_with_fallback` | SP1 (unchanged here) |
| `CatalogLoadError` surfacing for bad custom manifests | SP2 (unchanged) |

No external crate additions. Schema change is backward-compatible; no migration of existing manifests required.
