# Pet Manifest Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract Happy Cappy's animation table into a data-driven `PetManifest` loaded from an embedded JSON file, replacing the `AnimationGroup` and `SpriteRow` enums with a string-keyed resolver, without changing any user-visible behavior.

**Architecture:** Two-layer split inside a new `src/pet/` module. `manifest.rs` owns the JSON data model (per-animation sprite-index lists). `resolver.rs` is the pure-function mapping from runtime state to animation name with a candidate fallback chain. `runtime.rs` keeps every field and timing rule that exists today — frame duration stays state-based (200/100/500 ms + hover intensity rounding). Migration proceeds by adding new APIs alongside old, switching call sites, then deleting dead code. The build stays green between every task.

**Tech Stack:** Rust 2021, `serde`/`serde_json` (already in `Cargo.toml` via the settings module), `image` for spritesheet loading, `pixels`/`wgpu` for rendering. No new dependencies.

**Reference spec:** `docs/superpowers/specs/2026-05-26-pet-manifest-refactor-design.md` (revision 3).

---

## Task 1: Create the bundled manifest JSON file

**Files:**
- Create: `assets/manifests/happy_cappy.json`

- [ ] **Step 1: Create the manifests directory and JSON file**

Run:
```bash
mkdir -p assets/manifests
```

Create `assets/manifests/happy_cappy.json` with this exact content (sprite indices = `row * 4 + column` against today's 4×10 spritesheet at `assets/happy_cappy_spritesheet.png`):

```json
{
  "manifest_version": 1,
  "id": "happy-cappy",
  "displayName": "Happy Cappy",
  "spritesheetPath": "happy_cappy_spritesheet.png",
  "frame": { "width": 64, "height": 64, "columns": 4, "rows": 10 },
  "animations": {
    "idle":           { "frames": [0,  1,  2,  3] },
    "blink":          { "frames": [4,  5,  6,  7] },
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

- [ ] **Step 2: Verify nothing else changed**

Run: `cargo build`
Expected: build succeeds with no warnings beyond the existing baseline. The JSON file is data only; no code references it yet.

- [ ] **Step 3: Commit**

```bash
git add assets/manifests/happy_cappy.json
git commit -m "$(cat <<'EOF'
chore(assets): bundle happy_cappy.json manifest skeleton

Inert data file declaring the sprite-index layout that future tasks
will load via include_str! in src/pet/manifest.rs.
EOF
)"
```

---

## Task 2: Add manifest types and serde parsing

**Files:**
- Create: `src/pet/manifest.rs`
- Modify: `src/pet.rs` (add `pub mod manifest;` declaration)

- [ ] **Step 1: Create the pet submodule directory**

Run:
```bash
mkdir -p src/pet
```

Rust 2021 allows `src/pet.rs` and `src/pet/` to coexist — `src/pet.rs` stays as the module file, `src/pet/` holds submodules.

- [ ] **Step 2: Write the failing test inside the new manifest module**

Create `src/pet/manifest.rs` with placeholders so the test compiles, but the load function panics:

```rust
use std::collections::BTreeMap;

use serde::Deserialize;

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
    pub frames: Vec<u32>,
}

fn default_manifest_version() -> u32 {
    1
}

impl PetManifest {
    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn load_embedded_happy_cappy() -> Self {
        const JSON: &str = include_str!("../../assets/manifests/happy_cappy.json");
        Self::from_json_str(JSON).expect("bundled happy_cappy.json must parse")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bundled_manifest() {
        let manifest = PetManifest::load_embedded_happy_cappy();
        assert_eq!(manifest.id, "happy-cappy");
        assert_eq!(manifest.display_name, "Happy Cappy");
        assert_eq!(manifest.spritesheet_path, "happy_cappy_spritesheet.png");
        assert_eq!(manifest.frame.width, 64);
        assert_eq!(manifest.frame.height, 64);
        assert_eq!(manifest.frame.columns, 4);
        assert_eq!(manifest.frame.rows, 10);
        assert_eq!(manifest.manifest_version, 1);
        assert_eq!(manifest.animations.len(), 10);
        assert_eq!(manifest.animations["idle"].frames, vec![0, 1, 2, 3]);
        assert_eq!(manifest.animations["walk-right"].frames, vec![32, 33, 34, 35]);
        assert_eq!(manifest.animations["drag"].frames, vec![36, 37, 38, 39]);
    }
}
```

- [ ] **Step 3: Wire the module into src/pet.rs**

Open `src/pet.rs` and add this line near the top (after any existing `use` statements, before the `Personality` enum):

```rust
pub mod manifest;
```

- [ ] **Step 4: Run the test**

Run: `cargo test --lib parses_bundled_manifest`
Expected: PASS — the JSON file from Task 1 deserializes into the typed struct.

- [ ] **Step 5: Run the full test suite to confirm nothing broke**

Run: `cargo test`
Expected: all existing tests still pass, plus `parses_bundled_manifest`.

- [ ] **Step 6: Commit**

```bash
git add src/pet/manifest.rs src/pet.rs
git commit -m "$(cat <<'EOF'
feat(pet): add manifest types and embedded loader

Introduces PetManifest, FrameGeometry, and Animation structs in the new
src/pet/manifest.rs submodule. load_embedded_happy_cappy() parses the
bundled JSON via include_str!. No validation yet; that lands in the
next commit.
EOF
)"
```

---

## Task 3: Add manifest validation

**Files:**
- Modify: `src/pet/manifest.rs`

- [ ] **Step 1: Write the failing tests for the generic validation rules**

Add these tests at the bottom of the `#[cfg(test)] mod tests` block in `src/pet/manifest.rs`:

```rust
    fn minimal_valid_json() -> String {
        r#"{
            "id": "test",
            "displayName": "Test",
            "spritesheetPath": "test.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {"idle": {"frames": [0, 1, 2, 3]}}
        }"#
        .to_string()
    }

    #[test]
    fn rejects_manifest_missing_idle() {
        let json = r#"{
            "id": "test",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {"walk": {"frames": [0, 1]}}
        }"#;
        let err = PetManifest::from_json_str(json).unwrap_err();
        assert!(matches!(err, ManifestError::MissingIdleAnimation));
    }

    #[test]
    fn rejects_frame_index_out_of_bounds() {
        let json = r#"{
            "id": "test",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {"idle": {"frames": [0, 1, 99]}}
        }"#;
        let err = PetManifest::from_json_str(json).unwrap_err();
        assert!(matches!(
            err,
            ManifestError::SpriteIndexOutOfBounds { index: 99, max: 4, .. }
        ));
    }

    #[test]
    fn rejects_empty_animation() {
        let json = r#"{
            "id": "test",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {"idle": {"frames": []}}
        }"#;
        let err = PetManifest::from_json_str(json).unwrap_err();
        assert!(matches!(err, ManifestError::EmptyAnimation { .. }));
    }

    #[test]
    fn rejects_zero_frame_geometry() {
        let json = r#"{
            "id": "test",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 0, "height": 16, "columns": 4, "rows": 1},
            "animations": {"idle": {"frames": [0]}}
        }"#;
        let err = PetManifest::from_json_str(json).unwrap_err();
        assert!(matches!(err, ManifestError::ZeroGeometry));
    }

    #[test]
    fn rejects_manifest_version_zero() {
        let json = r#"{
            "manifest_version": 0,
            "id": "test",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {"idle": {"frames": [0]}}
        }"#;
        let err = PetManifest::from_json_str(json).unwrap_err();
        assert!(matches!(err, ManifestError::InvalidVersion(0)));
    }

    #[test]
    fn accepts_unknown_future_manifest_version() {
        let json = r#"{
            "manifest_version": 99,
            "id": "test",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {"idle": {"frames": [0]}}
        }"#;
        let manifest = PetManifest::from_json_str(json).unwrap();
        assert_eq!(manifest.manifest_version, 99);
    }

    #[test]
    fn rejects_too_many_frames_in_animation() {
        let frames: Vec<u32> = (0..65).map(|_| 0).collect();
        let json = format!(
            r#"{{
                "id": "test",
                "displayName": "Test",
                "spritesheetPath": "x.png",
                "frame": {{"width": 16, "height": 16, "columns": 4, "rows": 1}},
                "animations": {{"idle": {{"frames": {:?}}}}}
            }}"#,
            frames
        );
        let err = PetManifest::from_json_str(&json).unwrap_err();
        assert!(matches!(err, ManifestError::TooManyFrames { count: 65, .. }));
    }

    #[test]
    fn rejects_empty_id() {
        let json = r#"{
            "id": "",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {"idle": {"frames": [0]}}
        }"#;
        let err = PetManifest::from_json_str(json).unwrap_err();
        assert!(matches!(err, ManifestError::EmptyField("id")));
    }

    #[test]
    fn rejects_id_with_path_separator() {
        let json = r#"{
            "id": "bad/id",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {"idle": {"frames": [0]}}
        }"#;
        let err = PetManifest::from_json_str(json).unwrap_err();
        assert!(matches!(err, ManifestError::InvalidIdChars));
    }

    #[test]
    fn minimal_manifest_with_only_idle_is_valid() {
        let manifest = PetManifest::from_json_str(&minimal_valid_json()).unwrap();
        assert_eq!(manifest.animations.len(), 1);
    }
```

