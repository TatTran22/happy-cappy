# Pet Catalog & Custom Pet Loading — Design Spec (Sub-Project 2)

**Status:** approved
**Date:** 2026-05-27
**Predecessor:** [2026-05-26-pet-manifest-refactor-design.md](./2026-05-26-pet-manifest-refactor-design.md) (sub-project 1, merged at `dd8c750`)
**Roadmap position:** sub-project 2 of 4

## 1. Context

Sub-project 1 turned the pet's animation table into a data-driven manifest (`PetManifest`) and built a resolver that maps behavior + personality to animation names. The bundled Happy Cappy still ships as the only pet.

This sub-project adds:

1. **Disk loading** for additional pet manifests via `PetManifest::from_path` (parser core was already designed for symmetry in sub-project 1).
2. **A catalog** that holds the bundled pet plus any custom pets discovered on disk.
3. **Persisted active-pet selection** in `AppSettings`.
4. **Hot-swap at runtime** — change the active pet without restarting the app, preserving window position.
5. **A minimal selection surface** — a "Pet" submenu in the existing macOS menu bar.

The polished in-window picker UI is **sub-project 3** and is out of scope here. Sub-project 2's menu bar entry exists so the feature is testable end-to-end before sub-project 3 lands.

## 2. Scope

**In scope:**
- New module `src/pet/catalog.rs` (`PetCatalog`, `CatalogEntry`, `CatalogSource`, `CatalogLoadError`, `BundledPet`).
- `PetManifest::from_path` (thin wrapper over the existing `from_json_str`).
- `PetRuntime::new_with_manifest` (promote sub-project 1's `#[cfg(test)] new_with_manifest_for_test` into the public API; delete the test-only variant).
- `DesktopPetApp::activate_pet`, `DesktopPetApp::refresh_catalog`, `ActivationError`.
- `AppSettings::active_pet_id: Option<String>` (persisted, defaults to `None`).
- Menu bar additions: dynamic **Pet** submenu, "Reveal Pets Folder" item, `activatePet:` selector wiring.
- Filesystem layout under `~/Library/Application Support/Happy Cappy/pets/`: subdirectory-per-pet with `pet.json` + spritesheet, auto-created on first scan, `README.txt` written when missing.
- Comprehensive `catalog.rs` unit tests (15+ scenarios listed in §9), settings round-trip tests, runtime constructor test, app-level activation tests.

**Out of scope (explicitly deferred):**
- In-window picker UI with thumbnails / previews → sub-project 3.
- Surfacing `CatalogLoadError` in the UI (only logged in sub-project 2) → sub-project 3.
- Per-frame `ms`, `loop_start`, `fallback`, one-shot animations, notification → animation mapping → sub-project 4.
- Filesystem watcher / live reload while menu is closed → not planned (menu-open rescan covers it).
- iCloud / cloud-synced custom pets, downloadable pet packs → not planned.
- Editing custom pets inside the app → not planned.

## 3. Architecture

### 3.1 Module layout

Sub-project 2 adds one new file under `src/pet/`:

```
src/pet/
├── catalog.rs     ← NEW: filesystem discovery, ID collision policy, catalog data model
├── manifest.rs    ← +1 public method: PetManifest::from_path
├── mod.rs         ← + re-exports for catalog types
├── resolver.rs    ← unchanged
└── runtime.rs     ← PetRuntime::new_with_manifest promoted from test-only
```

**Dependency graph (no cycles):**
```
catalog ──► manifest
runtime ──► manifest, resolver
app     ──► catalog, runtime, sprite, settings, menu_bar
menu_bar──► (callbacks into app via selectors)
```

`catalog` has no AppKit, winit, or `wgpu` dependencies — it touches only `std::fs`, `std::path`, `serde`, and `crate::pet::manifest`. This keeps it cheap to test against a `tempfile::TempDir`.

### 3.2 Public API of `catalog`

```rust
pub struct PetCatalog {
    entries: Vec<CatalogEntry>,
    load_errors: Vec<CatalogLoadError>,
}

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub id: String,
    pub display_name: String,
    pub manifest: PetManifest,
    pub source: CatalogSource,
    pub spritesheet_path: PathBuf, // absolute, fully resolved
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSource { Bundled, Custom }

#[derive(Debug)]
pub enum CatalogLoadError {
    DirRead { path: PathBuf, error: io::Error },
    ManifestParse { path: PathBuf, error: ManifestError },
    SpritesheetMissing { manifest_path: PathBuf, sprite_path: PathBuf },
    DuplicateId { id: String, kept: PathBuf, dropped: PathBuf },
}

pub struct BundledPet {
    pub manifest: PetManifest,
    pub spritesheet_path: PathBuf,
}

impl PetCatalog {
    pub fn scan(bundled: BundledPet, custom_dir: &Path) -> Self { ... }
    pub fn entries(&self) -> &[CatalogEntry] { &self.entries }
    pub fn lookup(&self, id: &str) -> Option<&CatalogEntry> { ... }
    pub fn load_errors(&self) -> &[CatalogLoadError] { &self.load_errors }
}
```

The catalog is immutable after construction. `refresh_catalog` on the app builds a new `PetCatalog` and replaces the old one — there is no in-place mutation API, which keeps reasoning about "what is currently visible to the menu" trivial.

### 3.3 `DesktopPetApp` additions

New fields:
```rust
pub struct DesktopPetApp {
    // ...existing fields...
    catalog: PetCatalog,
    active_pet_id: String,
}
```

New methods:
```rust
pub fn activate_pet(&mut self, id: &str) -> Result<(), ActivationError>;
pub fn refresh_catalog(&mut self);
```

New error type (lives in `app.rs` next to `AppCommand`):
```rust
pub enum ActivationError {
    UnknownId(String),
    SpriteLoad { id: String, path: PathBuf, error: image::ImageError },
}
```

## 4. Filesystem layout & path resolution

### 4.1 Custom pets directory

Location: `~/Library/Application Support/Happy Cappy/pets/` — same parent as the existing `settings.json`.

Layout:
```
pets/
├── README.txt                  ← auto-created on first scan, never overwritten
├── shiba/
│   ├── pet.json
│   └── sprite.png
└── retro-pixel/
    ├── pet.json
    └── frames.png
```

### 4.2 Discovery rules

1. Enumerate immediate subdirectories of `pets/`. Files at the top level (besides `README.txt`) are silently ignored.
2. For each subdir, look for exactly `pet.json`. If absent → silent skip (subdir is not considered a pet candidate; no error recorded).
3. Parse via `PetManifest::from_json_str`. Parse/validation failure → record `CatalogLoadError::ManifestParse { path: dir.join("pet.json"), error }`, skip the pet.
4. Resolve `manifest.spritesheet_path` relative to `dir`. If the resolved file does not exist → record `CatalogLoadError::SpritesheetMissing`, skip the pet.
5. Construct `CatalogEntry { source: CatalogSource::Custom, spritesheet_path: <canonicalized absolute>, ... }`.
6. After all custom subdirs are processed, sort the custom entries by `display_name.to_lowercase()` (stable sort, lexicographic).
7. Insert each into the catalog, **after** the bundled entry. If an ID collides with an already-inserted entry → record `CatalogLoadError::DuplicateId { id, kept: <existing entry's path>, dropped: <this entry's path> }`, skip the new one.

The bundled pet is inserted into the catalog **first**. Any custom pet declaring `id: "happy-cappy"` therefore loses on collision (matches the design decision: bundled wins).

### 4.3 Spritesheet path resolution

- **Bundled:** resolved via the existing `src/bundle.rs::current_resource_paths()` (absolute path inside `Contents/Resources/`, or `assets/` in dev). Unchanged from sub-project 1.
- **Custom:** `manifest.spritesheet_path` is resolved relative to the manifest's parent directory, then verified to exist. Stored as an absolute `PathBuf` on `CatalogEntry`.

Relative paths inside the manifest are intentional so a user can move a `pets/<id>/` directory between machines without editing JSON.

### 4.4 Folder name vs. manifest ID

The subdirectory name (e.g., `shiba`) is a human-friendly grouping only. The canonical pet ID is `manifest.id`. A mismatch (folder `shiba` containing a manifest with `id: "totoro"`) is allowed — manifest wins, no error.

### 4.5 First-run setup

On the first `scan` call:
- `fs::create_dir_all(custom_dir)` ensures the directory exists. Failure records `CatalogLoadError::DirRead` and returns a catalog with bundled-only entries.
- `README.txt` is written via `fs::write` if and only if the file does not already exist. Content is a single short paragraph pointing to the manifest format and an example. Failures during `README.txt` write are ignored (best-effort).

## 5. Catalog scan flow

`PetCatalog::scan(bundled: BundledPet, custom_dir: &Path) -> Self` is the single entry point. Pseudocode:

```rust
pub fn scan(bundled: BundledPet, custom_dir: &Path) -> Self {
    let mut entries = Vec::new();
    let mut load_errors = Vec::new();
    let mut ids = HashSet::new();

    // 1. Bundled first (always succeeds — manifest validated at compile time via load_embedded_happy_cappy).
    let bundled_entry = CatalogEntry {
        id: bundled.manifest.id.clone(),
        display_name: bundled.manifest.display_name.clone(),
        manifest: bundled.manifest,
        source: CatalogSource::Bundled,
        spritesheet_path: bundled.spritesheet_path,
    };
    ids.insert(bundled_entry.id.clone());
    entries.push(bundled_entry);

    // 2. Best-effort dir creation.
    if let Err(error) = fs::create_dir_all(custom_dir) {
        load_errors.push(CatalogLoadError::DirRead {
            path: custom_dir.to_path_buf(),
            error,
        });
        return Self { entries, load_errors };
    }
    write_readme_if_missing(custom_dir);

    // 3. Enumerate subdirs.
    let read_dir = match fs::read_dir(custom_dir) {
        Ok(rd) => rd,
        Err(error) => {
            load_errors.push(CatalogLoadError::DirRead {
                path: custom_dir.to_path_buf(),
                error,
            });
            return Self { entries, load_errors };
        }
    };

    // 4. Load each candidate.
    let mut sub_entries: Vec<CatalogEntry> = read_dir
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().ok().is_some_and(|t| t.is_dir()))
        .filter_map(|e| match load_custom_pet(&e.path()) {
            Ok(Some(entry)) => Some(entry),
            Ok(None) => None,                       // no pet.json — silent skip
            Err(err) => { load_errors.push(err); None }
        })
        .collect();

    // 5. Stable sort by display_name (case-insensitive).
    sub_entries.sort_by(|a, b| {
        a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())
    });

    // 6. Apply ID collision policy.
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

    Self { entries, load_errors }
}
```

`load_custom_pet(dir: &Path) -> Result<Option<CatalogEntry>, CatalogLoadError>`:
- `Ok(None)` when `dir/pet.json` is absent (silent skip, no error).
- `Err(...)` for parse failure, missing sprite, etc.
- `Ok(Some(entry))` for success, with `entry.spritesheet_path` resolved + verified.

After scan, `load_errors` are immediately drained into structured `log::warn!` lines (see §8). They remain stored on the catalog for future UI surfaces (sub-project 3).

## 6. Hot-swap mechanics

### 6.1 Entry point

```rust
pub fn activate_pet(&mut self, id: &str) -> Result<(), ActivationError> {
    if id == self.active_pet_id {
        return Ok(()); // idempotent
    }

    let entry = self
        .catalog
        .lookup(id)
        .ok_or_else(|| ActivationError::UnknownId(id.to_string()))?;

    // Step 2: load sprite BEFORE touching runtime — failure here is recoverable.
    let new_sprite = SpriteSheet::load(&entry.spritesheet_path, &entry.manifest.frame)
        .map_err(|error| ActivationError::SpriteLoad {
            id: id.to_string(),
            path: entry.spritesheet_path.clone(),
            error,
        })?;

    // Step 3: build new runtime (resets transient state by design).
    let new_runtime = PetRuntime::new_with_manifest(entry.manifest.clone());

    // Step 4: atomic swap.
    let new_frame_size = new_runtime.frame_size();
    self.pet = new_runtime;
    self.sprite_sheet = new_sprite;
    self.active_pet_id = id.to_string();

    // Step 5: persist.
    self.settings.active_pet_id = Some(id.to_string());
    self.persist_settings();

    // Step 6: resize window content + redraw.
    self.resize_window_for_frame_size(new_frame_size); // existing helper used at startup
    self.request_redraw();

    Ok(())
}
```

### 6.2 What is preserved vs. reset

**Preserved (lives outside `PetRuntime`):**
- Window position on screen
- All `AppSettings` fields except `active_pet_id`
- Focus-mode state, fullscreen-hide state
- Workspace awareness state

**Reset (rebuilt fresh inside the new `PetRuntime`):**
- `state` → `PetState::Idle`
- `behavior_mode` → `BehaviorMode::Default`
- `action_override` → `None`
- `expression_index`, `expression_elapsed`, `frame_index`, `frame_elapsed`, `state_elapsed`
- Walk progress, sleep timer
- `direction` → seeded fresh via `new_with_seed(0)` semantics

### 6.3 Atomicity guarantee

The swap happens inside a single `&mut self` borrow with no `.await`s, no callbacks, and no observable intermediate state. Either:
- Steps 1–3 fail → app stays on previous pet, no fields mutated.
- Steps 1–3 succeed → steps 4–6 execute synchronously and the new pet is fully active.

There is no execution path where the user sees a half-swapped pet.

### 6.4 Frame size changes

If the new pet's `frame.width × frame.height` differs from the previous pet, the winit window content size must be updated.

Today (`src/app.rs:148`) the inner size is computed inline at window-construction time as `frame_size × WINDOW_SCALE`. The plan extracts this into a free function (`fn inner_size_for(frame: (u32, u32), scale: u32) -> PhysicalSize<u32>`) and reuses it for both window construction and hot-swap. Hot-swap calls `window.request_inner_size(inner_size_for(new_frame, WINDOW_SCALE))` (matching the existing pattern at `app.rs:382`).

### 6.5 `PetRuntime::new_with_manifest`

Sub-project 1 introduced `#[cfg(test)] pub fn new_with_manifest_for_test(manifest: PetManifest) -> Self`. Sub-project 2 promotes this to a non-test public constructor named `new_with_manifest`. The existing `PetRuntime::new()` becomes `Self::new_with_manifest(PetManifest::load_embedded_happy_cappy())`. The `_for_test` variant is deleted; tests use the public constructor.

## 7. Menu bar integration

### 7.1 Structure

A new **Pet** submenu sits above the existing settings entries, separated by a divider:

```
Pet ▸
├── ● Happy Cappy            (bundled, checkmark = active)
├── ○ Shiba                  (custom, sorted alphabetically by display_name)
├── ○ Retro Pixel
├── ──────────────
└── Reveal Pets Folder
```

The submenu always contains at least the bundled pet + divider + "Reveal Pets Folder" — the reveal item is the discoverability mechanism when no custom pets are installed.

### 7.2 Population flow

- On menu open, the existing `NSMenuDelegate`/`validateMenuItem:` path calls back into `DesktopPetApp::refresh_catalog()` synchronously.
- The Pet submenu is rebuilt: clear existing items, iterate `catalog.entries()`, add one `NSMenuItem` per entry. Set `state: .on` for `entry.id == active_pet_id`, else `.off`.
- Each item carries the pet `id` as its `representedObject` and targets the new selector `activatePet:`.

### 7.3 Selector wiring

A new selector `activatePet:` is added alongside the existing `dispatchSettingsValue:`. It reads the menu item's `representedObject` (a string `id`), forwards via the existing command channel to `DesktopPetApp::activate_pet(id)`. On error, the error is logged and the menu state stays consistent on next open (since refresh_catalog runs every open).

### 7.4 "Reveal Pets Folder" item

Implemented via `NSWorkspace::sharedWorkspace().openURL(file://<custom_dir>)`. Opens Finder at `~/Library/Application Support/Happy Cappy/pets/`.

### 7.5 Out of scope for sub-project 2 menu

- Visual error indicators ("⚠ N issues") — could be added as a single non-clickable item later if pain emerges, but not core scope.
- Per-pet preview thumbnails — sub-project 3.
- Drag-and-drop install — not planned.

## 8. Settings persistence

### 8.1 New field

`AppSettings` (in `src/settings.rs`) gains:
```rust
#[serde(default)]
pub active_pet_id: Option<String>,
```

- Default: `None` (means "use bundled default = `happy-cappy`").
- `#[serde(default)]` ensures pre-existing settings files (without this key) deserialize cleanly.
- Persisted via the existing settings save path; no new I/O code.

### 8.2 Startup resolution

In `DesktopPetApp::new` (or equivalent), after settings load + catalog scan:

```rust
let desired_id = settings
    .active_pet_id
    .as_deref()
    .unwrap_or("happy-cappy");

let active_entry = match catalog.lookup(desired_id) {
    Some(entry) => entry.clone(),
    None => {
        log::warn!(
            "activate_pet: persisted id missing, falling back to bundled requested={desired_id:?}"
        );
        settings.active_pet_id = None;
        settings.persist();
        catalog.lookup("happy-cappy").expect("bundled always present").clone()
    }
};
```

Covers: user deleted a custom pet folder between sessions, renamed it, or moved to a machine without the custom assets.

### 8.3 Write semantics

`activate_pet(id)` writes `settings.active_pet_id = Some(id.to_string())` and persists immediately. Pet switches are rare user-initiated events; no debouncing or batching.

### 8.4 Settings file location

Unchanged from today: `src/settings.rs`'s existing path resolution (typically `~/Library/Application Support/Happy Cappy/settings.json`, same parent as the new `pets/` folder).

## 9. Error policy & logging

### 9.1 Boundaries

| Boundary | Failure | Response |
|---|---|---|
| Catalog scan | Invalid JSON, missing sprite, dup ID, dir-read failure | Skip the pet. Record `CatalogLoadError`. Log `warn!`. App continues with bundled pet always present. |
| Pet activation startup | Persisted ID not in catalog | Log `warn!`. Fall back to bundled. Clear `settings.active_pet_id` to `None`. Persist. |
| Pet activation runtime | Unknown ID | `Err(ActivationError::UnknownId)`. Menu callback logs warn and swallows (item was stale; next menu open will be consistent). Previous pet remains active. |
| Pet activation | Spritesheet I/O / decode error | `Err(ActivationError::SpriteLoad)`. Log `error!`. Previous pet remains active (sprite load is step 2, before any mutation). |

### 9.2 Invariants

1. **App always launches with a working pet.** Bundled manifest is `include_str!`'d and validated at compile time (via `load_embedded_happy_cappy` + `validate_happy_cappy_required_keys`); bundled sprite ships in `Contents/Resources/`. No state where the user sees a blank window because of catalog scan failure.
2. **Hot-swap is atomic from the user's POV.** Either fully new pet or fully old pet — never mid-swap.
3. **Catalog is immutable.** `refresh_catalog` constructs a fresh `PetCatalog` and swaps it in. Borrow checker enforces that no held `&CatalogEntry` outlives the refresh.

### 9.3 Logging format

Uses the existing `log` facade (verify `env_logger` is wired during implementation; fallback `eprintln!` if not). Single-line structured format:

```
WARN  catalog: skipping custom pet manifest_parse path=/Users/.../pets/shiba/pet.json error="missing required animation 'idle'"
WARN  catalog: duplicate id, keeping existing id="happy-cappy" kept=/Applications/Happy Cappy.app/Contents/Resources/happy_cappy_spritesheet.png dropped=/Users/.../pets/custom-cappy/sprite.png
WARN  activate_pet: unknown id, falling back to bundled requested="ghost"
ERROR activate_pet: sprite load failed id="retro-pixel" path=/Users/.../pets/retro-pixel/frames.png error="..."
```

### 9.4 No UI surfaces in sub-project 2

`CatalogLoadError` values are logged but not displayed in the menu, no modals, no notifications. Sub-project 3 owns visual error surfacing.

## 10. Testing strategy

### 10.1 `src/pet/catalog.rs` unit tests

All tests use `tempfile::TempDir` to construct a fake `pets/` directory. The bundled pet is faked via a `BundledPet { manifest: <minimal valid>, spritesheet_path: <any path> }` — `PetCatalog::scan` does not open the bundled sprite, only records the path.

| Test | Scenario | Asserts |
|---|---|---|
| `scan_empty_dir_returns_bundled_only` | `pets/` doesn't exist | `entries().len() == 1`, source is Bundled, `load_errors` empty, dir created |
| `scan_picks_up_one_valid_custom_pet` | one subdir with valid `pet.json` + `sprite.png` | 2 entries; custom one has absolute resolved sprite path |
| `scan_skips_subdir_without_pet_json` | subdir exists but no `pet.json` | only bundled in entries; **no** load_error (silent skip) |
| `scan_records_manifest_parse_error` | `pet.json` malformed | bundled-only entries; `load_errors` has one `ManifestParse` |
| `scan_records_missing_idle_animation` | valid JSON but no `"idle"` key | one `ManifestParse` containing `ManifestError::MissingIdleAnimation` |
| `scan_records_missing_sprite` | valid `pet.json`, sprite file absent | one `SpritesheetMissing` load_error |
| `scan_drops_duplicate_id_keeping_bundled` | custom pet with `id: "happy-cappy"` | entries has only bundled "happy-cappy"; one `DuplicateId` load_error |
| `scan_drops_duplicate_id_between_two_customs` | two customs same id | first-by-display-name wins, second drops |
| `scan_sorts_custom_pets_by_display_name_case_insensitive` | customs "Zebra", "alpha", "Beta" | order: bundled, alpha, Beta, Zebra |
| `scan_ignores_files_at_top_level` | `pets/stray.txt` + `pets/README.txt` | no entries from these; no load_errors |
| `scan_resolves_sprite_path_relative_to_manifest` | manifest in `pets/foo/pet.json` with `spritesheetPath: "art/sprite.png"` | resolved path is `pets/foo/art/sprite.png` |
| `scan_writes_readme_when_missing` | empty `pets/` | `README.txt` exists with non-empty content |
| `scan_preserves_existing_readme` | `README.txt` with custom content | content unchanged after scan |
| `lookup_returns_entry_by_id` | populated catalog | `lookup("happy-cappy").unwrap().source == Bundled` |
| `lookup_returns_none_for_unknown_id` | populated catalog | `lookup("nope").is_none()` |

### 10.2 `src/settings.rs` additions

| Test | Asserts |
|---|---|
| `settings_default_has_no_active_pet_id` | `AppSettings::default().active_pet_id == None` |
| `settings_deserializes_legacy_file_without_active_pet_id` | JSON missing the field parses with `None` |
| `settings_roundtrip_with_active_pet_id` | serialize+deserialize preserves `Some("shiba")` |

### 10.3 `src/pet/runtime.rs` additions

| Test | Asserts |
|---|---|
| `new_with_manifest_uses_provided_manifest` | runtime built from custom manifest has matching `manifest().id` |
| (migration) existing callers of `new_with_manifest_for_test` are migrated to `new_with_manifest`; the `_for_test` variant is deleted |

### 10.4 `src/app.rs` integration tests

| Test | Asserts |
|---|---|
| `activate_pet_unknown_returns_error_and_keeps_previous` | `app.active_pet_id` unchanged on `ActivationError::UnknownId` |
| `activate_pet_idempotent_for_same_id` | calling `activate_pet(current_id)` returns `Ok`, no settings write, no runtime rebuild |
| `startup_falls_back_to_bundled_when_persisted_id_missing` | settings has `active_pet_id: Some("ghost")`, catalog lacks it → bundled active, persisted value cleared to `None` |

### 10.5 Out of scope for automated tests

- Menu bar AppKit rendering (no test infrastructure for AppKit menus in this codebase).
- Hot-swap visual correctness (different frame size, sprite decoded correctly, redraw scheduled).
- `NSWorkspace::openURL` "Reveal Pets Folder".

These go to the manual smoke test list.

### 10.6 Manual smoke test (plan exit criteria)

1. Launch app with no `pets/` directory → bundled Happy Cappy runs; `pets/README.txt` is created.
2. Drop a valid custom pet folder. Open Pet menu → see it listed. Click → pet hot-swaps; window position preserved.
3. Drop a custom pet with `id: "happy-cappy"` → menu still shows only one "Happy Cappy"; log shows duplicate warning.
4. Edit a custom pet's `pet.json` to invalid JSON → close & reopen Pet menu → pet disappears from list (silent skip + log).
5. Restart app → previously selected custom pet still active.
6. Delete the selected custom pet's folder, restart → app falls back to Happy Cappy; settings cleared to `None`.
7. Custom pet with different frame size (e.g., 96×96): activate → window resizes correctly.
8. "Reveal Pets Folder" item opens Finder at the right location.

## 11. Exit criteria

- All listed unit/integration tests pass (`cargo test`).
- `cargo fmt` clean, `cargo clippy` clean.
- Manual smoke checklist (§10.6) passes against the built `.app`.
- Existing 182-test baseline from sub-project 1 still passes (no regressions).

## 12. Open questions deferred to later sub-projects

- **Sub-project 3** — picker UI placement and design: standalone tab in Settings, or in-window panel? Where do `CatalogLoadError` values get displayed? Preview thumbnails?
- **Sub-project 4** — animation lifecycle: per-frame `ms`, `loop_start`, `fallback`, one-shot semantics, notification → animation name mapping, namespacing (e.g. `notify-running` to avoid colliding with `happy`/`sleepy`).

These are captured here only so they don't get lost; decisions belong to the respective sub-project specs.

## 13. Relationship to sub-project 1

| Sub-project 1 design decision | Sub-project 2 dependency |
|---|---|
| `PetManifest::from_json_str` as the parser core | `PetManifest::from_path` thin wrapper around it; `PetCatalog::load_custom_pet` uses `from_json_str` directly |
| Only `"idle"` required for arbitrary manifests; `validate_happy_cappy_required_keys` is bundled-only | Custom pets only need `"idle"`; resolver falls back gracefully for missing personality variants |
| `PetRuntime` carries `manifest: PetManifest` + `current_animation_name: String` | Hot-swap rebuilds `PetRuntime` with the new manifest; no mutation of existing runtime |
| `SpriteSheet { image, geometry: FrameGeometry }` constructed via `SpriteSheet::load(path, &geometry)` | Hot-swap calls the same constructor with the new pet's `&manifest.frame` |
| `#[cfg(test)] new_with_manifest_for_test` | Promoted to public `new_with_manifest`; test-only variant deleted |
| `bundle.rs::current_resource_paths()` for bundled assets | Reused; bundled `CatalogEntry` carries the same path it produces |

All of sub-project 1's seams are honored. Sub-project 2 is additive — no breaking changes to sub-project 1 APIs.
