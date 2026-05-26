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
    micro_action::MicroAction,
    pet::{Direction, PetRuntime, PetState},
    physics::{Bounds, Physics, Vec2},
    renderer::PetRenderer,
    settings::{default_settings_path, AppSettings, SettingsError},
    settings_window_macos::SettingsWindowController,
    sprite::SpriteSheet,
    window_macos::{apply_desktop_pet_window_behavior, set_pet_window_mouse_passthrough},
};

pub const WINDOW_SCALE: u32 = 2;

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
    SetFocusMode(bool),
    ToggleFocusMode,
    SetFollowCursorWhenIdle(bool),
    SetAvoidTextCursor(bool),
    SetHideOnFullscreen(bool),
    RequestAccessibilityPermission,
    Nap,
    CheerUp,
    Quit,
}

pub struct DesktopPetApp {
    window: Option<Arc<Window>>,
    renderer: Option<PetRenderer>,
    sprite_sheet: Option<SpriteSheet>,
    pet: PetRuntime,
    physics: Physics,
    last_tick: Instant,
    next_tick_at: Instant,
    menu_bar: Option<MenuBarController>,
    settings_window: Option<SettingsWindowController>,
    settings: AppSettings,
    settings_path: Option<std::path::PathBuf>,
    active_monitor_name: Option<String>,
    pet_visible: bool,
    auto_hidden: bool,
    interaction: InteractionState,
    last_cursor_local_position: Option<Vec2>,
    last_cursor_screen_position: Option<Vec2>,
    workspace_observer: crate::workspace::WorkspaceObserver,
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
            pet: PetRuntime::new_with_seed(seed),
            physics: default_physics(),
            last_tick: now,
            next_tick_at: now,
            menu_bar: None,
            settings_window: None,
            settings: AppSettings::default(),
            settings_path: default_settings_path().ok(),
            active_monitor_name: None,
            pet_visible: true,
            auto_hidden: false,
            interaction: InteractionState::default(),
            last_cursor_local_position: None,
            last_cursor_screen_position: None,
            workspace_observer: crate::workspace::WorkspaceObserver::new(),
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
            pet: PetRuntime::new_with_seed(seed),
            physics: default_physics(),
            last_tick: now,
            next_tick_at: now,
            menu_bar: None,
            settings_window: None,
            settings: AppSettings::default(),
            settings_path: default_settings_path().ok(),
            active_monitor_name: None,
            pet_visible: true,
            auto_hidden: false,
            interaction: InteractionState::default(),
            last_cursor_local_position: None,
            last_cursor_screen_position: None,
            workspace_observer: crate::workspace::WorkspaceObserver::new(),
            event_proxy,
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let attributes = WindowAttributes::default()
            .with_title("Happy Cappy")
            .with_inner_size({
                let (fw, fh) = self.pet.frame_size();
                LogicalSize::new((fw * WINDOW_SCALE) as f64, (fh * WINDOW_SCALE) as f64)
            })
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
        let settings = self.read_settings();
        self.settings.monitor_behavior = settings.monitor_behavior;
        self.update_bounds_from_window(event_loop);
        self.apply_settings(settings);
        self.workspace_observer
            .request_accessibility_on_startup_if_enabled(self.settings.avoid_text_cursor);
        if !self.pet_visible {
            window.set_visible(false);
        }
        self.move_window_to_pet();