- [ ] **Step 2: Run tests to verify they all fail**

Run: `cargo test --lib pet::manifest`
Expected: every new test FAILs with "no variant named ManifestError" or "function from_json_str returns serde_json::Error". The `parses_bundled_manifest` from Task 2 still passes.

- [ ] **Step 3: Implement ManifestError and validation logic**

Replace the contents of `src/pet/manifest.rs` with this expanded version (keeps everything from Task 2 and adds the error enum + validation):

```rust
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use serde::Deserialize;

const MAX_FRAMES_PER_ANIMATION: usize = 64;

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
    pub frames: Vec<u32>,
}

fn default_manifest_version() -> u32 {
    1
}

#[derive(Debug)]
pub enum ManifestError {
    Json(serde_json::Error),
    InvalidVersion(u32),
    EmptyField(&'static str),
    InvalidIdChars,
    ZeroGeometry,
    EmptyAnimation { name: String },
    TooManyFrames { name: String, count: usize },
    SpriteIndexOutOfBounds {
        animation: String,
        frame_pos: usize,
        index: u32,
        max: u32,
    },
    MissingIdleAnimation,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => write!(f, "manifest JSON error: {e}"),
            Self::InvalidVersion(v) => write!(f, "invalid manifest_version: {v}"),
            Self::EmptyField(name) => write!(f, "field '{name}' must not be empty"),
            Self::InvalidIdChars => write!(f, "id must not contain '/', '\\\\', or null bytes"),
            Self::ZeroGeometry => write!(f, "frame geometry values must be > 0"),
            Self::EmptyAnimation { name } => write!(f, "animation '{name}' has no frames"),
            Self::TooManyFrames { name, count } => write!(
                f,
                "animation '{name}' has {count} frames, max is {MAX_FRAMES_PER_ANIMATION}"
            ),
            Self::SpriteIndexOutOfBounds {
                animation,
                frame_pos,
                index,
                max,
            } => write!(
                f,
                "animation '{animation}' frame[{frame_pos}] index {index} >= {max}"
            ),
            Self::MissingIdleAnimation => {
                write!(f, "manifest must declare an 'idle' animation")
            }
        }
    }
}

impl Error for ManifestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for ManifestError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl PetManifest {
    pub fn from_json_str(json: &str) -> Result<Self, ManifestError> {
        let raw: PetManifest = serde_json::from_str(json)?;
        raw.validate()?;
        Ok(raw)
    }

    pub fn load_embedded_happy_cappy() -> Self {
        const JSON: &str = include_str!("../../assets/manifests/happy_cappy.json");
        Self::from_json_str(JSON).expect("bundled happy_cappy.json must parse and validate")
    }

    fn validate(&self) -> Result<(), ManifestError> {
        if self.manifest_version < 1 {
            return Err(ManifestError::InvalidVersion(self.manifest_version));
        }
        if self.id.is_empty() {
            return Err(ManifestError::EmptyField("id"));
        }
        if self.id.contains('/') || self.id.contains('\\') || self.id.contains('\0') {
            return Err(ManifestError::InvalidIdChars);
        }
        if self.display_name.is_empty() {
            return Err(ManifestError::EmptyField("displayName"));
        }
        if self.spritesheet_path.is_empty() {
            return Err(ManifestError::EmptyField("spritesheetPath"));
        }
        if self.frame.width == 0
            || self.frame.height == 0
            || self.frame.columns == 0
            || self.frame.rows == 0
        {
            return Err(ManifestError::ZeroGeometry);
        }

        let max_index = self.frame.columns * self.frame.rows;
        for (name, anim) in &self.animations {
            if anim.frames.is_empty() {
                return Err(ManifestError::EmptyAnimation { name: name.clone() });
            }
            if anim.frames.len() > MAX_FRAMES_PER_ANIMATION {
                return Err(ManifestError::TooManyFrames {
                    name: name.clone(),
                    count: anim.frames.len(),
                });
            }
            for (pos, index) in anim.frames.iter().enumerate() {
                if *index >= max_index {
                    return Err(ManifestError::SpriteIndexOutOfBounds {
                        animation: name.clone(),
                        frame_pos: pos,
                        index: *index,
                        max: max_index,
                    });
                }
            }
        }

        if !self.animations.contains_key("idle") {
            return Err(ManifestError::MissingIdleAnimation);
        }

        Ok(())
    }
}
```

- [ ] **Step 4: Run all manifest tests to confirm they pass**

Run: `cargo test --lib pet::manifest`
Expected: PASS — all 11 manifest tests (1 from Task 2 + 10 new) pass.

- [ ] **Step 5: Run the full suite to confirm nothing else broke**

Run: `cargo test`
Expected: every test passes.

- [ ] **Step 6: Commit**

```bash
git add src/pet/manifest.rs
git commit -m "$(cat <<'EOF'
feat(pet): validate manifest schema with structured errors

Adds ManifestError enum and validate() pass enforcing version, id
sanity, non-empty fields, positive geometry, animation non-emptiness,
frame-count cap, sprite-index bounds, and required 'idle' animation.
Tests cover each rejection path plus the minimal happy case.
EOF
)"
```

---

## Task 4: Add the resolver module

**Files:**
- Create: `src/pet/resolver.rs`
- Modify: `src/pet.rs` (add `pub mod resolver;`)

- [ ] **Step 1: Write the failing tests**

Create `src/pet/resolver.rs` with stubs and tests:

