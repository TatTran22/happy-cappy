use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use log::{error, warn};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    window::{Window, WindowAttributes, WindowId},
};

use crate::{
    bundle::current_resource_paths,
    interaction::{alpha_hit_test_with_flip, InteractionEvent, InteractionState, MouseButtonKind},
    menu_bar::MenuBarController,
    pet::{Direction, Pet, PetState},
    physics::{Bounds, Physics, Vec2},
    renderer::PetRenderer,
    settings::{default_settings_path, AppSettings, SettingsError},
    settings_window_macos::SettingsWindowController,
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
    settings_window: Option<SettingsWindowController>,
    settings: AppSettings,
    settings_path: Option<std::path::PathBuf>,
    pet_visible: bool,
    interaction: InteractionState,
    last_cursor_local_position: Option<Vec2>,
    last_cursor_screen_position: Option<Vec2>,
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
            settings_window: None,
            settings: AppSettings::default(),
            settings_path: default_settings_path().ok(),
            pet_visible: true,
            interaction: InteractionState::default(),
            last_cursor_local_position: None,
            last_cursor_screen_position: None,
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
            settings_window: None,
            settings: AppSettings::default(),
            settings_path: default_settings_path().ok(),
            pet_visible: true,
            interaction: InteractionState::default(),
            last_cursor_local_position: None,
            last_cursor_screen_position: None,
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

    fn handle_non_quit_command(&mut self, command: AppCommand) -> bool {
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
            AppCommand::Quit => return false,
        }
        true
    }

    fn handle_command(&mut self, command: AppCommand, event_loop: &ActiveEventLoop) {
        if !self.handle_non_quit_command(command) {
            event_loop.exit();
        }
    }

    fn open_settings_window(&mut self) {
        if self.settings_window.is_none() {
            #[cfg(not(test))]
            {
                self.settings_window =
                    SettingsWindowController::new(&self.settings, self.event_proxy.clone());
            }

            #[cfg(test)]
            {
                self.settings_window = self.event_proxy.as_ref().and_then(|event_proxy| {
                    SettingsWindowController::new(&self.settings, event_proxy.clone())
                });
            }
        }

        if let Some(settings_window) = &self.settings_window {
            settings_window.show();
        } else {
            warn!("settings window is not available on this platform or thread");
        }
    }

    fn show_context_menu(&self, local_position: Option<Vec2>) {
        let Some(window) = self.window.as_ref() else {
            return;
        };

        #[cfg(not(test))]
        {
            crate::window_macos::show_pet_context_menu(
                window,
                self.event_proxy.clone(),
                self.pet_visible,
                local_position,
            );
        }

        #[cfg(test)]
        {
            if let Some(event_proxy) = self.event_proxy.as_ref() {
                crate::window_macos::show_pet_context_menu(
                    window,
                    event_proxy.clone(),
                    self.pet_visible,
                    local_position,
                );
            }
        }
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

    fn mouse_button_kind(button: winit::event::MouseButton) -> MouseButtonKind {
        match button {
            winit::event::MouseButton::Left => MouseButtonKind::Left,
            winit::event::MouseButton::Right => MouseButtonKind::Right,
            _ => MouseButtonKind::Other,
        }
    }

    fn current_sprite_hit_test(&self, point: Vec2) -> bool {
        let Some(sprite_sheet) = &self.sprite_sheet else {
            return false;
        };
        let group = self.pet.current_animation_group();
        let rect = sprite_sheet.frame_rect(SpriteRow::from(group), self.pet.frame_index());
        let scale = if self.settings.scale.is_finite() && self.settings.scale > 0.0 {
            self.settings.scale
        } else {
            AppSettings::MIN_SCALE
        };
        let scaled_point = Vec2 {
            x: point.x / scale,
            y: point.y / scale,
        };
        let flip_x = group == crate::pet::AnimationGroup::WalkRight
            && self.pet.direction() == Direction::Left;
        alpha_hit_test_with_flip(sprite_sheet.image(), rect, scaled_point, flip_x)
    }

    fn cursor_logical_position(&self, physical: PhysicalPosition<f64>) -> Vec2 {
        let scale_factor = self
            .window
            .as_ref()
            .map(|window| window.scale_factor())
            .unwrap_or(1.0) as f32;
        cursor_logical_position_for_scale(physical, scale_factor)
    }

    fn cursor_screen_position(&self, local_logical: Vec2) -> Vec2 {
        cursor_screen_position_for_window(self.physics.position, local_logical)
    }

    fn handle_interaction_events(&mut self, events: Vec<InteractionEvent>) {
        for event in events {
            match event {
                InteractionEvent::HoverChanged(hovered) => {
                    self.pet.set_hovered(hovered);
                }
                InteractionEvent::DragStarted { .. } => {
                    self.pet.set_dragging(true);
                }
                InteractionEvent::DragMoved { delta } => {
                    self.physics.position.x += delta.x;
                    self.physics.position.y += delta.y;
                    self.physics.clamp_to_bounds();
                    self.move_window_to_pet();
                }
                InteractionEvent::DragEnded { .. } => {
                    self.pet.set_dragging(false);
                    self.physics.clamp_to_bounds();
                    self.move_window_to_pet();
                    self.persist_current_position();
                }
                InteractionEvent::ContextMenuRequested { .. } => {
                    self.show_context_menu(self.last_cursor_local_position);
                }
            }
        }
    }

    fn handle_cursor_left(&mut self) {
        let screen_logical = self
            .last_cursor_screen_position
            .unwrap_or(self.physics.position);
        let events = if self.interaction.is_dragging() {
            self.interaction
                .mouse_released(screen_logical, MouseButtonKind::Left, false)
        } else {
            self.interaction.pointer_moved(screen_logical, false)
        };
        self.handle_interaction_events(events);
        self.last_cursor_local_position = None;
        self.last_cursor_screen_position = None;
    }
}

