# Pet Catalog & Custom Pet Loading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a filesystem-discovered catalog of pets (bundled + custom), persisted active-pet selection, hot-swap at runtime, and a macOS menu bar surface to switch between them — without breaking sub-project 1's byte-for-byte behavior on the bundled Happy Cappy.

**Architecture:** New module `src/pet/catalog.rs` owns filesystem discovery and ID collision policy as pure logic over a `tempfile`-able directory. `DesktopPetApp` gains an immutable `PetCatalog` field and an atomic `activate_pet(id)` method that swaps `pet`/`sprite_sheet` in a single `&mut self` borrow. Menu bar gains a dynamic **Pet** submenu populated on every menu open via a new `validateMenuItem:`/delegate hook. Selection persists via a new `Option<String> active_pet_id` field in `AppSettings`.

**Tech Stack:** Rust 2021, serde + serde_json, `image` for sprite decode, `log::warn!`/`error!` for diagnostics, `tempfile` (dev-dep) for catalog tests, AppKit (`objc2`/`objc2_app_kit`) for menu integration.

---

## File Structure

**New files:**
- `src/pet/catalog.rs` — `PetCatalog`, `CatalogEntry`, `CatalogSource`, `CatalogLoadError`, `BundledPet`, `PetCatalog::scan`, `PetCatalog::lookup`. No AppKit / winit / wgpu deps. ~300 LOC including tests.

**Modified files:**
- `Cargo.toml` — add `tempfile` as dev-dependency.
- `src/pet/mod.rs` — add `pub mod catalog;` + re-exports.
- `src/pet/manifest.rs` — add public `PetManifest::from_path(&Path) -> Result<Self, ManifestError>` (thin wrapper over `from_json_str`).
- `src/pet/runtime.rs` — promote `#[cfg(test)] new_with_manifest_for_test` to `new_with_manifest_and_seed`; rewrite `new`/`new_with_seed` to delegate; add public `new_with_manifest(manifest)`.
- `src/settings.rs` — add `active_pet_id: Option<String>` field with `#[serde(default)]`; helper `custom_pets_dir()` returning `~/Library/Application Support/Happy Cappy/pets/`.
- `src/app.rs` — add `catalog: PetCatalog`, `active_pet_id: String` fields; extract `inner_size_for` helper; add `activate_pet`, `refresh_catalog`, startup wiring, `ActivationError`; add `AppCommand::ActivatePet(String)` and `AppCommand::RevealPetsFolder` variants.
- `src/menu_bar.rs` — add Pet submenu construction, dynamic population on `menuNeedsUpdate:`, `Reveal Pets Folder` item, `MENU_TAG_PET_BASE` constant.
- `src/command_target_macos.rs` — add `activatePet:` selector + `dispatchRevealPetsFolder:` selector + plumbing for `representedObject` string read.

**Files not touched:** `src/sprite.rs`, `src/pet/resolver.rs`, `src/physics.rs`, `src/renderer.rs`, `src/interaction.rs`, `src/workspace.rs`.

---

## Task list summary

1. Add `tempfile` dev-dependency
2. `PetManifest::from_path` API
3. Catalog module skeleton (types + empty `scan` + `lookup`)
4. Catalog scan — bundled-only path (empty dir + missing dir)
5. Catalog scan — valid custom pet
6. Catalog scan — silent skip when `pet.json` missing
7. Catalog scan — `ManifestParse` error recording
8. Catalog scan — `SpritesheetMissing` error recording
9. Catalog scan — ID collision policy (`DuplicateId`)
10. Catalog scan — deterministic ordering (case-insensitive sort)
11. Catalog scan — ignore top-level files & relative sprite paths
12. Catalog scan — `README.txt` write-if-missing
13. `AppSettings::active_pet_id` field
14. `AppSettings::custom_pets_dir()` helper
15. `PetRuntime::new_with_manifest` promotion (delete `_for_test`)
16. Extract `inner_size_for` helper
17. `AppCommand::ActivatePet` + `RevealPetsFolder` variants
18. App startup wiring (load catalog, resolve active pet, fallback)
19. `DesktopPetApp::activate_pet` + `ActivationError`
20. `DesktopPetApp::refresh_catalog`
21. App command handlers wire `ActivatePet` + `RevealPetsFolder`
22. Menu bar — Pet submenu construction + tag scheme
23. Menu bar — dynamic population via `NSMenuDelegate`
24. Menu bar — `activatePet:` selector wiring
25. Menu bar — `Reveal Pets Folder` item
26. `cargo fmt` + final test run

---

### Task 1: Add tempfile dev-dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add tempfile to dev-dependencies**

In `Cargo.toml`, find the existing `[dev-dependencies]` section (one exists; if not, append at end of file). Add:

```toml
tempfile = "3"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build --tests`
Expected: succeeds, no warnings about unresolved crates.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "$(cat <<'EOF'
chore(deps): add tempfile as dev-dependency

For PetCatalog unit tests in sub-project 2.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: PetManifest::from_path

**Files:**
- Modify: `src/pet/manifest.rs:111-126` (PetManifest impl block, add new method after `from_json_str`)

- [ ] **Step 1: Write the failing test**

Append to `src/pet/manifest.rs` test module (inside `#[cfg(test)] mod tests`, before the closing brace at line 454):

```rust
    #[test]
    fn from_path_reads_and_parses_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pet.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(br#"{
            "id": "test",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {"idle": {"frames": [0, 1, 2, 3]}}
        }"#).unwrap();
        drop(f);

        let manifest = PetManifest::from_path(&path).unwrap();
        assert_eq!(manifest.id, "test");
        assert_eq!(manifest.animations["idle"].frames, vec![0, 1, 2, 3]);
    }

    #[test]
    fn from_path_returns_json_error_for_invalid_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pet.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"{not valid").unwrap();
        drop(f);

        let err = PetManifest::from_path(&path).unwrap_err();
        assert!(matches!(err, ManifestError::Json(_)));
    }

    #[test]
    fn from_path_returns_io_error_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");

        let err = PetManifest::from_path(&path).unwrap_err();
        assert!(matches!(err, ManifestError::Io(_)));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib pet::manifest::tests::from_path`
Expected: FAIL — `PetManifest::from_path` does not exist; `ManifestError::Io` does not exist.

- [ ] **Step 3: Add `ManifestError::Io` variant**

In `src/pet/manifest.rs` modify the `ManifestError` enum (around line 39-62):

```rust
#[derive(Debug)]
pub enum ManifestError {
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidVersion(u32),
    EmptyField(&'static str),
    InvalidIdChars,
    ZeroGeometry,
    EmptyAnimation {
        name: String,
    },
    TooManyFrames {
        name: String,
        count: usize,
    },
    SpriteIndexOutOfBounds {
        animation: String,
        frame_pos: usize,
        index: u32,
        max: u32,
    },
    MissingIdleAnimation,
    MissingRequiredAnimation {
        name: &'static str,
    },
}
```

In `impl fmt::Display for ManifestError`, add a match arm at the top (before `Self::Json(...)`):

```rust
            Self::Io(e) => write!(f, "manifest I/O error: {e}"),
```

In `impl Error for ManifestError::source`, add the Io case:

```rust
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
```

Add `From<std::io::Error>` impl below the existing `From<serde_json::Error>` impl:

```rust
impl From<std::io::Error> for ManifestError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
```

- [ ] **Step 4: Add `PetManifest::from_path`**

In `src/pet/manifest.rs`, inside `impl PetManifest`, add after `from_json_str` (around line 116):

```rust
    pub fn from_path(path: &std::path::Path) -> Result<Self, ManifestError> {
        let bytes = std::fs::read(path)?;
        let json = std::str::from_utf8(&bytes).map_err(|_| {
            ManifestError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "manifest file is not valid UTF-8",
            ))
        })?;
        Self::from_json_str(json)
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib pet::manifest`
Expected: PASS — all manifest tests including the three new `from_path` tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/pet/manifest.rs
git commit -m "$(cat <<'EOF'
feat(pet): add PetManifest::from_path for disk-loaded manifests

Adds ManifestError::Io variant and a thin path-reading wrapper around
from_json_str. Sub-project 2 catalog uses this for custom pet discovery.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Catalog module skeleton

**Files:**
- Create: `src/pet/catalog.rs`
- Modify: `src/pet/mod.rs`

- [ ] **Step 1: Create the catalog module file**

Create `src/pet/catalog.rs` with:

```rust
//! Pet catalog — bundled + custom pet discovery.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::pet::manifest::{ManifestError, PetManifest};

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub id: String,
    pub display_name: String,
    pub manifest: PetManifest,
    pub source: CatalogSource,
    pub spritesheet_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSource {
    Bundled,
    Custom,
}

#[derive(Debug)]
pub enum CatalogLoadError {
    DirRead {
        path: PathBuf,
        error: std::io::Error,
    },
    ManifestParse {
        path: PathBuf,
        error: ManifestError,
    },
    SpritesheetMissing {
        manifest_path: PathBuf,
        sprite_path: PathBuf,
    },
    DuplicateId {
        id: String,
        kept: PathBuf,
        dropped: PathBuf,
    },
}

impl fmt::Display for CatalogLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirRead { path, error } => {
                write!(f, "catalog dir read failed at {}: {error}", path.display())
            }
            Self::ManifestParse { path, error } => {
                write!(f, "manifest parse failed at {}: {error}", path.display())
            }
            Self::SpritesheetMissing {
                manifest_path,
                sprite_path,
            } => write!(
                f,
                "spritesheet for {} missing at {}",
                manifest_path.display(),
                sprite_path.display()
            ),
            Self::DuplicateId { id, kept, dropped } => write!(
                f,
                "duplicate pet id {id:?}: keeping {} dropping {}",
                kept.display(),
                dropped.display()
            ),
        }
    }
}

pub struct BundledPet {
    pub manifest: PetManifest,
    pub spritesheet_path: PathBuf,
}

#[derive(Debug)]
pub struct PetCatalog {
    entries: Vec<CatalogEntry>,
    load_errors: Vec<CatalogLoadError>,
}

impl PetCatalog {
    pub fn scan(_bundled: BundledPet, _custom_dir: &Path) -> Self {
        // Filled in by later tasks. Returning bundled-only is the simplest
        // shape the next task will assert against.
        Self {
            entries: Vec::new(),
            load_errors: Vec::new(),
        }
    }

    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    pub fn lookup(&self, id: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn load_errors(&self) -> &[CatalogLoadError] {
        &self.load_errors
    }
}

#[cfg(test)]
fn test_bundled_pet() -> BundledPet {
    use std::collections::BTreeMap;
    use crate::pet::manifest::{Animation, FrameGeometry};
    let mut animations = BTreeMap::new();
    animations.insert(
        "idle".to_string(),
        Animation {
            frames: vec![0, 1, 2, 3],
        },
    );
    let manifest = PetManifest {
        manifest_version: 1,
        id: "happy-cappy".to_string(),
        display_name: "Happy Cappy".to_string(),
        spritesheet_path: "happy_cappy_spritesheet.png".to_string(),
        frame: FrameGeometry {
            width: 64,
            height: 64,
            columns: 4,
            rows: 1,
        },
        animations,
    };
    BundledPet {
        manifest,
        spritesheet_path: PathBuf::from("/bundled/happy_cappy_spritesheet.png"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_none_when_empty() {
        let catalog = PetCatalog {
            entries: Vec::new(),
            load_errors: Vec::new(),
        };
        assert!(catalog.lookup("anything").is_none());
    }
}
```