```rust
use crate::micro_action::MicroAction;
use crate::pet::manifest::PetManifest;
use crate::pet::{BehaviorMode, Personality};

pub fn resolve_animation_chain(
    _mode: BehaviorMode,
    _personality: Personality,
    _expression_index: usize,
    _action: Option<MicroAction>,
) -> &'static [&'static str] {
    &[]
}

pub fn lookup_with_fallback<'a>(
    _manifest: &'a PetManifest,
    _chain: &[&str],
) -> (&'static str, &'a crate::pet::manifest::Animation) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::manifest::{Animation, FrameGeometry, PetManifest};
    use std::collections::BTreeMap;

    fn fixture_manifest(animation_names: &[&str]) -> PetManifest {
        let mut animations = BTreeMap::new();
        for name in animation_names {
            animations.insert((*name).to_string(), Animation { frames: vec![0] });
        }
        PetManifest {
            manifest_version: 1,
            id: "fixture".into(),
            display_name: "Fixture".into(),
            spritesheet_path: "x.png".into(),
            frame: FrameGeometry { width: 16, height: 16, columns: 4, rows: 1 },
            animations,
        }
    }

    #[test]
    fn chain_for_hovered_uses_personality_variant() {
        let calm = resolve_animation_chain(BehaviorMode::Hovered, Personality::Calm, 0, None);
        assert_eq!(calm, &["hover-calm", "hover", "idle"]);

        let cheerful =
            resolve_animation_chain(BehaviorMode::Hovered, Personality::Cheerful, 0, None);
        assert_eq!(cheerful, &["hover-cheerful", "hover", "idle"]);

        let lively =
            resolve_animation_chain(BehaviorMode::Hovered, Personality::Lively, 0, None);
        assert_eq!(lively, &["hover-lively", "hover", "idle"]);
    }

    #[test]
    fn chain_for_default_cycles_through_5_expressions() {
        let p = Personality::Cheerful;
        assert_eq!(resolve_animation_chain(BehaviorMode::Default, p, 0, None), &["idle"]);
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Default, p, 1, None),
            &["blink", "idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Default, p, 2, None),
            &["happy", "idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Default, p, 3, None),
            &["curious", "idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Default, p, 4, None),
            &["sleepy", "idle"]
        );
        assert_eq!(resolve_animation_chain(BehaviorMode::Default, p, 5, None), &["idle"]);
    }

    #[test]
    fn chain_for_action_uses_micro_action_animation() {
        let p = Personality::Cheerful;
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Action, p, 0, Some(MicroAction::Nap)),
            &["sleepy", "idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Action, p, 0, Some(MicroAction::CheerUp)),
            &["happy", "idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Action, p, 0, None),
            &["idle"]
        );
    }

    #[test]
    fn chain_for_walking_uses_walk_right_then_walk_then_idle() {
        let chain = resolve_animation_chain(
            BehaviorMode::Walking,
            Personality::Cheerful,
            0,
            None,
        );
        assert_eq!(chain, &["walk-right", "walk", "idle"]);
    }

    #[test]
    fn chain_for_dragging_is_drag_then_idle() {
        let chain = resolve_animation_chain(
            BehaviorMode::Dragging,
            Personality::Cheerful,
            0,
            None,
        );
        assert_eq!(chain, &["drag", "idle"]);
    }

    #[test]
    fn chain_for_hidden_is_idle_only() {
        let chain =
            resolve_animation_chain(BehaviorMode::Hidden, Personality::Cheerful, 0, None);
        assert_eq!(chain, &["idle"]);
    }

    #[test]
    fn lookup_falls_back_when_specific_missing() {
        let manifest = fixture_manifest(&["idle"]);
        let (name, _) = lookup_with_fallback(&manifest, &["hover-lively", "hover", "idle"]);
        assert_eq!(name, "idle");
    }

    #[test]
    fn lookup_uses_second_tier_when_specific_missing() {
        let manifest = fixture_manifest(&["idle", "hover"]);
        let (name, _) = lookup_with_fallback(&manifest, &["hover-lively", "hover", "idle"]);
        assert_eq!(name, "hover");
    }

    #[test]
    fn lookup_uses_first_tier_when_present() {
        let manifest = fixture_manifest(&["idle", "hover", "hover-lively"]);
        let (name, _) = lookup_with_fallback(&manifest, &["hover-lively", "hover", "idle"]);
        assert_eq!(name, "hover-lively");
    }
}
```

This file references `BehaviorMode` and `Personality` from `crate::pet` and `MicroAction` from `crate::micro_action` — those types already exist in `src/pet.rs` and `src/micro_action.rs` today.

- [ ] **Step 2: Wire the module into src/pet.rs**

Open `src/pet.rs` and add directly below the `pub mod manifest;` line from Task 2:

```rust
pub mod resolver;
```

- [ ] **Step 3: Run tests, expect failure**

Run: `cargo test --lib pet::resolver`
Expected: chain tests FAIL ("left: []", expected non-empty slices). `lookup_*` tests FAIL with `todo!()` panic.

- [ ] **Step 4: Implement the resolver**

Replace the stub bodies in `src/pet/resolver.rs` (keep the test block intact):

```rust
use crate::micro_action::MicroAction;
use crate::pet::manifest::{Animation, PetManifest};
use crate::pet::{BehaviorMode, Personality};

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
            Personality::Calm => &["hover-calm", "hover", "idle"],
            Personality::Cheerful => &["hover-cheerful", "hover", "idle"],
            Personality::Lively => &["hover-lively", "hover", "idle"],
        },
        BehaviorMode::Action => match action {
            Some(MicroAction::Nap) => &["sleepy", "idle"],
            Some(MicroAction::CheerUp) => &["happy", "idle"],
            None => &["idle"],
        },
        BehaviorMode::Walking => &["walk-right", "walk", "idle"],
        BehaviorMode::Default => match expression_index % 5 {
            0 => &["idle"],
            1 => &["blink", "idle"],
            2 => &["happy", "idle"],
            3 => &["curious", "idle"],
            _ => &["sleepy", "idle"],
        },
    }
}

pub fn lookup_with_fallback<'a>(
    manifest: &'a PetManifest,
    chain: &[&str],
) -> (&'static str, &'a Animation) {
    for &name in chain {
        if let Some(anim) = manifest.animations.get(name) {
            // Safety: `name` originated as a &'static str literal from
            // resolve_animation_chain's tables, so promoting to 'static is sound.
            let static_name: &'static str = match name {
                "idle" => "idle",
                "blink" => "blink",
                "happy" => "happy",
                "curious" => "curious",
                "sleepy" => "sleepy",
                "hover" => "hover",
                "hover-calm" => "hover-calm",
                "hover-cheerful" => "hover-cheerful",
                "hover-lively" => "hover-lively",
                "walk" => "walk",
                "walk-right" => "walk-right",
                "drag" => "drag",
                _ => "idle",
            };
            return (static_name, anim);
        }
    }
    let idle = manifest
        .animations
        .get("idle")
        .expect("manifest validation guarantees 'idle' exists");
    ("idle", idle)
}
```

- [ ] **Step 5: Run tests, expect pass**

Run: `cargo test --lib pet::resolver`
Expected: all 9 resolver tests pass.

- [ ] **Step 6: Run the full suite**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/pet/resolver.rs src/pet.rs
git commit -m "$(cat <<'EOF'
feat(pet): resolve behavior-mode + personality to animation name

Pure function resolve_animation_chain returns the candidate animation
names a runtime should try, longest-specific-first. lookup_with_fallback
walks the chain and returns the first present entry, terminating at
'idle' (which manifest validation guarantees exists).
EOF
)"
```

---

## Task 5: Add `ActionOverride::action()` accessor

**Files:**
- Modify: `src/micro_action.rs`

- [ ] **Step 1: Write the failing test**

Open `src/micro_action.rs`, find the `#[cfg(test)] mod tests` block at the bottom, and add this test before the closing brace:

```rust
    #[test]
    fn action_accessor_returns_underlying_kind() {
        let nap = ActionOverride::new(MicroAction::Nap);
        assert_eq!(nap.action(), MicroAction::Nap);

        let cheer = ActionOverride::new(MicroAction::CheerUp);
        assert_eq!(cheer.action(), MicroAction::CheerUp);
    }
```

- [ ] **Step 2: Run the test, expect failure**

Run: `cargo test --lib action_accessor_returns_underlying_kind`
Expected: FAIL — "no method named `action`".

- [ ] **Step 3: Add the accessor**

In `src/micro_action.rs`, inside the `impl ActionOverride { ... }` block (currently has `new`, `remaining`, `animation_group`, `disables_movement`, `tick`), add:

```rust
    pub fn action(&self) -> MicroAction {
        self.action
    }
```

