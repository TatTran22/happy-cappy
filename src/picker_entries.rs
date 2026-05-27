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
