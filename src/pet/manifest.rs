use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use serde::Deserialize;

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

#[derive(Debug, Clone, Deserialize)]
pub struct Animation {
    pub frames: Vec<u32>,
}

fn default_manifest_version() -> u32 {
    1
}

#[derive(Debug)]
pub enum ManifestError {
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

impl PetManifest {
    pub fn from_json_str(json: &str) -> Result<Self, ManifestError> {
        let raw: PetManifest = serde_json::from_str(json)?;
        raw.validate()?;
        Ok(raw)
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
            for (pos, index) in anim.frames.iter().enumerate() {
                if *index >= max_index {
                    return Err(ManifestError::SpriteIndexOutOfBounds {
                        animation: name.clone(),
                        frame_pos: pos,
                        index: *index,
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
        assert_eq!(manifest.animations["idle"].frames, vec![0, 1, 2, 3]);
        assert_eq!(
            manifest.animations["walk-right"].frames,
            vec![32, 33, 34, 35]
        );
        assert_eq!(manifest.animations["drag"].frames, vec![36, 37, 38, 39]);
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
}