`MicroAction` is `#[derive(..., Copy, ...)]` already (it's referenced as `Copy` in `ActionOverride`'s own derive), so returning by value works.

- [ ] **Step 4: Run the test, expect pass**

Run: `cargo test --lib action_accessor_returns_underlying_kind`
Expected: PASS.

- [ ] **Step 5: Run full suite**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/micro_action.rs
git commit -m "$(cat <<'EOF'
feat(micro_action): expose ActionOverride::action() accessor

Lets PetRuntime feed the underlying MicroAction kind to the animation
resolver. ActionOverride::animation_group() stays for now and will be
removed once the resolver replaces every caller.
EOF
)"
```

---

## Task 6: Add geometry to `SpriteSheet` and a sprite-index-based `frame_rect`

**Files:**
- Modify: `src/sprite.rs`
- Modify: `src/app.rs:215` (callsite)

- [ ] **Step 1: Write the failing tests**

Open `src/sprite.rs`. At the bottom of the `#[cfg(test)] mod tests` block, add:

```rust
    use crate::pet::manifest::FrameGeometry;

    fn happy_cappy_geometry() -> FrameGeometry {
        FrameGeometry { width: 64, height: 64, columns: 4, rows: 10 }
    }

    #[test]
    fn frame_rect_by_index_for_zero_returns_top_left() {
        let sheet =
            SpriteSheet::from_image_with_geometry(sheet(256, 640), &happy_cappy_geometry())
                .unwrap();
        assert_eq!(
            sheet.frame_rect_by_index(0),
            FrameRect { x: 0, y: 0, width: 64, height: 64 }
        );
    }

    #[test]
    fn frame_rect_by_index_for_32_returns_walk_row_first_column() {
        let sheet =
            SpriteSheet::from_image_with_geometry(sheet(256, 640), &happy_cappy_geometry())
                .unwrap();
        assert_eq!(
            sheet.frame_rect_by_index(32),
            FrameRect { x: 0, y: 8 * 64, width: 64, height: 64 }
        );
    }

    #[test]
    fn frame_rect_by_index_for_39_returns_drag_row_last_column() {
        let sheet =
            SpriteSheet::from_image_with_geometry(sheet(256, 640), &happy_cappy_geometry())
                .unwrap();
        assert_eq!(
            sheet.frame_rect_by_index(39),
            FrameRect { x: 3 * 64, y: 9 * 64, width: 64, height: 64 }
        );
    }

    #[test]
    fn from_image_with_geometry_rejects_mismatched_dimensions() {
        let err = SpriteSheet::from_image_with_geometry(sheet(250, 640), &happy_cappy_geometry())
            .unwrap_err();
        assert!(matches!(err, SpriteError::InvalidDimensions { .. }));
    }

    #[test]
    fn from_image_with_geometry_accepts_matching_image() {
        let result =
            SpriteSheet::from_image_with_geometry(sheet(256, 640), &happy_cappy_geometry());
        assert!(result.is_ok());
    }
```

- [ ] **Step 2: Run tests, expect failure**

Run: `cargo test --lib sprite`
Expected: every new test FAILs to compile ("no method named `frame_rect_by_index`", "no method named `from_image_with_geometry`").

- [ ] **Step 3: Implement the new APIs alongside the existing ones**

In `src/sprite.rs`, add this import near the top (with the existing `use` statements):

```rust
use crate::pet::manifest::FrameGeometry;
```

In the same file, add these methods inside the existing `impl SpriteSheet { ... }` block (keep all existing methods intact for now):

```rust
    pub fn from_image_with_geometry(
        image: RgbaImage,
        geometry: &FrameGeometry,
    ) -> Result<Self, SpriteError> {
        let width = image.width();
        let height = image.height();
        let expected_width = geometry.width.checked_mul(geometry.columns);
        let expected_height = geometry.height.checked_mul(geometry.rows);

        if geometry.width == 0
            || geometry.height == 0
            || geometry.columns == 0
            || geometry.rows == 0
            || Some(width) != expected_width
            || Some(height) != expected_height
        {
            return Err(SpriteError::InvalidDimensions {
                width,
                height,
                expected_width,
                expected_height,
                frame_size: geometry.width.max(geometry.height),
            });
        }

        Ok(Self { image, frame_size: geometry.width })
    }

    pub fn load_with_geometry(
        path: impl AsRef<std::path::Path>,
        geometry: &FrameGeometry,
    ) -> Result<Self, SpriteError> {
        let image = image::open(path)?.into_rgba8();
        Self::from_image_with_geometry(image, geometry)
    }

    pub fn frame_rect_by_index(&self, sprite_index: u32) -> FrameRect {
        let columns = (self.image.width() / self.frame_size).max(1);
        let row = sprite_index / columns;
        let col = sprite_index % columns;
        FrameRect {
            x: col * self.frame_size,
            y: row * self.frame_size,
            width: self.frame_size,
            height: self.frame_size,
        }
    }
```

Note: we reuse the existing `frame_size: u32` field for now (which equals `geometry.width` for happy-cappy since the frames are square). Task 10 replaces it with a stored `FrameGeometry`.

- [ ] **Step 4: Run sprite tests, expect pass**

Run: `cargo test --lib sprite`
Expected: all sprite tests pass (existing + 5 new).

- [ ] **Step 5: Run full suite**

Run: `cargo test`
Expected: all tests pass; `app.rs` callsite still uses the old `SpriteSheet::load(path, FRAME_SIZE)` and continues to work.

- [ ] **Step 6: Commit**

```bash
git add src/sprite.rs
git commit -m "$(cat <<'EOF'
feat(sprite): add geometry-aware frame_rect_by_index and constructors

Adds from_image_with_geometry / load_with_geometry / frame_rect_by_index
on SpriteSheet, callable in parallel with the existing SpriteRow API.
A later task switches the only loader (app.rs) over and deletes the
SpriteRow path.
EOF
)"
```

---

## Task 7: Add manifest field + new accessors to `Pet` (coexist with `AnimationGroup`)

**Files:**
- Modify: `src/pet.rs`

- [ ] **Step 1: Write the failing tests**

Inside `src/pet.rs`, find the `#[cfg(test)] mod tests` block. Add these new tests at the end of the block:

```rust
    #[test]
    fn current_animation_name_is_idle_at_construction() {
        let pet = Pet::new();
        assert_eq!(pet.current_animation_name(), "idle");
    }

    #[test]
    fn current_animation_name_is_hover_calm_for_calm_hovered() {
        let mut pet = Pet::new();
        pet.apply_personality(Personality::Calm);
        pet.set_hovered(true);
        assert_eq!(pet.current_animation_name(), "hover-calm");
    }

    #[test]
    fn current_animation_name_is_walk_right_when_walking() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Walk);
        assert_eq!(pet.current_animation_name(), "walk-right");
    }

    #[test]
    fn current_sprite_index_starts_at_idle_frame_zero() {
        let pet = Pet::new();
        assert_eq!(pet.current_sprite_index(), 0);
    }

    #[test]
    fn current_sprite_index_for_walk_starts_at_thirty_two() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Walk);
        assert_eq!(pet.current_sprite_index(), 32);
    }

    #[test]
    fn frame_size_returns_manifest_geometry() {
        let pet = Pet::new();
        assert_eq!(pet.frame_size(), (64, 64));
    }

    #[test]
    fn animation_name_change_does_not_reset_frame_index() {
        // Force into Walk state, advance two full frame_durations (200ms total at 100ms/frame).
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Walk);
        pet.tick(Duration::from_millis(250));
        assert_eq!(pet.frame_index(), 2);
        assert_eq!(pet.current_animation_name(), "walk-right");

        // Engaging hover changes the animation name but must not reset frame_index.
        pet.set_hovered(true);
        assert_eq!(pet.current_animation_name(), "hover-cheerful");
        assert_eq!(pet.frame_index(), 2);

        // The corresponding sprite is hover-cheerful row, column 2 → index 26.
        assert_eq!(pet.current_sprite_index(), 26);
    }

    #[test]
    fn hover_intensity_fractional_value_preserves_rounding_boundary() {
        let mut pet = Pet::new();
        pet.apply_personality(Personality::Cheerful);
        pet.set_hover_intensity(1.3);
        pet.set_hovered(true);
        // base 140 / 1.3 = 107.692..., rounded to 108ms per frame
        pet.tick(Duration::from_millis(107));
        assert_eq!(pet.frame_index(), 0);
        pet.tick(Duration::from_millis(1));
        assert_eq!(pet.frame_index(), 1);
    }
```

