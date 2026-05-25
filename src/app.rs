use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use log::{error, warn};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    window::{Window, WindowAttributes, WindowId},
};

use crate::{
    bundle::current_resource_paths,
    menu_bar::MenuBarController,
    pet::{Direction, Pet, PetState},
    physics::{Bounds, Physics, Vec2},
    renderer::PetRenderer,
    settings::{default_settings_path, AppSettings, SettingsError},
    sprite::{SpriteRow, SpriteSheet},
    window_macos::apply_desktop_pet_window_behavior,
};

pub const FRAME_SIZE: u32 = 64;
pub const WINDOW_SCALE: u32 = 2;
pub const WINDOW_SIZE: u32 = FRAME_SIZE * WINDOW_SCALE;

const TARGET_FRAME_TIME: Duration = Duration::from_millis(16);
const IDLE_FRAME_TIME: Duration = Duration::from_millis(200);
const SLEEP_FRAME_TIME: Duration = Duration::from_millis(500);
const MAX_TICK_DELTA: Duration = Duration::from_secs(1);
const FALLBACK_BOUNDS_WIDTH: f32 = 800.0;
const FALLBACK_BOUNDS_HEIGHT: f32 = 600.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppCommand {
    OpenSettings,
    ShowPet,
    HidePet,
    TogglePetVisibility,
    ResetPosition,
    SetPersonality(crate::pet::Personality),
    SetScale(f32),
    SetMovementSpeed(f32),
    SetHoverIntensity(f32),
    SetMonitorBehavior(crate::settings::MonitorBehavior),
    Quit,
}

pub struct DesktopPetApp {
    window: Option<Arc<Window>>,
    renderer: Option<PetRenderer>,
    sprite_sheet: Option<SpriteSheet>,
    pet: Pet,
    physics: Physics,
    last_tick: Instant,
    next_tick_at: Instant,
    menu_bar: Option<MenuBarController>,
    settings: AppSettings,
    settings_path: Option<std::path::PathBuf>,
    pet_visible: bool,
    #[cfg(not(test))]
    event_proxy: EventLoopProxy<AppCommand>,
    #[cfg(test)]
    event_proxy: Option<EventLoopProxy<AppCommand>>,
}

impl DesktopPetApp {
    pub fn new(event_proxy: EventLoopProxy<AppCommand>) -> Self {
        let seed = fastrand::u64(..);
        let now = Instant::now();

        Self {
            window: None,
            renderer: None,
            sprite_sheet: None,
            pet: Pet::new_with_seed(seed),
            physics: default_physics(),
            last_tick: now,
            next_tick_at: now,
            menu_bar: None,
            settings: AppSettings::default(),
            settings_path: default_settings_path().ok(),
            pet_visible: true,
            #[cfg(not(test))]
            event_proxy,
            #[cfg(test)]
            event_proxy: Some(event_proxy),
        }
    }

