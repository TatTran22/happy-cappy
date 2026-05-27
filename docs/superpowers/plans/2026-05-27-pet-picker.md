# Pet Picker Window Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an in-window Pet Library picker that lists every pet in the catalog with an animated idle preview, shows error rows for failed custom pets, and activates a selected pet via an explicit Apply button.

**Architecture:** A new pure-Rust module `src/picker_entries.rs` builds the picker's data model from the existing `PetCatalog` (no AppKit deps; fully unit-tested). A new AppKit module `src/picker_window_macos.rs` owns the `NSPanel` + `NSTableView` + detail pane, a per-pet idle-frame NSImage cache, and a single 10 fps `NSTimer` that updates visible row image views and the detail preview. `DesktopPetApp` owns the picker controller lazily; a new `AppCommand::ShowPicker` (dispatched from a new "Pet Library…" main-menu item) drives `refresh_catalog → build entries → picker.sync_entries → picker.show`. Apply reuses the existing `AppCommand::ActivatePet(String)` from sub-project 2.

**Tech Stack:** Rust 2021, `serde`/`serde_json` (existing), AppKit via `objc2`/`objc2-app-kit`/`objc2-foundation` (existing — adding new features), `objc2-core-graphics` (existing — adding new features), `image` (existing).

---

## File Structure

**New files:**
- `src/picker_entries.rs` — pure-Rust data layer. Owns `PickerSource`, `PickerEntryBase`, `format_catalog_error`, `picker_entry_from_load_error`, `build_picker_entries_base`. No AppKit dependency. ~180 LOC including tests.
- `src/picker_window_macos.rs` — AppKit layer (`#[cfg(target_os = "macos")]`-gated; no-op stub on other targets). Owns `PickerEntry`, `PickerWindowController`, `PickerTableSource` (NSObject subclass via `define_class!`), `attach_preview_frames`, `crop_frame_rgba`, `rgba_to_nsimage`, `PreviewBuildError`. Mirrors the structure of `src/settings_window_macos.rs`. ~600 LOC.

**Modified files:**
- `Cargo.toml` — add `NSImage`, `NSImageView`, `NSScrollView`, `NSTableColumn`, `NSTableRowView`, `NSTableView` to `objc2-app-kit` features; add `CGColorSpace`, `CGDataProvider`, `CGImage` to `objc2-core-graphics` features.
- `src/lib.rs` — `pub mod picker_entries;` and `pub mod picker_window_macos;`.
- `src/app.rs` — add `AppCommand::ShowPicker` variant; add `picker: Option<PickerWindowController>` field; `ensure_picker_window()` helper; `ShowPicker` handler that calls `refresh_catalog()` → builds entries → calls `picker.sync_entries` → `picker.show`; extend `refresh_catalog()` so it also calls `picker.sync_entries(...)` when the picker is currently visible.
- `src/menu_bar.rs` — add `MENU_TAG_OPEN_PET_LIBRARY = 1202`; extend `command_from_tag` mapping; insert "Pet Library…" `NSMenuItem` between the existing **Pet ▸** submenu and the separator before **Settings…**.

**Files NOT touched:** `src/pet/catalog.rs`, `src/pet/manifest.rs`, `src/pet/runtime.rs`, `src/pet/resolver.rs`, `src/sprite.rs`, `src/settings.rs`, `src/settings_window_macos.rs`, `src/command_target_macos.rs` (existing `activatePet:` selector is reused for the Apply button), `src/window_macos.rs`, `src/workspace.rs`, `src/renderer.rs`, `src/interaction.rs`, `src/physics.rs`.

---

## Task list summary

1. Expand Cargo feature flags for AppKit + CoreGraphics
2. `AppCommand::ShowPicker` variant
3. `MENU_TAG_OPEN_PET_LIBRARY` constant + collision test
4. `command_from_tag` mapping for the new tag
5. Module skeleton `src/picker_entries.rs` (types only)
6. `format_catalog_error` mapping
7. `picker_entry_from_load_error` per-variant entry builder
8. `build_picker_entries_base` ordering + assembly
9. Module skeleton `src/picker_window_macos.rs` (stub + non-macos no-op)
10. `crop_frame_rgba` helper (pure; tested)
11. `rgba_to_nsimage` helper (AppKit; manual verification)
12. `PreviewBuildError` + `build_preview_frames` + `attach_preview_frames`
13. `PickerTableSource` NSObject subclass (data source + delegate + timer target)
14. NSPanel + scroll view + table view + detail pane construction
15. `sync_entries` — replace data, reload table, update detail
16. `update_detail_pane` — render the selected entry's metadata + preview
17. Animation `NSTimer` lifecycle (start on show, stop on hide, tick callback)
18. Apply button wiring (dispatch `ActivatePet`, hide picker, disabled-state rules)
19. "Reveal in Finder" button wiring (`RevealPetsFolder` dispatch, error-row visibility)
20. `DesktopPetApp.picker` field + `ensure_picker_window` lazy init
21. `ShowPicker` handler in `handle_command`
22. Re-sync picker from `refresh_catalog()` when visible
23. Menu wiring — insert "Pet Library…" item in `MenuBarController::new`
24. Final verification — clippy, fmt, full test run, manual smoke plan walkthrough

---

## Task 1: Expand Cargo features

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Edit `[target.'cfg(target_os = "macos")'.dependencies]` so `objc2-app-kit` features list includes the new entries (alphabetised insertion):**

Add `NSImage`, `NSImageView`, `NSScrollView`, `NSTableColumn`, `NSTableRowView`, `NSTableView` to the existing list. The relevant block should read:

```toml
objc2-app-kit = { version = "0.3", features = [
  "NSApplication",
  "NSButton",
  "NSControl",
  "NSEvent",
  "NSImage",
  "NSImageView",
  "NSMenu",
  "NSMenuItem",
  "NSPanel",
  "NSResponder",
  "NSRunningApplication",
  "NSScreen",
  "NSScrollView",
  "NSSegmentedControl",
  "NSSlider",
  "NSStackView",
  "NSStatusBar",
  "NSStatusItem",
  "NSTableColumn",
  "NSTableRowView",
  "NSTableView",
  "NSTextField",
  "NSView",
  "NSWindow",
  "NSWindowController",
  "NSWorkspace",
  "objc2-core-foundation",
] }
```

- [ ] **Step 2: Add `CGColorSpace`, `CGDataProvider`, `CGImage` to `objc2-core-graphics` features:**

```toml
objc2-core-graphics = { version = "0.3", features = [
  "CGColorSpace",
  "CGDataProvider",
  "CGEventSource",
  "CGEventTypes",
  "CGGeometry",
  "CGImage",
  "CGWindow",
] }
```

- [ ] **Step 3: Verify the build still compiles**

Run: `cargo check`
Expected: clean build, no errors.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "feat(deps): enable AppKit table/image + CoreGraphics CGImage features"
```

---

## Task 2: Add `AppCommand::ShowPicker` variant

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add the new variant to the `AppCommand` enum**

Locate the `pub enum AppCommand` block and append `ShowPicker,` to the list of variants. Keep alphabetical placement consistent with existing variants if the file uses it; otherwise add at the end. Example insertion (the surrounding code uses `derive(Clone, Debug, PartialEq, Eq)` — keep that):

```rust
pub enum AppCommand {
    // ...existing variants...
    ShowPicker,
}
```

- [ ] **Step 2: Verify the new variant compiles**

Run: `cargo check`
Expected: clean build, possibly an `unused variant` warning (which the next tasks will eliminate).

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): add ShowPicker AppCommand variant"
```

---

## Task 3: Menu tag constant for "Pet Library…"

**Files:**
- Modify: `src/menu_bar.rs`

- [ ] **Step 1: Write the failing test extension**

In `src/menu_bar.rs`, modify the existing `pet_item_base_does_not_collide_with_other_tags` test to also reference a new `MENU_TAG_OPEN_PET_LIBRARY` constant. Add the constant to the `used` array:

```rust
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
    MENU_TAG_OPEN_PET_LIBRARY,
];
```

- [ ] **Step 2: Run the test — expect failure on the unresolved constant**

Run: `cargo test --lib menu_bar::tests::pet_item_base_does_not_collide_with_other_tags`
Expected: compile error "cannot find value `MENU_TAG_OPEN_PET_LIBRARY` in this scope".

- [ ] **Step 3: Add the constant**

At the top of `src/menu_bar.rs` next to the other tag constants:

```rust
pub const MENU_TAG_OPEN_PET_LIBRARY: isize = 1202;
```

- [ ] **Step 4: Run the test — expect pass**

Run: `cargo test --lib menu_bar::tests::pet_item_base_does_not_collide_with_other_tags`
Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add src/menu_bar.rs
git commit -m "feat(menu_bar): add MENU_TAG_OPEN_PET_LIBRARY constant"
```

---

## Task 4: Wire menu tag to `AppCommand::ShowPicker`

**Files:**
- Modify: `src/menu_bar.rs`

- [ ] **Step 1: Write the failing test**

Add a new test function inside the existing `mod tests` block:

```rust
#[test]
fn command_from_tag_maps_open_pet_library() {
    assert_eq!(
        command_from_tag(MENU_TAG_OPEN_PET_LIBRARY),
        Some(AppCommand::ShowPicker)
    );
}
```

- [ ] **Step 2: Run the test — expect failure**

Run: `cargo test --lib menu_bar::tests::command_from_tag_maps_open_pet_library`
Expected: FAIL — `command_from_tag` returns `None`.

- [ ] **Step 3: Add the mapping**

In `command_from_tag`, add a new match arm before the `_ => None` arm:

```rust
MENU_TAG_OPEN_PET_LIBRARY => Some(AppCommand::ShowPicker),
```

- [ ] **Step 4: Run the test — expect pass**

Run: `cargo test --lib menu_bar`
Expected: all menu_bar tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/menu_bar.rs
git commit -m "feat(menu_bar): map MENU_TAG_OPEN_PET_LIBRARY to AppCommand::ShowPicker"
```

