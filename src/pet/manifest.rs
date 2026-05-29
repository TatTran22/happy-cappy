use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Deserializer};

const MAX_FRAMES_PER_ANIMATION: usize = 64;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetManifest {
    #[serde(rename = "manifest_version", default = "default_manifest_version")]
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

/// One animation frame: a sprite index plus an optional per-frame duration.
/// Deserializes from either a bare integer (v1: `7`) or an object (v2: `{ "index": 7, "ms": 120 }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub index: u32,
    /// `None` -> the runtime uses its state/personality-derived duration (v1 parity).
    pub ms: Option<u32>,
}

impl From<u32> for Frame {
    fn from(index: u32) -> Self {
        Frame { index, ms: None }
    }
}

impl<'de> Deserialize<'de> for Frame {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Index(u32),
            Object {
                index: u32,
                #[serde(default)]
                ms: Option<u32>,
            },
        }
        Ok(match Raw::deserialize(deserializer)? {
            Raw::Index(index) => Frame { index, ms: None },
            Raw::Object { index, ms } => Frame { index, ms },
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Animation {
    pub frames: Vec<Frame>,
    #[serde(default)]
    pub loop_start: Option<usize>,
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub one_shot: bool,
}

impl Animation {
    /// Build a plain v1-style animation (no per-frame ms, no lifecycle fields). For tests/fixtures.
    pub fn from_indices(indices: &[u32]) -> Self {
        Animation {
            frames: indices.iter().copied().map(Frame::from).collect(),
            loop_start: None,
            fallback: None,
            one_shot: false,
        }
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Sprite index at `pos` (wraps defensively so it never panics).
    pub fn sprite_index(&self, pos: usize) -> u32 {
        let len = self.frames.len().max(1);
        self.frames[pos % len].index
    }

    /// Per-frame duration override at `pos`, if the manifest specified one.
    pub fn frame_ms(&self, pos: usize) -> Option<u32> {
        let len = self.frames.len().max(1);
        self.frames.get(pos % len).and_then(|f| f.ms)
    }

    /// A "lifecycle" animation drives its own cursor (intro/one-shot) and must start at frame 0 on entry.
    pub fn is_lifecycle(&self) -> bool {
        self.one_shot || self.loop_start.is_some()
    }
}

fn default_manifest_version() -> u32 {
    1
}

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

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "manifest I/O error: {e}"),
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
            Self::MissingRequiredAnimation { name } => {
                write!(f, "manifest is missing required animation '{name}'")
            }
        }
    }
}

impl Error for ManifestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
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

impl From<std::io::Error> for ManifestError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl PetManifest {
    pub fn from_json_str(json: &str) -> Result<Self, ManifestError> {
        let raw: PetManifest = serde_json::from_str(json)?;
        raw.validate()?;
        Ok(raw)
    }

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

    pub fn load_embedded_happy_cappy() -> Self {
        const JSON: &str = include_str!("../../assets/manifests/happy_cappy.json");
        let manifest =
            Self::from_json_str(JSON).expect("bundled happy_cappy.json must parse and validate");
        manifest
            .validate_happy_cappy_required_keys()
            .expect("bundled happy_cappy.json must declare all required animations");
        manifest
    }

