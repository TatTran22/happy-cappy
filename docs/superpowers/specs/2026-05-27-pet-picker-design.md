# Pet Picker Window — Design Spec (Sub-Project 3)

**Status:** approved
**Date:** 2026-05-27
**Predecessor:** [2026-05-27-pet-catalog-design.md](./2026-05-27-pet-catalog-design.md) (sub-project 2, merged at `5906f62`)
**Roadmap position:** sub-project 3 of 4

## 1. Context

Sub-project 2 shipped `PetCatalog`, custom-pet loading from `~/Library/Application Support/Happy Cappy/pets/`, persisted `active_pet_id`, atomic hot-swap, and a minimal **Pet ▸** submenu in the menu bar. The submenu shows pet names only — no preview, no error messages for broken custom pets, no metadata.

This sub-project adds an in-window **Pet Picker** that lets the user:

1. **Browse** the catalog with an animated idle preview for every pet.
2. **See errors** for custom pets that failed to load (invalid `pet.json`, missing spritesheet, duplicate id), each with a path and reason.
3. **Inspect metadata** (id, source, frame size, available animations) before activating.
4. **Activate** a selected pet with a deliberate Apply step — no accidental hot-swap while browsing.

The menu bar submenu from sub-project 2 stays as the quick-swap path. The picker complements it; it does not replace it.

## 2. Scope

**In scope:**
- New module `src/picker_window_macos.rs` (cfg-gated on `target_os = "macos"`, stub on other platforms — same shape as `src/settings_window_macos.rs`).
- `PetCatalog::scan` change: collect per-entry load failures into a `Vec<CatalogLoadFailure>` field instead of dropping them. Existing logging stays.
- `AppCommand` additions: `ShowPicker`, `SyncPicker { entries, active_id }`. Existing `RefreshPetMenu`, `ActivatePet(String)`, `RevealPetsFolder` are reused.
- `DesktopPetApp` additions: `Option<PickerWindowController>` field, `show_picker()` helper, extension of `refresh_catalog()` to also sync the picker when visible.
- Menu wiring: new "Pet Library…" item between the **Pet ▸** submenu and **Settings…**, plus its selector on `CommandTarget`.
- AppKit list/detail UI: `NSPanel` containing `NSTableView` (left) + custom detail `NSView` (right) with Apply button. Single shared `NSTimer` drives idle-frame animation for visible rows and the detail preview.
- Frame decode helper: convert each pet's idle-animation frames from `SpriteSheet` RGBA bytes to `Retained<NSImage>`. Cached per-pet for the lifetime of one picker open/close cycle.

**Out of scope (explicitly deferred):**
- Per-frame `ms`, `loop_start`, `fallback`, animation lifecycle, one-shot, notification mapping → sub-project 4.
- Editing custom pets in the app (rename, delete, manifest editor) — not planned.
- Live filesystem watcher (rescan happens on picker show and on menu submenu open from SP2).
- Filtering / search / sorting controls in the picker — list is alphabetically sorted by display name, bundled first (matches SP2 ordering).
- iCloud / cloud sync, downloadable packs — not planned.

## 3. Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  app.rs (controller)                                          │
│   - DesktopPetApp owns PetCatalog (SP2) and active_pet_id    │
│   - NEW: Option<PickerWindowController>                       │
│   - Handles AppCommand::{ShowPicker, RefreshPetMenu,          │
│       ActivatePet, RevealPetsFolder, SyncPicker (internal)}   │
└──────────────────────────────────────────────────────────────┘
        │ push: sync_entries(entries, active_id)
        ▼
┌──────────────────────────────────────────────────────────────┐
│  picker_window_macos.rs (NEW, target_os = "macos")            │
│   - PickerWindowController (NSPanel + NSTableView + detail)   │
│   - PickerEntry { id, display_name, source, frame_w, frame_h, │
│       animations, frames: Vec<Retained<NSImage>>, error }     │
│   - PreviewTicker (NSTimer 10fps, updates every visible row   │
│       and the detail preview)                                 │
│   - Click row → select; Apply → AppCommand::ActivatePet       │
│   - Reveal in Finder → AppCommand::RevealPetsFolder           │
└──────────────────────────────────────────────────────────────┘
        ▲
        │ pull: build_picker_entries(&catalog, &failures, mtm)
        │