        let surface_size = window.inner_size();
        match PetRenderer::new(
            Arc::clone(&window),
            surface_size.width,
            surface_size.height,
            self.pet.frame_size().0,
            self.pet.frame_size().1,
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

        match SpriteSheet::load(&paths.sprite_sheet, &self.pet.manifest().frame) {
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
        let current_monitor = self
            .window
            .as_ref()
            .and_then(|window| window.current_monitor());
        let primary_monitor = event_loop.primary_monitor();
        let monitor = match self.settings.monitor_behavior {
            crate::settings::MonitorBehavior::CurrentDisplay => current_monitor.or(primary_monitor),
            crate::settings::MonitorBehavior::PrimaryDisplay => primary_monitor.or(current_monitor),
        };

        if let Some(monitor) = monitor {
            self.active_monitor_name = monitor.name();
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

            // Keep the workspace observer's active-display info in lockstep with physics bounds.
            let primary_height = event_loop
                .primary_monitor()
                .map(|m| (m.size().height as f32) / (m.scale_factor() as f32))
                .unwrap_or(0.0);
            self.workspace_observer
                .set_active_display(Some(crate::workspace::DisplayInfo {
                    name: monitor.name(),
                    bounds_logical: self.physics.bounds.into(),
                    scale_factor,
                    primary_display_height: primary_height,
                }));
        } else {
            self.active_monitor_name = None;
            self.workspace_observer.set_active_display(None);
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

        // Poll workspace state. Owned WorkspaceTick releases the &mut observer borrow,
        // letting us call &mut self methods (sync_settings_window, set_auto_hidden) below.
        let workspace_tick = self.workspace_observer.tick(now);
        let snapshot = workspace_tick.snapshot;

        // If AX trust changed, refresh the Settings UI label.
        if workspace_tick.trust_changed {
            self.sync_settings_window();
        }

        // Decide pet behavior intent based on observation + settings + current pet frame.
        let pet_frame = crate::physics::Rect {
            min: self.physics.position,
            max: crate::physics::Vec2 {
                x: self.physics.position.x + self.physics.size.x,
                y: self.physics.position.y + self.physics.size.y,
            },
        };
        let intent = decide_intent(&snapshot, &self.settings, pet_frame);
        self.pet.set_intent(intent);

        // Drive fullscreen auto-hide.
        let should_auto_hide = self.settings.hide_on_fullscreen && snapshot.fullscreen_active;
        if should_auto_hide != self.auto_hidden {
            self.set_auto_hidden(should_auto_hide);
        }

        let tick = self.pet.tick(dt);
        self.physics.velocity.x = tick.speed_x;
        let physics_step = self.physics.update(dt.as_secs_f32());
        if tick.state == PetState::Walk && physics_step.bounced_x {
            self.pet.turn_around();
        }
        self.move_window_to_pet();
        if self.effective_window_visible() {
            window.request_redraw();
        }
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
        let focus_mode_turned_on = !self.settings.focus_mode && settings.focus_mode;
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
        let (fw, fh) = self.pet.frame_size();
        self.physics.size = Vec2 {
            x: fw as f32 * settings.scale,
            y: fh as f32 * settings.scale,
        };

        let had_restored_position = settings.last_position.is_some();
        if let Some(position) =
            settings.restored_position_for_display(self.active_monitor_name.as_deref())
        {
            self.physics.position = position;
        }
        self.physics.clamp_to_bounds();
        if had_restored_position {
            settings.update_position_for_display(
                self.physics.position,
                self.active_monitor_name.as_deref(),
            );
        }

        self.settings = settings;
        if focus_mode_turned_on {
            self.clear_interaction_state();
        }
        if let Some(window) = &self.window {
            let size = LogicalSize::new(self.physics.size.x as f64, self.physics.size.y as f64);
            let _ = window.request_inner_size(size);
        }
        self.sync_settings_window();
        self.sync_menu_bar();
        self.move_window_to_pet();
        self.sync_window_passthrough();
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

    fn read_settings(&self) -> AppSettings {
        let Some(path) = &self.settings_path else {
            return AppSettings::default();
        };
        match AppSettings::load_from(path) {
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
        }
    }

    #[allow(dead_code)]
    fn set_pet_visible(&mut self, visible: bool) {
        self.settings.pet_visible = visible;
        self.pet_visible = visible;
        self.pet.set_hidden(!visible);
        self.apply_window_visibility();
        self.sync_settings_window();
        self.sync_menu_bar();
        self.save_settings();
    }

    pub fn set_auto_hidden(&mut self, hidden: bool) {
        if hidden && self.interaction.is_dragging() {
            let last_pointer = self
                .last_cursor_screen_position
                .unwrap_or(self.physics.position);
            let events = self.interaction.mouse_released(
                last_pointer,
                MouseButtonKind::Left,
                /*hit_visible_pixel=*/ false,
            );
            self.handle_interaction_events(events);
        }
        self.auto_hidden = hidden;
        self.apply_window_visibility();
    }

    #[allow(dead_code)]
    fn effective_window_visible(&self) -> bool {
        self.pet_visible && !self.auto_hidden
    }

    #[allow(dead_code)]
    fn apply_window_visibility(&mut self) {
        let visible = self.effective_window_visible();
        if let Some(window) = &self.window {
            window.set_visible(visible);
            if visible {
                window.request_redraw();
            }
        }
        if visible {
            self.next_tick_at = Instant::now();
        }
    }

    #[allow(dead_code)]
    fn reset_pet_position(&mut self) {
        self.physics.position = Vec2 {
            x: self.physics.bounds.min_x + 120.0,
            y: self.physics.bounds.min_y + 120.0,
        };
        self.physics.clamp_to_bounds();
        self.settings.update_position_for_display(
            self.physics.position,
            self.active_monitor_name.as_deref(),
        );
        self.move_window_to_pet();
        self.save_settings();
    }

    fn set_focus_mode(&mut self, focus_mode: bool) {
        let mut settings = self.settings.clone();
        settings.focus_mode = focus_mode;
        self.apply_settings(settings);
        self.save_settings();
    }

    fn clear_interaction_state(&mut self) {
        self.interaction = InteractionState::default();
        self.pet.set_hovered(false);
        self.pet.set_dragging(false);
        self.last_cursor_local_position = None;
        self.last_cursor_screen_position = None;
    }

    fn sync_window_passthrough(&self) {
        if let Some(window) = &self.window {
            if let Err(error) = set_pet_window_mouse_passthrough(window, self.settings.focus_mode) {
                warn!("failed to sync pet window mouse passthrough: {error}");
            }
        }
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
                self.set_monitor_behavior(monitor_behavior, None);
            }
            AppCommand::SetFocusMode(focus_mode) => self.set_focus_mode(focus_mode),
            AppCommand::ToggleFocusMode => self.set_focus_mode(!self.settings.focus_mode),
            AppCommand::SetFollowCursorWhenIdle(value) => {
                let mut settings = self.settings.clone();
                settings.follow_cursor_when_idle = value;
                self.apply_settings(settings);
                self.save_settings();
            }
            AppCommand::SetAvoidTextCursor(value) => {
                let mut settings = self.settings.clone();
                settings.avoid_text_cursor = value;
                self.apply_settings(settings);
                self.save_settings();
                // Toggling on with permission missing: trigger the prompt right
                // away. macOS may suppress the dialog after sticky denial; the
                // inline AX status label in Settings communicates the degraded
                // state. The checkbox stays checked because the persisted setting
                // is the user's intent.
                if value && !self.workspace_observer.is_accessibility_trusted() {
                    self.workspace_observer.request_accessibility_now();
                }
            }
            AppCommand::SetHideOnFullscreen(value) => {
                let mut settings = self.settings.clone();
                settings.hide_on_fullscreen = value;
                self.apply_settings(settings);
                self.save_settings();
            }
            AppCommand::RequestAccessibilityPermission => {
                self.workspace_observer.request_accessibility_now();
            }
            AppCommand::Nap => {
                self.pet.start_micro_action(MicroAction::Nap);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            AppCommand::CheerUp => {
                self.pet.start_micro_action(MicroAction::CheerUp);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            AppCommand::Quit => return false,
        }
        true
    }

    fn handle_command(&mut self, command: AppCommand, event_loop: &ActiveEventLoop) {
        match command {
            AppCommand::Quit => event_loop.exit(),
            AppCommand::SetMonitorBehavior(monitor_behavior) => {
                self.set_monitor_behavior(monitor_behavior, Some(event_loop));
            }
            _ => {
                self.handle_non_quit_command(command);
            }
        }
    }

    fn set_monitor_behavior(
        &mut self,
        monitor_behavior: crate::settings::MonitorBehavior,
        event_loop: Option<&ActiveEventLoop>,
    ) {
        let mut settings = self.settings.clone();
        settings.monitor_behavior = monitor_behavior;
        self.settings.monitor_behavior = monitor_behavior;
        if let Some(event_loop) = event_loop {
            self.update_bounds_from_window(event_loop);
        }
        settings.sanitize(self.physics.bounds, self.physics.size);
        self.apply_settings(settings);
        self.save_settings();
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
            settings_window.sync_settings(
                &self.settings,
                self.workspace_observer.is_accessibility_trusted(),
            );
            settings_window.show();
            self.next_tick_at = Instant::now();
        } else {
            warn!("settings window is not available on this platform or thread");
        }
    }

    fn sync_settings_window(&self) {
        if let Some(settings_window) = &self.settings_window {
            settings_window.sync_settings(
                &self.settings,
                self.workspace_observer.is_accessibility_trusted(),
            );
        }
    }

    fn sync_menu_bar(&self) {
        if let Some(menu_bar) = &self.menu_bar {
            menu_bar.sync_runtime_state(self.pet_visible, self.settings.focus_mode);
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
                self.settings.focus_mode,
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
                    self.settings.focus_mode,
                    local_position,
                );
            }
        }
    }