fn cursor_logical_position_for_scale(physical: PhysicalPosition<f64>, scale_factor: f32) -> Vec2 {
    let scale_factor = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    Vec2 {
        x: physical.x as f32 / scale_factor,
        y: physical.y as f32 / scale_factor,
    }
}

fn cursor_screen_position_for_window(window_position: Vec2, local_logical: Vec2) -> Vec2 {
    Vec2 {
        x: window_position.x + local_logical.x,
        y: window_position.y + local_logical.y,
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

    fn handle_non_quit_command_for_test(&mut self, command: AppCommand) -> bool {
        self.handle_non_quit_command(command)
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
            WindowEvent::CursorMoved { position, .. } => {
                let local_logical = self.cursor_logical_position(position);
                let screen_logical = self.cursor_screen_position(local_logical);
                self.last_cursor_local_position = Some(local_logical);
                self.last_cursor_screen_position = Some(screen_logical);
                let hit = self.current_sprite_hit_test(local_logical);
                let events = self.interaction.pointer_moved(screen_logical, hit);
                self.handle_interaction_events(events);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (Some(local_logical), Some(screen_logical)) = (
                    self.last_cursor_local_position,
                    self.last_cursor_screen_position,
                ) else {
                    return;
                };
                let button = Self::mouse_button_kind(button);
                let hit = self.current_sprite_hit_test(local_logical);
                let events = match state {
                    winit::event::ElementState::Pressed => {
                        self.interaction.mouse_pressed(screen_logical, button, hit)
                    }
                    winit::event::ElementState::Released => {
                        self.interaction.mouse_released(screen_logical, button, hit)
                    }
                };
                self.handle_interaction_events(events);
            }
            WindowEvent::CursorLeft { .. } => {
                self.handle_cursor_left();
            }
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

    #[test]
    fn non_quit_command_sets_personality_through_runtime_settings_path() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;

        assert!(
            app.handle_non_quit_command_for_test(AppCommand::SetPersonality(
                crate::pet::Personality::Calm,
            ))
        );

        assert_eq!(
            app.settings_for_test().personality,
            crate::pet::Personality::Calm
        );
        assert_eq!(app.pet.personality(), crate::pet::Personality::Calm);
    }

    #[test]
    fn non_quit_command_clamps_scale_through_runtime_settings_path() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;

        assert!(app.handle_non_quit_command_for_test(AppCommand::SetScale(99.0)));

        assert_eq!(app.settings_for_test().scale, AppSettings::MAX_SCALE);
        assert_eq!(
            app.physics.size,
            Vec2 {
                x: FRAME_SIZE as f32 * AppSettings::MAX_SCALE,
                y: FRAME_SIZE as f32 * AppSettings::MAX_SCALE,
            }
        );
    }

    #[test]
    fn mouse_button_kind_maps_winit_buttons_to_interaction_buttons() {
        assert_eq!(
            DesktopPetApp::mouse_button_kind(winit::event::MouseButton::Left),
            MouseButtonKind::Left
        );
        assert_eq!(
            DesktopPetApp::mouse_button_kind(winit::event::MouseButton::Right),
            MouseButtonKind::Right
        );
        assert_eq!(
            DesktopPetApp::mouse_button_kind(winit::event::MouseButton::Middle),
            MouseButtonKind::Other
        );
    }

    #[test]
    fn cursor_logical_position_converts_physical_pixels_with_scale_factor() {
        assert_eq!(
            cursor_logical_position_for_scale(winit::dpi::PhysicalPosition::new(80.0, 120.0), 2.0),
            Vec2 { x: 40.0, y: 60.0 }
        );
    }

    #[test]
    fn cursor_screen_position_offsets_local_point_by_window_position() {
        assert_eq!(
            cursor_screen_position_for_window(
                Vec2 { x: 120.0, y: 90.0 },
                Vec2 { x: 12.0, y: 34.0 }
            ),
            Vec2 { x: 132.0, y: 124.0 }
        );
    }

    #[test]
    fn cursor_left_ends_active_drag_and_clears_cached_positions() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;
        app.physics.position = Vec2 { x: 120.0, y: 90.0 };
        app.last_cursor_local_position = Some(Vec2 { x: 12.0, y: 34.0 });
        app.last_cursor_screen_position = Some(Vec2 { x: 132.0, y: 124.0 });
        app.interaction
            .pointer_moved(Vec2 { x: 132.0, y: 124.0 }, true);
        app.interaction
            .mouse_pressed(Vec2 { x: 132.0, y: 124.0 }, MouseButtonKind::Left, true);
        app.pet.set_hovered(true);
        app.pet.set_dragging(true);

        app.handle_cursor_left();

        assert!(!app.interaction.is_dragging());
        assert_eq!(app.pet.behavior_mode(), crate::pet::BehaviorMode::Default);
        assert_eq!(app.last_cursor_local_position, None);
        assert_eq!(app.last_cursor_screen_position, None);
        assert_eq!(
            app.settings.last_position,
            Some(crate::settings::StoredPosition { x: 120.0, y: 90.0 })
        );
    }
}
