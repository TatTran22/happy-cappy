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
        assert_eq!(
            entry.source_path.as_deref(),
            Some(std::path::Path::new("/tmp/pets/broken"))
        );
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
        let dir = tempdir().unwrap();
        let catalog = PetCatalog::scan(bundled_pet_for_test(), dir.path());
        let entries = build_picker_entries_base(&catalog);
        assert!(entries.iter().all(|e| !e.id.is_empty()));
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
}
