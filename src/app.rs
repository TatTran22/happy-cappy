use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use log::{error, warn};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    window::{Window, WindowAttributes, WindowId},
};

use crate::{
    bundle::current_resource_paths,
    menu_bar::MenuBarController,
    pet::{Direction, Pet, PetState},
    physics::{Bounds, Physics, Vec2},
    renderer::PetRenderer,
    sprite::{SpriteRow, SpriteSheet},
    window_macos::apply_desktop_pet_window_behavior,
};

pub const FRAME_SIZE: u32 = 64;
pub const WINDOW_SCALE: u32 = 2;
pub const WINDOW_SIZE: u32 = FRAME_SIZE * WINDOW_SCALE;

const TARGET_FRAME_TIME: Duration = Duration::from_millis(16);
const IDLE_FRAME_TIME: Duration = Duration::from_millis(200);
const SLEEP_FRAME_TIME: Duration = Duration::from_millis(500);
const MAX_TICK_DELTA: Duration = Duration::from_millis(100);
const FALLBACK_BOUNDS_WIDTH: f32 = 800.0;
const FALLBACK_BOUNDS_HEIGHT: f32 = 600.0;

pub struct DesktopPetApp {
    window: Option<Arc<Window>>,
    renderer: Option<PetRenderer>,
    sprite_sheet: Option<SpriteSheet>,
    pet: Pet,
    physics: Physics,
    last_tick: Instant,
    menu_bar: Option<MenuBarController>,
}

impl DesktopPetApp {
    pub fn new() -> Self {
        let seed = fastrand::u64(..);

        Self {
            window: None,
            renderer: None,
            sprite_sheet: None,
            pet: Pet::new_with_seed(seed),
            physics: Physics {
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
            },
            last_tick: Instant::now(),
            menu_bar: None,
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

    fn tick(&mut self) {
        let Some(window) = self.window.as_ref().map(Arc::clone) else {
            self.last_tick = Instant::now();
            return;
        };

        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).min(MAX_TICK_DELTA);
        self.last_tick = now;

        let tick = self.pet.tick(dt);
        self.physics.velocity.x = tick.speed_x;
        self.physics.update(dt.as_secs_f32());
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

        let row = match self.pet.state() {
            PetState::Idle => SpriteRow::Idle,
            PetState::Walk => SpriteRow::WalkRight,
            PetState::Sleep => SpriteRow::Sleep,
        };
        let flip_x = self.pet.state() == PetState::Walk && self.pet.direction() == Direction::Left;
        let rect = sprite_sheet.frame_rect(row, self.pet.frame_index());

        if let Err(error) = renderer.draw(sprite_sheet.image(), rect, flip_x) {
            warn!("failed to draw desktop pet frame: {error}");
        }
    }
}

impl Default for DesktopPetApp {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationHandler for DesktopPetApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.sprite_sheet.is_none() && !self.load_assets(event_loop) {
            return;
        }

        if self.window.is_none() && !self.create_window(event_loop) {
            return;
        }

        if self.menu_bar.is_none() {
            self.menu_bar = MenuBarController::new();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.tick();
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + self.next_tick_interval(),
        ));
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