    #[cfg(test)]
    fn new_with_event_proxy(event_proxy: Option<EventLoopProxy<AppCommand>>) -> Self {
        let seed = fastrand::u64(..);
        let now = Instant::now();

        Self {
            window: None,
            renderer: None,
            sprite_sheet: None,
            pet: Pet::new_with_seed(seed),
            physics: default_physics(),
            last_tick: now,
            next_tick_at: now,
            menu_bar: None,
            settings: AppSettings::default(),
            settings_path: default_settings_path().ok(),
            pet_visible: true,
            event_proxy,
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let attributes = WindowAttributes::default()
            .with_title("DesktopPet")
            .with_inner_size(LogicalSize::new(WINDOW_SIZE as f64, WINDOW_SIZE as f64))
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true);

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                error!("failed to create desktop pet window: {error}");
                event_loop.exit();
                return false;
            }
        };

        if let Err(error) = apply_desktop_pet_window_behavior(&window) {
            warn!("failed to apply desktop pet window behavior: {error}");
        }

        self.window = Some(Arc::clone(&window));
        self.update_bounds_from_window(event_loop);
        self.load_settings();
        if !self.pet_visible {
            window.set_visible(false);
        }
        self.move_window_to_pet();

        let surface_size = window.inner_size();
        match PetRenderer::new(
            Arc::clone(&window),
            surface_size.width,
            surface_size.height,
            FRAME_SIZE,
            FRAME_SIZE,
        ) {
            Ok(renderer) => {
                self.renderer = Some(renderer);
                let now = Instant::now();
                self.last_tick = now;
                self.next_tick_at = now;
                window.request_redraw();
                true
            }
            Err(error) => {
                error!("failed to create pet renderer: {error}");
                self.window = None;
                event_loop.exit();
                false
            }
        }
    }

    fn load_assets(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let paths = match current_resource_paths() {
            Ok(paths) => paths,
            Err(error) => {
                error!("failed to locate desktop pet resources: {error}");
                event_loop.exit();
                return false;
            }
        };

        match SpriteSheet::load(&paths.sprite_sheet, FRAME_SIZE) {
            Ok(sprite_sheet) => {
                self.sprite_sheet = Some(sprite_sheet);
                true
            }
            Err(error) => {
                error!(
                    "failed to load sprite sheet {}: {error}",
                    paths.sprite_sheet.display()
                );
                event_loop.exit();
                false
            }
        }
    }

    fn update_bounds_from_window(&mut self, event_loop: &ActiveEventLoop) {
        let monitor = self
            .window
            .as_ref()
            .and_then(|window| window.current_monitor())
            .or_else(|| event_loop.primary_monitor());

        if let Some(monitor) = monitor {
            let position = monitor.position();
            let size = monitor.size();
            let scale_factor = self
                .window
                .as_ref()
                .map(|window| window.scale_factor())
                .unwrap_or_else(|| monitor.scale_factor()) as f32;
            self.physics.bounds = Bounds {
                min_x: position.x as f32 / scale_factor,
                min_y: position.y as f32 / scale_factor,
                max_x: (position.x as f32 + size.width as f32) / scale_factor,
                max_y: (position.y as f32 + size.height as f32) / scale_factor,
            };
        }

        self.physics.clamp_to_bounds();
    }

    fn tick(&mut self, now: Instant) {
        let Some(window) = self.window.as_ref().map(Arc::clone) else {
            self.last_tick = now;
            return;
        };

        let dt = now.duration_since(self.last_tick).min(MAX_TICK_DELTA);
        self.last_tick = now;

        let tick = self.pet.tick(dt);
        self.physics.velocity.x = tick.speed_x;
        let physics_step = self.physics.update(dt.as_secs_f32());
        if tick.state == PetState::Walk && physics_step.bounced_x {
            self.pet.turn_around();
        }
        self.move_window_to_pet();
        window.request_redraw();
    }

    fn move_window_to_pet(&self) {
        if let Some(window) = &self.window {
            window.set_outer_position(LogicalPosition::new(
                f64::from(self.physics.position.x),
                f64::from(self.physics.position.y),
            ));
        }
    }

    fn apply_settings(&mut self, mut settings: AppSettings) {
        settings.scale = settings
            .scale
            .clamp(AppSettings::MIN_SCALE, AppSettings::MAX_SCALE);
        settings.movement_speed = settings.movement_speed.clamp(
            AppSettings::MIN_MOVEMENT_SPEED,
            AppSettings::MAX_MOVEMENT_SPEED,
        );
        settings.hover_intensity = settings.hover_intensity.clamp(
            AppSettings::MIN_HOVER_INTENSITY,
            AppSettings::MAX_HOVER_INTENSITY,
        );

        self.pet.apply_personality(settings.personality);
        self.pet
            .set_movement_speed_multiplier(settings.movement_speed);
        self.pet.set_hover_intensity(settings.hover_intensity);
        self.pet.set_hidden(!settings.pet_visible);
        self.pet_visible = settings.pet_visible;
        self.physics.size = Vec2 {
            x: FRAME_SIZE as f32 * settings.scale,
            y: FRAME_SIZE as f32 * settings.scale,
        };

        let had_restored_position = settings.last_position.is_some();
        if let Some(position) = settings.restored_position() {
            self.physics.position = position;
        }
        self.physics.clamp_to_bounds();
        if had_restored_position {
            settings.update_position(self.physics.position);
        }

        self.settings = settings;
        if let Some(window) = &self.window {
            let size = LogicalSize::new(self.physics.size.x as f64, self.physics.size.y as f64);
            let _ = window.request_inner_size(size);
        }
        self.move_window_to_pet();
    }

    #[allow(dead_code)]
    fn save_settings(&self) {
        let Some(path) = &self.settings_path else {
            warn!("settings path is unavailable");
            return;
        };
        if let Err(error) = self.settings.save_to(path) {
            warn!("failed to save settings to {}: {error}", path.display());
        }
    }

    fn load_settings(&mut self) {
        let Some(path) = &self.settings_path else {
            return;
        };
        let settings = match AppSettings::load_from(path) {
            Ok(settings) => settings,
            Err(error) => {
                if !matches!(
                    &error,
                    SettingsError::Io(io_error)
                        if io_error.kind() == std::io::ErrorKind::NotFound
                ) {
                    warn!("failed to load settings from {}: {error}", path.display());
                }
                AppSettings::default()
            }
        };
        self.apply_settings(settings);
    }

    #[allow(dead_code)]
    fn set_pet_visible(&mut self, visible: bool) {
        self.settings.pet_visible = visible;
        self.pet_visible = visible;
        self.pet.set_hidden(!visible);
        if let Some(window) = &self.window {
            window.set_visible(visible);
        }
        self.save_settings();
    }

    #[allow(dead_code)]
    fn reset_pet_position(&mut self) {
        self.physics.position = Vec2 {
            x: self.physics.bounds.min_x + 120.0,
            y: self.physics.bounds.min_y + 120.0,
        };
        self.physics.clamp_to_bounds();
        self.settings.update_position(self.physics.position);
        self.move_window_to_pet();
        self.save_settings();
    }

    fn handle_command(&mut self, command: AppCommand, event_loop: &ActiveEventLoop) {
        match command {
            AppCommand::OpenSettings => self.open_settings_window(),
            AppCommand::ShowPet => self.set_pet_visible(true),
            AppCommand::HidePet => self.set_pet_visible(false),
            AppCommand::TogglePetVisibility => self.set_pet_visible(!self.pet_visible),
            AppCommand::ResetPosition => self.reset_pet_position(),
            AppCommand::SetPersonality(personality) => {
                let mut settings = self.settings.clone();
                settings.personality = personality;
                self.apply_settings(settings);
                self.save_settings();
            }
            AppCommand::SetScale(scale) => {
                let mut settings = self.settings.clone();
                settings.scale = scale;
                settings.sanitize(self.physics.bounds, self.physics.size);
                self.apply_settings(settings);
                self.save_settings();
            }
            AppCommand::SetMovementSpeed(speed) => {
                let mut settings = self.settings.clone();
                settings.movement_speed = speed;
                settings.sanitize(self.physics.bounds, self.physics.size);
                self.apply_settings(settings);
                self.save_settings();
            }
            AppCommand::SetHoverIntensity(intensity) => {
                let mut settings = self.settings.clone();
                settings.hover_intensity = intensity;
                settings.sanitize(self.physics.bounds, self.physics.size);
                self.apply_settings(settings);
                self.save_settings();
            }
            AppCommand::SetMonitorBehavior(monitor_behavior) => {
                let mut settings = self.settings.clone();
                settings.monitor_behavior = monitor_behavior;
                self.apply_settings(settings);
                self.save_settings();
            }
            AppCommand::Quit => event_loop.exit(),
        }
    }

    fn open_settings_window(&mut self) {
        warn!("settings window is not available yet");
    }

    #[allow(dead_code)]
    fn persist_current_position(&mut self) {
        self.settings.update_position(self.physics.position);
        self.save_settings();
    }

    fn tick_due(&self, now: Instant) -> bool {
        now >= self.next_tick_at
    }

    fn next_tick_interval(&self) -> Duration {
        match self.pet.state() {
            PetState::Walk => TARGET_FRAME_TIME,
            PetState::Idle => IDLE_FRAME_TIME,
            PetState::Sleep => SLEEP_FRAME_TIME,
        }
    }

    fn draw(&mut self) {
        let (Some(renderer), Some(sprite_sheet)) =
            (self.renderer.as_mut(), self.sprite_sheet.as_ref())
        else {
            return;
        };

        let group = self.pet.current_animation_group();
        let row = SpriteRow::from(group);
        let flip_x = group == crate::pet::AnimationGroup::WalkRight
            && self.pet.direction() == Direction::Left;
        let rect = sprite_sheet.frame_rect(row, self.pet.frame_index());

        if let Err(error) = renderer.draw(sprite_sheet.image(), rect, flip_x) {
            warn!("failed to draw desktop pet frame: {error}");
        }
    }
}