    #[allow(dead_code)]
    fn persist_current_position(&mut self) {
        self.settings.update_position_for_display(
            self.physics.position,
            self.active_monitor_name.as_deref(),
        );
        self.save_settings();
    }

    fn tick_due(&self, now: Instant) -> bool {
        now >= self.next_tick_at
    }

    fn next_tick_interval(&self) -> Duration {
        let settings_visible = self
            .settings_window
            .as_ref()
            .is_some_and(|w| w.is_visible());
        if settings_visible {
            return Duration::from_millis(500);
        }
        if !self.pet_visible {
            return Duration::from_secs(5);
        }
        if self.auto_hidden {
            return Duration::from_millis(500);
        }
        match self.pet.behavior_mode() {
            crate::pet::BehaviorMode::Hovered
            | crate::pet::BehaviorMode::Dragging
            | crate::pet::BehaviorMode::Action
            | crate::pet::BehaviorMode::Walking => TARGET_FRAME_TIME,
            crate::pet::BehaviorMode::Hidden => Duration::from_secs(5),
            crate::pet::BehaviorMode::Default => match self.pet.state() {
                PetState::Walk => TARGET_FRAME_TIME,
                PetState::Idle => IDLE_FRAME_TIME,
                PetState::Sleep => SLEEP_FRAME_TIME,
            },
        }
    }

