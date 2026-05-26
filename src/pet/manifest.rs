use std::collections::BTreeMap;

use serde::Deserialize;

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

impl PetManifest {
    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn load_embedded_happy_cappy() -> Self {
        const JSON: &str = include_str!("../../assets/manifests/happy_cappy.json");
        Self::from_json_str(JSON).expect("bundled happy_cappy.json must parse")
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
        assert_eq!(manifest.animations["walk-right"].frames, vec![32, 33, 34, 35]);
        assert_eq!(manifest.animations["drag"].frames, vec![36, 37, 38, 39]);
    }
}
