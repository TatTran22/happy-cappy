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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StoredPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub personality: Personality,
    pub scale: f32,
    pub movement_speed: f32,
    pub hover_intensity: f32,
    pub monitor_behavior: MonitorBehavior,
    pub pet_visible: bool,
    pub last_position: Option<StoredPosition>,
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
            last_position: None,
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
        self.last_position = Some(StoredPosition {
            x: position.x,
            y: position.y,
        });
    }

    pub fn restored_position(&self) -> Option<Vec2> {
        self.last_position.map(|position| Vec2 {
            x: position.x,
            y: position.y,
        })
    }
}

pub fn default_settings_path() -> Result<PathBuf, SettingsError> {
    let home = std::env::var_os("HOME").ok_or(SettingsError::MissingHomeDirectory)?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Happy Cappy")
        .join("settings.json"))
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
            last_position: Some(StoredPosition { x: 999.0, y: -50.0 }),
            ..AppSettings::default()
        };

        settings.sanitize(bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(
            settings.last_position,
            Some(StoredPosition { x: 372.0, y: 0.0 })
        );
    }

    #[test]
    fn load_missing_file_returns_defaults() {
        let path = Path::new("/tmp/happy-cappy-missing-settings.json");

        let settings =
            AppSettings::load_or_default_from(path, bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(settings, AppSettings::default());
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
            last_position: Some(StoredPosition { x: 22.0, y: 33.0 }),
        };

        settings.save_to(&path).unwrap();
        let loaded =
            AppSettings::load_or_default_from(&path, bounds(), Vec2 { x: 128.0, y: 128.0 });

        assert_eq!(loaded, settings);

        let _ = fs::remove_dir_all(root);
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
}
