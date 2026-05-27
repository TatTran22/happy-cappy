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
