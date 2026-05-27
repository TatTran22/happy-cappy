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