fn default_physics() -> Physics {
    Physics {
        position: Vec2 { x: 120.0, y: 120.0 },
        velocity: Vec2 { x: 0.0, y: 0.0 },
        size: Vec2 {
            x: WINDOW_SIZE as f32,
            y: WINDOW_SIZE as f32,
        },
        bounds: Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: FALLBACK_BOUNDS_WIDTH,
            max_y: FALLBACK_BOUNDS_HEIGHT,
        },
    }
}

#[cfg(test)]
impl DesktopPetApp {
    fn new_for_test() -> Self {
        Self::new_with_event_proxy(None)
    }

    fn settings_for_test(&self) -> &crate::settings::AppSettings {
        &self.settings
    }

    fn apply_settings_for_test(&mut self, settings: crate::settings::AppSettings) {
        self.apply_settings(settings);
    }
}

impl ApplicationHandler<AppCommand> for DesktopPetApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.sprite_sheet.is_none() && !self.load_assets(event_loop) {
            return;
        }

        if self.window.is_none() && !self.create_window(event_loop) {
            return;
        }

        if self.menu_bar.is_none() {
            #[cfg(not(test))]
            {
                self.menu_bar = MenuBarController::new(self.event_proxy.clone());
            }

            #[cfg(test)]
            {
                let Some(event_proxy) = self.event_proxy.as_ref() else {
                    warn!("menu bar requires an event loop proxy");
                    return;
                };
                self.menu_bar = MenuBarController::new(event_proxy.clone());
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppCommand) {
        self.handle_command(event, event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.tick_due(now) {
            self.tick(now);
            self.next_tick_at = now + self.next_tick_interval();
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick_at));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(error) = renderer.resize(size.width, size.height) {
                        warn!("failed to resize pet renderer: {error}");
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.update_bounds_from_window(event_loop);
                self.move_window_to_pet();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_delta_cap_does_not_clip_scheduled_pet_intervals() {
        assert!(MAX_TICK_DELTA >= IDLE_FRAME_TIME);
        assert!(MAX_TICK_DELTA >= SLEEP_FRAME_TIME);
    }

    #[test]
    fn redraw_wakeups_do_not_bypass_next_tick_deadline() {
        let mut app = DesktopPetApp::new_for_test();
        let now = Instant::now();
        app.next_tick_at = now + IDLE_FRAME_TIME;

        assert!(!app.tick_due(now + Duration::from_millis(1)));
        assert!(app.tick_due(now + IDLE_FRAME_TIME));
    }

    #[test]
    fn applying_settings_updates_pet_personality_and_visibility() {
        let mut app = DesktopPetApp::new_for_test();
        let settings = crate::settings::AppSettings {
            personality: crate::pet::Personality::Lively,
            pet_visible: false,
            scale: 3.0,
            ..crate::settings::AppSettings::default()
        };

        app.apply_settings_for_test(settings);

        assert_eq!(
            app.settings_for_test().personality,
            crate::pet::Personality::Lively
        );
        assert!(!app.settings_for_test().pet_visible);
        assert_eq!(app.pet.personality(), crate::pet::Personality::Lively);
        assert_eq!(app.pet.behavior_mode(), crate::pet::BehaviorMode::Hidden);
        assert_eq!(app.physics.size, Vec2 { x: 192.0, y: 192.0 });
    }

    #[test]
    fn applying_settings_clamps_restored_position_after_scale_and_syncs_settings() {
        let mut app = DesktopPetApp::new_for_test();
        app.physics.bounds = Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 250.0,
            max_y: 250.0,
        };
        let settings = crate::settings::AppSettings {
            scale: 3.0,
            last_position: Some(crate::settings::StoredPosition { x: 200.0, y: 220.0 }),
            ..crate::settings::AppSettings::default()
        };

        app.apply_settings_for_test(settings);

        assert_eq!(app.physics.size, Vec2 { x: 192.0, y: 192.0 });
        assert_eq!(app.physics.position, Vec2 { x: 58.0, y: 58.0 });
        assert_eq!(
            app.settings_for_test().last_position,
            Some(crate::settings::StoredPosition { x: 58.0, y: 58.0 })
        );
    }

    #[test]
    fn applying_larger_scale_without_restored_position_clamps_current_position() {
        let mut app = DesktopPetApp::new_for_test();
        app.physics.bounds = Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 300.0,
            max_y: 300.0,
        };
        app.physics.position = Vec2 { x: 170.0, y: 180.0 };
        let settings = crate::settings::AppSettings {
            scale: 3.0,
            last_position: None,
            ..crate::settings::AppSettings::default()
        };

        app.apply_settings_for_test(settings);

        assert_eq!(app.physics.size, Vec2 { x: 192.0, y: 192.0 });
        assert_eq!(app.physics.position, Vec2 { x: 108.0, y: 108.0 });
        assert_eq!(app.settings_for_test().last_position, None);
    }
}