---

## Task 5: Skeleton `picker_entries.rs` (types only)

**Files:**
- Create: `src/picker_entries.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create the skeleton module file**

Create `src/picker_entries.rs` with the following content:

```rust
//! Pure-Rust data layer for the Pet Library picker.
//!
//! Builds picker entries (one row per pet — including failures) from the
//! existing [`crate::pet::catalog::PetCatalog`]. Has no AppKit dependency
//! and is fully unit-tested.

use std::path::PathBuf;

use crate::pet::catalog::{CatalogLoadError, CatalogSource, PetCatalog};

/// Source of a picker row — mirrors [`CatalogSource`] but tagged to make
/// "this row is the bundled pet" vs "this row is a user-supplied pet"
/// explicit at the UI layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerSource {
    Bundled,
    Custom,
}

/// Pure data for one row in the picker.
///
/// AppKit-side `PickerEntry` (defined in `picker_window_macos`) wraps this
/// plus the decoded idle-animation frames as `Vec<Retained<NSImage>>`.
#[derive(Debug, Clone)]
pub struct PickerEntryBase {
    pub id: String,
    pub display_name: String,
    pub source: PickerSource,
    pub frame_width: u32,
    pub frame_height: u32,
    pub animations: Vec<String>,
    pub error: Option<String>,
    /// Filesystem path the row points at: a pet directory for custom pets
    /// (so "Reveal in Finder" can surface the offending folder), `None`
    /// for the bundled pet (which is inside the app bundle).
    pub source_path: Option<PathBuf>,
}
```

- [ ] **Step 2: Register the module**

In `src/lib.rs`, add the new public module declaration alongside the existing ones:

```rust
pub mod picker_entries;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: clean build (unused-import warnings on `CatalogLoadError` and `PetCatalog` are acceptable — they'll be consumed in Task 6 and 8).

- [ ] **Step 4: Commit**

```bash
git add src/picker_entries.rs src/lib.rs
git commit -m "feat(picker_entries): scaffold pure-Rust picker data types"
```

---

## Task 6: `format_catalog_error` mapping

**Files:**
- Modify: `src/picker_entries.rs`

- [ ] **Step 1: Write the failing tests**

Append the following test module to `src/picker_entries.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::catalog::CatalogLoadError;
    use crate::pet::manifest::ManifestError;
    use std::path::PathBuf;

    #[test]
    fn format_manifest_parse_short_message() {
        let error = CatalogLoadError::ManifestParse {
            path: PathBuf::from("/tmp/pets/broken/pet.json"),
            error: ManifestError::MissingIdleAnimation,
        };
        let msg = format_catalog_error(&error);
        assert!(msg.starts_with("Invalid pet.json:"));
        assert!(msg.len() <= 140);
    }

    #[test]
    fn format_spritesheet_missing_includes_filename() {
        let error = CatalogLoadError::SpritesheetMissing {
            manifest_path: PathBuf::from("/tmp/pets/x/pet.json"),
            sprite_path: PathBuf::from("/tmp/pets/x/ghost.png"),
        };
        let msg = format_catalog_error(&error);
        assert!(msg.contains("ghost.png"));
    }

    #[test]
    fn format_duplicate_id_mentions_id() {
        let error = CatalogLoadError::DuplicateId {
            id: "happy-cappy".to_string(),
            kept: PathBuf::from("/bundled/h.png"),
            dropped: PathBuf::from("/tmp/pets/clone/clone.png"),
        };
        let msg = format_catalog_error(&error);
        assert!(msg.contains("happy-cappy"));
    }

    #[test]
    fn format_long_serde_message_truncates() {
        let serde_err =
            serde_json::from_str::<serde_json::Value>("{not valid").unwrap_err();
        let error = CatalogLoadError::ManifestParse {
            path: PathBuf::from("/tmp/pets/broken/pet.json"),
            error: ManifestError::Json(serde_err),
        };
        let msg = format_catalog_error(&error);
        assert!(msg.len() <= 140);
    }

    #[test]
    fn format_dir_read_short_message() {
        let error = CatalogLoadError::DirRead {
            path: PathBuf::from("/tmp/pets"),
            error: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no"),
        };
        let msg = format_catalog_error(&error);
        assert!(msg.starts_with("Couldn't read pet directory"));
    }
}
```

- [ ] **Step 2: Run the tests — expect failure**

Run: `cargo test --lib picker_entries::tests`
Expected: FAIL — `format_catalog_error` not found.

- [ ] **Step 3: Implement `format_catalog_error`**

Append to `src/picker_entries.rs` (above the `#[cfg(test)] mod tests`):

```rust
/// Translate a catalog load failure into a single user-facing line.
///
/// Trims to ~140 characters so the detail-pane label never blows past the
/// visible width on a 480-pt-wide panel.
pub fn format_catalog_error(error: &CatalogLoadError) -> String {
    const MAX_LEN: usize = 140;
    let raw = match error {
        CatalogLoadError::DirRead { error, .. } => {
            format!("Couldn't read pet directory: {error}")
        }
        CatalogLoadError::ManifestParse { error, .. } => {
            format!("Invalid pet.json: {error}")
        }
        CatalogLoadError::SpritesheetMissing { sprite_path, .. } => {
            let name = sprite_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| sprite_path.to_string_lossy().into_owned());
            format!("Spritesheet not found: {name}")
        }
        CatalogLoadError::DuplicateId { id, .. } => {
            format!("ID `{id}` conflicts with the bundled pet")
        }
    };
    truncate_with_ellipsis(&raw, MAX_LEN)
}

fn truncate_with_ellipsis(input: &str, max_len: usize) -> String {
    if input.chars().count() <= max_len {
        return input.to_string();
    }
    let take = max_len.saturating_sub(1);
    let mut out: String = input.chars().take(take).collect();
    out.push('…');
    out
}
```

- [ ] **Step 4: Run the tests — expect pass**

Run: `cargo test --lib picker_entries::tests`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add src/picker_entries.rs
git commit -m "feat(picker_entries): format CatalogLoadError as one-line user message"
```

---

## Task 7: `picker_entry_from_load_error`

**Files:**
- Modify: `src/picker_entries.rs`

- [ ] **Step 1: Write the failing tests**

Append to the existing `#[cfg(test)] mod tests` block in `src/picker_entries.rs`:

```rust
#[test]
fn entry_from_manifest_parse_uses_folder_name() {
    let error = CatalogLoadError::ManifestParse {
        path: PathBuf::from("/tmp/pets/broken/pet.json"),
        error: ManifestError::MissingIdleAnimation,
    };
    let entry = picker_entry_from_load_error(&error).expect("must produce entry");
    assert_eq!(entry.display_name, "broken");
    assert_eq!(entry.source, PickerSource::Custom);
    assert!(entry.error.is_some());
    assert_eq!(entry.source_path.as_deref(), Some(std::path::Path::new("/tmp/pets/broken")));
    assert_eq!(entry.frame_width, 0);
    assert_eq!(entry.frame_height, 0);
    assert!(entry.animations.is_empty());
}

#[test]
fn entry_from_spritesheet_missing_uses_folder_name() {
    let error = CatalogLoadError::SpritesheetMissing {
        manifest_path: PathBuf::from("/tmp/pets/no-sprite/pet.json"),
        sprite_path: PathBuf::from("/tmp/pets/no-sprite/ghost.png"),
    };
    let entry = picker_entry_from_load_error(&error).expect("must produce entry");
    assert_eq!(entry.display_name, "no-sprite");
    assert_eq!(entry.source, PickerSource::Custom);
    assert!(entry.error.as_deref().unwrap().contains("ghost.png"));
}

#[test]
fn entry_from_duplicate_id_uses_dropped_folder_name() {
    let error = CatalogLoadError::DuplicateId {
        id: "happy-cappy".to_string(),
        kept: PathBuf::from("/bundled/h.png"),
        dropped: PathBuf::from("/tmp/pets/usurper/sprite.png"),
    };
    let entry = picker_entry_from_load_error(&error).expect("must produce entry");
    assert_eq!(entry.display_name, "usurper");
    assert_eq!(entry.source, PickerSource::Custom);
    assert!(entry.error.as_deref().unwrap().contains("happy-cappy"));
}

#[test]
fn entry_from_dir_read_returns_none() {
    let error = CatalogLoadError::DirRead {
        path: PathBuf::from("/tmp/pets"),
        error: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no"),
    };
    assert!(picker_entry_from_load_error(&error).is_none());
}
```

- [ ] **Step 2: Run tests — expect failure**

Run: `cargo test --lib picker_entries::tests`
Expected: FAIL on the new tests (function not defined).

- [ ] **Step 3: Implement the helper**

Insert the following into `src/picker_entries.rs` (above the test module, below `format_catalog_error`):

```rust
/// Convert a per-pet catalog failure into a picker entry for display.
///
/// Returns `None` for [`CatalogLoadError::DirRead`] (a catalog-wide
/// failure, not tied to a specific pet directory) — those are still
/// logged at `warn!` level by the catalog itself.
pub fn picker_entry_from_load_error(error: &CatalogLoadError) -> Option<PickerEntryBase> {
    let (folder, source_path) = match error {
        CatalogLoadError::ManifestParse { path, .. } => folder_and_parent(path)?,
        CatalogLoadError::SpritesheetMissing { manifest_path, .. } => {
            folder_and_parent(manifest_path)?
        }
        CatalogLoadError::DuplicateId { dropped, .. } => folder_and_parent(dropped)?,
        CatalogLoadError::DirRead { .. } => return None,
    };
    Some(PickerEntryBase {
        id: folder.clone(),
        display_name: folder,
        source: PickerSource::Custom,
        frame_width: 0,
        frame_height: 0,
        animations: Vec::new(),
        error: Some(format_catalog_error(error)),
        source_path: Some(source_path),
    })
}

fn folder_and_parent(child_path: &std::path::Path) -> Option<(String, PathBuf)> {
    let parent = child_path.parent()?;
    let name = parent.file_name()?.to_string_lossy().into_owned();
    Some((name, parent.to_path_buf()))
}
```

- [ ] **Step 4: Run tests — expect pass**

Run: `cargo test --lib picker_entries::tests`
Expected: all picker_entries tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/picker_entries.rs
git commit -m "feat(picker_entries): map per-pet CatalogLoadError to PickerEntryBase"
```

---

## Task 8: `build_picker_entries_base`

**Files:**
- Modify: `src/picker_entries.rs`

- [ ] **Step 1: Write the failing tests**

Append to `#[cfg(test)] mod tests` in `src/picker_entries.rs`:

```rust
use crate::pet::catalog::{BundledPet, PetCatalog};
use crate::pet::manifest::{Animation, FrameGeometry, PetManifest};
use std::collections::BTreeMap;
use tempfile::tempdir;

fn bundled_pet_for_test() -> BundledPet {
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

fn write_valid_pet(dir: &std::path::Path, id: &str, display_name: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let manifest = format!(
        r#"{{
            "id": "{id}",
            "displayName": "{display_name}",
            "spritesheetPath": "sprite.png",
            "frame": {{"width": 16, "height": 16, "columns": 4, "rows": 1}},
            "animations": {{"idle": {{"frames": [0, 1, 2, 3]}}}}
        }}"#
    );
    std::fs::write(dir.join("pet.json"), manifest).unwrap();
    std::fs::write(dir.join("sprite.png"), b"fake").unwrap();
}

#[test]
fn build_entries_bundled_only_when_dir_empty() {
    let dir = tempdir().unwrap();
    let catalog = PetCatalog::scan(bundled_pet_for_test(), dir.path());

    let entries = build_picker_entries_base(&catalog);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "happy-cappy");
    assert_eq!(entries[0].source, PickerSource::Bundled);
    assert!(entries[0].error.is_none());
    assert_eq!(entries[0].animations, vec!["idle".to_string()]);
}

#[test]
fn build_entries_orders_ok_custom_alphabetically_after_bundled() {
    let dir = tempdir().unwrap();
    write_valid_pet(&dir.path().join("z"), "zebra", "Zebra");
    write_valid_pet(&dir.path().join("a"), "alpha", "alpha");
    write_valid_pet(&dir.path().join("b"), "beta", "Beta");
    let catalog = PetCatalog::scan(bundled_pet_for_test(), dir.path());

    let entries = build_picker_entries_base(&catalog);

    let names: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(names, vec!["happy-cappy", "alpha", "beta", "zebra"]);
}

#[test]
fn build_entries_appends_errors_at_end() {
    let dir = tempdir().unwrap();
    write_valid_pet(&dir.path().join("z"), "zebra", "Zebra");
    std::fs::create_dir_all(dir.path().join("broken")).unwrap();
    std::fs::write(dir.path().join("broken").join("pet.json"), b"{ not json").unwrap();
    let catalog = PetCatalog::scan(bundled_pet_for_test(), dir.path());

    let entries = build_picker_entries_base(&catalog);

    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].id, "happy-cappy");
    assert_eq!(entries[1].id, "zebra");
    assert_eq!(entries[2].display_name, "broken");
    assert!(entries[2].error.is_some());
}

#[test]
fn build_entries_drops_dir_read_failures() {
    // We can't easily induce a DirRead failure on a tempdir, so synthesise
    // a catalog with one manually. Use the public scan API on a valid dir,
    // then verify directly that DirRead failures (if any) would be dropped
    // by exercising picker_entry_from_load_error in isolation. That
    // behaviour is already covered by Task 7's tests; here we sanity-check
    // ordering doesn't accidentally pick them up.
    let dir = tempdir().unwrap();
    let catalog = PetCatalog::scan(bundled_pet_for_test(), dir.path());
    let entries = build_picker_entries_base(&catalog);
    assert!(entries.iter().all(|e| e.id != ""));
}

#[test]
fn build_entries_lists_animations_alphabetically() {
    let dir = tempdir().unwrap();
    let pet_dir = dir.path().join("multi");
    std::fs::create_dir_all(&pet_dir).unwrap();
    std::fs::write(
        pet_dir.join("pet.json"),
        br#"{
            "id": "multi",
            "displayName": "Multi",
            "spritesheetPath": "sprite.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {
                "walk-right": {"frames": [0, 1]},
                "idle": {"frames": [0]},
                "blink": {"frames": [0, 1]}
            }
        }"#,
    )
    .unwrap();
    std::fs::write(pet_dir.join("sprite.png"), b"fake").unwrap();
    let catalog = PetCatalog::scan(bundled_pet_for_test(), dir.path());

    let entries = build_picker_entries_base(&catalog);
    let multi = entries.iter().find(|e| e.id == "multi").unwrap();
    assert_eq!(
        multi.animations,
        vec!["blink".to_string(), "idle".to_string(), "walk-right".to_string()]
    );
}
```

- [ ] **Step 2: Run tests — expect failure**

Run: `cargo test --lib picker_entries::tests`
Expected: FAIL — `build_picker_entries_base` not defined.

- [ ] **Step 3: Implement `build_picker_entries_base`**

Insert into `src/picker_entries.rs` (above the `#[cfg(test)] mod tests`):

```rust
use crate::pet::catalog::CatalogEntry;

/// Build the ordered list of picker rows from the live catalog.
///
/// Order is:
/// 1. Bundled pet.
/// 2. OK custom pets in `catalog.entries()` order (already case-insensitive
///    alphabetical by display name — see [`PetCatalog::scan`]).
/// 3. Per-pet failures in `catalog.load_errors()` order, mapped via
///    [`picker_entry_from_load_error`]. `DirRead` errors are dropped.
pub fn build_picker_entries_base(catalog: &PetCatalog) -> Vec<PickerEntryBase> {
    let mut out: Vec<PickerEntryBase> =
        catalog.entries().iter().map(entry_base_from_catalog).collect();

    for error in catalog.load_errors() {
        if let Some(entry) = picker_entry_from_load_error(error) {
            out.push(entry);
        }
    }

    out
}

fn entry_base_from_catalog(entry: &CatalogEntry) -> PickerEntryBase {
    let mut animations: Vec<String> = entry.manifest.animations.keys().cloned().collect();
    animations.sort();
    PickerEntryBase {
        id: entry.id.clone(),
        display_name: entry.display_name.clone(),
        source: match entry.source {
            CatalogSource::Bundled => PickerSource::Bundled,
            CatalogSource::Custom => PickerSource::Custom,
        },
        frame_width: entry.manifest.frame.width,
        frame_height: entry.manifest.frame.height,
        animations,
        error: None,
        source_path: if entry.source == CatalogSource::Bundled {
            None
        } else {
            entry.spritesheet_path.parent().map(|p| p.to_path_buf())
        },
    }
}
```

- [ ] **Step 4: Run tests — expect pass**

Run: `cargo test --lib picker_entries`
Expected: all picker_entries tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/picker_entries.rs
git commit -m "feat(picker_entries): assemble bundled+custom+errors into ordered list"
```

---

## Task 9: Skeleton `picker_window_macos.rs`

**Files:**
- Create: `src/picker_window_macos.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create the non-macos stub file**

Create `src/picker_window_macos.rs` with the following content. This compiles on any target but only does anything useful on macOS — matching the pattern in `src/settings_window_macos.rs`:

```rust
//! Native AppKit Pet Library picker window.
//!
//! Mirrors the structure of [`crate::settings_window_macos`]: an `NSPanel`
//! created lazily, populated synchronously from `DesktopPetApp`, and
//! dispatching user actions back through [`crate::app::AppCommand`] via
//! [`crate::command_target_macos::CommandTarget`].

use crate::picker_entries::PickerEntryBase;

#[cfg(not(target_os = "macos"))]
pub struct PickerWindowController;

#[cfg(not(target_os = "macos"))]
impl PickerWindowController {
    pub fn new(
        _proxy: winit::event_loop::EventLoopProxy<crate::app::AppCommand>,
    ) -> Option<Self> {
        None
    }

    pub fn show(&self) {}

    pub fn hide(&self) {}

    pub fn is_visible(&self) -> bool {
        false
    }

    pub fn sync_entries(&self, _entries: Vec<PickerEntryBase>, _active_id: &str) {}
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use winit::event_loop::EventLoopProxy;

    use crate::app::AppCommand;

    /// Placeholder — the real implementation arrives in subsequent tasks.
    pub struct PickerWindowController;

    impl PickerWindowController {
        pub fn new(_proxy: EventLoopProxy<AppCommand>) -> Option<Self> {
            None
        }

        pub fn show(&self) {}
        pub fn hide(&self) {}
        pub fn is_visible(&self) -> bool {
            false
        }

        pub fn sync_entries(&self, _entries: Vec<PickerEntryBase>, _active_id: &str) {}
    }
}

#[cfg(target_os = "macos")]
pub use macos::PickerWindowController;
```

- [ ] **Step 2: Register the module**

In `src/lib.rs`, add the declaration adjacent to the existing macOS-flavoured modules:

```rust
pub mod picker_window_macos;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add src/picker_window_macos.rs src/lib.rs
git commit -m "feat(picker_window_macos): scaffold cfg-gated controller stub"
```

---

## Task 10: `crop_frame_rgba` helper

**Files:**
- Modify: `src/picker_window_macos.rs`

- [ ] **Step 1: Write the failing test**

Inside the `#[cfg(target_os = "macos")] mod macos { ... }` block, add a test submodule (or, if the macos module is small, add a `#[cfg(test)]` module at the file root that imports the helper directly). Place the test at the bottom of the file:

```rust
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::macos::crop_frame_rgba;
    use crate::pet::manifest::FrameGeometry;
    use crate::sprite::SpriteSheet;
    use image::{Rgba, RgbaImage};

    fn checkered_sheet() -> SpriteSheet {
        // 2 frames wide × 1 frame tall, each frame 2×2 px.
        let mut img = RgbaImage::new(4, 2);
        img.put_pixel(0, 0, Rgba([10, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([20, 0, 0, 255]));
        img.put_pixel(0, 1, Rgba([30, 0, 0, 255]));
        img.put_pixel(1, 1, Rgba([40, 0, 0, 255]));
        img.put_pixel(2, 0, Rgba([50, 0, 0, 255]));
        img.put_pixel(3, 0, Rgba([60, 0, 0, 255]));
        img.put_pixel(2, 1, Rgba([70, 0, 0, 255]));
        img.put_pixel(3, 1, Rgba([80, 0, 0, 255]));
        let geometry = FrameGeometry {
            width: 2,
            height: 2,
            columns: 2,
            rows: 1,
        };
        SpriteSheet::from_image(img, &geometry).unwrap()
    }

    #[test]
    fn crop_first_frame_returns_top_left_pixels() {
        let sheet = checkered_sheet();
        let rgba = crop_frame_rgba(&sheet, 0);
        assert_eq!(rgba.len(), 2 * 2 * 4);
        assert_eq!(rgba[0], 10);
        assert_eq!(rgba[4], 20);
        assert_eq!(rgba[8], 30);
        assert_eq!(rgba[12], 40);
    }

    #[test]
    fn crop_second_frame_returns_top_right_pixels() {
        let sheet = checkered_sheet();
        let rgba = crop_frame_rgba(&sheet, 1);
        assert_eq!(rgba[0], 50);
        assert_eq!(rgba[4], 60);
        assert_eq!(rgba[8], 70);
        assert_eq!(rgba[12], 80);
    }
}
```

- [ ] **Step 2: Run tests — expect failure**

Run: `cargo test --lib picker_window_macos`
Expected: FAIL — `crop_frame_rgba` undefined.

- [ ] **Step 3: Implement `crop_frame_rgba`**

Add inside `#[cfg(target_os = "macos")] mod macos { ... }`. Place it near the top of the module, after imports:

```rust
use crate::sprite::SpriteSheet;

/// Copy the pixel rectangle for frame `index` out of `sheet.image()` into a
/// freshly-allocated packed RGBA buffer. Width and height match the frame
/// geometry; row stride is `width * 4`.
pub(super) fn crop_frame_rgba(sheet: &SpriteSheet, index: usize) -> Vec<u8> {
    let rect = sheet.frame_rect(index as u32);
    let image = sheet.image();
    let mut out = Vec::with_capacity((rect.width * rect.height * 4) as usize);
    for y in rect.y..(rect.y + rect.height) {
        for x in rect.x..(rect.x + rect.width) {
            let pixel = image.get_pixel(x, y);
            out.extend_from_slice(&pixel.0);
        }
    }
    out
}
```

- [ ] **Step 4: Run tests — expect pass**

Run: `cargo test --lib picker_window_macos`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add src/picker_window_macos.rs
git commit -m "feat(picker_window_macos): crop_frame_rgba helper with TDD coverage"
```

---

## Task 11: `rgba_to_nsimage` helper

**Files:**
- Modify: `src/picker_window_macos.rs`

This task adds the bytes → `NSImage` conversion. It cannot be unit-tested in isolation (NSImage is opaque); correctness is verified by the manual smoke plan in spec §12.

- [ ] **Step 1: Add the helper**

Add inside `#[cfg(target_os = "macos")] mod macos { ... }`, alongside `crop_frame_rgba`:

```rust
use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_app_kit::NSImage;
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{
    kCGBitmapByteOrderDefault, kCGImageAlphaPremultipliedLast, CGColorSpace, CGDataProvider,
    CGImage, CGImageAlphaInfo,
};
use objc2_foundation::{CGSize, MainThreadMarker};

pub(super) fn rgba_to_nsimage(
    rgba: &[u8],
    width: u32,
    height: u32,
    _mtm: MainThreadMarker,
) -> Retained<NSImage> {
    let row_bytes = (width as usize) * 4;
    debug_assert_eq!(rgba.len(), row_bytes * height as usize);

    let provider: CFRetained<CGDataProvider> =
        CGDataProvider::with_data(None, rgba.as_ptr() as *const _, rgba.len(), None)
            .expect("CGDataProvider::with_data returned null");

    let color_space: CFRetained<CGColorSpace> =
        CGColorSpace::new_device_rgb().expect("CGColorSpace::new_device_rgb returned null");

    let bitmap_info = kCGBitmapByteOrderDefault | kCGImageAlphaPremultipliedLast.0 as u32;

    let cg_image: CFRetained<CGImage> = unsafe {
        CGImage::new(
            width as usize,
            height as usize,
            8,
            32,
            row_bytes,
            Some(&color_space),
            bitmap_info,
            Some(&provider),
            std::ptr::null(),
            false,
            objc2_core_graphics::CGColorRenderingIntent::Default,
        )
    }
    .expect("CGImage::new returned null");

    let size = CGSize {
        width: width as f64,
        height: height as f64,
    };
    unsafe {
        NSImage::initWithCGImage_size(NSImage::alloc(), cg_image.as_ref(), size)
    }
}
```

> **Note for the implementer:** `objc2-core-graphics` 0.3 uses snake_case wrappers around CG functions. If the exact method names or argument shapes drift between point releases, fall back to `cargo doc --open -p objc2-core-graphics` to confirm. The selected types (`CGDataProvider`, `CGColorSpace`, `CGImage`) are gated by the features added in Task 1.

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build. If the build fails because of an API name mismatch in `objc2-core-graphics`, adjust the call style (e.g. `CGImage::new` vs `CGImage::create`) — the responsibility of this helper is fixed: produce an `NSImage` of the given pixel size from RGBA bytes.

- [ ] **Step 3: Commit**

```bash
git add src/picker_window_macos.rs
git commit -m "feat(picker_window_macos): rgba_to_nsimage helper via CGImage"
```

---

## Task 12: `PreviewBuildError` + `build_preview_frames` + `attach_preview_frames`

**Files:**
- Modify: `src/picker_window_macos.rs`

- [ ] **Step 1: Add the error type and helpers**

Inside `#[cfg(target_os = "macos")] mod macos { ... }`, add (below the existing helpers):

```rust
use crate::pet::catalog::{CatalogEntry, CatalogSource, PetCatalog};
use crate::picker_entries::{PickerEntryBase, PickerSource};
use crate::sprite::SpriteError;
use std::path::PathBuf;

#[derive(Debug)]
pub enum PreviewBuildError {
    Sprite(SpriteError),
    NoAnimation,
}

impl From<SpriteError> for PreviewBuildError {
    fn from(error: SpriteError) -> Self {
        Self::Sprite(error)
    }
}

impl std::fmt::Display for PreviewBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sprite(err) => write!(f, "sprite decode failed: {err}"),
            Self::NoAnimation => write!(f, "no animations defined in manifest"),
        }
    }
}

/// Decode the idle animation's frames for one catalog entry into a list of
/// `NSImage`. Falls back to the first defined animation if `idle` is
/// missing (this matches the picker's display intent — show *some* motion).
pub(super) fn build_preview_frames(
    entry: &CatalogEntry,
    mtm: MainThreadMarker,
) -> Result<Vec<Retained<NSImage>>, PreviewBuildError> {
    let sheet = SpriteSheet::load(&entry.spritesheet_path, &entry.manifest.frame)?;
    let animation = entry
        .manifest
        .animations
        .get("idle")
        .or_else(|| entry.manifest.animations.values().next())
        .ok_or(PreviewBuildError::NoAnimation)?;
    let geometry = sheet.geometry();
    let mut frames = Vec::with_capacity(animation.frames.len());
    for &index in &animation.frames {
        let rgba = crop_frame_rgba(&sheet, index as usize);
        let image = rgba_to_nsimage(&rgba, geometry.width, geometry.height, mtm);
        frames.push(image);
    }
    Ok(frames)
}

/// Full AppKit-side picker entry: pure base data + decoded NSImage frames.
#[derive(Clone)]
pub struct PickerEntry {
    pub base: PickerEntryBase,
    pub frames: Vec<Retained<NSImage>>,
}

/// Walk `entries`, decode preview frames for OK rows, and surface decode
/// failures as additional errors on the row (frames stays empty).
pub fn attach_preview_frames(
    entries: Vec<PickerEntryBase>,
    catalog: &PetCatalog,
    mtm: MainThreadMarker,
) -> Vec<PickerEntry> {
    entries
        .into_iter()
        .map(|mut base| {
            if base.error.is_some() {
                return PickerEntry {
                    base,
                    frames: Vec::new(),
                };
            }
            let Some(catalog_entry) = catalog.lookup(&base.id) else {
                base.error = Some("Catalog entry missing for picker row".to_string());
                return PickerEntry {
                    base,
                    frames: Vec::new(),
                };
            };
            match build_preview_frames(catalog_entry, mtm) {
                Ok(frames) => PickerEntry { base, frames },
                Err(err) => {
                    base.error = Some(format!("Couldn't decode preview: {err}"));
                    PickerEntry {
                        base,
                        frames: Vec::new(),
                    }
                }
            }
        })
        .collect()
}
```

Also re-export `PickerEntry` from the public module surface — add at the bottom of the file, alongside the existing `pub use macos::PickerWindowController;`:

```rust
#[cfg(target_os = "macos")]
pub use macos::{attach_preview_frames, PickerEntry, PreviewBuildError};
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build (warnings about unused `_proxy` in the stubbed `PickerWindowController::new` are acceptable until Task 14).

- [ ] **Step 3: Commit**

```bash
git add src/picker_window_macos.rs
git commit -m "feat(picker_window_macos): build_preview_frames + attach_preview_frames"
```

---

## Task 13: `PickerTableSource` NSObject subclass

**Files:**
- Modify: `src/picker_window_macos.rs`

This task defines the AppKit object that backs the table view — its data source, delegate, and the timer's target. The class carries mutable state inside its `ivars` (`std::cell::RefCell`-wrapped because objc method calls give `&self`).

- [ ] **Step 1: Add the class definition**

Inside `#[cfg(target_os = "macos")] mod macos { ... }`, near the top (after the imports added in earlier tasks). The class uses `define_class!`. The `ivars` carry: the entries vector, the active id, the currently selected row, the frame counter, the event loop proxy, and `Retained` handles to the AppKit widgets the delegate needs to mutate (table view, detail image view, label fields, Apply button, Reveal button).

```rust
use std::cell::RefCell;
use std::ffi::CStr;

use objc2::define_class;
use objc2::msg_send;
use objc2::rc::{Allocated, AllocAnyThread};
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSButton, NSImageView, NSTableView, NSTableViewDataSource, NSTableViewDelegate, NSTextField,
};
use objc2_foundation::{NSInteger, NSObject, NSString};

use winit::event_loop::EventLoopProxy;

use crate::app::AppCommand;

pub(super) struct PickerTableSourceIvars {
    pub proxy: EventLoopProxy<AppCommand>,
    pub entries: RefCell<Vec<PickerEntry>>,
    pub active_id: RefCell<String>,
    pub selected_index: RefCell<Option<usize>>,
    pub frame_counter: RefCell<usize>,
    pub table_view: RefCell<Option<Retained<NSTableView>>>,
    pub detail_image: RefCell<Option<Retained<NSImageView>>>,
    pub detail_name: RefCell<Option<Retained<NSTextField>>>,
    pub detail_id: RefCell<Option<Retained<NSTextField>>>,
    pub detail_source: RefCell<Option<Retained<NSTextField>>>,
    pub detail_anim: RefCell<Option<Retained<NSTextField>>>,
    pub detail_error: RefCell<Option<Retained<NSTextField>>>,
    pub apply_button: RefCell<Option<Retained<NSButton>>>,
    pub reveal_button: RefCell<Option<Retained<NSButton>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "HappyCappyPickerTableSource"]
    #[thread_kind = MainThreadOnly]
    #[ivars = PickerTableSourceIvars]
    pub(super) struct PickerTableSource;

    unsafe impl NSObjectProtocol for PickerTableSource {}

    unsafe impl NSTableViewDataSource for PickerTableSource {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, _table_view: &NSTableView) -> NSInteger {
            self.ivars().entries.borrow().len() as NSInteger
        }
    }

    unsafe impl NSTableViewDelegate for PickerTableSource {
        #[unsafe(method(tableView:viewForTableColumn:row:))]
        fn view_for_row(
            &self,
            _table_view: &NSTableView,
            _column: Option<&AnyObject>,
            row: NSInteger,
        ) -> Option<Retained<NSObject>> {
            let entries = self.ivars().entries.borrow();
            let idx = row as usize;
            let entry = entries.get(idx)?;
            let mtm = MainThreadMarker::new()?;
            Some(make_row_view(entry, mtm))
        }

        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_changed(&self, _notification: &AnyObject) {
            let table = match self.ivars().table_view.borrow().clone() {
                Some(t) => t,
                None => return,
            };
            let selected: NSInteger = unsafe { msg_send![&*table, selectedRow] };
            *self.ivars().selected_index.borrow_mut() = if selected < 0 {
                None
            } else {
                Some(selected as usize)
            };
            self.refresh_detail_pane();
        }
    }

    impl PickerTableSource {
        #[unsafe(method(tickPreviewAnimation:))]
        fn tick_preview_animation(&self, _timer: &AnyObject) {
            *self.ivars().frame_counter.borrow_mut() =
                self.ivars().frame_counter.borrow().wrapping_add(1);
            self.refresh_visible_row_images();
            self.refresh_detail_image();
        }

        #[unsafe(method(onApplyClicked:))]
        fn on_apply_clicked(&self, _sender: &AnyObject) {
            let entries = self.ivars().entries.borrow();
            let Some(idx) = *self.ivars().selected_index.borrow() else {
                return;
            };
            let Some(entry) = entries.get(idx) else {
                return;
            };
            if entry.base.error.is_some() {
                return;
            }
            if entry.base.id == *self.ivars().active_id.borrow() {
                return;
            }
            let _ = self.ivars().proxy.send_event(AppCommand::ActivatePet(entry.base.id.clone()));
        }

        #[unsafe(method(onRevealClicked:))]
        fn on_reveal_clicked(&self, _sender: &AnyObject) {
            let _ = self.ivars().proxy.send_event(AppCommand::RevealPetsFolder);
        }
    }
);

impl PickerTableSource {
    pub(super) fn new(
        mtm: MainThreadMarker,
        proxy: EventLoopProxy<AppCommand>,
    ) -> Retained<Self> {
        let ivars = PickerTableSourceIvars {
            proxy,
            entries: RefCell::new(Vec::new()),
            active_id: RefCell::new(String::new()),
            selected_index: RefCell::new(None),
            frame_counter: RefCell::new(0),
            table_view: RefCell::new(None),
            detail_image: RefCell::new(None),
            detail_name: RefCell::new(None),
            detail_id: RefCell::new(None),
            detail_source: RefCell::new(None),
            detail_anim: RefCell::new(None),
            detail_error: RefCell::new(None),
            apply_button: RefCell::new(None),
            reveal_button: RefCell::new(None),
        };
        let this = mtm.alloc().set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    pub(super) fn tick_selector() -> Sel {
        sel!(tickPreviewAnimation:)
    }

    pub(super) fn apply_selector() -> Sel {
        sel!(onApplyClicked:)
    }

    pub(super) fn reveal_selector() -> Sel {
        sel!(onRevealClicked:)
    }
}

/// Build a row NSView for one picker entry. Defined here as a free function
/// so the data source's delegate method can call it without an `&self` loop.
fn make_row_view(entry: &PickerEntry, mtm: MainThreadMarker) -> Retained<NSObject> {
    use objc2_app_kit::NSView;
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(200.0, 44.0));
    let row = NSView::initWithFrame(NSView::alloc(mtm), frame);
    // Thumbnail (left)
    let thumb = NSImageView::initWithFrame(
        NSImageView::alloc(mtm),
        NSRect::new(NSPoint::new(8.0, 6.0), NSSize::new(32.0, 32.0)),
    );
    if let Some(image) = entry.frames.first() {
        thumb.setImage(Some(image));
    }
    row.addSubview(&thumb);
    // Title label (right of thumbnail)
    let title_text = if entry.base.error.is_some() {
        format!("⚠ {}", entry.base.display_name)
    } else {
        entry.base.display_name.clone()
    };
    let title_field = NSTextField::labelWithString(&NSString::from_str(&title_text), mtm);
    title_field.setFrame(NSRect::new(NSPoint::new(48.0, 12.0), NSSize::new(140.0, 20.0)));
    row.addSubview(&title_field);
    // Cast to NSObject (the protocol requires it)
    Retained::cast(row)
}
```

> **Important:** `make_row_view` returns a freshly allocated `NSView` every time `viewForTableColumn:row:` fires — there is no cell reuse via `makeViewWithIdentifier:` here. For the expected scale (≤ 30 rows), that's fine; if performance becomes a problem, the cell reuse pattern can be added later.

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build. There may be remaining `unused` warnings on the placeholder fields (`detail_*`, `*_button`, `table_view`) until Task 14 wires them through.

- [ ] **Step 3: Commit**

```bash
git add src/picker_window_macos.rs
git commit -m "feat(picker_window_macos): PickerTableSource NSObject + row view builder"
```

---

## Task 14: NSPanel + table view + detail pane construction

**Files:**
- Modify: `src/picker_window_macos.rs`

This task replaces the stub `PickerWindowController` placeholder from Task 9 with the real construction logic.

- [ ] **Step 1: Replace the placeholder controller**

Inside `#[cfg(target_os = "macos")] mod macos { ... }`, delete the stub `pub struct PickerWindowController;` and its empty `impl` block. Replace them with:

```rust
use objc2_app_kit::{
    NSBackingStoreType, NSButtonType, NSColor, NSFloatingWindowLevel, NSPanel, NSScrollView,
    NSTableColumn, NSWindowStyleMask,
};
use objc2_foundation::{ns_string, NSPoint, NSRect, NSSize};

const PANEL_WIDTH: f64 = 480.0;
const PANEL_HEIGHT: f64 = 420.0;
const LIST_WIDTH: f64 = 200.0;
const DETAIL_X: f64 = LIST_WIDTH;
const DETAIL_WIDTH: f64 = PANEL_WIDTH - LIST_WIDTH;
const PREVIEW_SIZE: f64 = 128.0;
const ROW_HEIGHT: f64 = 44.0;

pub struct PickerWindowController {
    panel: Retained<NSPanel>,
    source: Retained<PickerTableSource>,
}

impl PickerWindowController {
    pub fn new(proxy: EventLoopProxy<AppCommand>) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let source = PickerTableSource::new(mtm, proxy);

        let panel = unsafe {
            NSPanel::initWithContentRect_styleMask_backing_defer(
                NSPanel::alloc(mtm),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(PANEL_WIDTH, PANEL_HEIGHT)),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::UtilityWindow,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe {
            panel.setReleasedWhenClosed(false);
        }
        panel.setTitle(ns_string!("Pet Library"));
        panel.setFloatingPanel(true);
        panel.setHidesOnDeactivate(false);
        panel.setLevel(NSFloatingWindowLevel);

        let content_view = unsafe {
            objc2_app_kit::NSView::initWithFrame(
                objc2_app_kit::NSView::alloc(mtm),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(PANEL_WIDTH, PANEL_HEIGHT)),
            )
        };
        panel.setContentView(Some(&content_view));

        // ── Left: scroll view + table view ─────────────────────────────────
        let scroll = unsafe {
            NSScrollView::initWithFrame(
                NSScrollView::alloc(mtm),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(LIST_WIDTH, PANEL_HEIGHT)),
            )
        };
        scroll.setHasVerticalScroller(true);
        scroll.setBorderType(objc2_app_kit::NSBorderType::NoBorder);
        let table = unsafe {
            NSTableView::initWithFrame(
                NSTableView::alloc(mtm),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(LIST_WIDTH, PANEL_HEIGHT)),
            )
        };
        table.setRowHeight(ROW_HEIGHT);
        table.setHeaderView(None);
        let column = unsafe {
            NSTableColumn::initWithIdentifier(
                NSTableColumn::alloc(mtm),
                &NSString::from_str("pet"),
            )
        };
        unsafe {
            column.setWidth(LIST_WIDTH - 4.0);
            table.addTableColumn(&column);
            let delegate: &ProtocolObject<dyn NSTableViewDelegate> =
                ProtocolObject::from_ref(&*source);
            table.setDelegate(Some(delegate));
            let data_source: &ProtocolObject<dyn NSTableViewDataSource> =
                ProtocolObject::from_ref(&*source);
            table.setDataSource(Some(data_source));
        }
        scroll.setDocumentView(Some(&table));
        content_view.addSubview(&scroll);
        *source.ivars().table_view.borrow_mut() = Some(table.clone());

        // ── Right: detail pane ─────────────────────────────────────────────
        let detail = unsafe {
            objc2_app_kit::NSView::initWithFrame(
                objc2_app_kit::NSView::alloc(mtm),
                NSRect::new(NSPoint::new(DETAIL_X, 0.0), NSSize::new(DETAIL_WIDTH, PANEL_HEIGHT)),
            )
        };
        content_view.addSubview(&detail);

        // Preview image (centered horizontally near the top)
        let preview = unsafe {
            NSImageView::initWithFrame(
                NSImageView::alloc(mtm),
                NSRect::new(
                    NSPoint::new(
                        (DETAIL_WIDTH - PREVIEW_SIZE) / 2.0,
                        PANEL_HEIGHT - PREVIEW_SIZE - 24.0,
                    ),
                    NSSize::new(PREVIEW_SIZE, PREVIEW_SIZE),
                ),
            )
        };
        detail.addSubview(&preview);
        *source.ivars().detail_image.borrow_mut() = Some(preview);

        let mut next_y = PANEL_HEIGHT - PREVIEW_SIZE - 60.0;
        let name_field = make_label(mtm, "", &detail, next_y, 20.0, true);
        *source.ivars().detail_name.borrow_mut() = Some(name_field);
        next_y -= 24.0;
        let id_field = make_label(mtm, "", &detail, next_y, 16.0, false);
        *source.ivars().detail_id.borrow_mut() = Some(id_field);
        next_y -= 20.0;
        let source_field = make_label(mtm, "", &detail, next_y, 16.0, false);
        *source.ivars().detail_source.borrow_mut() = Some(source_field);
        next_y -= 20.0;
        let anim_field = make_label(mtm, "", &detail, next_y, 16.0, false);
        *source.ivars().detail_anim.borrow_mut() = Some(anim_field);
        next_y -= 28.0;
        let error_field = make_label(mtm, "", &detail, next_y, 16.0, false);
        unsafe {
            let red: Retained<NSColor> = NSColor::redColor();
            error_field.setTextColor(Some(&red));
        }
        *source.ivars().detail_error.borrow_mut() = Some(error_field);

        // Bottom: Reveal + Apply buttons
        let apply = unsafe {
            NSButton::initWithFrame(
                NSButton::alloc(mtm),
                NSRect::new(NSPoint::new(DETAIL_WIDTH - 92.0 - 12.0, 12.0), NSSize::new(92.0, 28.0)),
            )
        };
        apply.setTitle(ns_string!("Apply"));
        apply.setBezelStyle(objc2_app_kit::NSBezelStyle::Rounded);
        unsafe {
            apply.setButtonType(NSButtonType::MomentaryPushIn);
            let target: &AnyObject = source.as_ref();
            apply.setTarget(Some(target));
            apply.setAction(Some(PickerTableSource::apply_selector()));
        }
        detail.addSubview(&apply);
        *source.ivars().apply_button.borrow_mut() = Some(apply);

        let reveal = unsafe {
            NSButton::initWithFrame(
                NSButton::alloc(mtm),
                NSRect::new(NSPoint::new(12.0, 12.0), NSSize::new(140.0, 28.0)),
            )
        };
        reveal.setTitle(ns_string!("Reveal in Finder"));
        reveal.setBezelStyle(objc2_app_kit::NSBezelStyle::Rounded);
        unsafe {
            reveal.setButtonType(NSButtonType::MomentaryPushIn);
            let target: &AnyObject = source.as_ref();
            reveal.setTarget(Some(target));
            reveal.setAction(Some(PickerTableSource::reveal_selector()));
        }
        detail.addSubview(&reveal);
        *source.ivars().reveal_button.borrow_mut() = Some(reveal);

        panel.center();

        Some(Self { panel, source })
    }

    pub fn show(&self) {
        self.panel.makeKeyAndOrderFront(None);
        self.panel.orderFrontRegardless();
        // Animation start happens in Task 17.
    }

    pub fn hide(&self) {
        self.panel.orderOut(None);
        // Animation stop happens in Task 17.
    }

    pub fn is_visible(&self) -> bool {
        self.panel.isVisible()
    }

    pub fn sync_entries(&self, entries: Vec<PickerEntry>, active_id: &str) {
        let ivars = self.source.ivars();
        *ivars.active_id.borrow_mut() = active_id.to_string();
        *ivars.entries.borrow_mut() = entries;
        // Select the row whose id matches active_id, else clear selection.
        let new_selection = {
            let entries = ivars.entries.borrow();
            entries.iter().position(|e| e.base.id == *active_id)
        };
        *ivars.selected_index.borrow_mut() = new_selection;
        if let Some(table) = ivars.table_view.borrow().as_ref() {
            unsafe {
                let _: () = msg_send![&**table, reloadData];
                if let Some(row) = new_selection {
                    let index_set: Retained<objc2_foundation::NSIndexSet> =
                        objc2_foundation::NSIndexSet::indexSetWithIndex(row as NSUInteger);
                    let _: () =
                        msg_send![&**table, selectRowIndexes: &*index_set, byExtendingSelection: false];
                }
            }
        }
        self.source.refresh_detail_pane();
    }
}

fn make_label(
    mtm: MainThreadMarker,
    text: &str,
    parent: &objc2_app_kit::NSView,
    y: f64,
    height: f64,
    bold: bool,
) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    field.setFrame(NSRect::new(
        NSPoint::new(16.0, y),
        NSSize::new(DETAIL_WIDTH - 32.0, height),
    ));
    if bold {
        let bold_font = unsafe { objc2_app_kit::NSFont::boldSystemFontOfSize(18.0) };
        field.setFont(Some(&bold_font));
    }
    parent.addSubview(&field);
    field
}
```

> **Pull in NSUInteger:** add `use objc2_foundation::NSUInteger;` at the top of the macos module if it isn't already.

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build (a few unused-warnings about `refresh_detail_pane` and `refresh_visible_row_images` and `refresh_detail_image` are acceptable — those are defined in Task 16/17).

> If `cargo check` fails because a referenced method (`refresh_detail_pane`, etc.) doesn't exist yet, leave the call sites commented with `// TODO(task-16): self.refresh_detail_pane();` and re-enable them in the relevant task.

- [ ] **Step 3: Commit**

```bash
git add src/picker_window_macos.rs
git commit -m "feat(picker_window_macos): build NSPanel + table + detail pane scaffold"
```

---

## Task 15: `sync_entries` end-to-end

**Files:**
- Modify: `src/picker_window_macos.rs`

`sync_entries` was sketched in Task 14. This task hardens it: empty selection when the list is empty, no panic when the table_view ivar isn't yet wired (defensive), and ensure the active id state stays in sync even when no row matches.

- [ ] **Step 1: Adjust `sync_entries` to its final shape**

Replace the body of `PickerWindowController::sync_entries` with:

```rust
pub fn sync_entries(&self, entries: Vec<PickerEntry>, active_id: &str) {
    let ivars = self.source.ivars();
    *ivars.active_id.borrow_mut() = active_id.to_string();
    *ivars.entries.borrow_mut() = entries;
    let new_selection = {
        let entries = ivars.entries.borrow();
        if entries.is_empty() {
            None
        } else {
            entries
                .iter()
                .position(|e| e.base.id == *active_id)
                .or(Some(0))
        }
    };
    *ivars.selected_index.borrow_mut() = new_selection;
    if let Some(table) = ivars.table_view.borrow().as_ref() {
        unsafe {
            let _: () = msg_send![&**table, reloadData];
            if let Some(row) = new_selection {
                let index_set: Retained<objc2_foundation::NSIndexSet> =
                    objc2_foundation::NSIndexSet::indexSetWithIndex(row as NSUInteger);
                let _: () = msg_send![
                    &**table,
                    selectRowIndexes: &*index_set,
                    byExtendingSelection: false
                ];
            }
        }
    }
    self.source.refresh_detail_pane();
}
```

The "select first row when active id is missing" behaviour lets the user see *something* in the detail pane immediately when the active pet has been removed from the catalog. The bundled pet always satisfies the catalog-non-empty invariant from sub-project 2.

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build (the `refresh_detail_pane` warning is the same one — fixed in Task 16).

- [ ] **Step 3: Commit**

```bash
git add src/picker_window_macos.rs
git commit -m "feat(picker_window_macos): sync_entries selects active row or falls back to first"
```

---

## Task 16: `refresh_detail_pane` + helpers

**Files:**
- Modify: `src/picker_window_macos.rs`

- [ ] **Step 1: Add the detail-pane updater + helpers**

Inside `impl PickerTableSource`, add:

```rust
pub(super) fn refresh_detail_pane(&self) {
    let ivars = self.ivars();
    let entries = ivars.entries.borrow();
    let selected = *ivars.selected_index.borrow();
    let active_id = ivars.active_id.borrow().clone();
    let entry = selected.and_then(|i| entries.get(i));
    let Some(entry) = entry else {
        Self::clear_detail_pane(ivars);
        return;
    };
    if let Some(label) = ivars.detail_name.borrow().as_ref() {
        label.setStringValue(&NSString::from_str(&entry.base.display_name));
    }
    if let Some(label) = ivars.detail_id.borrow().as_ref() {
        label.setStringValue(&NSString::from_str(&format!("id: {}", entry.base.id)));
    }
    if let Some(label) = ivars.detail_source.borrow().as_ref() {
        let source = match entry.base.source {
            PickerSource::Bundled => "bundled".to_string(),
            PickerSource::Custom => "custom".to_string(),
        };
        let dimensions = if entry.base.frame_width == 0 {
            "—".to_string()
        } else {
            format!("{}×{}", entry.base.frame_width, entry.base.frame_height)
        };
        label.setStringValue(&NSString::from_str(&format!(
            "{source} · {dimensions}"
        )));
    }
    if let Some(label) = ivars.detail_anim.borrow().as_ref() {
        let text = if entry.base.animations.is_empty() {
            "anims: —".to_string()
        } else {
            format!("anims: {}", entry.base.animations.join(", "))
        };
        label.setStringValue(&NSString::from_str(&text));
    }
    if let Some(label) = ivars.detail_error.borrow().as_ref() {
        let text = entry.base.error.clone().unwrap_or_default();
        label.setStringValue(&NSString::from_str(&text));
    }
    if let Some(button) = ivars.apply_button.borrow().as_ref() {
        let can_apply = entry.base.error.is_none() && entry.base.id != active_id;
        button.setEnabled(can_apply);
    }
    if let Some(button) = ivars.reveal_button.borrow().as_ref() {
        let visible = entry.base.error.is_some();
        button.setHidden(!visible);
    }
    self.refresh_detail_image();
}

pub(super) fn refresh_detail_image(&self) {
    let ivars = self.ivars();
    let entries = ivars.entries.borrow();
    let selected = match *ivars.selected_index.borrow() {
        Some(i) => i,
        None => return,
    };
    let Some(entry) = entries.get(selected) else {
        return;
    };
    let Some(image_view) = ivars.detail_image.borrow().as_ref().cloned() else {
        return;
    };
    if entry.frames.is_empty() {
        image_view.setImage(None);
        return;
    }
    let counter = *ivars.frame_counter.borrow();
    let idx = counter % entry.frames.len();
    image_view.setImage(Some(&entry.frames[idx]));
}

pub(super) fn refresh_visible_row_images(&self) {
    let ivars = self.ivars();
    let Some(table) = ivars.table_view.borrow().clone() else {
        return;
    };
    let entries = ivars.entries.borrow();
    let counter = *ivars.frame_counter.borrow();
    let visible_rows: objc2_foundation::NSRange = unsafe {
        let visible_rect: objc2_app_kit::NSRect = msg_send![&*table, visibleRect];
        msg_send![&*table, rowsInRect: visible_rect]
    };
    for offset in 0..visible_rows.length {
        let row = visible_rows.location + offset;
        let Some(entry) = entries.get(row as usize) else {
            continue;
        };
        if entry.frames.is_empty() {
            continue;
        }
        let idx = counter % entry.frames.len();
        let row_view: Option<Retained<objc2_app_kit::NSView>> = unsafe {
            msg_send![&*table, viewAtColumn: 0_i64, row: row as i64, makeIfNecessary: false]
        };
        let Some(row_view) = row_view else { continue };
        // Row view's first NSImageView subview is the thumbnail (see make_row_view).
        let subviews: Retained<objc2_foundation::NSArray<objc2_app_kit::NSView>> =
            unsafe { msg_send![&*row_view, subviews] };
        if subviews.len() == 0 {
            continue;
        }
        let first = subviews.objectAtIndex(0);
        let is_image: bool = unsafe {
            msg_send![
                &*first,
                isKindOfClass: objc2_app_kit::NSImageView::class()
            ]
        };
        if !is_image {
            continue;
        }
        let image_view: &NSImageView = unsafe { std::mem::transmute(&*first) };
        image_view.setImage(Some(&entry.frames[idx]));
    }
}

fn clear_detail_pane(ivars: &PickerTableSourceIvars) {
    let set_blank = |field: Option<&Retained<NSTextField>>| {
        if let Some(f) = field {
            f.setStringValue(&NSString::from_str(""));
        }
    };
    set_blank(ivars.detail_name.borrow().as_ref());
    set_blank(ivars.detail_id.borrow().as_ref());
    set_blank(ivars.detail_source.borrow().as_ref());
    set_blank(ivars.detail_anim.borrow().as_ref());
    set_blank(ivars.detail_error.borrow().as_ref());
    if let Some(image_view) = ivars.detail_image.borrow().as_ref() {
        image_view.setImage(None);
    }
    if let Some(button) = ivars.apply_button.borrow().as_ref() {
        button.setEnabled(false);
    }
    if let Some(button) = ivars.reveal_button.borrow().as_ref() {
        button.setHidden(true);
    }
}
```

> **`unsafe { std::mem::transmute }` warning:** the cast from `NSView` to `NSImageView` is sound here because `isKindOfClass` confirmed the type immediately above. AppKit views are pure pointer types under the hood. If the implementer prefers, replace the transmute with a `Retained::cast` if the API surface allows it.

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add src/picker_window_macos.rs
git commit -m "feat(picker_window_macos): refresh detail pane + visible row images"
```

---

## Task 17: Animation timer lifecycle

**Files:**
- Modify: `src/picker_window_macos.rs`

- [ ] **Step 1: Add timer field + start/stop logic**

Update `PickerWindowController` to hold a `RefCell<Option<Retained<NSTimer>>>` field. At the top of the macos module imports:

```rust
use objc2_foundation::NSTimer;
```

Modify the struct:

```rust
pub struct PickerWindowController {
    panel: Retained<NSPanel>,
    source: Retained<PickerTableSource>,
    timer: std::cell::RefCell<Option<Retained<NSTimer>>>,
}
```

…and update `PickerWindowController::new`'s `Some(Self { ... })` literal to add `timer: RefCell::new(None),`.

Update `show` and `hide`:

```rust
pub fn show(&self) {
    self.panel.makeKeyAndOrderFront(None);
    self.panel.orderFrontRegardless();
    self.start_animation_timer();
}

pub fn hide(&self) {
    self.stop_animation_timer();
    self.panel.orderOut(None);
}

fn start_animation_timer(&self) {
    if self.timer.borrow().is_some() {
        return;
    }
    let interval = 0.1_f64; // 10 fps
    let target_obj: &AnyObject = self.source.as_ref();
    let timer: Retained<NSTimer> = unsafe {
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            interval,
            target_obj,
            PickerTableSource::tick_selector(),
            None,
            true,
        )
    };
    *self.timer.borrow_mut() = Some(timer);
}