These tests reference `Pet::new()`, `apply_personality`, `set_hovered`, `set_hover_intensity`, `force_state_for_test`, `tick`, `frame_index` — all of which already exist on the current `Pet` struct.

- [ ] **Step 2: Run tests, expect failure**

Run: `cargo test --lib pet::tests`
Expected: the new tests fail — "no method named `current_animation_name`", "no method named `current_sprite_index`", "no method named `frame_size`". Pre-existing tests still pass.

- [ ] **Step 3: Modify `Pet` to hold the manifest and resolve animation names**

Open `src/pet.rs`. Apply these edits:

**Edit A — add imports at the top** (after the existing `use` statements):

```rust
use crate::pet::manifest::PetManifest;
use crate::pet::resolver::{lookup_with_fallback, resolve_animation_chain};
```

**Edit B — rename the frame-duration constants.** Find:

```rust
const FRAME_COUNT: usize = 4;
const IDLE_FRAME_MS: u64 = 200;
const WALK_FRAME_MS: u64 = 100;
const SLEEP_FRAME_MS: u64 = 500;
```

Replace with:

```rust
const FRAME_COUNT: usize = 4;
const IDLE_STATE_MS: u64 = 200;
const WALK_STATE_MS: u64 = 100;
const SLEEP_STATE_MS: u64 = 500;
```

Then find the `frame_duration` method body (currently uses `IDLE_FRAME_MS`/`WALK_FRAME_MS`/`SLEEP_FRAME_MS`) and update each reference to the new name.

**Edit C — add fields to the `Pet` struct.** Find the `pub struct Pet { ... }` block. Add these two fields at the end of the field list, just before the closing `}`:

```rust
    manifest: PetManifest,
    current_animation_name: String,
```

**Edit D — initialize fields in `Pet::new_with_seed`.** Find `Self { ... }` inside that function. Add at the end of the initializer:

```rust
            manifest: PetManifest::load_embedded_happy_cappy(),
            current_animation_name: "idle".to_string(),
```

**Edit E — populate `current_animation_name` in `refresh_behavior_mode`.** At the very end of the method (after the `self.animation_group = ...` match), append:

```rust
        // Compute string animation name alongside the legacy enum. The two
        // values are kept in sync; the enum will be removed in a later task.
        let chain = resolve_animation_chain(
            self.behavior_mode,
            self.personality,
            self.expression_index,
            self.action_override.map(|a| a.action()),
        );
        let (name, _) = lookup_with_fallback(&self.manifest, chain);
        self.current_animation_name = name.to_string();
```

**Edit F — add the four public accessors.** Inside `impl Pet { ... }`, add (near the other accessors like `state()`, `direction()`, `frame_index()`, `current_animation_group()`):

```rust
    pub fn current_animation_name(&self) -> &str {
        &self.current_animation_name
    }

    pub fn current_sprite_index(&self) -> u32 {
        let anim = self
            .manifest
            .animations
            .get(&self.current_animation_name)
            .or_else(|| self.manifest.animations.get("idle"))
            .expect("manifest validation guarantees 'idle' exists");
        anim.frames[self.frame_index % anim.frames.len()]
    }

    pub fn frame_size(&self) -> (u32, u32) {
        (self.manifest.frame.width, self.manifest.frame.height)
    }

    pub fn manifest(&self) -> &PetManifest {
        &self.manifest
    }
```

- [ ] **Step 4: Run new tests, expect pass**

Run: `cargo test --lib pet::tests`
Expected: every test (legacy and new) passes. The new accessors return the expected names and sprite indices.

- [ ] **Step 5: Run full suite**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 6: Verify clippy still clean**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/pet.rs
git commit -m "$(cat <<'EOF'
feat(pet): wire manifest + resolver into Pet alongside AnimationGroup

Pet now owns a PetManifest, derives current_animation_name from the
resolver each refresh, and exposes current_sprite_index / frame_size /
manifest() accessors. animation_group stays in place so app.rs and the
SpriteRow path keep compiling; the next task migrates app.rs draw and
hit-test to the new accessors. State-tier frame duration constants are
renamed (IDLE/WALK/SLEEP_STATE_MS) to reflect that timing is a runtime,
not animation, concern.
EOF
)"
```

---

## Task 8: Migrate `app.rs::draw()` and `current_sprite_hit_test()`

**Files:**
- Modify: `src/app.rs:729-749` (draw)
- Modify: `src/app.rs:759-777` (current_sprite_hit_test)

- [ ] **Step 1: Replace `draw()`**

Open `src/app.rs`. Find the `fn draw(&mut self) { ... }` method (around line 729). Replace its body so it reads:

```rust
    fn draw(&mut self) {
        if !self.pet_visible {
            return;
        }

        let (Some(renderer), Some(sprite_sheet)) =
            (self.renderer.as_mut(), self.sprite_sheet.as_ref())
        else {
            return;
        };

        let sprite_index = self.pet.current_sprite_index();
        let flip_x = self.pet.current_animation_name() == "walk-right"
            && self.pet.direction() == Direction::Left;
        let rect = sprite_sheet.frame_rect_by_index(sprite_index);

        if let Err(error) = renderer.draw(sprite_sheet.image(), rect, flip_x) {
            warn!("failed to draw desktop pet frame: {error}");
        }
    }
```

- [ ] **Step 2: Replace `current_sprite_hit_test()`**

Find `fn current_sprite_hit_test(&self, point: Vec2) -> bool { ... }` (around line 759). Replace it:

```rust
    fn current_sprite_hit_test(&self, point: Vec2) -> bool {
        let Some(sprite_sheet) = &self.sprite_sheet else {
            return false;
        };
        let sprite_index = self.pet.current_sprite_index();
        let rect = sprite_sheet.frame_rect_by_index(sprite_index);
        let scale = if self.settings.scale.is_finite() && self.settings.scale > 0.0 {
            self.settings.scale
        } else {
            AppSettings::MIN_SCALE
        };
        let scaled_point = Vec2 {
            x: point.x / scale,
            y: point.y / scale,
        };
        let flip_x = self.pet.current_animation_name() == "walk-right"
            && self.pet.direction() == Direction::Left;
        alpha_hit_test_with_flip(sprite_sheet.image(), rect, scaled_point, flip_x)
    }
```

- [ ] **Step 3: Remove now-unused imports**

In `src/app.rs`, find the top-level `use crate::sprite::{SpriteRow, SpriteSheet};` line. Edit it to:

```rust
use crate::sprite::SpriteSheet;
```

(`SpriteRow` is no longer referenced from `app.rs`. The enum itself still exists in `sprite.rs`; we delete it in Task 10.)

- [ ] **Step 4: Build and run tests**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 5: Clippy clean check**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "$(cat <<'EOF'
refactor(app): switch draw and hit-test to sprite-index API

Both paths now read current_sprite_index() + current_animation_name()
from Pet and call SpriteSheet::frame_rect_by_index. The 'walk-right'
flip check is byte-equivalent to the old AnimationGroup::WalkRight
match. SpriteRow is removed from the imports here; the enum itself
is deleted in a later task.
EOF
)"
```

---

