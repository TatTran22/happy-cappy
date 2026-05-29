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

// v2 animation — per-frame ms + one-shot + fallback:
"notify-success": {
  "frames": [
    { "index": 8,  "ms": 80 },
    { "index": 9,  "ms": 80 },
    { "index": 10, "ms": 120 },
    { "index": 11, "ms": 300 }
  ],
  "oneShot": true,        // play once, then switch to `fallback`
  "fallback": "idle",     // animation to enter when a one-shot completes (default: "idle")
  "loopStart": 0          // (looping animations) frame to loop back to after the last frame
}
```

Rust types:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Animation {
    pub frames: Vec<Frame>,
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
- The existing `Animation { frames: Vec<u32> }` callers (`current_sprite_index`, `advance_animation`, resolver fixtures, picker preview) switch from `frames: Vec<u32>` to `frames: Vec<Frame>`; sprite index is read via `frame.index`.

### Validation (extends existing `validate()`)

Keep all current checks (non-empty, `MAX_FRAMES_PER_ANIMATION`, sprite index bounds via `frame.index`, `idle` present). Add:

- `loop_start`, when present, must be `< frames.len()`.
- `fallback`, when present, must name an animation that exists in the manifest **or** be `"idle"` (which is guaranteed present). An unresolved `fallback` is a validation error.
- `ms`, when present, must be `> 0`.

## 5. Runtime semantics (`src/pet/runtime.rs`)

### 5.1 Per-frame timing

In `advance_animation`, the duration for the current frame is:

- `frame.ms` (as `Duration::from_millis`) if present, **else**
- the existing `self.frame_duration()` (state/mode/personality/hover-derived).

Because v1 manifests carry no `ms`, every existing animation keeps runtime-driven timing → **exact parity** with SP1. `advance_animation` must consult the *current frame's* `ms` rather than a single per-animation duration, so the `while self.frame_elapsed >= frame_duration` loop reads the duration of the frame it is leaving.

### 5.2 loop_start

When the animation index passes the last frame, it wraps to `loop_start` (default `0`) instead of `0`. Frames `0..loop_start` act as a one-time intro; `loop_start..len` is the steady-state loop. Default `0` preserves current full-cycle looping.

### 5.3 one_shot + fallback

When a `one_shot` animation reaches its final frame:

- the runtime sets `current_animation_name` to `fallback` (or `"idle"` if `fallback` is `None`), resets `frame_index`/`frame_elapsed`, and
- exposes a signal that the one-shot just completed (e.g. `tick()` returns it via `PetTick`, or a `took_oneshot_completed()` accessor). SP4-B uses this to clear a notification whose animation has finished.

A non-`one_shot` animation never auto-transitions; it loops per §5.2.

### 5.4 Interaction with behavior-mode selection

`refresh_behavior_mode` / `resolve_animation_chain` are unchanged in SP4-A. The engine only changes *how a chosen animation advances and terminates*, not *which* animation the behavior model selects. (SP4-B adds a new selector branch.)

## 6. Error handling

- Manifest parse/validation errors use the existing `ManifestError` enum, extended with variants for: `LoopStartOutOfBounds`, `UnresolvedFallback { animation, fallback }`, `ZeroFrameDuration { animation, frame_pos }`.
- The bundled manifest is validated at load (`load_embedded_happy_cappy` already `expect`s a valid manifest); the new checks apply there too.
- Custom-pet manifests that fail v2 validation flow through the existing SP2 `CatalogLoadError` path (skipped + surfaced), no new error surface.

## 7. Testing (pure Rust, no AppKit)

- **Parse:** bare-int frames; object frames; mixed bare+object in one animation; `loopStart`/`fallback`/`oneShot` present and absent; defaults applied.
- **Validation failures:** `loop_start >= len`; unresolved `fallback`; `ms == 0`; plus all existing checks still pass.
- **Runtime timing:** an animation with per-frame `ms` advances at exactly those durations; an animation without `ms` uses runtime-computed durations.
- **loop_start:** after the last frame, index returns to `loop_start`, not 0.
- **one_shot:** plays each frame once, then `current_animation_name` becomes `fallback` (or `idle`), and the completion signal fires exactly once.
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