    fn draw(&mut self) {
        if !self.pet_visible {
            return;
        }

        let (Some(renderer), Some(sprite_sheet)) =
            (self.renderer.as_mut(), self.sprite_sheet.as_ref())
        else {
            return;
        };

        let sprite_index = self.pet.current_sprite_index();
        let flip_x = self.pet.current_animation_name() == "walk-right"
            && self.pet.direction() == Direction::Left;
        let rect = sprite_sheet.frame_rect(sprite_index);

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
        let sprite_index = self.pet.current_sprite_index();
        let rect = sprite_sheet.frame_rect(sprite_index);
        let scale = if self.settings.scale.is_finite() && self.settings.scale > 0.0 {
            self.settings.scale
        } else {
            AppSettings::MIN_SCALE
        };
        let scaled_point = Vec2 {
            x: point.x / scale,
            y: point.y / scale,
        };
        let flip_x = self.pet.current_animation_name() == "walk-right"
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
            x: (64 * WINDOW_SCALE) as f32,
            y: (64 * WINDOW_SCALE) as f32,
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
        self.sync_menu_bar();
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

pub(crate) fn decide_intent(
    snapshot: &crate::workspace::WorkspaceSnapshot,
    settings: &crate::settings::AppSettings,
    pet_frame: crate::physics::Rect,
) -> crate::pet::BehaviorIntent {
    use crate::pet::{BehaviorIntent, Direction};

    let pet_center_x = (pet_frame.min.x + pet_frame.max.x) * 0.5;

    if settings.avoid_text_cursor {
        if let Some(caret) = snapshot.caret_rect {
            if caret.intersects(&pet_frame) {
                // Pick the side of the caret rect that's closer to the pet center.
                let exit_left_dx = (pet_center_x - caret.min.x).abs();
                let exit_right_dx = (caret.max.x - pet_center_x).abs();
                let direction = if exit_left_dx < exit_right_dx {
                    Direction::Left
                } else {
                    Direction::Right
                };
                return BehaviorIntent::AvoidRectHorizontal { direction };
            }
        }
    }

    if settings.follow_cursor_when_idle {
        if snapshot.is_idle() {
            let direction = if snapshot.cursor_pos.x > pet_center_x {
                Direction::Right
            } else {
                Direction::Left
            };
            return BehaviorIntent::ChaseHorizontal { direction };
        }
        if snapshot.is_busy() {
            let direction = if snapshot.cursor_pos.x > pet_center_x {
                Direction::Left
            } else {
                Direction::Right
            };
            return BehaviorIntent::AvoidHorizontal { direction };
        }
    }

    BehaviorIntent::Idle
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
    fn hidden_pet_uses_slow_tick_interval() {
        let mut app = DesktopPetApp::new_for_test();
        app.pet_visible = false;
        app.pet.set_hidden(true);

        assert_eq!(app.next_tick_interval(), Duration::from_secs(5));
    }

    #[test]
    fn hovered_pet_uses_target_frame_interval() {
        let mut app = DesktopPetApp::new_for_test();
        app.pet.set_hovered(true);

        assert_eq!(app.next_tick_interval(), TARGET_FRAME_TIME);
    }

    #[test]
    fn showing_hidden_pet_reschedules_next_tick_immediately() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;
        app.pet_visible = false;
        app.pet.set_hidden(true);
        app.next_tick_at = Instant::now() + Duration::from_secs(5);

        app.set_pet_visible(true);

        assert!(app.next_tick_at <= Instant::now());
        assert!(app.pet_visible);
        assert_eq!(app.pet.behavior_mode(), crate::pet::BehaviorMode::Default);
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
            last_position: Some(crate::settings::StoredPosition {
                x: 200.0,
                y: 220.0,
                display_name: None,
            }),
            ..crate::settings::AppSettings::default()
        };

        app.apply_settings_for_test(settings);

        assert_eq!(app.physics.size, Vec2 { x: 192.0, y: 192.0 });
        assert_eq!(app.physics.position, Vec2 { x: 58.0, y: 58.0 });
        assert_eq!(
            app.settings_for_test().last_position,
            Some(crate::settings::StoredPosition {
                x: 58.0,
                y: 58.0,
                display_name: None,
            })
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
    fn applying_settings_skips_position_from_different_display() {
        let mut app = DesktopPetApp::new_for_test();
        app.active_monitor_name = Some("Built-in Display".to_string());
        let settings = crate::settings::AppSettings {
            last_position: Some(crate::settings::StoredPosition {
                x: 300.0,
                y: 300.0,
                display_name: Some("External Display".to_string()),
            }),
            ..crate::settings::AppSettings::default()
        };

        app.apply_settings_for_test(settings);

        assert_eq!(app.physics.position, Vec2 { x: 120.0, y: 120.0 });
        assert_eq!(
            app.settings_for_test().last_position,
            Some(crate::settings::StoredPosition {
                x: 120.0,
                y: 120.0,
                display_name: Some("Built-in Display".to_string()),
            })
        );
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
                x: 64.0 * AppSettings::MAX_SCALE,
                y: 64.0 * AppSettings::MAX_SCALE,
            }
        );
    }

