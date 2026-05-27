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
        write_readme_if_missing(custom_dir);

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
}
