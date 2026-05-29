# SP4-A Animation Lifecycle Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the pet manifest + runtime to support per-frame durations, loop points, and one-shot animations with a completion signal — fully backward-compatible with v1 manifests.

**Architecture:** A manifest `Animation.frames` becomes `Vec<Frame>` where each `Frame` deserializes from either a bare integer (v1) or `{index, ms}` (v2). Accessor methods (`sprite_index`, `frame_count`, `frame_ms`, `is_lifecycle`) keep call sites stable. The runtime reads per-frame `ms` when present (else the existing state/personality-derived duration), wraps to `loop_start` instead of 0, resets the cursor when entering a "lifecycle" animation, and surfaces a `oneshot_completed` flag on `PetTick` without overwriting the behavior-selected animation.

**Tech Stack:** Rust 2021, `serde`/`serde_json`, existing `PetManifest`/`PetRuntime`. No new dependencies.

Spec: `docs/superpowers/specs/2026-05-29-sp4a-animation-lifecycle-design.md`

---

## File Structure

- Modify `src/pet/manifest.rs`: add `Frame` type + custom untagged `Deserialize`; change `Animation.frames` to `Vec<Frame>`; add `Animation` lifecycle fields (`loop_start`, `fallback`, `one_shot`) + `#[serde(rename_all="camelCase")]`; add accessors + `from_indices` constructor; extend validation + `ManifestError`.
- Modify `src/pet/resolver.rs`: migrate the test fixture constructor to `Animation::from_indices`.
- Modify `src/pet/runtime.rs`: per-frame timing, `loop_start` wrap, entry-reset on lifecycle animations, `oneshot_completed` on `PetTick`, `fallback` accessor; migrate `.frames[...]` reads; add test hooks.
- Modify `src/pet/catalog.rs`, `src/app.rs`, `src/picker_entries.rs`, `src/picker_window_macos.rs`: migrate `Animation { frames: ... }` construction / `.frames` iteration to the new shape.

---

### Task 1: `Frame` type with dual-form deserialization

**Files:**
- Modify: `src/pet/manifest.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `src/pet/manifest.rs`:

```rust
#[test]
fn frame_parses_from_bare_integer() {
    let f: Frame = serde_json::from_str("7").unwrap();
    assert_eq!(f.index, 7);
    assert_eq!(f.ms, None);
}