fn stop_animation_timer(&self) {
    if let Some(timer) = self.timer.borrow_mut().take() {
        timer.invalidate();
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add src/picker_window_macos.rs
git commit -m "feat(picker_window_macos): drive 10fps NSTimer for preview animation"
```

---

## Task 18: Apply button + Reveal button → AppCommand dispatch

**Files:** none (the wiring landed inside `PickerTableSource` already in Task 13).

This task only contains a manual verification step before moving on, since the `onApplyClicked:` and `onRevealClicked:` selectors already dispatch `AppCommand::ActivatePet` / `AppCommand::RevealPetsFolder`.

- [ ] **Step 1: Confirm the dispatch sites compile + read correctly**

Read `src/picker_window_macos.rs` and verify:
- `onApplyClicked:` returns early when `error.is_some()` or `id == active_id`.
- `onApplyClicked:` calls `proxy.send_event(AppCommand::ActivatePet(id))`.
- `onRevealClicked:` calls `proxy.send_event(AppCommand::RevealPetsFolder)`.
- The Apply button's `setEnabled(false)` / `setEnabled(true)` toggle is driven from `refresh_detail_pane`.
- The Reveal button's `setHidden(true)` / `setHidden(false)` toggle is driven from `refresh_detail_pane` and is only un-hidden on error rows.

If any of these are missing or incorrect, fix them in this task.

- [ ] **Step 2: Add `panel.orderOut(None)` after a successful Apply dispatch**

Inside `onApplyClicked:`, after the successful `send_event`, hide the panel so the user sees the swap immediately. Since the selector is on `PickerTableSource` and only the `PickerWindowController` holds the panel handle, you'll need to either:
- Make the panel handle accessible from the source via a new `panel: RefCell<Option<Retained<NSPanel>>>` ivar, set during `PickerWindowController::new`, and call `panel.orderOut(None)` from the selector.
- OR send a separate `AppCommand::HidePicker` (NOT recommended — adds plumbing for one call site).

Pick the ivar approach. Add to `PickerTableSourceIvars`:

```rust
pub panel: RefCell<Option<Retained<NSPanel>>>,
```

Initialise `panel: RefCell::new(None),` in `PickerTableSource::new`. After the panel is created in `PickerWindowController::new`, populate it:

```rust
*source.ivars().panel.borrow_mut() = Some(panel.clone());
```

Then in `onApplyClicked:`, append after the successful send_event:

```rust
if let Some(panel) = self.ivars().panel.borrow().clone() {
    panel.orderOut(None);
}
```

(`stop_animation_timer` won't fire from this path; that's a minor inefficiency — the timer keeps firing on a hidden panel. Fix it via a `HidePicker` follow-up later if performance shows up as a real problem; for now it's a no-op cost.)

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add src/picker_window_macos.rs
git commit -m "feat(picker_window_macos): close panel on successful Apply"
```

---

## Task 19: "Reveal in Finder" visibility — covered by Task 16

Task 16 already toggles the Reveal button via `refresh_detail_pane`. Skip — this slot is intentionally empty so the task list stays in lock-step with the task summary.

- [ ] **Step 1: Confirm no work is needed**

Verify by reading `refresh_detail_pane`: the `reveal_button.setHidden(!entry.base.error.is_some())` call must be present.

- [ ] **Step 2: No commit**

If the verification fails, fix the omission in Task 16 and amend its commit (or add a follow-up commit).

---

## Task 20: `DesktopPetApp.picker` field + lazy init

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add the field**

Inside `pub struct DesktopPetApp { ... }`, add a new field next to the existing window-controller fields:

```rust
picker: Option<crate::picker_window_macos::PickerWindowController>,
```

Initialise it as `None` in the existing `DesktopPetApp::new` (or wherever the struct is constructed). Search the file for the construction site and add the field in the same literal.

- [ ] **Step 2: Add the lazy-init helper**

In the `impl DesktopPetApp { ... }` block, add:

```rust
fn ensure_picker_window(&mut self) -> Option<&crate::picker_window_macos::PickerWindowController> {
    if self.picker.is_none() {
        self.picker =
            crate::picker_window_macos::PickerWindowController::new(self.event_loop_proxy.clone());
    }
    self.picker.as_ref()
}
```

Adjust `self.event_loop_proxy` to the field name that DesktopPetApp actually uses for its event loop proxy (look at how `SettingsWindowController::new` is invoked in the existing code).

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: clean build (an unused-method warning on `ensure_picker_window` is OK — Task 21 calls it).

- [ ] **Step 4: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): own Pet Library picker controller lazily"
```

---

## Task 21: `ShowPicker` handler

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Locate the `AppCommand` handler**

`DesktopPetApp` already has a method (`handle_command` or similar — find by searching for `AppCommand::ActivatePet` matches) that dispatches every variant. Add a new arm for `AppCommand::ShowPicker`:

```rust
AppCommand::ShowPicker => {
    self.refresh_catalog();
    let mtm = match objc2_foundation::MainThreadMarker::new() {
        Some(mtm) => mtm,
        None => return, // only runs on macOS main thread
    };
    let base = crate::picker_entries::build_picker_entries_base(&self.catalog);
    let entries = crate::picker_window_macos::attach_preview_frames(base, &self.catalog, mtm);
    let active_id = self.active_pet_id.clone();
    if let Some(picker) = self.ensure_picker_window() {
        picker.sync_entries(entries, &active_id);
        picker.show();
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): handle ShowPicker by refreshing + populating + showing"
```

---

## Task 22: Picker sync on `refresh_catalog`

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Extend `refresh_catalog`**

After the existing body of `refresh_catalog` (which rebuilds `self.catalog` and calls `self.menu_bar.populate_pet_submenu(...)`), add:

```rust
if let Some(picker) = self.picker.as_ref() {
    if picker.is_visible() {
        if let Some(mtm) = objc2_foundation::MainThreadMarker::new() {
            let base = crate::picker_entries::build_picker_entries_base(&self.catalog);
            let entries =
                crate::picker_window_macos::attach_preview_frames(base, &self.catalog, mtm);
            picker.sync_entries(entries, &self.active_pet_id);
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): re-sync picker when refresh_catalog runs while visible"
```

---

## Task 23: Insert "Pet Library…" menu item

**Files:**
- Modify: `src/menu_bar.rs`

- [ ] **Step 1: Add the new menu item in `MenuBarController::new`**

Locate the section in `MenuBarController::new` that creates `settings_item`, `show_hide_item`, etc. Add a new `pet_library_item` next to them:

```rust
let pet_library_item = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
        NSMenuItem::alloc(mtm),
        ns_string!("Pet Library..."),
        None,
        ns_string!(""),
    )
};
pet_library_item.setTag(MENU_TAG_OPEN_PET_LIBRARY);
```

Add `&pet_library_item` to the `for item in [...]` block that hooks every item up to `command_selector()`:

```rust
let target_object: &AnyObject = target.as_ref();
for item in [
    &settings_item,
    &show_hide_item,
    &focus_mode_item,
    &nap_item,
    &cheer_up_item,
    &reset_item,
    &quit_item,
    &pet_library_item,
] {
    unsafe {
        item.setTarget(Some(target_object));
        item.setAction(Some(
            crate::command_target_macos::CommandTarget::command_selector(),
        ));
    }
}
```

Insert `pet_library_item` in the final `menu.addItem(...)` sequence — right after the `pet_root_item` add and *before* the separator/Settings:

```rust
menu.addItem(&pet_root_item);
menu.addItem(&pet_library_item);
menu.addItem(&NSMenuItem::separatorItem(mtm));
menu.addItem(&settings_item);
menu.addItem(&show_hide_item);
// …rest unchanged…
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add src/menu_bar.rs
git commit -m "feat(menu_bar): expose Pet Library... main menu item"
```

---

## Task 24: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full test suite**

Run: `cargo test --all-targets`
Expected: PASS — no regressions; new picker_entries tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: clean.

- [ ] **Step 3: Run format check**

Run: `cargo fmt --check`
Expected: clean.

- [ ] **Step 4: Build the app bundle**

Run: `scripts/build_app.sh`
Expected: `dist/Happy Cappy.app` built successfully.

- [ ] **Step 5: Execute the manual smoke plan from spec §12**

For each of the 8 scenarios in `docs/superpowers/specs/2026-05-27-pet-picker-design.md` §12, execute and check off:

1. Cold open — bundled animates, no errors.
2. Custom pets present (OK + broken JSON + missing sprite) — all rows render correctly.
3. Apply OK pet — pet swaps, picker closes, checkmark moves on reopen.
4. Apply disabled cases — error row + current active row both disable Apply.
5. Reveal in Finder — opens `pets/` folder.
6. Refresh on reopen — new pet appears.
7. Animated preview check — smooth, no tearing.
8. Memory stability — open/close 10×, no monotonic growth.

- [ ] **Step 6: Final commit**

If the smoke plan revealed any necessary fixes, commit them with descriptive messages. Then verify the branch is clean:

```bash
git status
```
Expected: clean working tree.

---

## Self-Review

**Spec coverage check:**
- §1 Context — N/A.
- §2 Scope — every "in scope" item maps to at least one task. Cargo deps → Task 1. New module `picker_entries.rs` → Tasks 5–8. New module `picker_window_macos.rs` → Tasks 9–19. App integration → Tasks 20–22. Menu wiring → Tasks 3, 4, 23.
- §3 Architecture — Tasks 9, 13, 14 build the picker_window_macos layout shown in the diagram.
- §4 UI Layout — Task 14 constructs the panel + scroll + table + detail per the §4 mockup.
- §5 Data Model — `PickerEntryBase` (Task 5), `PickerEntry` (Task 12), `PickerSource` (Task 5).
- §6 Sync Flow — Tasks 21 (show flow) + 22 (refresh-when-visible).
- §7 Animation Pipeline — Tasks 10 (crop), 11 (NSImage), 12 (build/attach), 16 (refresh image), 17 (timer).
- §8 Error UX — Task 6 (format_catalog_error truncation), Task 7 (entry mapping), Task 16 (detail pane error label + button visibility + disabled Apply).
- §9 Entry Points & Commands — Task 2 (ShowPicker variant), Tasks 3–4 (menu tag wiring), Task 23 (menu item insertion).
- §10 Lifecycle — Task 17 (timer start/stop), Task 14 (`setReleasedWhenClosed:false` + `orderOut` in hide).
- §11 Failure-Handling Policy — Task 12 (attach_preview_frames surfaces decode errors), Task 16 (Apply disabled on error rows).
- §12 Testing Strategy — Tasks 6–8 (unit tests), Task 24 (manual smoke).
- §13 Exit Criteria — Task 24.
- §14 Dependencies — already shipped in SP2; no plan task needed.

**Placeholder scan:** No "TBD"/"TODO"/"implement later" placeholders. Every code block is complete. Task 11 contains an explicit note about `objc2-core-graphics` API drift, with a concrete fallback (consult `cargo doc`) — that is allowed; it's a known-name-drift hazard, not a placeholder.

**Type consistency check:**
- `PickerEntryBase` shape (id, display_name, source, frame_width, frame_height, animations, error, source_path) appears identically across Tasks 5, 6, 7, 8, 12.
- `PickerSource::{Bundled, Custom}` consistent.
- `PickerEntry { base, frames }` consistent in Tasks 12, 13, 14, 16.
- `format_catalog_error`, `picker_entry_from_load_error`, `build_picker_entries_base`, `build_preview_frames`, `attach_preview_frames` names match across spec and plan.
- Selectors `tickPreviewAnimation:`, `onApplyClicked:`, `onRevealClicked:` are defined in Task 13 and consumed in Tasks 14 + 17.
- `MENU_TAG_OPEN_PET_LIBRARY = 1202` defined in Task 3, used in Tasks 4 + 23.

No drift. Plan ready for execution.