    fn validate_happy_cappy_required_keys(&self) -> Result<(), ManifestError> {
        const REQUIRED: &[&str] = &[
            "idle",
            "blink",
            "happy",
            "curious",
            "sleepy",
            "hover-calm",
            "hover-cheerful",
            "hover-lively",
            "walk-right",
            "drag",
        ];
        for name in REQUIRED {
            if !self.animations.contains_key(*name) {
                return Err(ManifestError::MissingRequiredAnimation { name });
            }
        }
        Ok(())
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
            for (pos, frame) in anim.frames.iter().enumerate() {
                if frame.index >= max_index {
                    return Err(ManifestError::SpriteIndexOutOfBounds {
                        animation: name.clone(),
                        frame_pos: pos,
                        index: frame.index,
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
        assert_eq!(
            manifest.animations["idle"].frames.iter().map(|f| f.index).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(
            manifest.animations["walk-right"].frames.iter().map(|f| f.index).collect::<Vec<_>>(),
            vec![32, 33, 34, 35]
        );
        assert_eq!(
            manifest.animations["drag"].frames.iter().map(|f| f.index).collect::<Vec<_>>(),
            vec![36, 37, 38, 39]
        );
    }

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
            ManifestError::SpriteIndexOutOfBounds {
                index: 99,
                max: 4,
                ..
            }
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
        assert!(matches!(
            err,
            ManifestError::TooManyFrames { count: 65, .. }
        ));
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

    #[test]
    fn bundled_manifest_declares_all_required_happy_cappy_keys() {
        let manifest = PetManifest::load_embedded_happy_cappy();
        // Should not panic — implicitly asserts validate_happy_cappy_required_keys passed.
        // Spot-check that the required keys are present.
        for required in &[
            "idle",
            "blink",
            "happy",
            "curious",
            "sleepy",
            "hover-calm",
            "hover-cheerful",
            "hover-lively",
            "walk-right",
            "drag",
        ] {
            assert!(
                manifest.animations.contains_key(*required),
                "bundled manifest must declare '{required}'"
            );
        }
    }

    #[test]
    fn validate_happy_cappy_required_keys_rejects_missing_blink() {
        let json = r#"{
            "id": "test",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {
                "idle":          {"frames": [0]},
                "happy":         {"frames": [0]},
                "curious":       {"frames": [0]},
                "sleepy":        {"frames": [0]},
                "hover-calm":    {"frames": [0]},
                "hover-cheerful":{"frames": [0]},
                "hover-lively":  {"frames": [0]},
                "walk-right":    {"frames": [0]},
                "drag":          {"frames": [0]}
            }
        }"#;
        let manifest = PetManifest::from_json_str(json).unwrap();
        let err = manifest.validate_happy_cappy_required_keys().unwrap_err();
        assert!(matches!(
            err,
            ManifestError::MissingRequiredAnimation { name: "blink" }
        ));
    }

    #[test]
    fn validate_happy_cappy_required_keys_accepts_extra_animations() {
        // A manifest with all 10 required keys plus extras still validates.
        let json = r#"{
            "id": "test",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {
                "idle":          {"frames": [0]},
                "blink":         {"frames": [0]},
                "happy":         {"frames": [0]},
                "curious":       {"frames": [0]},
                "sleepy":        {"frames": [0]},
                "hover-calm":    {"frames": [0]},
                "hover-cheerful":{"frames": [0]},
                "hover-lively":  {"frames": [0]},
                "walk-right":    {"frames": [0]},
                "drag":          {"frames": [0]},
                "wave":          {"frames": [0]}
            }
        }"#;
        let manifest = PetManifest::from_json_str(json).unwrap();
        assert!(manifest.validate_happy_cappy_required_keys().is_ok());
    }

    #[test]
    fn from_path_reads_and_parses_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pet.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(
            br#"{
            "id": "test",
            "displayName": "Test",
            "spritesheetPath": "x.png",
            "frame": {"width": 16, "height": 16, "columns": 4, "rows": 1},
            "animations": {"idle": {"frames": [0, 1, 2, 3]}}
        }"#,
        )
        .unwrap();
        drop(f);

        let manifest = PetManifest::from_path(&path).unwrap();
        assert_eq!(manifest.id, "test");
        assert_eq!(
            manifest.animations["idle"].frames.iter().map(|f| f.index).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
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

    #[test]
    fn frame_parses_from_bare_integer() {
        let f: Frame = serde_json::from_str("7").unwrap();
        assert_eq!(f.index, 7);
        assert_eq!(f.ms, None);
    }

    #[test]
    fn frame_parses_from_object_with_ms() {
        let f: Frame = serde_json::from_str(r#"{ "index": 9, "ms": 120 }"#).unwrap();
        assert_eq!(f.index, 9);
        assert_eq!(f.ms, Some(120));
    }

    #[test]
    fn frame_parses_from_object_without_ms() {
        let f: Frame = serde_json::from_str(r#"{ "index": 3 }"#).unwrap();
        assert_eq!(f.index, 3);
        assert_eq!(f.ms, None);
    }

    #[test]
    fn frame_from_u32_has_no_ms() {
        let f = Frame::from(5u32);
        assert_eq!(f.index, 5);
        assert_eq!(f.ms, None);
    }
}
