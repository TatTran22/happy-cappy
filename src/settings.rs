use crate::pet::Personality;
use crate::physics::{Bounds, Vec2};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorBehavior {
    CurrentDisplay,
    PrimaryDisplay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredPosition {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_personality")]
    pub personality: Personality,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default = "default_movement_speed")]
    pub movement_speed: f32,
    #[serde(default = "default_hover_intensity")]
    pub hover_intensity: f32,
    #[serde(default = "default_monitor_behavior")]
    pub monitor_behavior: MonitorBehavior,
    #[serde(default = "default_pet_visible")]
    pub pet_visible: bool,
    #[serde(default = "default_focus_mode")]
    pub focus_mode: bool,
    #[serde(default = "default_true")]
    pub follow_cursor_when_idle: bool,
    #[serde(default = "default_true")]
    pub avoid_text_cursor: bool,
    #[serde(default = "default_true")]
    pub hide_on_fullscreen: bool,
    #[serde(default)]
    pub last_position: Option<StoredPosition>,
    #[serde(default)]
    pub active_pet_id: Option<String>,
}

#[derive(Debug)]
pub enum SettingsError {
    Io(io::Error),
    Json(serde_json::Error),
    MissingHomeDirectory,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "settings I/O error: {error}"),
            Self::Json(error) => write!(f, "settings JSON error: {error}"),
            Self::MissingHomeDirectory => write!(f, "HOME is not set"),
        }
    }
}

impl Error for SettingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::MissingHomeDirectory => None,
        }
    }
}

impl From<io::Error> for SettingsError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for SettingsError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            personality: Personality::Cheerful,
            scale: 2.0,
            movement_speed: 1.0,
            hover_intensity: 1.0,
            monitor_behavior: MonitorBehavior::CurrentDisplay,
            pet_visible: true,
            focus_mode: false,
            follow_cursor_when_idle: true,
            avoid_text_cursor: true,
            hide_on_fullscreen: true,
            last_position: None,
            active_pet_id: None,
        }
    }
}

impl AppSettings {
    pub const MIN_SCALE: f32 = 1.0;
    pub const MAX_SCALE: f32 = 4.0;
    pub const MIN_MOVEMENT_SPEED: f32 = 0.0;
    pub const MAX_MOVEMENT_SPEED: f32 = 3.0;
    pub const MIN_HOVER_INTENSITY: f32 = 0.0;
    pub const MAX_HOVER_INTENSITY: f32 = 3.0;

    pub fn sanitize(&mut self, bounds: Bounds, pet_size: Vec2) {
        self.scale = self.scale.clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        self.movement_speed = self
            .movement_speed
            .clamp(Self::MIN_MOVEMENT_SPEED, Self::MAX_MOVEMENT_SPEED);
        self.hover_intensity = self
            .hover_intensity
            .clamp(Self::MIN_HOVER_INTENSITY, Self::MAX_HOVER_INTENSITY);

        if let Some(position) = &mut self.last_position {
            let max_x = (bounds.max_x - pet_size.x).max(bounds.min_x);
            let max_y = (bounds.max_y - pet_size.y).max(bounds.min_y);
            position.x = position.x.clamp(bounds.min_x, max_x);
            position.y = position.y.clamp(bounds.min_y, max_y);
        }
    }

    pub fn load_or_default_from(path: &Path, bounds: Bounds, pet_size: Vec2) -> Self {
        match Self::load_from(path) {
            Ok(mut settings) => {
                settings.sanitize(bounds, pet_size);
                settings
            }
            Err(_) => Self::default(),
        }
    }