## Task 9: Replace `FRAME_SIZE` / `WINDOW_SIZE` with `frame_size()` accessor

**Files:**
- Modify: `src/app.rs:29-31, 150, 185-186, 215, 356-357, 860-861, 1228-1229`

- [ ] **Step 1: Drop the unused constants and keep `WINDOW_SCALE`**

Open `src/app.rs`. Find lines 29-31:

```rust
pub const FRAME_SIZE: u32 = 64;
pub const WINDOW_SCALE: u32 = 2;
pub const WINDOW_SIZE: u32 = FRAME_SIZE * WINDOW_SCALE;
```

Replace with:

```rust
pub const WINDOW_SCALE: u32 = 2;
```

- [ ] **Step 2: Update window creation (line 150 area)**

Find the `create_window` block where the window attributes are configured. The current line:

```rust
            .with_inner_size(LogicalSize::new(WINDOW_SIZE as f64, WINDOW_SIZE as f64))
```

Replace with:

```rust
            .with_inner_size({
                let (fw, fh) = self.pet.frame_size();
                LogicalSize::new(
                    (fw * WINDOW_SCALE) as f64,
                    (fh * WINDOW_SCALE) as f64,
                )
            })
```

- [ ] **Step 3: Update renderer buffer size (lines 185-186 area)**

The current `PetRenderer::new` call passes `FRAME_SIZE, FRAME_SIZE` as the last two args. Replace those two lines:

```rust
            FRAME_SIZE,
            FRAME_SIZE,
```

with:

```rust
            self.pet.frame_size().0,
            self.pet.frame_size().1,
```

- [ ] **Step 4: Update spritesheet loading (line 215 area)**

The current line uses `SpriteSheet::load(&paths.sprite_sheet, FRAME_SIZE)`. Replace it with:

```rust
        match SpriteSheet::load_with_geometry(&paths.sprite_sheet, &self.pet.manifest().frame) {
```

(Keep the surrounding `match ... { Ok(...) => ..., Err(...) => ... }` arms intact — only the call expression changes.)

- [ ] **Step 5: Update physics size derived from settings.scale (lines 356-357 area)**

Find the block that constructs the pet size from `FRAME_SIZE as f32 * settings.scale`:

```rust
            x: FRAME_SIZE as f32 * settings.scale,
            y: FRAME_SIZE as f32 * settings.scale,
```

Replace with (introduce a local for the frame size just above this struct literal):

```rust
        let (fw, fh) = self.pet.frame_size();
        // ... (then in the struct literal below)
            x: fw as f32 * settings.scale,
            y: fh as f32 * settings.scale,
```

Position the `let (fw, fh) = ...` line directly before the struct literal that uses these fields. If the existing code already constructs the size via a builder, adjust the lines accordingly — the key change is replacing `FRAME_SIZE` with `fw`/`fh`.

- [ ] **Step 6: Update default physics fallback (lines 860-861 area)**

Find:

```rust
            x: WINDOW_SIZE as f32,
            y: WINDOW_SIZE as f32,
```

Replace with:

```rust
            x: (self.pet.frame_size().0 * WINDOW_SCALE) as f32,
            y: (self.pet.frame_size().1 * WINDOW_SCALE) as f32,
```

- [ ] **Step 7: Update test fixture (lines 1228-1229 area)**

In the test module of `src/app.rs`, find:

```rust
                x: FRAME_SIZE as f32 * AppSettings::MAX_SCALE,
                y: FRAME_SIZE as f32 * AppSettings::MAX_SCALE,
```