┌──────────────────────────────────────────────────────────────┐
│  pet/catalog.rs (SP2; small extension)                        │
│   - PetCatalog::scan still consumes the bundled pet and       │
│     enumerates custom dirs, but now retains failures.         │
│   - NEW: PetCatalog::failures() -> &[CatalogLoadFailure]      │
│   - NEW: struct CatalogLoadFailure { id_or_dirname, path,    │
│       error: CatalogLoadError }                               │
└──────────────────────────────────────────────────────────────┘
```

Dependencies flow one direction: `catalog → picker_window_macos → app`. No new module-level dependency cycles. The picker module depends on AppKit (`objc2`, `objc2_app_kit`, `objc2_foundation`), on `CommandTarget` for the proxy selector pattern, on `SpriteSheet` for frame decoding, and on the catalog types it consumes.

`picker_window_macos.rs` is target-gated like `settings_window_macos.rs`: on non-macOS targets it exposes a no-op `PickerWindowController` so `app.rs` compiles unchanged. The codebase is macOS-only in practice (winit window backed by NSWindow), but the existing stub pattern keeps cross-platform `cargo check` green.

## 4. UI Layout

NSPanel sized 480×420 (titled, closable, non-resizable for v1).

```
NSPanel
└── contentView (NSView)
    ├── NSScrollView (left side, 200pt wide, full height)
    │   └── NSTableView (single column, row height 44pt, headerless)
    │         └── per-row custom NSView:
    │               [NSImageView 32×32] [name+meta NSTextField] [⚠ if error]
    └── detail NSView (right side, fills remaining width)
          ├── NSImageView 128×128 (preview, centered horizontally)
          ├── NSTextField display name  (system bold 18pt)
          ├── NSTextField "id: {id}"
          ├── NSTextField "{source} · {w}×{h}"
          ├── NSTextField "anims: idle, blink, walk-right, …"
          ├── NSTextField error message (red, hidden when entry.error is None)
          ├── NSButton "Reveal in Finder" (left, shown only for custom + error rows)
          └── NSButton "Apply" (right, bottom)