    #[test]
    fn non_quit_command_updates_monitor_behavior_setting() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;

        assert!(
            app.handle_non_quit_command_for_test(AppCommand::SetMonitorBehavior(
                crate::settings::MonitorBehavior::PrimaryDisplay
            ),)
        );

        assert_eq!(
            app.settings_for_test().monitor_behavior,
            crate::settings::MonitorBehavior::PrimaryDisplay
        );
    }

    #[test]
    fn non_quit_command_toggles_focus_mode() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;

        assert!(app.handle_non_quit_command_for_test(AppCommand::ToggleFocusMode));
        assert!(app.settings_for_test().focus_mode);

        assert!(app.handle_non_quit_command_for_test(AppCommand::ToggleFocusMode));
        assert!(!app.settings_for_test().focus_mode);
    }

    #[test]
    fn set_focus_mode_command_sets_exact_value() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;

        assert!(app.handle_non_quit_command_for_test(AppCommand::SetFocusMode(true)));
        assert!(app.settings_for_test().focus_mode);

        assert!(app.handle_non_quit_command_for_test(AppCommand::SetFocusMode(false)));
        assert!(!app.settings_for_test().focus_mode);
    }

    #[test]
    fn nap_command_starts_sleepy_action() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;

        assert!(app.handle_non_quit_command_for_test(AppCommand::Nap));

        assert_eq!(app.pet.behavior_mode(), crate::pet::BehaviorMode::Action);
        assert_eq!(app.pet.current_animation_name(), "sleepy");
    }

    #[test]
    fn cheer_up_command_starts_happy_action() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;

        assert!(app.handle_non_quit_command_for_test(AppCommand::CheerUp));

        assert_eq!(app.pet.behavior_mode(), crate::pet::BehaviorMode::Action);
        assert_eq!(app.pet.current_animation_name(), "happy");
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
            Some(crate::settings::StoredPosition {
                x: 120.0,
                y: 90.0,
                display_name: None,
            })
        );
    }

    #[test]
    fn effective_window_visible_truth_table() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        app.pet_visible = true;
        app.auto_hidden = false;
        assert!(app.effective_window_visible());

        app.auto_hidden = true;
        assert!(!app.effective_window_visible());

        app.pet_visible = false;
        app.auto_hidden = false;
        assert!(!app.effective_window_visible());

        app.auto_hidden = true;
        assert!(!app.effective_window_visible());
    }

    #[test]
    fn set_auto_hidden_does_not_modify_settings() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        app.settings.pet_visible = true;
        app.set_auto_hidden(true);
        assert!(
            app.settings.pet_visible,
            "auto-hide must not touch the persisted setting"
        );
        assert!(app.auto_hidden);
    }

    #[test]
    fn set_auto_hidden_persistence_sequence() {
        let mut app = DesktopPetApp::new_with_event_proxy(None);
        app.pet_visible = true;
        app.auto_hidden = false;

        app.set_auto_hidden(true);
        assert!(!app.effective_window_visible());

        app.set_pet_visible(false);
        assert!(!app.effective_window_visible());

        app.set_auto_hidden(false);
        assert!(
            !app.effective_window_visible(),
            "pet_visible drives the final result"
        );
    }
}