Replace with (inline the known happy-cappy frame size so the test doesn't depend on app constants):

```rust
                x: 64.0 * AppSettings::MAX_SCALE,
                y: 64.0 * AppSettings::MAX_SCALE,
```

- [ ] **Step 8: Build and run tests**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 9: Clippy clean**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: no warnings.

- [ ] **Step 10: Commit**

```bash
git add src/app.rs
git commit -m "$(cat <<'EOF'
refactor(app): replace FRAME_SIZE/WINDOW_SIZE with PetRuntime::frame_size()

WINDOW_SCALE stays as a display preference. Frame dimensions now flow
from the loaded manifest. For happy-cappy (64x64) every numeric value
is identical to before; the constants are removed so a future
non-square pet doesn't get clipped.
EOF
)"
```

---

## Task 10: Delete `AnimationGroup`, `SpriteRow`, and related dead code

**Files:**
- Modify: `src/pet.rs`
- Modify: `src/sprite.rs`
- Modify: `src/micro_action.rs`

- [ ] **Step 1: Rewrite existing pet tests that still reference `AnimationGroup`**

In `src/pet.rs`'s `#[cfg(test)] mod tests`, rewrite these tests to assert on `current_animation_name()` instead of `current_animation_group()`:

`personality_changes_hover_group`:

```rust
    #[test]
    fn personality_changes_hover_group() {
        let mut pet = Pet::new();

        pet.apply_personality(Personality::Calm);
        pet.set_hovered(true);
        assert_eq!(pet.current_animation_name(), "hover-calm");

        pet.apply_personality(Personality::Cheerful);
        assert_eq!(pet.current_animation_name(), "hover-cheerful");

        pet.apply_personality(Personality::Lively);
        assert_eq!(pet.current_animation_name(), "hover-lively");
    }
```

`dragging_overrides_hover_and_movement`:

```rust
    #[test]
    fn dragging_overrides_hover_and_movement() {
        let mut pet = Pet::new();
        pet.set_hovered(true);
        pet.set_dragging(true);

        let tick = pet.tick(Duration::from_millis(100));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Dragging);
        assert_eq!(pet.current_animation_name(), "drag");
        assert_eq!(tick.speed_x, 0.0);
    }
```

`expression_loop_advances_without_requiring_walk`:

```rust
    #[test]
    fn expression_loop_advances_without_requiring_walk() {
        let mut pet = Pet::new();
        let first = pet.current_animation_name().to_string();
        pet.tick(Duration::from_secs(3));
        let second = pet.current_animation_name().to_string();

        assert_ne!(first, second);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
    }
```

`movement_speed_update_refreshes_behavior_immediately`:

```rust
    #[test]
    fn movement_speed_update_refreshes_behavior_immediately() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Walk);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Walking);

        pet.set_movement_speed_multiplier(0.0);

        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
        assert_eq!(pet.current_animation_name(), "idle");
    }
```

`nap_micro_action_uses_sleepy_group_and_stops_movement`:

```rust
    #[test]
    fn nap_micro_action_uses_sleepy_group_and_stops_movement() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Walk);

        pet.start_micro_action(MicroAction::Nap);
        let tick = pet.tick(Duration::from_millis(16));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Action);
        assert_eq!(pet.current_animation_name(), "sleepy");
        assert_eq!(tick.speed_x, 0.0);
    }
```

`cheer_up_micro_action_uses_happy_group_temporarily`:

```rust
    #[test]
    fn cheer_up_micro_action_uses_happy_group_temporarily() {
        let mut pet = Pet::new();

        pet.start_micro_action(MicroAction::CheerUp);
        pet.tick(Duration::from_secs(7));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Action);
        assert_eq!(pet.current_animation_name(), "happy");

        pet.tick(Duration::from_secs(1));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Walking);
        assert_eq!(pet.current_animation_name(), "walk-right");
    }
```

`hover_overrides_micro_action_until_hover_ends`:

```rust
    #[test]
    fn hover_overrides_micro_action_until_hover_ends() {
        let mut pet = Pet::new();

        pet.start_micro_action(MicroAction::CheerUp);
        pet.set_hovered(true);

        assert_eq!(pet.behavior_mode(), BehaviorMode::Hovered);
        assert_eq!(pet.current_animation_name(), "hover-cheerful");

        pet.set_hovered(false);

        assert_eq!(pet.behavior_mode(), BehaviorMode::Action);
        assert_eq!(pet.current_animation_name(), "happy");
    }
```

- [ ] **Step 2: Delete `AnimationGroup` and supporting bits in `src/pet.rs`**

In `src/pet.rs`:

1. Delete the entire `pub enum AnimationGroup { ... }` declaration.
2. Delete the `animation_group: AnimationGroup` field from `pub struct Pet { ... }`.
3. Delete the line `animation_group: AnimationGroup::Idle,` from `Pet::new_with_seed`'s struct initializer.
4. Inside `refresh_behavior_mode`, delete the entire `self.animation_group = match self.behavior_mode { ... };` block (the new resolver-driven `current_animation_name` block remains).
5. Delete the public method `pub fn current_animation_group(&self) -> AnimationGroup { ... }`.
6. Delete the private method `fn default_expression_group(&self) -> AnimationGroup { ... }`.

- [ ] **Step 3: Delete `SpriteRow` and grid constants in `src/sprite.rs`**

In `src/sprite.rs`:

1. Delete the `pub enum SpriteRow { ... }` declaration.
2. Delete `const EXPECTED_COLUMNS: u32 = 4;` and `const EXPECTED_ROWS: u32 = 10;`.
3. Delete the existing `pub fn load(path: ..., frame_size: u32) -> ...` method on `SpriteSheet`. Rename `load_with_geometry` to `load` (drop the suffix).
4. Delete the existing `pub fn from_image(image: ..., frame_size: u32) -> ...` method. Rename `from_image_with_geometry` to `from_image`.
5. Delete the existing `pub fn frame_rect(&self, row: SpriteRow, frame_index: usize) -> FrameRect`. Rename `frame_rect_by_index` to `frame_rect`.
6. Delete the existing `pub fn frame_count(&self) -> u32` and `pub fn row_count(&self) -> u32` methods.
7. Delete the entire `impl From<AnimationGroup> for SpriteRow { ... }` block.
8. Remove the `use crate::pet::AnimationGroup;` import (no longer needed).
9. Remove the `frame_size: u32` field from `SpriteSheet`; replace it with `geometry: FrameGeometry`. Update `frame_rect` to compute `row = sprite_index / self.geometry.columns` and `col = sprite_index % self.geometry.columns`, using `self.geometry.width`/`height` for the rect dimensions. Update `from_image`/`load` constructors to store `geometry` and validate against it. Add a public `pub fn geometry(&self) -> &FrameGeometry { &self.geometry }` accessor.
10. Delete the existing tests `accepts_ten_rows_and_four_columns_for_happy_cappy`, `rejects_dimensions_that_do_not_match_grid`, `rejects_zero_frame_size`, `rejects_frame_size_that_overflows_expected_dimensions`, `invalid_dimensions_display_includes_actual_expected_and_frame_size`, `returns_frame_rect_for_state_row_and_index`, `returns_frame_rect_for_hover_lively_group`, `frame_rect_wraps_frame_index_at_four_columns`, `maps_animation_group_to_sprite_row` — every test that referenced `SpriteRow`, `EXPECTED_COLUMNS`, `EXPECTED_ROWS`, `frame_count`, or `row_count`. The geometry-based tests added in Task 6 cover the replacement surface.
11. Update the call sites in `app.rs` (line 215 area) that use `SpriteSheet::load_with_geometry(...)` — rename to `SpriteSheet::load(...)`. Likewise update `sprite_sheet.frame_rect_by_index(sprite_index)` calls in `draw()` and `current_sprite_hit_test()` to `sprite_sheet.frame_rect(sprite_index)`.

After this step `SpriteSheet` looks like:

```rust
#[derive(Debug, Clone)]
pub struct SpriteSheet {
    image: RgbaImage,
    geometry: FrameGeometry,
}

impl SpriteSheet {
    pub fn load(
        path: impl AsRef<std::path::Path>,
        geometry: &FrameGeometry,
    ) -> Result<Self, SpriteError> {
        let image = image::open(path)?.into_rgba8();
        Self::from_image(image, geometry)
    }

    pub fn from_image(
        image: RgbaImage,
        geometry: &FrameGeometry,
    ) -> Result<Self, SpriteError> {
        let width = image.width();
        let height = image.height();
        let expected_width = geometry.width.checked_mul(geometry.columns);
        let expected_height = geometry.height.checked_mul(geometry.rows);
        if geometry.width == 0
            || geometry.height == 0
            || geometry.columns == 0
            || geometry.rows == 0
            || Some(width) != expected_width
            || Some(height) != expected_height
        {
            return Err(SpriteError::InvalidDimensions {
                width,
                height,
                expected_width,
                expected_height,
                frame_size: geometry.width.max(geometry.height),
            });
        }
        Ok(Self {
            image,
            geometry: *geometry,
        })
    }

    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    pub fn geometry(&self) -> &FrameGeometry {
        &self.geometry
    }

    pub fn frame_rect(&self, sprite_index: u32) -> FrameRect {
        let row = sprite_index / self.geometry.columns;
        let col = sprite_index % self.geometry.columns;
        FrameRect {
            x: col * self.geometry.width,
            y: row * self.geometry.height,
            width: self.geometry.width,
            height: self.geometry.height,
        }
    }
}
```

- [ ] **Step 4: Delete `ActionOverride::animation_group()` in `src/micro_action.rs`**

Delete the entire method:

```rust
    pub fn animation_group(&self) -> AnimationGroup {
        match self.action {
            MicroAction::Nap => AnimationGroup::Sleepy,
            MicroAction::CheerUp => AnimationGroup::Happy,
        }
    }
```

Also remove the `use crate::pet::AnimationGroup;` import at the top of `src/micro_action.rs`.

Delete the corresponding tests in `src/micro_action.rs` that asserted `animation_group()` returns the expected group:

- `nap_last_30_seconds_and_uses_sleepy_group`: trim it to remove the `assert_eq!(action.animation_group(), ...)` assertion. Keep the duration and `disables_movement` assertions.
- `cheer_up_last_8_seconds_and_uses_happy_group`: same trim.

Resulting tests:

```rust
    #[test]
    fn nap_lasts_30_seconds_and_disables_movement() {
        let action = ActionOverride::new(MicroAction::Nap);

        assert_eq!(action.remaining(), Duration::from_secs(30));
        assert!(action.disables_movement());
    }

    #[test]
    fn cheer_up_lasts_8_seconds_and_keeps_movement() {
        let action = ActionOverride::new(MicroAction::CheerUp);

        assert_eq!(action.remaining(), Duration::from_secs(8));
        assert!(!action.disables_movement());
    }
```

- [ ] **Step 5: Run the full test suite**

Run: `cargo test`
Expected: every remaining test passes. The compiler should report zero references to `AnimationGroup` or `SpriteRow`.

Verify there are no stragglers:

```bash
grep -RIn 'AnimationGroup\|SpriteRow' src/ && echo 'still has refs' || echo 'clean'
```

Expected output: `clean` (the grep prints nothing and exits non-zero).

- [ ] **Step 6: Clippy clean**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/pet.rs src/sprite.rs src/micro_action.rs src/app.rs
git commit -m "$(cat <<'EOF'
refactor(pet): delete AnimationGroup and SpriteRow

AnimationGroup, current_animation_group, default_expression_group, the
animation_group field, SpriteRow, EXPECTED_COLUMNS, EXPECTED_ROWS,
frame_count(), row_count(), and the old frame_rect(SpriteRow, usize)
all disappear in lockstep. ActionOverride loses animation_group() too.
SpriteSheet now stores FrameGeometry directly, and the new sprite-index
APIs drop their _with_geometry / _by_index suffixes.
EOF
)"
```

---

## Task 11: Rename `Pet` to `PetRuntime` and move into `src/pet/runtime.rs`

**Files:**
- Move: `src/pet.rs` → `src/pet/runtime.rs`
- Create: new `src/pet/mod.rs` that re-exports everything
- Modify: `src/app.rs` (rename type references)
- Modify: `src/menu_bar.rs`, `src/settings_window_macos.rs`, `src/interaction.rs` — anywhere else `crate::pet::Pet` appears

- [ ] **Step 1: Find every reference to the `Pet` type**

Run:
```bash
grep -RIn 'crate::pet::Pet\b\| Pet\b' src/ | grep -v 'PetState\|PetTick\|PetRuntime\|PetManifest\|fn Pet\|//.*Pet'
```

Note every file/line and the surrounding context.

Inside `src/pet.rs` itself, the type and impl are declared as `pub struct Pet` / `impl Pet`. Callers (`src/app.rs`) use `Pet::new_with_seed`, `Pet::default`, field `pet: Pet`, parameters `pet: &Pet`, etc.

- [ ] **Step 2: Rename the type inside `src/pet.rs`**

In `src/pet.rs`:

1. Rename `pub struct Pet { ... }` → `pub struct PetRuntime { ... }`.
2. Rename every `impl Pet { ... }` → `impl PetRuntime { ... }`.
3. Rename `impl Default for Pet` → `impl Default for PetRuntime`.
4. In test code, rename `Pet::new()` / `Pet::new_with_seed()` → `PetRuntime::new()` / `PetRuntime::new_with_seed()`.

- [ ] **Step 3: Move the file**

```bash
mkdir -p src/pet  # may already exist from earlier tasks
git mv src/pet.rs src/pet/runtime.rs
```

- [ ] **Step 4: Create the new `src/pet/mod.rs`**

```rust
pub mod manifest;
pub mod resolver;
pub mod runtime;