#[test]
fn frame_parses_from_object_with_ms() {
    let f: Frame = serde_json::from_str(r#"{ "index": 9, "ms": 120 }"#).unwrap();
    assert_eq!(f.index, 9);
    assert_eq!(f.ms, Some(120));
}

#[test]
fn frame_parses_from_object_without_ms() {
    let f: Frame = serde_json::from_str(r#"{ "index": 3 }"#).unwrap();
    assert_eq!(f.index, 3);
    assert_eq!(f.ms, None);
}

#[test]
fn frame_from_u32_has_no_ms() {
    let f = Frame::from(5u32);
    assert_eq!(f.index, 5);
    assert_eq!(f.ms, None);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib pet::manifest::tests::frame_ -- --nocapture`
Expected: FAIL — `cannot find type Frame in this scope`.

- [ ] **Step 3: Add the `Frame` type and its deserializer**

In `src/pet/manifest.rs`, add `Deserializer` to the serde import and insert the `Frame` type just above `pub struct Animation`:

```rust
use serde::{Deserialize, Deserializer};
```

```rust
/// One animation frame: a sprite index plus an optional per-frame duration.
/// Deserializes from either a bare integer (v1: `7`) or an object (v2: `{ "index": 7, "ms": 120 }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub index: u32,
    /// `None` -> the runtime uses its state/personality-derived duration (v1 parity).
    pub ms: Option<u32>,
}

impl From<u32> for Frame {
    fn from(index: u32) -> Self {
        Frame { index, ms: None }
    }
}

impl<'de> Deserialize<'de> for Frame {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Index(u32),
            Object {
                index: u32,
                #[serde(default)]
                ms: Option<u32>,
            },
        }
        Ok(match Raw::deserialize(deserializer)? {
            Raw::Index(index) => Frame { index, ms: None },
            Raw::Object { index, ms } => Frame { index, ms },
        })
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib pet::manifest::tests::frame_ -- --nocapture`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src/pet/manifest.rs
git commit -m "feat(manifest): add Frame type with bare-int/object deserialization"
```

---

### Task 2: Migrate `Animation` to `Vec<Frame>` + lifecycle fields + accessors (parity refactor)

This task changes the `Animation` shape and updates every caller so the existing suite stays green. No behavior change — green tests ARE the acceptance criterion.

**Files:**
- Modify: `src/pet/manifest.rs`, `src/pet/resolver.rs`, `src/pet/runtime.rs`, `src/pet/catalog.rs`, `src/app.rs`, `src/picker_entries.rs`, `src/picker_window_macos.rs`

- [ ] **Step 1: Redefine `Animation` with the new shape, accessors, and a test constructor**

In `src/pet/manifest.rs`, replace the existing `Animation` definition:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Animation {
    pub frames: Vec<u32>,
}
```

with:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Animation {
    pub frames: Vec<Frame>,
    #[serde(default)]
    pub loop_start: Option<usize>,
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub one_shot: bool,
}

impl Animation {
    /// Build a plain v1-style animation (no per-frame ms, no lifecycle fields).
    /// Used by tests and fixtures.
    pub fn from_indices(indices: &[u32]) -> Self {
        Animation {
            frames: indices.iter().copied().map(Frame::from).collect(),
            loop_start: None,
            fallback: None,
            one_shot: false,
        }
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Sprite index at the given position (callers pass an already-wrapped or raw index;
    /// this wraps defensively so it never panics on an out-of-range position).
    pub fn sprite_index(&self, pos: usize) -> u32 {
        let len = self.frames.len().max(1);
        self.frames[pos % len].index
    }

    /// Per-frame duration override at `pos`, if the manifest specified one.
    pub fn frame_ms(&self, pos: usize) -> Option<u32> {
        let len = self.frames.len().max(1);
        self.frames.get(pos % len).and_then(|f| f.ms)
    }

    /// A "lifecycle" animation drives its own cursor (intro/one-shot) and must
    /// start at frame 0 on entry (see runtime entry-reset).
    pub fn is_lifecycle(&self) -> bool {
        self.one_shot || self.loop_start.is_some()
    }
}
```

- [ ] **Step 2: Update manifest validation to read `frame.index`**

In `src/pet/manifest.rs` `validate()`, the loop currently does `for (pos, index) in anim.frames.iter().enumerate()` with `*index`. Replace its body to read `.index`:

```rust
            for (pos, frame) in anim.frames.iter().enumerate() {
                if frame.index >= max_index {
                    return Err(ManifestError::SpriteIndexOutOfBounds {
                        animation: name.clone(),
                        frame_pos: pos,
                        index: frame.index,
                        max: max_index,
                    });
                }
            }
```

- [ ] **Step 3: Update the manifest unit-test assertions**

In `src/pet/manifest.rs` tests, the assertions compare `frames` to `vec![0,1,2,3]`. Replace each such assertion to compare extracted indices. For example:

```rust
        assert_eq!(
            manifest.animations["idle"]
                .frames
                .iter()
                .map(|f| f.index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(
            manifest.animations["walk-right"]
                .frames
                .iter()
                .map(|f| f.index)
                .collect::<Vec<_>>(),
            vec![32, 33, 34, 35]
        );
        assert_eq!(
            manifest.animations["drag"]
                .frames
                .iter()
                .map(|f| f.index)
                .collect::<Vec<_>>(),
            vec![36, 37, 38, 39]
        );
```

Apply the same `.iter().map(|f| f.index).collect::<Vec<_>>()` transform to the two assertions near line 240 and the one near line 495.

- [ ] **Step 4: Migrate the resolver test fixture**

In `src/pet/resolver.rs`, change:

```rust
            animations.insert((*name).to_string(), Animation { frames: vec![0] });
```

to:

```rust
            animations.insert((*name).to_string(), Animation::from_indices(&[0]));
```

- [ ] **Step 5: Migrate runtime reads + fixtures**

In `src/pet/runtime.rs` `current_sprite_index` (line ~123-131), replace:

```rust
        anim.frames[self.frame_index % anim.frames.len()]
```

with:

```rust
        anim.sprite_index(self.frame_index)
```

In `advance_animation` (line ~253-262), replace `.frames.len()` with `.frame_count()`:

```rust
            .expect("manifest validation guarantees 'idle' exists")
            .frame_count()
            .max(1);
```

In the two runtime test fixtures (lines ~955 and ~995), change `Animation { frames: vec![0, 1, 2, 3, 4, 5] }` and `Animation { frames: vec![0, 1, 2, 3] }` to `Animation::from_indices(&[0, 1, 2, 3, 4, 5])` and `Animation::from_indices(&[0, 1, 2, 3])` respectively.

- [ ] **Step 6: Migrate the remaining construction sites**

In `src/pet/catalog.rs` (~line 241), `src/app.rs` (~line 1649), and `src/picker_entries.rs` (~line 278), replace each:

```rust
        Animation {
            frames: vec![0, 1, 2, 3],
        },
```

with:

```rust
        Animation::from_indices(&[0, 1, 2, 3]),
```

- [ ] **Step 7: Migrate the picker preview iteration**

In `src/picker_window_macos.rs` (~line 787-788), replace:

```rust
        let mut frames = Vec::with_capacity(animation.frames.len());
        for &index in &animation.frames {
            let rgba = crop_frame_rgba(&sheet, index as usize);
```

with:

```rust
        let mut frames = Vec::with_capacity(animation.frame_count());
        for frame in &animation.frames {
            let rgba = crop_frame_rgba(&sheet, frame.index as usize);
```

- [ ] **Step 8: Build and run the full suite to confirm parity**

Run: `cargo test`
Expected: PASS — the entire existing suite compiles and passes unchanged (no behavior change). If anything fails to compile, it is a missed `.frames` migration; fix per the same pattern.

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add src/pet/manifest.rs src/pet/resolver.rs src/pet/runtime.rs src/pet/catalog.rs src/app.rs src/picker_entries.rs src/picker_window_macos.rs
git commit -m "refactor(manifest): Animation.frames -> Vec<Frame> with accessors (parity)"
```

---

### Task 3: Validation for lifecycle fields

**Files:**
- Modify: `src/pet/manifest.rs`

- [ ] **Step 1: Write the failing tests**

Add to the manifest tests module:

```rust
fn manifest_json(animations: &str) -> String {
    format!(
        r#"{{ "id": "x", "displayName": "X", "spritesheetPath": "x.png",
              "frame": {{ "width": 16, "height": 16, "columns": 4, "rows": 1 }},
              "animations": {{ {animations} }} }}"#
    )
}