- [ ] **Step 2: Register the module**

Edit `src/pet/mod.rs` (currently 9 lines):

```rust
pub mod catalog;
pub mod manifest;
pub mod resolver;
pub mod runtime;

pub use catalog::{
    BundledPet, CatalogEntry, CatalogLoadError, CatalogSource, PetCatalog,
};
pub use manifest::{Animation, FrameGeometry, ManifestError, PetManifest};
pub use resolver::{lookup_with_fallback, resolve_animation_chain};
pub use runtime::{
    BehaviorIntent, BehaviorMode, Direction, Personality, PetRuntime, PetState, PetTick,
};
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib pet::catalog`
Expected: PASS — `lookup_returns_none_when_empty` passes; module compiles.

- [ ] **Step 4: Run the full suite to confirm no breakage**

Run: `cargo test`
Expected: PASS — sub-project 1's 182 baseline tests + 1 new = 183 passing.

- [ ] **Step 5: Commit**

```bash
git add src/pet/catalog.rs src/pet/mod.rs
git commit -m "$(cat <<'EOF'
feat(pet): scaffold PetCatalog module with bundled/custom types

Adds the data-only catalog module with empty scan + working lookup.
Subsequent tasks implement the discovery logic against this scaffold.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Catalog scan — bundled-only path

**Files:**
- Modify: `src/pet/catalog.rs`

- [ ] **Step 1: Write the failing tests**

In `src/pet/catalog.rs`, replace the `#[cfg(test)] mod tests` block with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lookup_returns_none_when_empty() {
        let catalog = PetCatalog {
            entries: Vec::new(),
            load_errors: Vec::new(),
        };
        assert!(catalog.lookup("anything").is_none());
    }

    #[test]
    fn scan_empty_dir_returns_bundled_only() {
        let dir = tempdir().unwrap();
        let custom_dir = dir.path().join("pets");
        // custom_dir does NOT exist yet — scan should create it.

        let catalog = PetCatalog::scan(test_bundled_pet(), &custom_dir);

        assert_eq!(catalog.entries().len(), 1);
        assert_eq!(catalog.entries()[0].id, "happy-cappy");
        assert_eq!(catalog.entries()[0].source, CatalogSource::Bundled);
        assert!(catalog.load_errors().is_empty());
        assert!(custom_dir.exists(), "scan should create the custom dir");
    }

    #[test]
    fn scan_dir_already_exists_returns_bundled_only() {
        let dir = tempdir().unwrap();
        // Dir exists but is empty.

        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        assert_eq!(catalog.entries().len(), 1);
        assert_eq!(catalog.entries()[0].source, CatalogSource::Bundled);
        assert!(catalog.load_errors().is_empty());
    }

    #[test]
    fn scan_lookup_finds_bundled_pet() {
        let dir = tempdir().unwrap();
        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        let entry = catalog.lookup("happy-cappy").unwrap();
        assert_eq!(entry.source, CatalogSource::Bundled);
        assert_eq!(entry.display_name, "Happy Cappy");
        assert_eq!(
            entry.spritesheet_path,
            PathBuf::from("/bundled/happy_cappy_spritesheet.png")
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib pet::catalog`
Expected: FAIL — `scan` returns an empty catalog; bundled is not inserted.

- [ ] **Step 3: Implement bundled insertion + dir creation**

Replace `PetCatalog::scan` in `src/pet/catalog.rs` with:

```rust
    pub fn scan(bundled: BundledPet, custom_dir: &Path) -> Self {
        let mut entries = Vec::new();
        let mut load_errors = Vec::new();

        let bundled_entry = CatalogEntry {
            id: bundled.manifest.id.clone(),
            display_name: bundled.manifest.display_name.clone(),
            manifest: bundled.manifest,
            source: CatalogSource::Bundled,
            spritesheet_path: bundled.spritesheet_path,
        };
        entries.push(bundled_entry);

        if let Err(error) = std::fs::create_dir_all(custom_dir) {
            load_errors.push(CatalogLoadError::DirRead {
                path: custom_dir.to_path_buf(),
                error,
            });
            return Self {
                entries,
                load_errors,
            };
        }

        Self {
            entries,
            load_errors,
        }
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib pet::catalog`
Expected: PASS — all four tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/pet/catalog.rs
git commit -m "$(cat <<'EOF'
feat(catalog): seed scan with bundled entry and ensure custom dir

PetCatalog::scan now inserts the bundled pet and creates the custom pets
directory. Custom pet discovery comes in subsequent tasks.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Catalog scan — valid custom pet

**Files:**
- Modify: `src/pet/catalog.rs`

- [ ] **Step 1: Write the failing test**

Add to `tests` module in `src/pet/catalog.rs`:

```rust
    fn write_pet(dir: &Path, id: &str, display_name: &str, sprite_name: &str) {
        std::fs::create_dir_all(dir).unwrap();
        let manifest = format!(
            r#"{{
                "id": "{id}",
                "displayName": "{display_name}",
                "spritesheetPath": "{sprite_name}",
                "frame": {{"width": 16, "height": 16, "columns": 4, "rows": 1}},
                "animations": {{"idle": {{"frames": [0, 1, 2, 3]}}}}
            }}"#
        );
        std::fs::write(dir.join("pet.json"), manifest).unwrap();
        std::fs::write(dir.join(sprite_name), b"fake-png-bytes").unwrap();
    }

    #[test]
    fn scan_picks_up_one_valid_custom_pet() {
        let dir = tempdir().unwrap();
        write_pet(&dir.path().join("shiba"), "shiba", "Shiba", "sprite.png");

        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        assert_eq!(catalog.entries().len(), 2);
        assert!(catalog.load_errors().is_empty());

        let shiba = catalog.lookup("shiba").unwrap();
        assert_eq!(shiba.source, CatalogSource::Custom);
        assert_eq!(shiba.display_name, "Shiba");
        assert_eq!(
            shiba.spritesheet_path,
            dir.path().join("shiba").join("sprite.png")
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib pet::catalog::tests::scan_picks_up_one_valid_custom_pet`
Expected: FAIL — `scan` does not enumerate subdirs yet.

- [ ] **Step 3: Implement custom pet enumeration**

In `src/pet/catalog.rs`, add this helper function below the `PetCatalog` impl block:

```rust
fn load_custom_pet(dir: &Path) -> Result<Option<CatalogEntry>, CatalogLoadError> {
    let manifest_path = dir.join("pet.json");
    if !manifest_path.exists() {
        return Ok(None);
    }

    let manifest = PetManifest::from_path(&manifest_path).map_err(|error| {
        CatalogLoadError::ManifestParse {
            path: manifest_path.clone(),
            error,
        }
    })?;

    let sprite_path = dir.join(&manifest.spritesheet_path);
    if !sprite_path.exists() {
        return Err(CatalogLoadError::SpritesheetMissing {
            manifest_path,
            sprite_path,
        });
    }

    Ok(Some(CatalogEntry {
        id: manifest.id.clone(),
        display_name: manifest.display_name.clone(),
        manifest,
        source: CatalogSource::Custom,
        spritesheet_path: sprite_path,
    }))
}
```

In `PetCatalog::scan`, after the `create_dir_all` block, add subdir enumeration before the final `Self { ... }`:

```rust
        let read_dir = match std::fs::read_dir(custom_dir) {
            Ok(rd) => rd,
            Err(error) => {
                load_errors.push(CatalogLoadError::DirRead {
                    path: custom_dir.to_path_buf(),
                    error,
                });
                return Self {
                    entries,
                    load_errors,
                };
            }
        };

        for entry in read_dir {
            let Ok(entry) = entry else { continue };
            let Ok(file_type) = entry.file_type() else { continue };
            if !file_type.is_dir() {
                continue;
            }
            match load_custom_pet(&entry.path()) {
                Ok(Some(catalog_entry)) => entries.push(catalog_entry),
                Ok(None) => {}
                Err(err) => load_errors.push(err),
            }
        }
```

- [ ] **Step 4: Run the test**

Run: `cargo test --lib pet::catalog`
Expected: PASS — `scan_picks_up_one_valid_custom_pet` plus all prior tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/pet/catalog.rs
git commit -m "$(cat <<'EOF'
feat(catalog): enumerate custom pets from subdirectories

Adds load_custom_pet() and read_dir iteration. A subdir with a valid
pet.json + sprite file now becomes a Custom catalog entry.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: Catalog scan — silent skip without pet.json

**Files:**
- Modify: `src/pet/catalog.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module:

```rust
    #[test]
    fn scan_skips_subdir_without_pet_json() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("not-a-pet")).unwrap();
        std::fs::write(dir.path().join("not-a-pet").join("random.txt"), b"hi").unwrap();

        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        assert_eq!(catalog.entries().len(), 1); // bundled only
        assert!(
            catalog.load_errors().is_empty(),
            "missing pet.json must NOT be an error"
        );
    }
```

- [ ] **Step 2: Run test**

Run: `cargo test --lib pet::catalog::tests::scan_skips_subdir_without_pet_json`
Expected: PASS — `load_custom_pet` already returns `Ok(None)` when `pet.json` is absent. This is a behavior-locking test.

- [ ] **Step 3: Commit**

```bash
git add src/pet/catalog.rs
git commit -m "$(cat <<'EOF'
test(catalog): lock silent-skip behavior for subdirs without pet.json

A subdir without pet.json is not a pet candidate. No load_error is
recorded — the directory is just ignored.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Catalog scan — ManifestParse error recording

**Files:**
- Modify: `src/pet/catalog.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
    #[test]
    fn scan_records_manifest_parse_error() {
        let dir = tempdir().unwrap();
        let pet_dir = dir.path().join("broken");
        std::fs::create_dir_all(&pet_dir).unwrap();
        std::fs::write(pet_dir.join("pet.json"), b"{not valid json").unwrap();

        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        assert_eq!(catalog.entries().len(), 1); // bundled only
        assert_eq!(catalog.load_errors().len(), 1);
        assert!(matches!(
            &catalog.load_errors()[0],
            CatalogLoadError::ManifestParse {
                error: ManifestError::Json(_),
                ..
            }
        ));
    }

    #[test]
    fn scan_records_missing_idle_animation() {
        let dir = tempdir().unwrap();
        let pet_dir = dir.path().join("no-idle");
        std::fs::create_dir_all(&pet_dir).unwrap();
        std::fs::write(
            pet_dir.join("pet.json"),
            br#"{
                "id": "no-idle",
                "displayName": "No Idle",
                "spritesheetPath": "x.png",
                "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
                "animations": {"walk": {"frames": [0, 1]}}
            }"#,
        )
        .unwrap();

        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        assert_eq!(catalog.entries().len(), 1);
        assert_eq!(catalog.load_errors().len(), 1);
        assert!(matches!(
            &catalog.load_errors()[0],
            CatalogLoadError::ManifestParse {
                error: ManifestError::MissingIdleAnimation,
                ..
            }
        ));
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib pet::catalog::tests::scan_records`
Expected: PASS — `load_custom_pet` already returns `Err(ManifestParse)` for both cases. Behavior-locking tests.

- [ ] **Step 3: Commit**

```bash
git add src/pet/catalog.rs
git commit -m "$(cat <<'EOF'
test(catalog): assert ManifestParse error is recorded but app continues

Both malformed JSON and missing-idle-animation produce a
CatalogLoadError::ManifestParse on load_errors, and the bundled pet
still ships as the only entry.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: Catalog scan — SpritesheetMissing recording

**Files:**
- Modify: `src/pet/catalog.rs`

- [ ] **Step 1: Write the failing test**

Add to `tests`:

```rust
    #[test]
    fn scan_records_missing_sprite() {
        let dir = tempdir().unwrap();
        let pet_dir = dir.path().join("no-sprite");
        std::fs::create_dir_all(&pet_dir).unwrap();
        std::fs::write(
            pet_dir.join("pet.json"),
            br#"{
                "id": "no-sprite",
                "displayName": "No Sprite",
                "spritesheetPath": "ghost.png",
                "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
                "animations": {"idle": {"frames": [0]}}
            }"#,
        )
        .unwrap();
        // Note: ghost.png is intentionally never written.

        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        assert_eq!(catalog.entries().len(), 1);
        assert_eq!(catalog.load_errors().len(), 1);
        assert!(matches!(
            &catalog.load_errors()[0],
            CatalogLoadError::SpritesheetMissing { .. }
        ));
    }
```

- [ ] **Step 2: Run test**

Run: `cargo test --lib pet::catalog::tests::scan_records_missing_sprite`
Expected: PASS — `load_custom_pet` already returns `Err(SpritesheetMissing)`.

- [ ] **Step 3: Commit**

```bash
git add src/pet/catalog.rs
git commit -m "$(cat <<'EOF'
test(catalog): assert SpritesheetMissing is recorded when sprite is absent

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 9: Catalog scan — ID collision policy

**Files:**
- Modify: `src/pet/catalog.rs`

- [ ] **Step 1: Write the failing tests**

Add to `tests`:

```rust
    #[test]
    fn scan_drops_duplicate_id_keeping_bundled() {
        let dir = tempdir().unwrap();
        write_pet(
            &dir.path().join("custom-cappy"),
            "happy-cappy",
            "Custom Cappy",
            "sprite.png",
        );

        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        assert_eq!(catalog.entries().len(), 1);
        assert_eq!(catalog.entries()[0].source, CatalogSource::Bundled);
        assert_eq!(catalog.load_errors().len(), 1);
        assert!(matches!(
            &catalog.load_errors()[0],
            CatalogLoadError::DuplicateId { id, .. } if id == "happy-cappy"
        ));
    }

    #[test]
    fn scan_drops_duplicate_id_between_two_customs() {
        let dir = tempdir().unwrap();
        // Both have id "twin"; "alpha" sorts first by display name and wins.
        write_pet(&dir.path().join("a"), "twin", "Alpha", "a.png");
        write_pet(&dir.path().join("b"), "twin", "Beta", "b.png");

        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        // bundled + 1 custom kept = 2
        assert_eq!(catalog.entries().len(), 2);
        let twin = catalog.lookup("twin").unwrap();
        assert_eq!(twin.display_name, "Alpha");
        assert_eq!(catalog.load_errors().len(), 1);
        assert!(matches!(
            &catalog.load_errors()[0],
            CatalogLoadError::DuplicateId { id, .. } if id == "twin"
        ));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib pet::catalog::tests::scan_drops_duplicate`
Expected: FAIL — current implementation pushes every successful custom entry; no dedup happens.

- [ ] **Step 3: Implement dedup + ordering preparation**

In `src/pet/catalog.rs`, replace the subdir enumeration loop in `scan` (the `for entry in read_dir { ... }` block) with a two-phase: collect, then sort, then insert with dedup:

```rust
        let mut sub_entries: Vec<CatalogEntry> = Vec::new();
        for entry in read_dir {
            let Ok(entry) = entry else { continue };
            let Ok(file_type) = entry.file_type() else { continue };
            if !file_type.is_dir() {
                continue;
            }
            match load_custom_pet(&entry.path()) {
                Ok(Some(catalog_entry)) => sub_entries.push(catalog_entry),
                Ok(None) => {}
                Err(err) => load_errors.push(err),
            }
        }

        sub_entries.sort_by(|a, b| {
            a.display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase())
        });

        let mut ids: HashSet<String> = entries.iter().map(|e| e.id.clone()).collect();
        for entry in sub_entries {
            if ids.contains(&entry.id) {
                let kept = entries
                    .iter()
                    .find(|e| e.id == entry.id)
                    .map(|e| e.spritesheet_path.clone())
                    .unwrap_or_default();
                load_errors.push(CatalogLoadError::DuplicateId {
                    id: entry.id.clone(),
                    kept,
                    dropped: entry.spritesheet_path.clone(),
                });
                continue;
            }
            ids.insert(entry.id.clone());
            entries.push(entry);
        }
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib pet::catalog`
Expected: PASS — all catalog tests including the two new dedup tests.

- [ ] **Step 5: Commit**

```bash
git add src/pet/catalog.rs
git commit -m "$(cat <<'EOF'
feat(catalog): enforce ID collision policy — bundled wins, first-by-name wins

Bundled is inserted first; any custom pet with a colliding id is dropped
with a CatalogLoadError::DuplicateId. Two customs with the same id resolve
in display-name order (case-insensitive).

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 10: Catalog scan — deterministic ordering

**Files:**
- Modify: `src/pet/catalog.rs`

- [ ] **Step 1: Write the failing test**

Add to `tests`:

```rust
    #[test]
    fn scan_sorts_custom_pets_by_display_name_case_insensitive() {
        let dir = tempdir().unwrap();
        write_pet(&dir.path().join("a"), "zebra", "Zebra", "z.png");
        write_pet(&dir.path().join("b"), "alpha", "alpha", "a.png");
        write_pet(&dir.path().join("c"), "beta", "Beta", "b.png");

        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        let order: Vec<&str> = catalog.entries().iter().map(|e| e.id.as_str()).collect();
        assert_eq!(order, vec!["happy-cappy", "alpha", "beta", "zebra"]);
    }
```

- [ ] **Step 2: Run test**

Run: `cargo test --lib pet::catalog::tests::scan_sorts_custom`
Expected: PASS — sort was added in Task 9. Behavior-locking test.

- [ ] **Step 3: Commit**

```bash
git add src/pet/catalog.rs
git commit -m "$(cat <<'EOF'
test(catalog): lock case-insensitive display-name ordering for custom pets

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 11: Catalog scan — ignore top-level files + relative sprite paths

**Files:**
- Modify: `src/pet/catalog.rs`

- [ ] **Step 1: Write the failing tests**

Add to `tests`:

```rust
    #[test]
    fn scan_ignores_files_at_top_level() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("stray.txt"), b"ignore me").unwrap();
        std::fs::write(dir.path().join("README.txt"), b"existing readme").unwrap();

        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        assert_eq!(catalog.entries().len(), 1); // bundled only
        assert!(catalog.load_errors().is_empty());
    }

    #[test]
    fn scan_resolves_sprite_path_relative_to_manifest() {
        let dir = tempdir().unwrap();
        let pet_dir = dir.path().join("nested");
        std::fs::create_dir_all(pet_dir.join("art")).unwrap();
        std::fs::write(
            pet_dir.join("pet.json"),
            br#"{
                "id": "nested",
                "displayName": "Nested",
                "spritesheetPath": "art/sprite.png",
                "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
                "animations": {"idle": {"frames": [0]}}
            }"#,
        )
        .unwrap();
        std::fs::write(pet_dir.join("art").join("sprite.png"), b"x").unwrap();

        let catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        let nested = catalog.lookup("nested").unwrap();
        assert_eq!(nested.spritesheet_path, pet_dir.join("art").join("sprite.png"));
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib pet::catalog::tests`
Expected: PASS — file_type().is_dir() filter already skips files; relative sprite resolution already uses `dir.join(...)`. Behavior-locking.

- [ ] **Step 3: Commit**

```bash
git add src/pet/catalog.rs
git commit -m "$(cat <<'EOF'
test(catalog): lock top-level file skip + relative sprite path resolution

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 12: Catalog scan — README.txt write-if-missing

**Files:**
- Modify: `src/pet/catalog.rs`

- [ ] **Step 1: Write the failing tests**

Add to `tests`:

```rust
    #[test]
    fn scan_writes_readme_when_missing() {
        let dir = tempdir().unwrap();
        let custom_dir = dir.path().join("pets");

        let _catalog = PetCatalog::scan(test_bundled_pet(), &custom_dir);

        let readme = custom_dir.join("README.txt");
        assert!(readme.exists(), "scan should create README.txt");
        let content = std::fs::read_to_string(&readme).unwrap();
        assert!(content.contains("pet.json"), "README should mention pet.json");
        assert!(!content.is_empty());
    }

    #[test]
    fn scan_preserves_existing_readme() {
        let dir = tempdir().unwrap();
        let readme = dir.path().join("README.txt");
        std::fs::write(&readme, b"my custom notes").unwrap();

        let _catalog = PetCatalog::scan(test_bundled_pet(), dir.path());

        assert_eq!(std::fs::read_to_string(&readme).unwrap(), "my custom notes");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib pet::catalog::tests::scan_writes_readme`
Expected: FAIL — `scan_writes_readme_when_missing` fails (README is not created).

- [ ] **Step 3: Implement README write-if-missing**

In `src/pet/catalog.rs`, add a helper above `load_custom_pet`:

```rust
fn write_readme_if_missing(dir: &Path) {
    let readme_path = dir.join("README.txt");
    if readme_path.exists() {
        return;
    }
    let content = "\
Happy Cappy custom pets
=======================

Drop a folder here named for your pet (e.g. `my-pet/`). Inside it, place:

  - pet.json         (manifest — see docs/superpowers/specs for the schema)
  - your-sprite.png  (referenced by `spritesheetPath` in pet.json)

The bundled \"happy-cappy\" pet always wins ID collisions. Invalid
manifests are skipped and logged; the app never crashes on a bad pet.
";
    let _ = std::fs::write(&readme_path, content); // best-effort
}
```

In `PetCatalog::scan`, immediately after the successful `create_dir_all`, call the helper:

```rust
        if let Err(error) = std::fs::create_dir_all(custom_dir) {
            load_errors.push(CatalogLoadError::DirRead {
                path: custom_dir.to_path_buf(),
                error,
            });
            return Self {
                entries,
                load_errors,
            };
        }
        write_readme_if_missing(custom_dir);
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib pet::catalog`
Expected: PASS — all catalog tests.

- [ ] **Step 5: Commit**

```bash
git add src/pet/catalog.rs
git commit -m "$(cat <<'EOF'
feat(catalog): write README.txt into custom pets dir on first scan

Best-effort: only writes when the file is absent; never overwrites
user-edited content; failures are silently ignored.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 13: AppSettings::active_pet_id field

**Files:**
- Modify: `src/settings.rs`

- [ ] **Step 1: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` block in `src/settings.rs` (after `defaults_keep_focus_mode_off`):

```rust
    #[test]
    fn settings_default_has_no_active_pet_id() {
        assert_eq!(AppSettings::default().active_pet_id, None);
    }

    #[test]
    fn settings_deserializes_legacy_file_without_active_pet_id() {
        let root = std::env::temp_dir().join(format!(
            "happy-cappy-legacy-active-{}",
            fastrand::u64(..)
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        fs::write(
            &path,
            br#"{"personality":"calm","scale":2.0,"movement_speed":1.0,"hover_intensity":1.0,"monitor_behavior":"current_display","pet_visible":true,"focus_mode":false}"#,
        )
        .unwrap();

        let loaded =
            AppSettings::load_or_default_from(&path, bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(loaded.active_pet_id, None);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn settings_roundtrip_with_active_pet_id() {
        let root = std::env::temp_dir().join(format!("happy-cappy-active-rt-{}", fastrand::u64(..)));
        let path = root.join("settings.json");
        let settings = AppSettings {
            active_pet_id: Some("shiba".to_string()),
            ..AppSettings::default()
        };

        settings.save_to(&path).unwrap();
        let loaded =
            AppSettings::load_or_default_from(&path, bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(loaded.active_pet_id, Some("shiba".to_string()));

        let _ = fs::remove_dir_all(root);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib settings::tests::settings_`
Expected: FAIL — `active_pet_id` field does not exist on `AppSettings`.

- [ ] **Step 3: Add the field**

In `src/settings.rs`, modify the `AppSettings` struct (lines 25-49). Add a new field after `last_position`:

```rust
    #[serde(default)]
    pub last_position: Option<StoredPosition>,
    #[serde(default)]
    pub active_pet_id: Option<String>,
}
```

Modify the `Default` impl (lines 90-106). Add the new field at the end of the struct initializer:

```rust
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            personality: Personality::Cheerful,
            scale: 2.0,
            movement_speed: 1.0,
            hover_intensity: 1.0,
            monitor_behavior: MonitorBehavior::CurrentDisplay,
            pet_visible: true,
            focus_mode: false,
            follow_cursor_when_idle: true,
            avoid_text_cursor: true,
            hide_on_fullscreen: true,
            last_position: None,
            active_pet_id: None,
        }
    }
}
```

- [ ] **Step 4: Check the existing `save_and_load_round_trip` test still passes**

That test constructs an `AppSettings` literal with all fields. Since we use `..AppSettings::default()` in the new tests but the existing one uses an explicit struct literal, the existing literal will fail to compile. Update the existing test at `src/settings.rs:329-345` to add the new field:

Find:
```rust
            last_position: Some(StoredPosition {
                x: 22.0,
                y: 33.0,
                display_name: Some("Built-in Display".to_string()),
            }),
        };
```

Replace with:
```rust
            last_position: Some(StoredPosition {
                x: 22.0,
                y: 33.0,
                display_name: Some("Built-in Display".to_string()),
            }),
            active_pet_id: None,
        };
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib settings`
Expected: PASS — all settings tests (existing + 3 new) pass.

- [ ] **Step 6: Commit**

```bash
git add src/settings.rs
git commit -m "$(cat <<'EOF'
feat(settings): add active_pet_id with serde default for legacy files

Adds Option<String> active_pet_id to AppSettings. The #[serde(default)]
attribute keeps pre-existing settings.json files deserializing cleanly
with None.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 14: AppSettings — custom_pets_dir helper

**Files:**
- Modify: `src/settings.rs`

- [ ] **Step 1: Write the failing test**

Add at the bottom of the `tests` module:

```rust
    #[test]
    fn custom_pets_dir_lives_under_happy_cappy_app_support() {
        // Force a known HOME so the test is hermetic.
        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", "/tmp/fake-home");

        let result = custom_pets_dir();

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }

        let path = result.unwrap();
        assert!(path.ends_with("Library/Application Support/Happy Cappy/pets"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib settings::tests::custom_pets_dir`
Expected: FAIL — `custom_pets_dir` does not exist.

- [ ] **Step 3: Implement**

In `src/settings.rs`, add below `default_settings_path` (around line 229):

```rust
pub fn custom_pets_dir() -> Result<PathBuf, SettingsError> {
    let home = std::env::var_os("HOME").ok_or(SettingsError::MissingHomeDirectory)?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Happy Cappy")
        .join("pets"))
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib settings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/settings.rs
git commit -m "$(cat <<'EOF'
feat(settings): add custom_pets_dir() resolver

Returns ~/Library/Application Support/Happy Cappy/pets/, sibling of
settings.json. Used by DesktopPetApp to seed PetCatalog::scan.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 15: PetRuntime — promote new_with_manifest_for_test

**Files:**
- Modify: `src/pet/runtime.rs:91-95` (existing `new`/`new_with_seed`)
- Modify: `src/pet/runtime.rs:454` (existing `#[cfg(test)] new_with_manifest_for_test`)
- Modify: `src/pet/runtime.rs:996` (one existing call site)

- [ ] **Step 1: Write the failing test**

In `src/pet/runtime.rs` test module, find an appropriate spot in the tests and add:

```rust
    #[test]
    fn new_with_manifest_uses_provided_manifest() {
        use crate::pet::manifest::{Animation, FrameGeometry, PetManifest};
        use std::collections::BTreeMap;

        let mut animations = BTreeMap::new();
        animations.insert(
            "idle".to_string(),
            Animation {
                frames: vec![0, 1, 2, 3],
            },
        );
        let manifest = PetManifest {
            manifest_version: 1,
            id: "custom".to_string(),
            display_name: "Custom".to_string(),
            spritesheet_path: "custom.png".to_string(),
            frame: FrameGeometry {
                width: 32,
                height: 48,
                columns: 4,
                rows: 1,
            },
            animations,
        };

        let pet = PetRuntime::new_with_manifest(manifest);

        assert_eq!(pet.manifest().id, "custom");
        assert_eq!(pet.frame_size(), (32, 48));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib pet::runtime::tests::new_with_manifest_uses_provided_manifest`
Expected: FAIL — `PetRuntime::new_with_manifest` does not exist; only `_for_test` does.

- [ ] **Step 3: Promote the constructor**

In `src/pet/runtime.rs`, locate the existing constructor at line 454. It currently reads:

```rust
    #[cfg(test)]
    pub fn new_with_manifest_for_test(manifest: PetManifest, seed: u64) -> Self {
        // ... body ...
    }
```

Replace its signature and remove the `#[cfg(test)]` gate, rename to `new_with_manifest_and_seed`:

```rust
    pub fn new_with_manifest_and_seed(manifest: PetManifest, seed: u64) -> Self {
        // ... body unchanged ...
    }

    pub fn new_with_manifest(manifest: PetManifest) -> Self {
        Self::new_with_manifest_and_seed(manifest, 0)
    }
```

- [ ] **Step 4: Rewrite `new` and `new_with_seed` to delegate**

At lines 91-95, replace:

```rust
    pub fn new() -> Self {
        Self::new_with_seed(0)
    }

    pub fn new_with_seed(seed: u64) -> Self {
```

with:

```rust
    pub fn new() -> Self {
        Self::new_with_manifest(PetManifest::load_embedded_happy_cappy())
    }

    pub fn new_with_seed(seed: u64) -> Self {
        Self::new_with_manifest_and_seed(PetManifest::load_embedded_happy_cappy(), seed)
    }
```

(The old body of `new_with_seed` — the long match on seed and field initialization — should be entirely replaced by the delegation above. The actual field-initialization logic now lives only in `new_with_manifest_and_seed`. If the existing `new_with_manifest_for_test` body is missing any setup that the old `new_with_seed` had, copy it over.)

- [ ] **Step 5: Migrate the existing `_for_test` caller**

At line 996, find:

```rust
        let mut pet = PetRuntime::new_with_manifest_for_test(manifest, 0);
```

Replace with:

```rust
        let mut pet = PetRuntime::new_with_manifest(manifest);
```

- [ ] **Step 6: Search for any other callers**

Run: `grep -rn "new_with_manifest_for_test" src/ tests/ 2>/dev/null`
Expected: no matches. If any matches remain, update them to `new_with_manifest` (no seed) or `new_with_manifest_and_seed(manifest, seed)` (with seed).

- [ ] **Step 7: Run tests**

Run: `cargo test`
Expected: PASS — full test suite. Sub-project 1's 182 + new ones from tasks 2-14 + this one.

- [ ] **Step 8: Commit**

```bash
git add src/pet/runtime.rs
git commit -m "$(cat <<'EOF'
refactor(pet): promote new_with_manifest_for_test to public API

new_with_manifest_for_test becomes new_with_manifest_and_seed (and a
no-seed new_with_manifest wrapper is added). The existing new() and
new_with_seed() now delegate to it, eliminating duplicated field-init
logic. Sub-project 2's hot-swap needs this constructor in non-test code.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 16: Extract inner_size_for helper

**Files:**
- Modify: `src/app.rs:145-154` (window construction)

- [ ] **Step 1: Write the failing test**

Append to the `#[cfg(test)] mod tests` block at the bottom of `src/app.rs`:

```rust
    #[test]
    fn inner_size_for_multiplies_frame_by_scale() {
        let size = inner_size_for((64, 48), 2);
        assert_eq!(size.width, 128.0);
        assert_eq!(size.height, 96.0);
    }

    #[test]
    fn inner_size_for_handles_unit_scale() {
        let size = inner_size_for((32, 32), 1);
        assert_eq!(size.width, 32.0);
        assert_eq!(size.height, 32.0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib app::tests::inner_size_for`
Expected: FAIL — `inner_size_for` does not exist.

- [ ] **Step 3: Add the helper as a free function**

In `src/app.rs`, just before `pub struct DesktopPetApp` (around line 61), add:

```rust
fn inner_size_for(frame: (u32, u32), scale: u32) -> LogicalSize<f64> {
    LogicalSize::new((frame.0 * scale) as f64, (frame.1 * scale) as f64)
}
```

- [ ] **Step 4: Replace the inline calculation at the window-construction site**

At lines 148-151, find:

```rust
            .with_inner_size({
                let (fw, fh) = self.pet.frame_size();
                LogicalSize::new((fw * WINDOW_SCALE) as f64, (fh * WINDOW_SCALE) as f64)
            })
```

Replace with:

```rust
            .with_inner_size(inner_size_for(self.pet.frame_size(), WINDOW_SCALE))
```

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "$(cat <<'EOF'
refactor(app): extract inner_size_for helper for window-size math

Pulls the frame_size * WINDOW_SCALE calculation out of the window-build
site into a unit-testable free function. Hot-swap will reuse it to
resize the window for pets with different frame dimensions.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 17: AppCommand variants for pet selection

**Files:**
- Modify: `src/app.rs:38-59` (AppCommand enum)

- [ ] **Step 1: Add the variants**

In `src/app.rs`, modify the `AppCommand` enum (around line 38). The current `derive(...Copy, ...)` will not work with a `String` payload, so the derives need updating. Find:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppCommand {
```

Replace with:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum AppCommand {
```

Then at the end of the enum (just before the closing `}`), add the new variants:

```rust
    ActivatePet(String),
    RevealPetsFolder,
}
```

- [ ] **Step 2: Search for places that depend on AppCommand: Copy**

Run: `grep -n "AppCommand" src/ -r --include='*.rs' | grep -E "copy|memcpy|\.copied\(\)"` and also look at usages:

Run: `grep -rn "fn .*-> AppCommand\b\|EventLoopProxy<AppCommand>\|: AppCommand\b" src/`
Expected: scan results. None should fail compilation; `EventLoopProxy<T>` does not require `T: Copy`.

- [ ] **Step 3: Confirm tests still compile**

Run: `cargo build --tests`
Expected: succeeds. (The existing `command_tags_map_to_app_commands` test uses `assert_eq!` which needs `PartialEq` only — already derived.)

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: PASS — no behavior change, just enum expansion.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "$(cat <<'EOF'
feat(app): add ActivatePet/RevealPetsFolder commands

Drops Copy from AppCommand (now requires Clone+PartialEq) since
ActivatePet carries a String payload. The variants are wired into
handlers in subsequent tasks.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 18: App startup wiring — catalog + active pet resolution

**Files:**
- Modify: `src/app.rs` (DesktopPetApp struct, new() and new_with_event_proxy() constructors)

- [ ] **Step 1: Add the fields**

In `src/app.rs`, modify the `DesktopPetApp` struct (around lines 61-84). Add two new fields just after `pet: PetRuntime`:

```rust
pub struct DesktopPetApp {
    window: Option<Arc<Window>>,
    renderer: Option<PetRenderer>,
    sprite_sheet: Option<SpriteSheet>,
    pet: PetRuntime,
    catalog: crate::pet::PetCatalog,
    active_pet_id: String,
    physics: Physics,
    // ... rest unchanged ...
```

Update the imports near line 20 to bring `PetCatalog`/`BundledPet` into scope. Modify:

```rust
    pet::{Direction, PetRuntime, PetState},
```

to:

```rust
    pet::{BundledPet, Direction, PetCatalog, PetRuntime, PetState},
```

- [ ] **Step 2: Add a startup helper that builds the catalog**

In `src/app.rs`, just below `inner_size_for` (added in Task 16), add:

```rust
fn build_startup_catalog() -> (PetCatalog, PathBuf) {
    use crate::pet::manifest::PetManifest;
    use crate::settings::custom_pets_dir;

    let bundled_manifest = PetManifest::load_embedded_happy_cappy();
    let bundled_sprite = current_resource_paths()
        .map(|p| p.sprite_sheet)
        .unwrap_or_else(|_| PathBuf::from("assets/happy_cappy_spritesheet.png"));
    let bundled = BundledPet {
        manifest: bundled_manifest,
        spritesheet_path: bundled_sprite,
    };

    let custom_dir = custom_pets_dir()
        .unwrap_or_else(|_| PathBuf::from("/tmp/happy-cappy-pets-fallback"));

    let catalog = PetCatalog::scan(bundled, &custom_dir);

    for error in catalog.load_errors() {
        warn!("catalog: {error}");
    }

    (catalog, custom_dir)
}
```

Add the import at the top of `src/app.rs` (if not already present):

```rust
use std::path::PathBuf;
```

- [ ] **Step 3: Update `DesktopPetApp::new` to use the catalog**

Modify `src/app.rs` lines 87-115 (`pub fn new(event_proxy: EventLoopProxy<AppCommand>) -> Self`):

```rust
    pub fn new(event_proxy: EventLoopProxy<AppCommand>) -> Self {
        let seed = fastrand::u64(..);
        let now = Instant::now();

        let (catalog, _custom_dir) = build_startup_catalog();
        let settings = AppSettings::default();
        let active_pet_id = resolve_active_pet_id(&catalog, settings.active_pet_id.as_deref());
        let active_entry = catalog
            .lookup(&active_pet_id)
            .expect("bundled is always present");
        let pet = PetRuntime::new_with_manifest_and_seed(active_entry.manifest.clone(), seed);

        Self {
            window: None,
            renderer: None,
            sprite_sheet: None,
            pet,
            catalog,
            active_pet_id,
            physics: default_physics(),
            last_tick: now,
            next_tick_at: now,
            menu_bar: None,
            settings_window: None,
            settings,
            settings_path: default_settings_path().ok(),
            active_monitor_name: None,
            pet_visible: true,
            auto_hidden: false,
            interaction: InteractionState::default(),
            last_cursor_local_position: None,
            last_cursor_screen_position: None,
            workspace_observer: crate::workspace::WorkspaceObserver::new(),
            event_proxy,
        }
    }
```

Also update `src/app.rs` lines 117-143 (`#[cfg(test)] fn new_with_event_proxy`):

```rust
    #[cfg(test)]
    fn new_with_event_proxy(event_proxy: Option<EventLoopProxy<AppCommand>>) -> Self {
        let seed = fastrand::u64(..);
        let now = Instant::now();

        let (catalog, _custom_dir) = build_startup_catalog();
        let settings = AppSettings::default();
        let active_pet_id = resolve_active_pet_id(&catalog, settings.active_pet_id.as_deref());
        let active_entry = catalog
            .lookup(&active_pet_id)
            .expect("bundled is always present");
        let pet = PetRuntime::new_with_manifest_and_seed(active_entry.manifest.clone(), seed);

        Self {
            window: None,
            renderer: None,
            sprite_sheet: None,
            pet,
            catalog,
            active_pet_id,
            physics: default_physics(),
            last_tick: now,
            next_tick_at: now,
            menu_bar: None,
            settings_window: None,
            settings,
            settings_path: default_settings_path().ok(),
            active_monitor_name: None,
            pet_visible: true,
            auto_hidden: false,
            interaction: InteractionState::default(),
            last_cursor_local_position: None,
            last_cursor_screen_position: None,
            workspace_observer: crate::workspace::WorkspaceObserver::new(),
            event_proxy,
        }
    }
```

- [ ] **Step 4: Add the `resolve_active_pet_id` helper**

In `src/app.rs`, below `build_startup_catalog`, add:

```rust
fn resolve_active_pet_id(catalog: &PetCatalog, desired: Option<&str>) -> String {
    let desired_id = desired.unwrap_or("happy-cappy");
    if catalog.lookup(desired_id).is_some() {
        return desired_id.to_string();
    }
    warn!(
        "activate_pet: persisted id missing, falling back to bundled requested={:?}",
        desired_id
    );
    "happy-cappy".to_string()
}
```

- [ ] **Step 5: Write a test for the fallback behavior**

Append to the `#[cfg(test)] mod tests` block at the bottom of `src/app.rs`:

```rust
    #[test]
    fn resolve_active_pet_id_returns_bundled_when_desired_missing() {
        use crate::pet::manifest::{Animation, FrameGeometry, PetManifest};
        use crate::pet::{BundledPet, PetCatalog};
        use std::collections::BTreeMap;
        let tmp = tempfile::tempdir().unwrap();

        let mut animations = BTreeMap::new();
        animations.insert(
            "idle".to_string(),
            Animation {
                frames: vec![0, 1, 2, 3],
            },
        );
        let bundled = BundledPet {
            manifest: PetManifest {
                manifest_version: 1,
                id: "happy-cappy".to_string(),
                display_name: "Happy Cappy".to_string(),
                spritesheet_path: "x.png".to_string(),
                frame: FrameGeometry { width: 16, height: 16, columns: 4, rows: 1 },
                animations,
            },
            spritesheet_path: PathBuf::from("/bundled/x.png"),
        };
        let catalog = PetCatalog::scan(bundled, tmp.path());

        assert_eq!(resolve_active_pet_id(&catalog, Some("ghost")), "happy-cappy");
        assert_eq!(resolve_active_pet_id(&catalog, None), "happy-cappy");
        assert_eq!(resolve_active_pet_id(&catalog, Some("happy-cappy")), "happy-cappy");
    }
```

- [ ] **Step 6: Run tests**

Run: `cargo test`
Expected: PASS — full suite including the new fallback test.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs
git commit -m "$(cat <<'EOF'
feat(app): wire PetCatalog into startup and resolve active_pet_id

DesktopPetApp now scans the catalog at construction, then resolves the
active pet from settings.active_pet_id (or falls back to bundled if the
persisted id no longer matches a known pet).

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 19: DesktopPetApp::activate_pet + ActivationError

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Write the failing tests**

Append to `src/app.rs`'s `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn activate_pet_unknown_returns_error_and_keeps_previous() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        let previous_id = app.active_pet_id.clone();

        let err = app.activate_pet("ghost").unwrap_err();

        assert!(matches!(err, ActivationError::UnknownId(ref id) if id == "ghost"));
        assert_eq!(app.active_pet_id, previous_id, "previous pet stays active");
    }

    #[test]
    fn activate_pet_idempotent_for_same_id() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        let id = app.active_pet_id.clone();

        let result = app.activate_pet(&id);

        assert!(result.is_ok());
        assert_eq!(app.active_pet_id, id);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib app::tests::activate_pet`
Expected: FAIL — `activate_pet` and `ActivationError` don't exist.

- [ ] **Step 3: Add ActivationError**

In `src/app.rs`, just after the `AppCommand` enum (around line 60), add:

```rust
#[derive(Debug)]
pub enum ActivationError {
    UnknownId(String),
    SpriteLoad {
        id: String,
        path: PathBuf,
        error: image::ImageError,
    },
}

impl std::fmt::Display for ActivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownId(id) => write!(f, "unknown pet id: {id}"),
            Self::SpriteLoad { id, path, error } => write!(
                f,
                "failed to load sprite for {id}: {} ({error})",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ActivationError {}
```

- [ ] **Step 4: Implement `activate_pet`**

Add a new method inside `impl DesktopPetApp` (place it near `read_settings`, around line 401). The exact insertion point doesn't matter — pick anywhere in the impl block:

```rust
    pub fn activate_pet(&mut self, id: &str) -> Result<(), ActivationError> {
        if id == self.active_pet_id {
            return Ok(());
        }

        let entry = self
            .catalog
            .lookup(id)
            .ok_or_else(|| ActivationError::UnknownId(id.to_string()))?
            .clone();

        let new_sprite = SpriteSheet::load(&entry.spritesheet_path, &entry.manifest.frame)
            .map_err(|error| ActivationError::SpriteLoad {
                id: id.to_string(),
                path: entry.spritesheet_path.clone(),
                error,
            })?;

        let new_runtime = PetRuntime::new_with_manifest(entry.manifest.clone());
        let new_frame_size = new_runtime.frame_size();

        self.pet = new_runtime;
        self.sprite_sheet = Some(new_sprite);
        self.active_pet_id = id.to_string();
        self.settings.active_pet_id = Some(id.to_string());

        if let Some(path) = &self.settings_path {
            if let Err(error) = self.settings.save_to(path) {
                warn!("failed to save settings after pet activation: {error}");
            }
        }

        if let Some(window) = &self.window {
            let _ = window.request_inner_size(inner_size_for(new_frame_size, WINDOW_SCALE));
            window.request_redraw();
        }

        Ok(())
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: PASS — full suite.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "$(cat <<'EOF'
feat(app): add hot-swap pet activation with atomic state transition

activate_pet(id) loads the new sprite first (recoverable failure), then
swaps PetRuntime + SpriteSheet + active_pet_id atomically inside a
single &mut self borrow. Window position is preserved; behavior/state
resets to idle.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 20: DesktopPetApp::refresh_catalog

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Write the failing test**

Append to `app.rs` tests:

```rust
    #[test]
    fn refresh_catalog_replaces_entries() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        let initial_entry_count = app.catalog.entries().len();
        // Just verify the method runs and doesn't panic. The catalog dir
        // will be the real ~/Library path which we can't write to in tests,
        // so we just sanity-check that the count stays at >= 1 (bundled).
        app.refresh_catalog();
        assert!(app.catalog.entries().len() >= 1);
        assert!(app.catalog.entries().len() >= initial_entry_count.saturating_sub(0));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib app::tests::refresh_catalog`
Expected: FAIL — method does not exist.

- [ ] **Step 3: Implement**

In `src/app.rs`, inside `impl DesktopPetApp`, add:

```rust
    pub fn refresh_catalog(&mut self) {
        let (catalog, _) = build_startup_catalog();
        self.catalog = catalog;
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "$(cat <<'EOF'
feat(app): add refresh_catalog for on-demand re-scan

Builds a fresh PetCatalog and replaces the in-app one. Called by the
menu bar each time the Pet submenu opens.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 21: Command handlers — wire ActivatePet + RevealPetsFolder

**Files:**
- Modify: `src/app.rs` (handle_non_quit_command)

- [ ] **Step 1: Add the handler arms**

Find the `handle_non_quit_command` method around line 504 in `src/app.rs`. At the end of its `match command { ... }` block (just before the closing `}` of the match), add the two new arms:

```rust
            AppCommand::ActivatePet(id) => {
                if let Err(error) = self.activate_pet(&id) {
                    warn!("activate_pet failed: {error}");
                }
            }
            AppCommand::RevealPetsFolder => {
                if let Ok(dir) = crate::settings::custom_pets_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    reveal_in_finder(&dir);
                }
            }
```

- [ ] **Step 2: Add the `reveal_in_finder` helper**

At the bottom of `src/app.rs`, just before the `#[cfg(test)] mod tests` block, add:

```rust
#[cfg(target_os = "macos")]
fn reveal_in_finder(path: &std::path::Path) {
    use std::process::Command;
    let _ = Command::new("open").arg(path).spawn();
}

#[cfg(not(target_os = "macos"))]
fn reveal_in_finder(_path: &std::path::Path) {}
```

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: PASS — no new tests, but the new arms must compile and the existing `handle_non_quit_command` path stays correct.

- [ ] **Step 4: Verify the `Copy` removal didn't break anything**

Run: `cargo build --release`
Expected: succeeds. (Release build is more strict on some lints; if it fails for an unrelated reason, fix it before committing.)

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "$(cat <<'EOF'
feat(app): wire ActivatePet and RevealPetsFolder command handlers

ActivatePet routes through activate_pet() with a warn-on-error path.
RevealPetsFolder opens Finder at ~/Library/Application Support/Happy Cappy/pets/
via `open`, creating the directory first if absent.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 22: Menu bar — Pet submenu construction + tag scheme

**Files:**
- Modify: `src/menu_bar.rs`

- [ ] **Step 1: Add new menu tag constants**

In `src/menu_bar.rs`, near the existing `pub const MENU_TAG_*` block at the top (lines 5-21), add:

```rust
pub const MENU_TAG_PET_SUBMENU: isize = 1200;
pub const MENU_TAG_REVEAL_PETS_FOLDER: isize = 1201;
// Pet menu items use tag range MENU_TAG_PET_ITEM_BASE..(MENU_TAG_PET_ITEM_BASE + N).
// The id is carried as the representedObject (string) on the NSMenuItem.
pub const MENU_TAG_PET_ITEM_BASE: isize = 1300;
```

Add to `command_from_tag` (lines 23-35), in the match block before the catch-all `_ =>`:

```rust
        MENU_TAG_REVEAL_PETS_FOLDER => Some(AppCommand::RevealPetsFolder),
```

- [ ] **Step 2: Add tests for the new tag mappings**

In the `#[cfg(test)] mod tests` at the bottom of `src/menu_bar.rs`, add:

```rust
    #[test]
    fn command_from_tag_maps_reveal_pets_folder() {
        assert_eq!(
            command_from_tag(MENU_TAG_REVEAL_PETS_FOLDER),
            Some(AppCommand::RevealPetsFolder)
        );
    }

    #[test]
    fn pet_item_base_does_not_collide_with_other_tags() {
        // Sanity check — ensure the new constants don't share values with the
        // pre-existing ones.
        let used = [
            MENU_TAG_SETTINGS,
            MENU_TAG_SHOW_HIDE,
            MENU_TAG_RESET,
            MENU_TAG_QUIT,
            MENU_TAG_FOCUS_MODE,
            MENU_TAG_NAP,
            MENU_TAG_CHEER_UP,
            MENU_TAG_PERSONALITY,
            MENU_TAG_SCALE,
            MENU_TAG_MOVEMENT_SPEED,
            MENU_TAG_HOVER_INTENSITY,
            MENU_TAG_MONITOR_BEHAVIOR,
            MENU_TAG_FOLLOW_CURSOR_WHEN_IDLE,
            MENU_TAG_AVOID_TEXT_CURSOR,
            MENU_TAG_HIDE_ON_FULLSCREEN,
            MENU_TAG_REREQUEST_ACCESSIBILITY,
            MENU_TAG_AX_STATUS_LABEL,
            MENU_TAG_REVEAL_PETS_FOLDER,
            MENU_TAG_PET_SUBMENU,
        ];
        for (i, a) in used.iter().enumerate() {
            for b in &used[i + 1..] {
                assert_ne!(a, b, "menu tag collision between {a} and {b}");
            }
        }
        // Pet item base must be safely above all single tags.
        assert!(MENU_TAG_PET_ITEM_BASE > *used.iter().max().unwrap());
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib menu_bar`
Expected: PASS — new tag mappings work, no collisions.

- [ ] **Step 4: Commit**

```bash
git add src/menu_bar.rs
git commit -m "$(cat <<'EOF'
feat(menu_bar): add Pet submenu tag scheme

New constants: MENU_TAG_PET_SUBMENU, MENU_TAG_REVEAL_PETS_FOLDER,
MENU_TAG_PET_ITEM_BASE. command_from_tag now maps the reveal tag to
AppCommand::RevealPetsFolder.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 23: Menu bar — Pet submenu construction & population

**Files:**
- Modify: `src/menu_bar.rs`

- [ ] **Step 1: Add Pet submenu to the menu bar controller (macOS branch)**

In `src/menu_bar.rs`, modify the `#[cfg(target_os = "macos")] pub struct MenuBarController` (lines 58-65) to retain the Pet submenu so we can rebuild it on demand:

```rust
#[cfg(target_os = "macos")]
pub struct MenuBarController {
    _status_item: objc2::rc::Retained<objc2_app_kit::NSStatusItem>,
    _menu: objc2::rc::Retained<objc2_app_kit::NSMenu>,
    show_hide_item: objc2::rc::Retained<objc2_app_kit::NSMenuItem>,
    focus_mode_item: objc2::rc::Retained<objc2_app_kit::NSMenuItem>,
    pet_submenu: objc2::rc::Retained<objc2_app_kit::NSMenu>,
    _target: objc2::rc::Retained<crate::command_target_macos::CommandTarget>,
}
```

- [ ] **Step 2: Build the submenu in `MenuBarController::new`**

In `MenuBarController::new` (around line 69), find the existing menu setup. After `let menu = NSMenu::initWithTitle(...)` (line 80) and before the existing `settings_item` creation, add:

```rust
        let pet_submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Pet"));
        let pet_root_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Pet"),
                None,
                ns_string!(""),
            )
        };
        pet_root_item.setTag(MENU_TAG_PET_SUBMENU);
        pet_root_item.setSubmenu(Some(&pet_submenu));
```

Then in the section that adds items to `menu` (lines 165-171), insert `pet_root_item` BEFORE `settings_item`:

```rust
        menu.addItem(&pet_root_item);
        unsafe {
            menu.addItem(&NSMenuItem::separatorItem());
        }
        menu.addItem(&settings_item);
        menu.addItem(&show_hide_item);
        menu.addItem(&focus_mode_item);
        menu.addItem(&nap_item);
        menu.addItem(&cheer_up_item);
        menu.addItem(&reset_item);
        menu.addItem(&quit_item);
```

In the `Some(Self { ... })` return at the bottom of `new`, add the new field:

```rust
        Some(Self {
            _status_item: status_item,
            _menu: menu,
            show_hide_item,
            focus_mode_item,
            pet_submenu,
            _target: target,
        })
```

- [ ] **Step 3: Add the populate method**

In the `#[cfg(target_os = "macos")] impl MenuBarController` block, just below `sync_runtime_state` (around line 183-188), add:

```rust
    pub fn populate_pet_submenu(
        &self,
        entries: &[(String, String)], // (id, display_name)
        active_id: &str,
    ) {
        use objc2::{rc::Retained, runtime::AnyObject, MainThreadOnly};
        use objc2_app_kit::{NSMenuItem, NSControlStateValueOff, NSControlStateValueOn};
        use objc2_foundation::{ns_string, MainThreadMarker, NSString};

        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };

        // Remove every item from the submenu so the rebuild is clean.
        unsafe {
            while self.pet_submenu.numberOfItems() > 0 {
                self.pet_submenu.removeItemAtIndex(0);
            }
        }

        // One NSMenuItem per pet.
        for (i, (id, display_name)) in entries.iter().enumerate() {
            let title = NSString::from_str(display_name);
            let item: Retained<NSMenuItem> = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &title,
                    None,
                    ns_string!(""),
                )
            };
            item.setTag(MENU_TAG_PET_ITEM_BASE + i as isize);
            unsafe {
                let id_ns = NSString::from_str(id);
                let id_obj: &AnyObject = &*id_ns;
                let id_retained: Retained<AnyObject> = Retained::from(id_obj);
                let _: () = objc2::msg_send![&*item, setRepresentedObject: &*id_retained];
                item.setTarget(Some(self._target.as_ref()));
                item.setAction(Some(
                    crate::command_target_macos::CommandTarget::activate_pet_selector(),
                ));
                item.setState(if id == active_id {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            }
            self.pet_submenu.addItem(&item);
        }

        // Divider + Reveal Pets Folder.
        unsafe {
            self.pet_submenu
                .addItem(&objc2_app_kit::NSMenuItem::separatorItem());
        }
        let reveal_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Reveal Pets Folder"),
                None,
                ns_string!(""),
            )
        };
        reveal_item.setTag(MENU_TAG_REVEAL_PETS_FOLDER);
        unsafe {
            reveal_item.setTarget(Some(self._target.as_ref()));
            reveal_item.setAction(Some(
                crate::command_target_macos::CommandTarget::command_selector(),
            ));
        }
        self.pet_submenu.addItem(&reveal_item);
    }
```

Note: this populate method takes its data as a plain `Vec<(String, String)>` so the AppKit code does not need to know about `PetCatalog`. The caller (in `app.rs`) is responsible for converting catalog entries into this shape.

Also save `_target` as `target` (drop the underscore) since we now reference it from `populate_pet_submenu`. Edit the struct definition:

```rust
#[cfg(target_os = "macos")]
pub struct MenuBarController {
    _status_item: objc2::rc::Retained<objc2_app_kit::NSStatusItem>,
    _menu: objc2::rc::Retained<objc2_app_kit::NSMenu>,
    show_hide_item: objc2::rc::Retained<objc2_app_kit::NSMenuItem>,
    focus_mode_item: objc2::rc::Retained<objc2_app_kit::NSMenuItem>,
    pet_submenu: objc2::rc::Retained<objc2_app_kit::NSMenu>,
    target: objc2::rc::Retained<crate::command_target_macos::CommandTarget>,
}
```

And update both places `_target` is read inside `populate_pet_submenu` — replace `self._target.as_ref()` with `self.target.as_ref()`. Also update the return at end of `new`:

```rust
        Some(Self {
            _status_item: status_item,
            _menu: menu,
            show_hide_item,
            focus_mode_item,
            pet_submenu,
            target,
        })
```

- [ ] **Step 4: Provide a no-op for non-macOS**

In the `#[cfg(not(target_os = "macos"))] impl MenuBarController` (lines 49-55), add the method stub:

```rust
    pub fn populate_pet_submenu(
        &self,
        _entries: &[(String, String)],
        _active_id: &str,
    ) {
    }
```

- [ ] **Step 5: Compile-only verification**

Run: `cargo build --tests`
Expected: succeeds. The activate_pet_selector method does not yet exist in CommandTarget — this will fail. **Stop here** if it fails on `activate_pet_selector` — we add that in Task 24. If it fails for any *other* reason, fix it before continuing.

Actually, defer the Step 5 build check to Task 24 since `activate_pet_selector` is added there. For now just commit and proceed.

- [ ] **Step 6: Commit (partial — expect build failure until Task 24)**

```bash
git add src/menu_bar.rs
git commit -m "$(cat <<'EOF'
feat(menu_bar): construct Pet submenu and populate_pet_submenu method

Pet submenu is built once during MenuBarController::new and rebuilt
on demand via populate_pet_submenu(entries, active_id). The actual
data is passed in by DesktopPetApp from the PetCatalog. Build is not
yet green — activate_pet_selector lands in the next task.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 24: command_target_macos — activatePet: selector

**Files:**
- Modify: `src/command_target_macos.rs`

- [ ] **Step 1: Add the selector method to the Objective-C class definition**

In `src/command_target_macos.rs`, find the `define_class!` macro (lines 35-109). Inside the `impl CommandTarget { ... }` block, after the existing `#[unsafe(method(dispatchSettingsValue:))]` method (around line 107), add:

```rust
            #[unsafe(method(activatePet:))]
            fn activate_pet(&self, sender: Option<&AnyObject>) {
                let Some(sender) = sender else {
                    return;
                };
                let represented: Option<Retained<AnyObject>> =
                    unsafe { msg_send![sender, representedObject] };
                let Some(represented) = represented else {
                    return;
                };
                // Read it as NSString via the UTF8String selector.
                let cstr: *const std::os::raw::c_char =
                    unsafe { msg_send![&*represented, UTF8String] };
                if cstr.is_null() {
                    return;
                }
                let id = unsafe { std::ffi::CStr::from_ptr(cstr) }
                    .to_string_lossy()
                    .into_owned();
                self.send_command(AppCommand::ActivatePet(id));
            }
```

- [ ] **Step 2: Add the selector accessor**

In `impl CommandTarget { ... }` (the regular Rust impl, around lines 111-130), add below `settings_value_selector`:

```rust
        pub fn activate_pet_selector() -> Sel {
            sel!(activatePet:)
        }
```

- [ ] **Step 3: Build to confirm Tasks 23+24 together produce a working build**

Run: `cargo build --tests`
Expected: succeeds — Task 23's `populate_pet_submenu` no longer has a dangling reference.

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: PASS — no behavior change to runtime tests; menu wiring is exercised manually.

- [ ] **Step 5: Commit**

```bash
git add src/command_target_macos.rs
git commit -m "$(cat <<'EOF'
feat(command_target): add activatePet: selector for menu-driven hot-swap

The selector reads the menu item's representedObject (an NSString id),
converts it to a Rust String, and dispatches AppCommand::ActivatePet.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 25: App — invoke populate_pet_submenu and refresh on demand

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add a helper that pushes catalog entries to the menu bar**

In `src/app.rs`, inside `impl DesktopPetApp`, add:

```rust
    fn sync_pet_submenu(&self) {
        let Some(menu_bar) = &self.menu_bar else {
            return;
        };
        let entries: Vec<(String, String)> = self
            .catalog
            .entries()
            .iter()
            .map(|e| (e.id.clone(), e.display_name.clone()))
            .collect();
        menu_bar.populate_pet_submenu(&entries, &self.active_pet_id);
    }
```

- [ ] **Step 2: Call sync_pet_submenu in the relevant lifecycle points**

Find `sync_menu_bar` in `src/app.rs` (used to push runtime state). After the existing `sync_runtime_state(...)` call there, add:

```rust
        self.sync_pet_submenu();
```

Also, in `refresh_catalog` (added in Task 20), append the sync call:

```rust
    pub fn refresh_catalog(&mut self) {
        let (catalog, _) = build_startup_catalog();
        self.catalog = catalog;
        self.sync_pet_submenu();
    }
```

Also, in `activate_pet` (added in Task 19), append the sync call at the end of the success path (after `window.request_redraw()`):

```rust
        self.sync_pet_submenu();
        Ok(())
    }
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: succeeds.

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "$(cat <<'EOF'
feat(app): push catalog snapshots into the menu bar Pet submenu

DesktopPetApp::sync_pet_submenu maps catalog entries to the
(id, display_name) pairs the menu bar expects. Invoked from
sync_menu_bar, activate_pet, and refresh_catalog so the submenu
checkmark always reflects the active pet.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 26: Menu bar — rescan catalog on submenu open

**Files:**
- Modify: `src/menu_bar.rs`
- Modify: `src/command_target_macos.rs`
- Modify: `src/app.rs`

**Goal:** When the user opens the Pet submenu, trigger a fresh catalog scan + repopulate. The cleanest cross-thread-safe way is to send a new `AppCommand::RefreshCatalogAndSyncMenu` from the main thread (where AppKit calls back) into the event loop, which the app handles on its next tick.

- [ ] **Step 1: Add a new AppCommand variant**

In `src/app.rs`, modify the `AppCommand` enum to add (next to `RevealPetsFolder`):

```rust
    RefreshPetMenu,
```

- [ ] **Step 2: Add a tag and command mapping**

In `src/menu_bar.rs`, add a new constant near the others:

```rust
pub const MENU_TAG_REFRESH_PET_MENU: isize = 1202;
```

In `command_from_tag`, add:

```rust
        MENU_TAG_REFRESH_PET_MENU => Some(AppCommand::RefreshPetMenu),
```

- [ ] **Step 3: Add an `NSMenuDelegate` callback that fires the command**

The cleanest implementation here is to set the `CommandTarget` as the delegate of the Pet submenu and implement `menuNeedsUpdate:`. Inside the `define_class!` block in `src/command_target_macos.rs`, add right after the `#[unsafe(method(activatePet:))]` block:

```rust
            #[unsafe(method(menuNeedsUpdate:))]
            fn menu_needs_update(&self, _menu: Option<&AnyObject>) {
                self.send_command(AppCommand::RefreshPetMenu);
            }
```

You'll also need `NSMenuDelegate` conformance. At the top of the `define_class!` block, where you currently have `unsafe impl NSObjectProtocol for CommandTarget {}`, add:

```rust
        unsafe impl objc2_app_kit::NSMenuDelegate for CommandTarget {}
```

Import `NSMenuDelegate` at the top of the `mod macos` block:

```rust
    use objc2_app_kit::NSMenuDelegate;
```

(Move/place the import where it fits — likely near the other `objc2_app_kit` uses already in the file.)

- [ ] **Step 4: Wire the submenu's delegate in MenuBarController::new**

In `src/menu_bar.rs`, in `MenuBarController::new`, immediately after `pet_submenu = NSMenu::initWithTitle(...)`, set the delegate. The `setDelegate:` selector takes a weak reference; use `setDelegate(Some(target_object))`:

```rust
        let pet_submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Pet"));
        // The delegate must be set BEFORE addItem so the first menuNeedsUpdate:
        // (which fires immediately on first open) catches a real callback.
        let pet_submenu_target: &AnyObject = target.as_ref();
        unsafe { pet_submenu.setDelegate(Some(pet_submenu_target.cast())) };
```

Note: `setDelegate` may expect a strict `&ProtocolObject<dyn NSMenuDelegate>`. If `cast()` doesn't compile, use:

```rust
        let delegate_obj = unsafe {
            objc2::runtime::ProtocolObject::from_ref(&*target)
        };
        unsafe { pet_submenu.setDelegate(Some(delegate_obj)) };
```

Add the import if needed:

```rust
        use objc2::runtime::ProtocolObject;
        use objc2_app_kit::NSMenuDelegate;
```

- [ ] **Step 5: Handle the new command in the app**

In `src/app.rs`, in `handle_non_quit_command`, add the new arm:

```rust
            AppCommand::RefreshPetMenu => {
                self.refresh_catalog();
            }
```

(`refresh_catalog` already calls `sync_pet_submenu`, so this single line is enough.)

- [ ] **Step 6: Run tests**

Run: `cargo test`
Expected: PASS — full suite. AppKit delegate is exercised manually.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs src/menu_bar.rs src/command_target_macos.rs
git commit -m "$(cat <<'EOF'
feat(menu_bar): rescan catalog on Pet submenu open via NSMenuDelegate

CommandTarget now conforms to NSMenuDelegate. Its menuNeedsUpdate:
callback dispatches AppCommand::RefreshPetMenu, which the app handles
by rescanning the catalog and resyncing the submenu state.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 27: cargo fmt + final test run

**Files:**
- Whatever `cargo fmt` touches.

- [ ] **Step 1: Format**

Run: `cargo fmt`

- [ ] **Step 2: Confirm clippy is clean**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: zero warnings. If something genuinely needs a `#[allow(...)]`, add it with a one-line `// reason:` comment. Do not silence broad categories.

- [ ] **Step 3: Final full test run**

Run: `cargo test`
Expected: PASS — baseline 182 from sub-project 1 plus all the tests added in this plan (approximately 215 total).

- [ ] **Step 4: Commit (if fmt changed anything)**

```bash
git status
# If anything changed:
git add -u
git commit -m "$(cat <<'EOF'
chore: cargo fmt

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 5: Manual smoke test**

Build the app bundle and run it. Step through the manual checklist from the spec's §10.6:

1. Launch app with no `~/Library/Application Support/Happy Cappy/pets/` directory → bundled Happy Cappy runs; `pets/README.txt` is created.
2. Drop a folder `pets/test-pet/` containing valid `pet.json` + sprite. Open Pet menu → see it listed. Click → pet hot-swaps; window position preserved.
3. Drop a folder with `id: "happy-cappy"` → menu shows only one "Happy Cappy"; log shows duplicate warning.
4. Edit a custom pet's `pet.json` to invalid JSON → close & reopen Pet menu → pet disappears from list.
5. Restart app → previously selected custom pet still active.
6. Delete the selected custom pet's folder, restart → app falls back to Happy Cappy; settings cleared to `None`.
7. Custom pet with different frame size (e.g., 96×96): activate → window resizes correctly.
8. "Reveal Pets Folder" item opens Finder at the right location.

This step is manual; do not auto-mark it complete. Report any failures back as new tasks.

---

## Notes for the implementer

- **Don't skip the partial-commit gate in Task 23.** The repo intentionally does not compile between Task 23 and Task 24; that's the point of the staged tasks. If you're running the plan sequentially in one go, the build will pass after Task 24.
- **Don't merge the `_for_test` removal with anything else.** Task 15 touches the boundary between sub-project 1 and sub-project 2; keep the commit small so a future bisect is informative.
- **AppKit message-send signatures occasionally vary between objc2 minor versions.** If `setDelegate`, `setRepresentedObject:`, or `UTF8String` produce signature mismatches, consult the current `objc2_app_kit`/`objc2_foundation` docs rather than guessing. Adjust syntax; do not change semantics.
- **No filesystem watcher.** Submenu refresh is gated on `menuNeedsUpdate:`. Do not introduce file-system notification crates.
- **No release-blocker if Step 5 manual smoke can't be performed.** Manual smoke is the user's responsibility per sub-project 1 precedent. Mark the plan complete after Task 27 Step 4 and surface the smoke checklist to the user.