pub use manifest::{Animation, FrameGeometry, ManifestError, PetManifest};
pub use resolver::{lookup_with_fallback, resolve_animation_chain};
pub use runtime::{
    BehaviorIntent, BehaviorMode, Direction, Personality, PetRuntime, PetState, PetTick,
};
```

The previous `pub mod manifest;` and `pub mod resolver;` declarations that lived inside `src/pet.rs` (now `runtime.rs`) need to be **removed from `runtime.rs`** — `mod.rs` is now the module root, so submodule declarations move there.

In `src/pet/runtime.rs`, delete the lines:

```rust
pub mod manifest;
pub mod resolver;
```

- [ ] **Step 5: Update callers**

In `src/app.rs`:

1. Find `use crate::pet::{Pet, ...};` (or any equivalent) and replace `Pet` with `PetRuntime`.
2. Find every standalone `Pet::` or `Pet,` token in the file and replace with `PetRuntime`.
3. Type annotations like `pet: Pet`, `&Pet`, `Option<Pet>` → `PetRuntime`, `&PetRuntime`, `Option<PetRuntime>`.

Run:
```bash
grep -RIn 'Pet\b' src/app.rs | grep -v 'PetState\|PetTick\|PetRuntime\|PetManifest\|//'
```
Expected: empty (no remaining bare `Pet`).

Repeat for any other file flagged by Step 1. Note that `src/menu_bar.rs`, `src/settings_window_macos.rs`, and `src/interaction.rs` typically reference `Personality` or `AppCommand` only, not `Pet` directly — verify with the grep but expect zero changes there.

- [ ] **Step 6: Build and run tests**

Run: `cargo test`
Expected: every test passes.

- [ ] **Step 7: Clippy clean**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Confirm the module layout matches the spec**

```bash
ls src/pet/
```
Expected: `manifest.rs  mod.rs  resolver.rs  runtime.rs`.

```bash
test ! -e src/pet.rs && echo 'old file gone' || echo 'still present'
```
Expected: `old file gone`.

- [ ] **Step 9: Commit**

```bash
git add src/pet/ src/app.rs
git commit -m "$(cat <<'EOF'
refactor(pet): rename Pet to PetRuntime and move into pet/runtime.rs

Final layout matches the spec: pet/mod.rs is the module root with
re-exports, pet/runtime.rs owns the state machine, pet/manifest.rs the
data model, pet/resolver.rs the chain mapping. No behavior change.
EOF
)"
```

---

## Task 12: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Run formatter**

Run: `cargo fmt`
Expected: no changes (or only the changes you'd accept from the formatter — if anything, commit them as a follow-up `chore(fmt)` commit).

- [ ] **Step 2: Run full test suite**

Run: `cargo test`
Expected: every test passes. Count check:

```bash
cargo test 2>&1 | grep 'test result' | tail
```
Expected: total passing tests ≥ 47 (the spec's exit criterion).

- [ ] **Step 3: Run clippy with denied warnings**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Confirm no `AnimationGroup` or `SpriteRow` references remain**

```bash
grep -RIn 'AnimationGroup\|SpriteRow\|EXPECTED_COLUMNS\|EXPECTED_ROWS\|FRAME_SIZE\|WINDOW_SIZE' src/
```
Expected: empty output.

- [ ] **Step 5: Run the project verify script**

Run: `./scripts/verify.sh`
Expected: succeeds end-to-end (fmt + tests + clippy + release build + bundle assembly + codesign verification if available).

- [ ] **Step 6: Manual smoke test**

Build and launch:

```bash
./scripts/build_app.sh
open "dist/Happy Cappy.app"
```

Confirm by direct observation:

1. Capybara appears at last-stored position (or default).
2. Idle animation loops; after ~5 s the capybara walks.
3. Walking right then idle for ~5 s walks again; after ~2 walk cycles enters sleep for ~12 s.
4. Hover with mouse: animation switches to hover variant; verify across all three personalities via Settings (Calm → calm-tempo bobbing, Cheerful → default, Lively → fast).
5. Drag to a new position and release; close + reopen the app; position is restored.
6. Right-click the pet, choose **Nap**: lies down / sleepy animation; movement stops.
7. Right-click the pet, choose **Cheer Up**: happy animation plays for several seconds, then resumes default behavior.
8. Menu bar `HC` → **Hide Pet** removes it; **Show Pet** brings it back.
9. Settings → **Focus Mode**: clicks pass through to underlying windows.
10. Enter fullscreen in another app (or use Mission Control) on the same display: pet auto-hides; exit fullscreen → reappears.
11. Avoid-text-caret: focus a text field in another app, type; verify the pet steers away.

If anything misbehaves, capture the symptom and revisit the relevant task. The most likely regression sources are Task 7 (resolver wiring) and Task 9 (FRAME_SIZE replacement).

- [ ] **Step 7: Commit any incidental fixes**

If steps 1-6 surface fmt drift or a small behavioral fix, commit them as their own small commit(s) referencing this plan. If everything is clean, no commit is required — the implementation is complete.

---

## Spec coverage summary

- Goal: split `Pet` into `PetManifest` + `PetRuntime` → Tasks 2, 7, 11.
- Goal: replace `AnimationGroup` with string-keyed map → Tasks 4, 7, 10.
- Goal: replace `SpriteRow` with index-based slicing → Tasks 6, 8, 10.
- Goal: bundle capybara JSON via `include_str!` → Tasks 1, 2.
- Goal: byte-for-byte behavior parity → preserved by keeping `frame_duration` formula, cursor reset rules, and flip semantics unchanged (Tasks 7, 8, 10) + verification (Task 12).
- Non-goal: per-frame `ms`, `loop_start`, `fallback`, picker UI, custom-pet disk loading → none of these appear in any task; deferred per spec.
- Risk mitigations from spec: `animation_name_change_does_not_reset_frame_index` test (Task 7 Step 1), `hover_intensity_fractional_value_preserves_rounding_boundary` test (Task 7 Step 1), structured `ManifestError` with CI-runnable bundled-manifest test (Tasks 2 + 3), explicit hit-test migration (Task 8), explicit `FRAME_SIZE`/`WINDOW_SIZE` replacement plan (Task 9).