#[test]
fn rejects_loop_start_out_of_bounds() {
    let json = manifest_json(r#""idle": { "frames": [0, 1], "loopStart": 5 }"#);
    let err = PetManifest::from_json_str(&json).unwrap_err();
    assert!(matches!(err, ManifestError::LoopStartOutOfBounds { .. }), "{err:?}");
}

#[test]
fn rejects_unresolved_fallback() {
    let json = manifest_json(r#""idle": { "frames": [0], "fallback": "nope" }"#);
    let err = PetManifest::from_json_str(&json).unwrap_err();
    assert!(matches!(err, ManifestError::UnresolvedFallback { .. }), "{err:?}");
}

#[test]
fn accepts_fallback_idle_even_if_only_idle_defined() {
    let json = manifest_json(r#""idle": { "frames": [0], "fallback": "idle" }"#);
    assert!(PetManifest::from_json_str(&json).is_ok());
}

#[test]
fn rejects_zero_frame_duration() {
    let json = manifest_json(r#""idle": { "frames": [{ "index": 0, "ms": 0 }] }"#);
    let err = PetManifest::from_json_str(&json).unwrap_err();
    assert!(matches!(err, ManifestError::ZeroFrameDuration { .. }), "{err:?}");
}

#[test]
fn rejects_one_shot_with_loop_start() {
    let json = manifest_json(r#""idle": { "frames": [0, 1], "oneShot": true, "loopStart": 1 }"#);
    let err = PetManifest::from_json_str(&json).unwrap_err();
    assert!(matches!(err, ManifestError::OneShotWithLoopStart { .. }), "{err:?}");
}

#[test]
fn accepts_v2_fields_on_manifest_version_1() {
    // Lenient version policy: v2 fields valid on any manifest_version.
    let json = manifest_json(
        r#""idle": { "frames": [{ "index": 0, "ms": 80 }, { "index": 1, "ms": 80 }], "loopStart": 1 }"#,
    );
    assert!(PetManifest::from_json_str(&json).is_ok());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib pet::manifest::tests`
Expected: FAIL — the new `ManifestError` variants don't exist.

- [ ] **Step 3: Add the new `ManifestError` variants**

In `src/pet/manifest.rs`, add to the `ManifestError` enum:

```rust
    LoopStartOutOfBounds {
        animation: String,
        loop_start: usize,
        frames: usize,
    },
    UnresolvedFallback {
        animation: String,
        fallback: String,
    },
    ZeroFrameDuration {
        animation: String,
        frame_pos: usize,
    },
    OneShotWithLoopStart {
        animation: String,
    },
```

And add their `Display` arms in the `match self` block:

```rust
            Self::LoopStartOutOfBounds { animation, loop_start, frames } => write!(
                f,
                "animation '{animation}' loopStart {loop_start} >= frame count {frames}"
            ),
            Self::UnresolvedFallback { animation, fallback } => write!(
                f,
                "animation '{animation}' fallback '{fallback}' is not a defined animation"
            ),
            Self::ZeroFrameDuration { animation, frame_pos } => write!(
                f,
                "animation '{animation}' frame[{frame_pos}] has ms = 0"
            ),
            Self::OneShotWithLoopStart { animation } => write!(
                f,
                "animation '{animation}' sets both oneShot and loopStart (mutually exclusive)"
            ),
```

- [ ] **Step 4: Add the validation checks**

In `validate()`, inside the `for (name, anim) in &self.animations` loop (after the existing frame-index bounds check), add:

```rust
            for (pos, frame) in anim.frames.iter().enumerate() {
                if matches!(frame.ms, Some(0)) {
                    return Err(ManifestError::ZeroFrameDuration {
                        animation: name.clone(),
                        frame_pos: pos,
                    });
                }
            }
            if anim.one_shot && anim.loop_start.is_some() {
                return Err(ManifestError::OneShotWithLoopStart {
                    animation: name.clone(),
                });
            }
            if let Some(loop_start) = anim.loop_start {
                if loop_start >= anim.frames.len() {
                    return Err(ManifestError::LoopStartOutOfBounds {
                        animation: name.clone(),
                        loop_start,
                        frames: anim.frames.len(),
                    });
                }
            }
```

After the per-animation loop (where `idle` presence is already checked), add fallback resolution (needs the full set of names, so do it in a second pass):

```rust
        for (name, anim) in &self.animations {
            if let Some(fallback) = &anim.fallback {
                if fallback != "idle" && !self.animations.contains_key(fallback) {
                    return Err(ManifestError::UnresolvedFallback {
                        animation: name.clone(),
                        fallback: fallback.clone(),
                    });
                }
            }
        }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib pet::manifest::tests`
Expected: PASS (including the 6 new tests and all prior ones).

- [ ] **Step 6: Commit**

```bash
git add src/pet/manifest.rs
git commit -m "feat(manifest): validate loopStart/fallback/ms/oneShot lifecycle fields"
```

---

### Task 4: Per-frame `ms` timing in the runtime

**Files:**
- Modify: `src/pet/runtime.rs`

- [ ] **Step 1: Write the failing tests**

Add to the runtime tests module (these use a fixture where `idle` has explicit per-frame ms):

```rust
fn lifecycle_fixture(anim_name: &str, animation: crate::pet::manifest::Animation) -> PetRuntime {
    use crate::pet::manifest::{Animation, FrameGeometry, PetManifest};
    use std::collections::BTreeMap;
    let mut animations = BTreeMap::new();
    animations.insert("idle".to_string(), Animation::from_indices(&[0, 1, 2, 3]));
    animations.insert(anim_name.to_string(), animation);
    let manifest = PetManifest {
        manifest_version: 1,
        id: "fixture".into(),
        display_name: "Fixture".into(),
        spritesheet_path: "x.png".into(),
        frame: FrameGeometry { width: 16, height: 16, columns: 8, rows: 1 },
        animations,
    };
    PetRuntime::new_with_manifest(manifest)
}

#[test]
fn per_frame_ms_overrides_runtime_timing() {
    use crate::pet::manifest::{Animation, Frame};
    // idle with explicit 50ms frames.
    let idle = Animation {
        frames: vec![
            Frame { index: 0, ms: Some(50) },
            Frame { index: 1, ms: Some(50) },
        ],
        loop_start: None,
        fallback: None,
        one_shot: false,
    };
    let mut pet = lifecycle_fixture("idle2", idle.clone());
    // Replace idle with the timed version and pin it.
    pet.set_current_animation_for_test("idle2");
    pet.replace_animation_for_test("idle2", idle);
    pet.set_current_animation_for_test("idle2");

    pet.tick(Duration::from_millis(50));
    assert_eq!(pet.frame_index(), 1);
    pet.tick(Duration::from_millis(50));
    assert_eq!(pet.frame_index(), 0); // wrapped (2 frames)
}
```

Note: this test relies on two `#[cfg(test)]` hooks added in Step 3.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib pet::runtime::tests::per_frame_ms_overrides_runtime_timing`
Expected: FAIL — helper methods `set_current_animation_for_test` / `replace_animation_for_test` not found.

- [ ] **Step 3: Add a per-frame duration helper, test hooks, and use it in `advance_animation`**

In `src/pet/runtime.rs`, add a private helper that returns the current animation reference:

```rust
    fn current_animation(&self) -> &crate::pet::manifest::Animation {
        self.manifest
            .animations
            .get(&self.current_animation_name)
            .or_else(|| self.manifest.animations.get("idle"))
            .expect("manifest validation guarantees 'idle' exists")
    }

    fn frame_duration_for(&self, pos: usize) -> Duration {
        if let Some(ms) = self.current_animation().frame_ms(pos) {
            Duration::from_millis(ms as u64)
        } else {
            self.frame_duration()
        }
    }
```

Add the `#[cfg(test)]` hooks (place them near `force_state_for_test`):

```rust
    #[cfg(test)]
    pub fn set_current_animation_for_test(&mut self, name: &str) {
        self.current_animation_name = name.to_string();
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
    }

    #[cfg(test)]
    pub fn replace_animation_for_test(&mut self, name: &str, animation: crate::pet::manifest::Animation) {
        self.manifest.animations.insert(name.to_string(), animation);
    }
```

Rewrite `advance_animation` to consult per-frame duration:

```rust
    fn advance_animation(&mut self) {
        let frame_count = self.current_animation().frame_count().max(1);
        loop {
            let frame_duration = self.frame_duration_for(self.frame_index);
            if self.frame_elapsed < frame_duration {
                break;
            }
            self.frame_elapsed -= frame_duration;
            self.frame_index = (self.frame_index + 1) % frame_count;
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib pet::runtime::tests`
Expected: PASS — the new timing test and all existing runtime tests (parity: animations without `ms` still use `frame_duration()`).

- [ ] **Step 5: Commit**

```bash
git add src/pet/runtime.rs
git commit -m "feat(runtime): per-frame ms timing with v1 parity fallback"
```

---

### Task 5: `loop_start` wrap + entry-reset for lifecycle animations

**Files:**
- Modify: `src/pet/runtime.rs`

- [ ] **Step 1: Write the failing tests**

Add to the runtime tests module:

```rust
#[test]
fn loop_start_wraps_to_intro_boundary_not_zero() {
    use crate::pet::manifest::{Animation, Frame};
    let looping = Animation {
        frames: vec![
            Frame { index: 0, ms: Some(50) },
            Frame { index: 1, ms: Some(50) },
            Frame { index: 2, ms: Some(50) },
        ],
        loop_start: Some(1),
        fallback: None,
        one_shot: false,
    };
    let mut pet = lifecycle_fixture("loopy", looping);
    pet.set_current_animation_for_test("loopy"); // cursor at 0
    pet.tick(Duration::from_millis(50)); // -> 1
    pet.tick(Duration::from_millis(50)); // -> 2
    assert_eq!(pet.frame_index(), 2);
    pet.tick(Duration::from_millis(50)); // past last -> loop_start (1), not 0
    assert_eq!(pet.frame_index(), 1);
}

#[test]
fn entering_lifecycle_animation_resets_cursor() {
    use crate::pet::manifest::{Animation, Frame};
    // Fixture: Default-mode expression slot 2 picks "happy"; make "happy" a lifecycle anim.
    let happy = Animation {
        frames: vec![Frame { index: 5, ms: None }, Frame { index: 6, ms: None }],
        loop_start: Some(1),
        fallback: None,
        one_shot: false,
    };
    let mut pet = lifecycle_fixture("happy", happy);
    // Advance idle a couple frames so frame_index != 0.
    pet.tick(Duration::from_millis(200));
    pet.tick(Duration::from_millis(200));
    assert_ne!(pet.frame_index(), 0);
    // Force selection of the lifecycle "happy" animation.
    pet.set_expression_index_for_test(2);
    pet.refresh_behavior_mode_for_test();
    assert_eq!(pet.current_animation_name(), "happy");
    assert_eq!(pet.frame_index(), 0); // entry-reset fired
}

#[test]
fn entering_non_lifecycle_animation_preserves_cursor() {
    // Parity guard: switching to a plain animation keeps the cursor.
    let mut pet = PetRuntime::new();
    pet.force_state_for_test(PetState::Walk);
    pet.tick(Duration::from_millis(250));
    assert_eq!(pet.frame_index(), 2);
    pet.set_hovered(true);
    assert_eq!(pet.frame_index(), 2); // hover-cheerful is not lifecycle -> preserved
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib pet::runtime::tests::loop_start_wraps pet::runtime::tests::entering_`
Expected: FAIL — `set_expression_index_for_test` / `refresh_behavior_mode_for_test` missing, and `loop_start` not honored.

- [ ] **Step 3: Honor `loop_start` in `advance_animation`**

Replace the index advance in `advance_animation` to wrap to `loop_start`:

```rust
    fn advance_animation(&mut self) {
        let (frame_count, loop_start) = {
            let anim = self.current_animation();
            (anim.frame_count().max(1), anim.loop_start.unwrap_or(0))
        };
        loop {
            let frame_duration = self.frame_duration_for(self.frame_index);
            if self.frame_elapsed < frame_duration {
                break;
            }
            self.frame_elapsed -= frame_duration;
            let next = self.frame_index + 1;
            self.frame_index = if next >= frame_count { loop_start } else { next };
        }
    }
```

- [ ] **Step 4: Add entry-reset in `refresh_behavior_mode` + test hooks**

In `refresh_behavior_mode`, the final lines currently are:

```rust
        let (name, _) = lookup_with_fallback(&self.manifest, chain);
        self.current_animation_name = name.to_string();
```

Replace with a name-change-aware reset:

```rust
        let (name, _) = lookup_with_fallback(&self.manifest, chain);
        self.set_selected_animation(name);
    }

    /// Set the current animation name, resetting the cursor when entering a
    /// "lifecycle" animation (loopStart/oneShot) so its intro/one-shot starts at frame 0.
    /// Non-lifecycle name changes preserve the cursor (existing parity behavior).
    fn set_selected_animation(&mut self, name: &str) {
        if name == self.current_animation_name {
            return;
        }
        let is_lifecycle = self
            .manifest
            .animations
            .get(name)
            .map(|a| a.is_lifecycle())
            .unwrap_or(false);
        self.current_animation_name = name.to_string();
        if is_lifecycle {
            self.frame_index = 0;
            self.frame_elapsed = Duration::ZERO;
        }
```

(The existing closing brace of `refresh_behavior_mode` now closes `set_selected_animation`; keep brace balance — `set_selected_animation` is a new method, so ensure `refresh_behavior_mode` is closed before it.)

Add the test hooks near the other `#[cfg(test)]` helpers:

```rust
    #[cfg(test)]
    pub fn set_expression_index_for_test(&mut self, idx: usize) {
        self.expression_index = idx;
    }

    #[cfg(test)]
    pub fn refresh_behavior_mode_for_test(&mut self) {
        self.refresh_behavior_mode();
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib pet::runtime::tests`
Expected: PASS (new loop_start + entry-reset tests, parity test, and all prior runtime tests).

- [ ] **Step 6: Commit**

```bash
git add src/pet/runtime.rs
git commit -m "feat(runtime): loop_start wrap + entry-reset for lifecycle animations"
```

---

### Task 6: One-shot completion signal on `PetTick` + `fallback` accessor

**Files:**
- Modify: `src/pet/runtime.rs`

- [ ] **Step 1: Write the failing tests**

Add to the runtime tests module:

```rust
#[test]
fn one_shot_completion_fires_after_final_frame_full_duration() {
    use crate::pet::manifest::{Animation, Frame};
    let success = Animation {
        frames: vec![
            Frame { index: 4, ms: Some(50) },
            Frame { index: 5, ms: Some(50) },
        ],
        loop_start: None,
        fallback: Some("idle".to_string()),
        one_shot: true,
    };
    let mut pet = lifecycle_fixture("success", success);
    pet.set_current_animation_for_test("success"); // frame 0

    let t1 = pet.tick(Duration::from_millis(50)); // show frame 0 done -> frame 1
    assert_eq!(pet.frame_index(), 1);
    assert!(!t1.oneshot_completed);

    let t2 = pet.tick(Duration::from_millis(50)); // final frame shown full duration
    assert!(t2.oneshot_completed, "completion should fire after final frame duration");
    assert_eq!(pet.frame_index(), 1, "one-shot holds the last frame (no wrap)");
}

#[test]
fn looping_animation_never_reports_oneshot_completed() {
    let mut pet = PetRuntime::new(); // bundled idle, not one-shot
    let t = pet.tick(Duration::from_millis(200));
    assert!(!t.oneshot_completed);
}

#[test]
fn current_fallback_exposes_manifest_value() {
    use crate::pet::manifest::{Animation, Frame};
    let success = Animation {
        frames: vec![Frame { index: 4, ms: Some(50) }],
        loop_start: None,
        fallback: Some("idle".to_string()),
        one_shot: true,
    };
    let mut pet = lifecycle_fixture("success", success);
    pet.set_current_animation_for_test("success");
    assert_eq!(pet.current_fallback(), "idle");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib pet::runtime::tests::one_shot pet::runtime::tests::looping_ pet::runtime::tests::current_fallback`
Expected: FAIL — `PetTick.oneshot_completed` and `current_fallback` don't exist.

- [ ] **Step 3: Add the field, the accessor, and completion detection**

Add the field to `PetTick`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PetTick {
    pub state: PetState,
    pub frame_index: usize,
    pub speed_x: f32,
    pub oneshot_completed: bool,
}
```

Add the `fallback` accessor:

```rust
    pub fn current_fallback(&self) -> String {
        self.current_animation()
            .fallback
            .clone()
            .unwrap_or_else(|| "idle".to_string())
    }
```

Make `advance_animation` return whether a one-shot completed (held the last frame for its full duration):

```rust
    fn advance_animation(&mut self) -> bool {
        let (frame_count, loop_start, one_shot) = {
            let anim = self.current_animation();
            (anim.frame_count().max(1), anim.loop_start.unwrap_or(0), anim.one_shot)
        };
        let mut completed = false;
        loop {
            let frame_duration = self.frame_duration_for(self.frame_index);
            if self.frame_elapsed < frame_duration {
                break;
            }
            self.frame_elapsed -= frame_duration;
            if one_shot && self.frame_index + 1 >= frame_count {
                // Final frame has now been shown for its full duration. Hold it;
                // the owner reacts to the completion signal (it does NOT auto-advance).
                completed = true;
                self.frame_elapsed = Duration::ZERO;
                break;
            }
            let next = self.frame_index + 1;
            self.frame_index = if next >= frame_count { loop_start } else { next };
        }
        completed
    }
```

In `tick`, capture the flag and put it on every returned `PetTick`. The hidden early-return `PetTick` and the final `PetTick` both need the field. Update the call site:

```rust
        let oneshot_completed = self.advance_animation();
```

and the final return:

```rust
        PetTick {
            state: self.state,
            frame_index: self.frame_index,
            speed_x: self.speed_x(),
            oneshot_completed,
        }
```

For the hidden early-return `PetTick` (line ~217), add `oneshot_completed: false,`.

- [ ] **Step 4: Fix any other `PetTick { ... }` literals**

Run: `cargo build 2>&1 | head -40`
Any "missing field `oneshot_completed`" errors point to other `PetTick` literals (tests included) — add `oneshot_completed: false` to each. There is a `PetTick` literal in `advance_state`-adjacent test assertions; update construction-style literals only (pattern matches like `PetTick { frame_index, .. }` are unaffected by `..`).

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib pet::runtime::tests`
Expected: PASS — completion fires after the final frame's full duration, looping never completes, fallback exposed, all prior tests green.

- [ ] **Step 6: Commit**

```bash
git add src/pet/runtime.rs
git commit -m "feat(runtime): one-shot completion signal on PetTick + fallback accessor"
```

---

### Task 7: Final verification (parity + lint)

**Files:** none (verification only)

- [ ] **Step 1: Full test suite**

Run: `cargo test`
Expected: PASS, no failures.

- [ ] **Step 2: Lint + format**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: clean (no warnings).

Run: `cargo fmt --check`
Expected: clean. If it reports diffs, run `cargo fmt` and re-check.

- [ ] **Step 3: Project verify script**

Run: `./scripts/verify.sh`
Expected: PASS.

- [ ] **Step 4: Manual parity smoke**

Run: `cargo run` (or `./scripts/build_app.sh` then launch `dist/Happy Cappy.app`).
Expected: the capybara looks and animates exactly as on `main` — idle/blink/walk/hover/sleep cadence unchanged. SP4-A ships no user-visible change on its own.

- [ ] **Step 5: Confirm no dead code / required-animation regressions**

Run: `cargo test --lib pet::manifest::tests::parses_bundled_manifest`
Expected: PASS — the bundled `happy_cappy.json` (still v1) parses and validates under the v2 parser.

---

## Self-Review notes

- **Spec coverage:** Frame dual-form (Task 1), schema v2 + accessors + parity (Task 2), validation incl. oneShot/loopStart exclusivity + version policy (Task 3), per-frame ms with parity (Task 4), loop_start + entry-reset (Task 5), one-shot completion signal + fallback accessor (Task 6), exit criteria (Task 7). All §4–§8 spec points map to a task.
- **Type consistency:** `Frame { index, ms }`, `Animation::{from_indices, frame_count, sprite_index, frame_ms, is_lifecycle, fallback}`, `PetTick.oneshot_completed`, `current_animation`, `frame_duration_for`, `set_selected_animation` are defined where first used and reused with the same names downstream (SP4-B depends on `oneshot_completed`, `current_fallback`, `set_selected_animation`, `is_lifecycle`).
- **Engine-only:** one-shot `fallback` is exposed + tested but has no consumer in SP4-A (per spec §5.3); SP4-B's notification owns the transition.