#[cfg(test)]
mod decide_intent_tests {
    use super::*;
    use crate::pet::{BehaviorIntent, Direction};
    use crate::physics::{Rect, Vec2};
    use crate::settings::AppSettings;
    use crate::workspace::WorkspaceSnapshot;

    fn settings_all_on() -> AppSettings {
        AppSettings::default()
    }

    fn snap(idle: f32, cursor_x: f32, caret: Option<Rect>) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            workspace_available: true,
            seconds_idle: idle,
            typing_rate_per_sec: 0.0,
            frontmost_bundle_id: None,
            frontmost_is_editor: false,
            caret_rect: caret,
            fullscreen_active: false,
            cursor_pos: Vec2 {
                x: cursor_x,
                y: 0.0,
            },
        }
    }

    fn pet_frame_at(x: f32) -> Rect {
        Rect {
            min: Vec2 { x, y: 0.0 },
            max: Vec2 {
                x: x + 100.0,
                y: 100.0,
            },
        }
    }

    #[test]
    fn idle_when_no_signals() {
        let settings = settings_all_on();
        let intent = decide_intent(&snap(3.5, 50.0, None), &settings, pet_frame_at(0.0));
        assert_eq!(intent, BehaviorIntent::Idle);
    }

    #[test]
    fn chase_right_when_idle_and_cursor_to_right() {
        let intent = decide_intent(
            &snap(6.0, 1000.0, None),
            &settings_all_on(),
            pet_frame_at(0.0),
        );
        assert_eq!(
            intent,
            BehaviorIntent::ChaseHorizontal {
                direction: Direction::Right
            }
        );
    }

    #[test]
    fn chase_left_when_idle_and_cursor_to_left() {
        let intent = decide_intent(
            &snap(6.0, -50.0, None),
            &settings_all_on(),
            pet_frame_at(0.0),
        );
        assert_eq!(
            intent,
            BehaviorIntent::ChaseHorizontal {
                direction: Direction::Left
            }
        );
    }

    #[test]
    fn avoid_horizontal_when_busy_and_cursor_to_right() {
        // seconds_idle < 2.0 → busy
        let intent = decide_intent(
            &snap(0.5, 1000.0, None),
            &settings_all_on(),
            pet_frame_at(0.0),
        );
        assert_eq!(
            intent,
            BehaviorIntent::AvoidHorizontal {
                direction: Direction::Left
            }
        );
    }

    #[test]
    fn avoid_rect_overrides_chase_when_caret_intersects_pet() {
        let caret = Rect {
            min: Vec2 { x: 60.0, y: 40.0 },
            max: Vec2 { x: 120.0, y: 60.0 },
        };
        // pet_center_x = 100.0; exit_left_dx = 40, exit_right_dx = 20 → exit Right.
        let intent = decide_intent(
            &snap(6.0, 1000.0, Some(caret)),
            &settings_all_on(),
            pet_frame_at(50.0),
        );
        assert_eq!(
            intent,
            BehaviorIntent::AvoidRectHorizontal {
                direction: Direction::Right
            }
        );
    }

    #[test]
    fn caret_rect_not_intersecting_does_not_trigger_avoid_rect() {
        let caret = Rect {
            min: Vec2 { x: 500.0, y: 40.0 },
            max: Vec2 { x: 560.0, y: 60.0 },
        };
        // Idle and cursor to right; AvoidRect should NOT fire since rect is far away.
        let intent = decide_intent(
            &snap(6.0, 1000.0, Some(caret)),
            &settings_all_on(),
            pet_frame_at(0.0),
        );
        assert_eq!(
            intent,
            BehaviorIntent::ChaseHorizontal {
                direction: Direction::Right
            }
        );
    }

    #[test]
    fn all_gates_off_returns_idle() {
        let mut settings = settings_all_on();
        settings.follow_cursor_when_idle = false;
        settings.avoid_text_cursor = false;
        let caret = Rect {
            min: Vec2 { x: 60.0, y: 40.0 },
            max: Vec2 { x: 120.0, y: 60.0 },
        };
        let intent = decide_intent(
            &snap(6.0, 1000.0, Some(caret)),
            &settings,
            pet_frame_at(50.0),
        );
        assert_eq!(intent, BehaviorIntent::Idle);
    }

    #[test]
    fn avoid_horizontal_disabled_when_follow_cursor_off() {
        let mut settings = settings_all_on();
        settings.follow_cursor_when_idle = false;
        // seconds_idle < 2.0 → busy, cursor to right.
        // With follow_cursor_when_idle off, the avoid arm must also be disabled.
        let intent = decide_intent(&snap(0.5, 1000.0, None), &settings, pet_frame_at(0.0));
        assert_eq!(intent, BehaviorIntent::Idle);
    }

    #[test]
    fn handle_command_set_follow_cursor_updates_settings() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;
        app.settings.follow_cursor_when_idle = true;
        app.handle_non_quit_command_for_test(AppCommand::SetFollowCursorWhenIdle(false));
        assert!(!app.settings.follow_cursor_when_idle);
    }

    #[test]
    fn handle_command_set_hide_on_fullscreen_updates_settings() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;
        app.settings.hide_on_fullscreen = true;
        app.handle_non_quit_command_for_test(AppCommand::SetHideOnFullscreen(false));
        assert!(!app.settings.hide_on_fullscreen);
    }

    #[test]
    fn handle_command_set_avoid_text_cursor_updates_settings() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;
        app.settings.avoid_text_cursor = false;
        app.handle_non_quit_command_for_test(AppCommand::SetAvoidTextCursor(true));
        assert!(app.settings.avoid_text_cursor);
    }
}