    pub fn load_from(path: &Path) -> Result<Self, SettingsError> {
        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn update_position(&mut self, position: Vec2) {
        self.update_position_for_display(position, None);
    }

    pub fn update_position_for_display(&mut self, position: Vec2, display_name: Option<&str>) {
        self.last_position = Some(StoredPosition {
            x: position.x,
            y: position.y,
            display_name: display_name.map(str::to_owned),
        });
    }

    pub fn restored_position(&self) -> Option<Vec2> {
        self.restored_position_for_display(None)
    }

    pub fn restored_position_for_display(&self, display_name: Option<&str>) -> Option<Vec2> {
        let position = self.last_position.as_ref()?;
        if let (Some(stored_display), Some(active_display)) =
            (position.display_name.as_deref(), display_name)
        {
            if stored_display != active_display {
                return None;
            }
        }

        self.last_position.as_ref().map(|position| Vec2 {
            x: position.x,
            y: position.y,
        })
    }
}

fn default_personality() -> Personality {
    Personality::Cheerful
}

fn default_scale() -> f32 {
    2.0
}

fn default_movement_speed() -> f32 {
    1.0
}

fn default_hover_intensity() -> f32 {
    1.0
}

fn default_monitor_behavior() -> MonitorBehavior {
    MonitorBehavior::CurrentDisplay
}

fn default_pet_visible() -> bool {
    true
}

fn default_focus_mode() -> bool {
    false
}

fn default_true() -> bool {
    true
}

pub fn app_support_dir() -> Result<PathBuf, SettingsError> {
    let home = std::env::var_os("HOME").ok_or(SettingsError::MissingHomeDirectory)?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Happy Cappy"))
}

pub fn default_settings_path() -> Result<PathBuf, SettingsError> {
    Ok(app_support_dir()?.join("settings.json"))
}

pub fn custom_pets_dir() -> Result<PathBuf, SettingsError> {
    Ok(app_support_dir()?.join("pets"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Bounds {
        Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 500.0,
            max_y: 400.0,
        }
    }

    #[test]
    fn defaults_are_cheerful_visible_and_bounded() {
        let settings = AppSettings::default();

        assert_eq!(settings.personality, Personality::Cheerful);
        assert_eq!(settings.monitor_behavior, MonitorBehavior::CurrentDisplay);
        assert!(settings.pet_visible);
        assert_eq!(settings.scale, 2.0);
        assert_eq!(settings.movement_speed, 1.0);
        assert_eq!(settings.hover_intensity, 1.0);
        assert_eq!(settings.last_position, None);
        assert!(settings.follow_cursor_when_idle);
        assert!(settings.avoid_text_cursor);
        assert!(settings.hide_on_fullscreen);
    }

    #[test]
    fn defaults_keep_focus_mode_off() {
        let settings = AppSettings::default();

        assert!(!settings.focus_mode);
    }

    #[test]
    fn settings_default_has_no_active_pet_id() {
        assert_eq!(AppSettings::default().active_pet_id, None);
    }

    #[test]
    fn settings_deserializes_legacy_file_without_active_pet_id() {
        let root =
            std::env::temp_dir().join(format!("happy-cappy-legacy-active-{}", fastrand::u64(..)));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        fs::write(
            &path,
            br#"{"personality":"calm","scale":2.0,"movement_speed":1.0,"hover_intensity":1.0,"monitor_behavior":"current_display","pet_visible":true,"focus_mode":false}"#,
        )
        .unwrap();

        let loaded =
            AppSettings::load_or_default_from(&path, bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(loaded.active_pet_id, None);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn custom_pets_dir_lives_under_happy_cappy_app_support() {
        // Force a known HOME so the test is hermetic.
        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", "/tmp/fake-home");

        let result = custom_pets_dir();

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }

        let path = result.unwrap();
        assert!(path.ends_with("Library/Application Support/Happy Cappy/pets"));
    }

    #[test]
    fn settings_roundtrip_with_active_pet_id() {
        let root =
            std::env::temp_dir().join(format!("happy-cappy-active-rt-{}", fastrand::u64(..)));
        let path = root.join("settings.json");
        let settings = AppSettings {
            active_pet_id: Some("shiba".to_string()),
            ..AppSettings::default()
        };

        settings.save_to(&path).unwrap();
        let loaded =
            AppSettings::load_or_default_from(&path, bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(loaded.active_pet_id, Some("shiba".to_string()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sanitize_clamps_numeric_values() {
        let mut settings = AppSettings {
            scale: 99.0,
            movement_speed: -4.0,
            hover_intensity: 9.0,
            ..AppSettings::default()
        };

        settings.sanitize(bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(settings.scale, 4.0);
        assert_eq!(settings.movement_speed, 0.0);
        assert_eq!(settings.hover_intensity, 3.0);
    }

    #[test]
    fn sanitize_clamps_saved_position_inside_bounds() {
        let mut settings = AppSettings {
            last_position: Some(StoredPosition {
                x: 999.0,
                y: -50.0,
                display_name: None,
            }),
            ..AppSettings::default()
        };

        settings.sanitize(bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(
            settings.last_position,
            Some(StoredPosition {
                x: 372.0,
                y: 0.0,
                display_name: None,
            })
        );
    }

    #[test]
    fn load_missing_file_returns_defaults() {
        let root = std::env::temp_dir().join(format!(
            "happy-cappy-settings-missing-{}",
            fastrand::u64(..)
        ));
        let path = root.join("settings.json");
        let _ = fs::remove_dir_all(&root);

        assert!(!path.exists());

        let settings =
            AppSettings::load_or_default_from(&path, bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(settings, AppSettings::default());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn save_and_load_round_trip() {
        let root = std::env::temp_dir().join(format!("happy-cappy-settings-{}", fastrand::u64(..)));
        let path = root.join("settings.json");
        let settings = AppSettings {
            personality: Personality::Lively,
            scale: 3.0,
            movement_speed: 2.0,
            hover_intensity: 2.5,
            monitor_behavior: MonitorBehavior::PrimaryDisplay,
            pet_visible: false,
            focus_mode: true,
            follow_cursor_when_idle: false,
            avoid_text_cursor: false,
            hide_on_fullscreen: false,
            last_position: Some(StoredPosition {
                x: 22.0,
                y: 33.0,
                display_name: Some("Built-in Display".to_string()),
            }),
            active_pet_id: None,
        };

        settings.save_to(&path).unwrap();
        let loaded =
            AppSettings::load_or_default_from(&path, bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(loaded, settings);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn partial_settings_load_with_defaults_for_missing_fields() {
        let root = std::env::temp_dir().join(format!("happy-cappy-partial-{}", fastrand::u64(..)));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        fs::write(
            &path,
            br#"{"personality":"calm","scale":3.0,"last_position":{"x":44.0,"y":55.0}}"#,
        )
        .unwrap();

        let loaded =
            AppSettings::load_or_default_from(&path, bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(loaded.personality, Personality::Calm);
        assert_eq!(loaded.scale, 3.0);
        assert_eq!(loaded.movement_speed, AppSettings::default().movement_speed);
        assert_eq!(
            loaded.monitor_behavior,
            AppSettings::default().monitor_behavior
        );
        assert!(loaded.pet_visible);
        assert_eq!(
            loaded.last_position,
            Some(StoredPosition {
                x: 44.0,
                y: 55.0,
                display_name: None,
            })
        );
        assert!(loaded.follow_cursor_when_idle);
        assert!(loaded.avoid_text_cursor);
        assert!(loaded.hide_on_fullscreen);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn partial_settings_load_defaults_focus_mode_to_off() {
        let root =
            std::env::temp_dir().join(format!("happy-cappy-partial-focus-{}", fastrand::u64(..)));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        fs::write(&path, br#"{"personality":"calm","scale":3.0}"#).unwrap();

        let loaded =
            AppSettings::load_or_default_from(&path, bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert!(!loaded.focus_mode);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restored_position_ignores_mismatched_display_identity() {
        let settings = AppSettings {
            last_position: Some(StoredPosition {
                x: 22.0,
                y: 33.0,
                display_name: Some("External Display".to_string()),
            }),
            ..AppSettings::default()
        };

        assert_eq!(
            settings.restored_position_for_display(Some("External Display")),
            Some(Vec2 { x: 22.0, y: 33.0 })
        );
        assert_eq!(
            settings.restored_position_for_display(Some("Built-in Display")),
            None
        );
    }

    #[test]
    fn corrupt_file_returns_defaults() {
        let root = std::env::temp_dir().join(format!(
            "happy-cappy-settings-corrupt-{}",
            fastrand::u64(..)
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        fs::write(&path, b"{not json").unwrap();

        let settings =
            AppSettings::load_or_default_from(&path, bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(settings, AppSettings::default());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn app_support_dir_is_happy_cappy_root() {
        let path = app_support_dir().unwrap();
        assert!(path.ends_with("Library/Application Support/Happy Cappy"));
    }

    #[test]
    fn settings_and_pets_paths_live_under_app_support_dir() {
        let root = app_support_dir().unwrap();
        assert_eq!(default_settings_path().unwrap(), root.join("settings.json"));
        assert_eq!(custom_pets_dir().unwrap(), root.join("pets"));
    }
}

#[cfg(test)]
mod workspace_awareness_settings_tests {
    use super::*;

    #[test]
    fn missing_workspace_keys_default_to_true() {
        let json = r#"{"personality":"calm","scale":2.0,"movement_speed":1.0,"hover_intensity":1.0,"monitor_behavior":"current_display","pet_visible":true,"focus_mode":false}"#;
        let settings: AppSettings = serde_json::from_str(json).expect("parse");
        assert!(settings.follow_cursor_when_idle);
        assert!(settings.avoid_text_cursor);
        assert!(settings.hide_on_fullscreen);
    }

    #[test]
    fn explicit_workspace_keys_round_trip() {
        let json = r#"{"personality":"calm","scale":2.0,"movement_speed":1.0,"hover_intensity":1.0,"monitor_behavior":"current_display","pet_visible":true,"focus_mode":false,"follow_cursor_when_idle":false,"avoid_text_cursor":false,"hide_on_fullscreen":false}"#;
        let settings: AppSettings = serde_json::from_str(json).expect("parse");
        assert!(!settings.follow_cursor_when_idle);
        assert!(!settings.avoid_text_cursor);
        assert!(!settings.hide_on_fullscreen);
    }

    #[test]
    fn default_settings_have_workspace_features_enabled() {
        let s = AppSettings::default();
        assert!(s.follow_cursor_when_idle);
        assert!(s.avoid_text_cursor);
        assert!(s.hide_on_fullscreen);
    }
}