```

Frame-based positioning (no Auto Layout), matching `settings_window_macos.rs`. All control coordinates are constants at the top of `picker_window_macos.rs`.

Bundled pet rows show "bundled" as source. Custom pet rows show "custom". Error rows show the directory name in place of display name when the manifest itself failed to parse (id is unknown until parse succeeds).

## 5. Data Model

```rust
// picker_window_macos.rs
#[derive(Clone)]  // for sync_entries push
pub struct PickerEntry {
    pub id: String,
    pub display_name: String,
    pub source: PickerSource,
    pub frame_width: u32,
    pub frame_height: u32,
    pub animations: Vec<String>,        // sorted, lowercased animation names
    pub frames: Vec<Retained<NSImage>>, // pre-decoded idle frames; empty when error
    pub error: Option<String>,          // user-facing one-line message
    pub source_path: Option<PathBuf>,   // for "Reveal in Finder" on custom errors
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PickerSource { Bundled, Custom }
```

`Retained<NSImage>` is `Clone` via `Retained::clone`, so `PickerEntry: Clone` works.

```rust
// pet/catalog.rs (extension)
pub struct CatalogLoadFailure {
    pub id_or_dirname: String,  // pet id if manifest parsed, else folder name
    pub path: PathBuf,           // the offending pet directory
    pub error: CatalogLoadError, // existing enum from SP2
}

impl PetCatalog {
    pub fn failures(&self) -> &[CatalogLoadFailure] { &self.failures }
}
```

Catalog scan flow (changes vs SP2):
1. Insert bundled (unchanged).
2. For each subdirectory in `pets/`:
   - `load_custom_pet(dir)` already returns `Result<Option<CatalogEntry>, CatalogLoadError>`.
   - On `Ok(Some(entry))`: insert (dedup against bundled wins — unchanged).
   - On `Ok(None)`: skip (it's a non-directory or a hidden file — unchanged).
   - On `Err(error)`: push to `failures` with the directory's folder name as `id_or_dirname` and the absolute dir path. `warn!` logging stays. **(NEW: previously the error was logged and dropped.)**

## 6. Sync Flow

```
User: clicks main menu "Pet Library…"
   │
   ▼
CommandTarget.showPicker:  →  AppCommand::ShowPicker
   │
   ▼
DesktopPetApp::handle_command(ShowPicker):
   1. self.refresh_catalog()       ← already exists; rebuilds PetCatalog
   2. let mtm = MainThreadMarker::new().expect(...);
   3. let entries = build_picker_entries(&self.catalog, mtm);
   4. picker.sync_entries(entries, &self.active_pet_id);
   5. picker.show();
```

`build_picker_entries` is in `app.rs` to keep `picker_window_macos.rs` from depending directly on `PetCatalog`. Internally it is split for testability:

1. `build_picker_entries_base(catalog) -> Vec<PickerEntryBase>` — pure Rust, no AppKit; returns entries with id/display_name/source/frame/anim/error fields and `source_path`, but no `frames`. Iterates `catalog.entries()` (OK → `error: None`) and `catalog.failures()` (error → `frames` left empty, `error.is_some()`). Ordering: bundled first, then OK custom alphabetical (matches SP2 catalog ordering), then failures alphabetical at the bottom.
2. `attach_preview_frames(&mut [PickerEntry], catalog, mtm)` — AppKit-side; for each OK entry, calls `build_preview_frames` and stores the `Vec<Retained<NSImage>>` into the entry. Failures are skipped.

`build_picker_entries` simply chains the two. Tests cover the base function directly without needing `MainThreadMarker`.

**On Apply click** (`PickerWindowController`):
- Read `selected_entry.id` from internal state.
- Dispatch `AppCommand::ActivatePet(id)` on the proxy.
- Call `[panel orderOut:self]` immediately. The app's existing activation path handles success/failure (warn log on failure — SP2 path). The picker doesn't wait for confirmation; if activation fails, the next picker show will reflect the unchanged active id.

**On menu bar submenu refresh** (existing `RefreshPetMenu` handler):
- After `refresh_catalog`, if the picker is currently visible, call `picker.sync_entries(...)` with the new snapshot. The picker module exposes `is_visible() -> bool`.

**On window close** (NSPanel `windowShouldClose:` or `[panel close]`):
- Stop the preview timer.
- Drop the cached NSImage frames? **No** — keep them so re-show is instant. They're dropped only on the next `sync_entries` (when the catalog has changed).

## 7. Animation Pipeline

A single `NSTimer` owned by `PickerWindowController` ticks every 100ms (10 fps).

```rust
// PickerWindowController internals
struct AnimationState {
    timer: Option<Retained<NSTimer>>,
    frame_index: usize,       // shared index, wraps via modulo per-row
}

fn on_tick(&self) {
    let idx = self.animation.frame_index.wrapping_add(1);
    self.animation.frame_index = idx;
    // Update detail pane
    if let Some(entry) = self.selected_entry() {
        if !entry.frames.is_empty() {
            let i = idx % entry.frames.len();
            self.detail_image_view.setImage(Some(&entry.frames[i]));
        }
    }
    // Update visible table rows
    for row_idx in self.table.visible_row_range() {
        let entry = &self.entries[row_idx];
        if entry.frames.is_empty() { continue; }
        let i = idx % entry.frames.len();
        if let Some(cell) = self.table.view_at(row_idx) {
            cell.image_view.setImage(Some(&entry.frames[i]));
        }
    }
}
```

`frame_index` is a single counter shared across all rows. Pets with different frame counts wrap independently via `% entry.frames.len()`. Two pets that happen to share frame count will appear in lockstep — acceptable; idle animations aren't expected to be synchronized in a meaningful way.

**Frame decode** (`build_preview_frames` in `app.rs`, called from `build_picker_entries`):

```rust
fn build_preview_frames(
    catalog_entry: &CatalogEntry,
    mtm: MainThreadMarker,
) -> Result<Vec<Retained<NSImage>>, SpriteError> {
    // 1. Load spritesheet (already cached on disk by SP2)
    let sheet = SpriteSheet::load(
        &catalog_entry.spritesheet_path,
        catalog_entry.manifest.frame.width,
        catalog_entry.manifest.frame.height,
        catalog_entry.manifest.frame.columns,
        catalog_entry.manifest.frame.rows,
    )?;
    // 2. Pick the idle animation (or first defined animation as fallback)
    let anim = catalog_entry.manifest.animations.get("idle")
        .or_else(|| catalog_entry.manifest.animations.values().next())
        .ok_or(SpriteError::NoAnimation)?;
    // 3. For each frame index, decode RGBA → CGImage → NSImage
    anim.frames.iter()
        .map(|&fi| {
            let rgba = sheet.frame_rgba(fi as usize);
            rgba_to_nsimage(&rgba, sheet.frame_width(), sheet.frame_height(), mtm)
        })
        .collect()
}
```

`SpriteSheet::frame_rgba` exists from SP1. `rgba_to_nsimage` is a new helper in `picker_window_macos.rs` (~30 LOC) using the `core-graphics` crate as a direct `Cargo.toml` dependency. It creates a `CGDataProvider` from the RGBA bytes, builds a `CGImage` (8 bits per component, `kCGImageAlphaPremultipliedLast`, sRGB color space), and constructs `NSImage::initWithCGImage_size`.

**Memory** at typical usage (one bundled pet + ~10 custom pets, each ~8 idle frames @ 64×64×4 RGBA):
- ~11 pets × 8 frames × 64×64×4 ≈ 1.4 MB. NSImage retains the underlying bitmap.

**CPU**: 10 fps × ~10 visible rows × 1 pointer-swap ≈ 100 ops/sec. Negligible.

**Timer lifecycle**:
- Started in `showWindow` (also if already visible — idempotent).
- Stopped in `windowWillClose` and on `hide()` calls.
- `NSTimer` retains its target — break the cycle on stop by invalidating before dropping.

## 8. Error UX

Each `CatalogLoadError` variant from SP2 maps to a user-facing message:

| Variant | User message |
|---|---|
| `DirRead(io_err)` | "Couldn't read pet directory: {io_err}" |
| `ManifestParse(serde_err)` | "Invalid pet.json: {serde_err}" (truncated to 140 chars) |
| `SpritesheetMissing(path)` | "Spritesheet not found: {path.file_name()}" |
| `DuplicateId(id)` | "ID `{id}` conflicts with the bundled pet" |

The truncation applies only to the user-facing string in the detail pane; the original error remains in `warn!` logs.

**Detail pane when error.is_some():**
- Preview area shows a system placeholder (NSImage `NSImageNameCaution` or grey-filled NSView, 128×128).
- Display name shows the folder name (errors typically can't yield an id).
- Source line shows "custom · {dirname}".
- Animations line shows "—".
- Error label shows the mapped message in red.
- "Reveal in Finder" button visible (calls `RevealPetsFolder`; on click, the system reveals the parent `pets/` directory — sufficient because `pets/<dirname>/` is the user's natural editing context).
- Apply button **disabled**.

**Apply disabled when:**
- The selected entry has `error.is_some()`, **or**
- The selected entry's id equals the current `active_pet_id` (nothing to do).

## 9. Entry Points & Commands

**Menu wiring** — extend `menu_bar.rs` to insert a new item between the **Pet ▸** submenu and **Settings…**:

```
HC ▸
├── Pet ▸                  (SP2: Pet submenu)
├── Pet Library…           ← NEW (selector: openPetLibrary:)
├── ────────
├── Settings…
└── (rest unchanged)
```

New tag constant: `MENU_TAG_OPEN_PET_LIBRARY = 1202` (slots into the existing pet-related tag block from SP2).

**New `AppCommand` variants:**
```rust
ShowPicker,
SyncPicker { entries: Vec<PickerEntry>, active_id: String },
```

Reused (no change): `ActivatePet(String)`, `RevealPetsFolder`, `RefreshPetMenu`.

`SyncPicker` is dispatched internally by `app.rs` after a catalog refresh, so the picker module only handles a "here are the latest entries" event. The variant exists so the picker can be updated from event-loop callbacks without exposing `PickerWindowController` to the public API surface — keeping the dispatch model consistent with the other AppCommand-driven flows.

**`CommandTarget` (`command_target_macos.rs`)** gains one selector:

```rust
#[unsafe(method(openPetLibrary:))]
fn open_pet_library(&self, _sender: *mut AnyObject) {
    self.dispatch(AppCommand::ShowPicker);
}
```

Apply button targets `CommandTarget` with the existing `activatePet:` selector pattern (from SP2). The button's `representedObject` carries the selected id as an `NSString`.

## 10. Lifecycle Details

**Construction:** `DesktopPetApp::create_window` creates `PickerWindowController` once, lazily, on first `ShowPicker` (same pattern as `SettingsWindowController` — controllers exist as `Option<...>` initialised on demand). This avoids paying any cost for users who never open the picker.

**Visibility:**
- `show()` — `[panel makeKeyAndOrderFront:nil]`, start timer.
- `hide()` — `[panel orderOut:nil]`, stop timer.
- `is_visible() -> bool` — wraps `[panel isVisible]`.

**Re-sync without flash:** When `RefreshPetMenu` runs while the picker is visible, the new `sync_entries` call replaces the entries; the selected row remembers its id and re-selects (if still present) or falls back to the bundled pet. NSImage caches for unchanged pets are rebuilt — acceptable since refreshes are user-triggered (menu open or picker show).

**Quit:** Quitting the app does not require special picker cleanup; AppKit will tear down the NSPanel and invalidate timers as the run loop ends.

## 11. Failure-Handling Policy

The policy mirrors SP2:
- Errors are surfaced visually (no swallowing) but never crash the app.
- A pet that fails to load still appears in the catalog's failures list — the picker shows it so the user can fix the underlying file.
- Activation failure (e.g. spritesheet vanished between scan and activate) is already handled by SP2's `ActivationError` path: `warn!` + leave the current pet active. The picker doesn't see the error directly; the next `RefreshPetMenu` reflects the new state.
- Sprite frame decode failure in `build_preview_frames` is treated as a per-pet error: the entry is added with `frames: vec![]` and `error: Some("Couldn't decode preview frames: …")`. The pet is still listed but not activatable from the picker until the underlying issue is fixed.

## 12. Testing Strategy

**Unit testable (pure Rust, no AppKit):**

1. `pet/catalog.rs` — `failures` collection:
   - Scan dir with broken JSON → `failures.len() == 1`, error variant is `ManifestParse`.
   - Scan dir with missing spritesheet → `failures` has `SpritesheetMissing`.
   - Scan dir with duplicate id → `failures` has `DuplicateId`.
   - Scan dir with unreadable subdir (best effort: simulate via permission-denied file). If platform-fragile, skip.
   - Successful scan → `failures.is_empty()`.

2. `app.rs` — `build_picker_entries` decomposed:
   - Split into `build_picker_entries_base(catalog) -> Vec<PickerEntryBase>` (no NSImage; pure) and `attach_preview_frames(&mut PickerEntry, mtm)` (AppKit).
   - Tests cover `build_picker_entries_base`:
     - Ordering: bundled first, then OK custom alphabetical, then failures alphabetical.
     - Failure → entry with `error.is_some()`, `frames: vec![]`, `source: Custom`.
     - Active id passthrough.
     - Empty catalog → entries contain only bundled.

3. `CatalogLoadError → user message` mapping function (`format_catalog_error(&CatalogLoadError) -> String`):
   - All four variants produce non-empty strings under ~140 chars.
   - Long serde messages truncate with ellipsis.

**Manual smoke (cannot automate AppKit reliably):**

1. **Cold open:** Launch app, open Pet Library, verify bundled pet appears with animated preview, no error rows.
2. **Custom pets present:** Drop 3 custom pets (1 OK, 1 broken JSON, 1 missing sprite) into `pets/`. Open Pet Library, verify: OK pet animates, broken-JSON row shows ⚠ + dirname, missing-sprite row shows ⚠ + dirname. Click each row → detail pane reflects state correctly.
3. **Apply OK pet:** Select custom OK pet → Apply enabled → click. Pet swaps on desktop, picker closes. Reopen picker, verify checkmark moved (active row indicated visually).
4. **Apply disabled cases:** Select error row → Apply disabled. Select current active row → Apply disabled.
5. **Reveal in Finder:** On error row, click "Reveal in Finder", verify Finder opens at `pets/` folder.
6. **Refresh on reopen:** Close picker, add new custom pet to filesystem, reopen picker, verify new entry appears.
7. **Animated preview check:** Verify multiple rows animate visibly and smoothly at 10 fps (no tearing, no stuck frames).
8. **Memory stability:** Open/close picker 10 times in a row; Activity Monitor footprint should stabilise (no monotonic growth beyond first open).

**Cross-feature regression checks (manual):**
- Quick-swap via menu bar **Pet ▸** still works while picker is closed.
- Persisted `active_pet_id` from picker apply survives app restart.
- Settings window still opens and operates independently.

## 13. Exit Criteria

- All new unit tests pass under `cargo test`.
- `cargo clippy --all-targets --all-features -- -D warnings` clean.
- `cargo fmt --check` clean.
- All 8 manual smoke scenarios in §12 pass.
- The picker opens within 200 ms on a cold launch with ~10 custom pets (informal — eyeball, not benchmarked).
- No regressions in SP2 menu bar quick-swap, settings, focus mode, nap, cheer up, or pet drag.

## 14. Sub-Project 2 Dependencies

| SP3 needs | Already shipped in SP2? |
|---|---|
| `PetCatalog::scan` | ✅ |
| `CatalogEntry`, `CatalogSource`, `CatalogLoadError` | ✅ (extending with `CatalogLoadFailure`) |
| `DesktopPetApp::activate_pet` | ✅ (reused via `AppCommand::ActivatePet`) |
| `DesktopPetApp::refresh_catalog` | ✅ |
| `AppCommand::ActivatePet(String)` | ✅ |
| `AppCommand::RevealPetsFolder` | ✅ |
| `AppCommand::RefreshPetMenu` | ✅ (extending: also syncs picker when visible) |
| `CommandTarget` selector pattern | ✅ (adding `openPetLibrary:`) |
| `SpriteSheet::frame_rgba` | ✅ (SP1) |
| `settings_window_macos.rs` pattern to mirror | ✅ |

No blocking gaps. SP3 layers cleanly on SP2 with one small backward-compatible catalog extension (`failures` field).
