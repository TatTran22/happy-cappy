# Desktop Pet V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a bundled native macOS desktop pet app with a personalized pixel-art sprite, transparent click-through window behavior, basic movement/animation, menu bar Quit, and release verification.

**Architecture:** Implement the testable Rust core first: pet state, physics, sprite grid validation, and alpha blitting. Then wire `winit + pixels` into a macOS `.app` bundle and keep unsafe/AppKit behavior isolated in small macOS-only modules. Asset creation uses `imagegen` plus chroma-key cleanup, with validation before integration.

**Tech Stack:** Rust 2021, `winit 0.30.13`, `pixels 0.17.1`, `image 0.25`, `fastrand 2`, `env_logger 0.11`, `log 0.4`, `objc2 0.6`, `objc2-app-kit 0.3`, macOS app bundle with `LSUIElement=true`.

---

## Scope Check

This plan covers one coherent V1: a single native desktop pet app. It includes domain logic, rendering, macOS window behavior, app bundling, one personalized sprite sheet, and verification. It does not include settings UI, auto-update, multi-pet, drag/feed interactions, persistent mood, or full multi-monitor roaming.

## File Structure

- Create `.gitignore`: ignores Rust build outputs, local app bundle outputs, and visual brainstorming scratch.
- Create `Cargo.toml`: package metadata, pinned runtime dependencies, release profile.
- Create `src/lib.rs`: exports testable modules.
- Create `src/main.rs`: logging setup and `winit` event loop entrypoint.
- Create `src/app.rs`: `ApplicationHandler` runtime orchestration.
- Create `src/physics.rs`: pure movement, bounds clamp, edge bounce.
- Create `src/pet.rs`: pure pet state machine and animation timing.
- Create `src/sprite.rs`: sprite sheet decode/validation/frame lookup.
- Create `src/renderer.rs`: testable alpha blitting and `pixels` presentation wrapper.
- Create `src/bundle.rs`: resource path lookup for bundled and development modes.
- Create `src/window_macos.rs`: macOS-only window behavior tweaks.
- Create `src/menu_bar.rs`: macOS-only status item with Quit.
- Create `assets/pet_spritesheet.png`: final transparent sprite sheet.
- Create `packaging/Info.plist`: `DesktopPet.app` metadata.
- Create `scripts/build_app.sh`: release build and app bundle assembly.
- Create `scripts/verify.sh`: automated verification bundle.

---

### Task 1: Rust Project Scaffold

**Files:**
- Create: `.gitignore`
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/main.rs`
- Create: stub module files under `src/`
- Create: `scripts/verify.sh`

- [ ] **Step 1: Create repository ignore rules**

Write `.gitignore`:

```gitignore
/target/
/dist/
/.superpowers/
*.log
.DS_Store
```

- [ ] **Step 2: Create Cargo manifest**

Write `Cargo.toml`:

```toml
[package]
name = "desktop-pet"
version = "0.1.0"
edition = "2021"
description = "A small native macOS desktop pet."
license = "UNLICENSED"

[dependencies]
env_logger = "0.11"
fastrand = "2"
image = { version = "0.25", default-features = false, features = ["png"] }
log = "0.4"
pixels = "0.17.1"
winit = "0.30.13"

[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-app-kit = { version = "0.3", features = [
  "NSApplication",
  "NSMenu",
  "NSMenuItem",
  "NSResponder",
  "NSStatusBar",
  "NSStatusItem",
  "NSView",
  "NSWindow",
  "objc2-core-foundation",
] }
objc2-foundation = "0.3"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
panic = "abort"
```

- [ ] **Step 3: Create module declarations**

Write `src/lib.rs`:

```rust
pub mod app;
pub mod bundle;
pub mod menu_bar;
pub mod pet;
pub mod physics;
pub mod renderer;
pub mod sprite;
pub mod window_macos;
```

- [ ] **Step 4: Create minimal entrypoint**

Write `src/main.rs`:

```rust
use desktop_pet::app::DesktopPetApp;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = DesktopPetApp::new();
    event_loop.run_app(&mut app)?;

    Ok(())
}
```

- [ ] **Step 5: Create compiling stubs**

Write `src/app.rs`:

```rust
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

#[derive(Debug, Default)]
pub struct DesktopPetApp;

impl DesktopPetApp {
    pub fn new() -> Self {
        Self
    }
}

impl ApplicationHandler for DesktopPetApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}
```

Write these empty module files:

```rust
// src/bundle.rs
```

```rust
// src/menu_bar.rs
```

```rust
// src/pet.rs
```

```rust
// src/physics.rs
```

```rust
// src/renderer.rs
```

```rust
// src/sprite.rs
```

```rust
// src/window_macos.rs
```

- [ ] **Step 6: Add verification script**

Write `scripts/verify.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

Run:

```bash
chmod +x scripts/verify.sh
```

- [ ] **Step 7: Run scaffold verification**

Run:

```bash
cargo fmt
cargo check
```

Expected: `cargo check` exits 0.

- [ ] **Step 8: Commit scaffold**

Run:

```bash
git add .gitignore Cargo.toml src scripts/verify.sh
git commit -m "chore: scaffold desktop pet crate"
```

---

### Task 2: Physics Core

**Files:**
- Modify: `src/physics.rs`

- [ ] **Step 1: Write failing physics tests**

Replace `src/physics.rs` with:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Physics {
    pub position: Vec2,
    pub velocity: Vec2,
    pub size: Vec2,
    pub bounds: Bounds,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn physics() -> Physics {
        Physics {
            position: Vec2 { x: 10.0, y: 20.0 },
            velocity: Vec2 { x: 40.0, y: -10.0 },
            size: Vec2 { x: 64.0, y: 64.0 },
            bounds: Bounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 200.0,
                max_y: 200.0,
            },
        }
    }

    #[test]
    fn update_moves_position_by_velocity_times_delta() {
        let mut physics = physics();
        physics.update(0.5);
        assert_eq!(physics.position, Vec2 { x: 30.0, y: 15.0 });
    }

    #[test]
    fn clamp_keeps_pet_inside_bounds_using_sprite_size() {
        let mut physics = physics();
        physics.position = Vec2 { x: 190.0, y: -20.0 };
        physics.clamp_to_bounds();
        assert_eq!(physics.position, Vec2 { x: 136.0, y: 0.0 });
    }

    #[test]
    fn update_bounces_velocity_when_hitting_horizontal_edge() {
        let mut physics = physics();
        physics.position = Vec2 { x: 135.0, y: 20.0 };
        physics.velocity = Vec2 { x: 40.0, y: 0.0 };
        physics.update(1.0);
        assert_eq!(physics.position.x, 136.0);
        assert_eq!(physics.velocity.x, -40.0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test physics -- --nocapture
```

Expected: FAIL with missing methods `update` and `clamp_to_bounds`.

- [ ] **Step 3: Implement physics methods**

Add this implementation above the test module in `src/physics.rs`:

```rust
impl Physics {
    pub fn update(&mut self, dt_seconds: f32) {
        self.position.x += self.velocity.x * dt_seconds;
        self.position.y += self.velocity.y * dt_seconds;

        let hit_x = self.position.x < self.bounds.min_x
            || self.position.x > self.bounds.max_x - self.size.x;
        let hit_y = self.position.y < self.bounds.min_y
            || self.position.y > self.bounds.max_y - self.size.y;

        self.clamp_to_bounds();

        if hit_x {
            self.velocity.x = -self.velocity.x;
        }
        if hit_y {
            self.velocity.y = -self.velocity.y;
        }
    }

    pub fn clamp_to_bounds(&mut self) {
        let max_x = (self.bounds.max_x - self.size.x).max(self.bounds.min_x);
        let max_y = (self.bounds.max_y - self.size.y).max(self.bounds.min_y);

        self.position.x = self.position.x.clamp(self.bounds.min_x, max_x);
        self.position.y = self.position.y.clamp(self.bounds.min_y, max_y);
    }
}
```

- [ ] **Step 4: Run physics tests**

Run:

```bash
cargo test physics -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit physics core**

Run:

```bash
git add src/physics.rs
git commit -m "feat: add pet physics core"
```

---

### Task 3: Pet State Machine

**Files:**
- Modify: `src/pet.rs`

- [ ] **Step 1: Write failing pet behavior tests**

Replace `src/pet.rs` with:

```rust
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetState {
    Idle,
    Walk,
    Sleep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PetTick {
    pub state: PetState,
    pub frame_index: usize,
    pub speed_x: f32,
}

#[derive(Debug)]
pub struct Pet {
    state: PetState,
    direction: Direction,
    frame_index: usize,
    frame_elapsed: Duration,
    state_elapsed: Duration,
    walk_distance_remaining: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_idle_on_frame_zero() {
        let pet = Pet::new();
        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(pet.frame_index(), 0);
    }

    #[test]
    fn idle_animation_advances_every_200ms() {
        let mut pet = Pet::new();
        let tick = pet.tick(Duration::from_millis(200));
        assert_eq!(tick.frame_index, 1);
        assert_eq!(tick.state, PetState::Idle);
    }

    #[test]
    fn idle_transitions_to_walk_after_threshold() {
        let mut pet = Pet::new_with_seed(1);
        pet.tick(Duration::from_secs(5));
        assert_eq!(pet.state(), PetState::Walk);
    }

    #[test]
    fn sleep_uses_slow_animation_rate() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Sleep);
        pet.tick(Duration::from_millis(499));
        assert_eq!(pet.frame_index(), 0);
        pet.tick(Duration::from_millis(1));
        assert_eq!(pet.frame_index(), 1);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test pet -- --nocapture
```

Expected: FAIL with missing `Pet` methods.

- [ ] **Step 3: Implement deterministic pet state machine**

Add this implementation above the test module in `src/pet.rs`:

```rust
const FRAME_COUNT: usize = 4;
const IDLE_FRAME_MS: u64 = 200;
const WALK_FRAME_MS: u64 = 100;
const SLEEP_FRAME_MS: u64 = 500;
const WALK_SPEED: f32 = 45.0;

impl Pet {
    pub fn new() -> Self {
        Self::new_with_seed(0)
    }

    pub fn new_with_seed(seed: u64) -> Self {
        let direction = if seed % 2 == 0 {
            Direction::Right
        } else {
            Direction::Left
        };

        Self {
            state: PetState::Idle,
            direction,
            frame_index: 0,
            frame_elapsed: Duration::ZERO,
            state_elapsed: Duration::ZERO,
            walk_distance_remaining: 0.0,
        }
    }

    pub fn state(&self) -> PetState {
        self.state
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    pub fn frame_index(&self) -> usize {
        self.frame_index
    }

    pub fn tick(&mut self, dt: Duration) -> PetTick {
        self.state_elapsed += dt;
        self.frame_elapsed += dt;

        self.advance_animation();
        self.advance_state(dt);

        PetTick {
            state: self.state,
            frame_index: self.frame_index,
            speed_x: self.speed_x(),
        }
    }

    fn advance_animation(&mut self) {
        let frame_duration = self.frame_duration();
        while self.frame_elapsed >= frame_duration {
            self.frame_elapsed -= frame_duration;
            self.frame_index = (self.frame_index + 1) % FRAME_COUNT;
        }
    }

    fn advance_state(&mut self, dt: Duration) {
        match self.state {
            PetState::Idle if self.state_elapsed >= Duration::from_secs(5) => {
                self.enter_walk();
            }
            PetState::Walk => {
                self.walk_distance_remaining -= WALK_SPEED * dt.as_secs_f32();
                if self.walk_distance_remaining <= 0.0 {
                    self.enter_idle();
                }
            }
            PetState::Sleep if self.state_elapsed >= Duration::from_secs(12) => {
                self.enter_idle();
            }
            _ => {}
        }
    }

    fn enter_idle(&mut self) {
        self.state = PetState::Idle;
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
        self.state_elapsed = Duration::ZERO;
        self.walk_distance_remaining = 0.0;
    }

    fn enter_walk(&mut self) {
        self.state = PetState::Walk;
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
        self.state_elapsed = Duration::ZERO;
        self.walk_distance_remaining = 120.0;
    }

    fn frame_duration(&self) -> Duration {
        match self.state {
            PetState::Idle => Duration::from_millis(IDLE_FRAME_MS),
            PetState::Walk => Duration::from_millis(WALK_FRAME_MS),
            PetState::Sleep => Duration::from_millis(SLEEP_FRAME_MS),
        }
    }

    fn speed_x(&self) -> f32 {
        if self.state != PetState::Walk {
            return 0.0;
        }

        match self.direction {
            Direction::Left => -WALK_SPEED,
            Direction::Right => WALK_SPEED,
        }
    }

    #[cfg(test)]
    fn force_state_for_test(&mut self, state: PetState) {
        self.state = state;
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
        self.state_elapsed = Duration::ZERO;
    }
}
```

- [ ] **Step 4: Run pet tests**

Run:

```bash
cargo test pet -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit pet state machine**

Run:

```bash
git add src/pet.rs
git commit -m "feat: add pet state machine"
```

---

### Task 4: Sprite Sheet Loading and Validation

**Files:**
- Modify: `src/sprite.rs`

- [ ] **Step 1: Write failing sprite tests**

Replace `src/sprite.rs` with:

```rust
use std::path::Path;

use image::RgbaImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpriteRow {
    Idle,
    WalkRight,
    Sleep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub enum SpriteError {
    Image(image::ImageError),
    InvalidDimensions { width: u32, height: u32 },
}

impl From<image::ImageError> for SpriteError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
    }
}

#[derive(Debug, Clone)]
pub struct SpriteSheet {
    image: RgbaImage,
    frame_size: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 0]))
    }

    #[test]
    fn accepts_three_rows_and_four_columns() {
        let sheet = SpriteSheet::from_image(sheet(256, 192), 64).unwrap();
        assert_eq!(sheet.frame_count(), 4);
        assert_eq!(sheet.row_count(), 3);
    }

    #[test]
    fn rejects_dimensions_that_do_not_match_grid() {
        let err = SpriteSheet::from_image(sheet(250, 192), 64).unwrap_err();
        assert!(matches!(err, SpriteError::InvalidDimensions { .. }));
    }

    #[test]
    fn returns_frame_rect_for_state_row_and_index() {
        let sheet = SpriteSheet::from_image(sheet(256, 192), 64).unwrap();
        let rect = sheet.frame_rect(SpriteRow::WalkRight, 2);
        assert_eq!(
            rect,
            FrameRect {
                x: 128,
                y: 64,
                width: 64,
                height: 64
            }
        );
    }
}
```

- [ ] **Step 2: Run sprite tests to verify they fail**

Run:

```bash
cargo test sprite -- --nocapture
```

Expected: FAIL with missing `SpriteSheet` methods.

- [ ] **Step 3: Implement sprite loader**

Add this implementation above the test module in `src/sprite.rs`:

```rust
const EXPECTED_COLUMNS: u32 = 4;
const EXPECTED_ROWS: u32 = 3;

impl SpriteSheet {
    pub fn load(path: impl AsRef<Path>, frame_size: u32) -> Result<Self, SpriteError> {
        let image = image::open(path)?.into_rgba8();
        Self::from_image(image, frame_size)
    }

    pub fn from_image(image: RgbaImage, frame_size: u32) -> Result<Self, SpriteError> {
        let width = image.width();
        let height = image.height();
        let expected_width = frame_size * EXPECTED_COLUMNS;
        let expected_height = frame_size * EXPECTED_ROWS;

        if width != expected_width || height != expected_height {
            return Err(SpriteError::InvalidDimensions { width, height });
        }

        Ok(Self { image, frame_size })
    }

    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    pub fn frame_size(&self) -> u32 {
        self.frame_size
    }

    pub fn frame_count(&self) -> u32 {
        EXPECTED_COLUMNS
    }

    pub fn row_count(&self) -> u32 {
        EXPECTED_ROWS
    }

    pub fn frame_rect(&self, row: SpriteRow, frame_index: usize) -> FrameRect {
        let row_index = match row {
            SpriteRow::Idle => 0,
            SpriteRow::WalkRight => 1,
            SpriteRow::Sleep => 2,
        };
        let column = (frame_index as u32) % EXPECTED_COLUMNS;

        FrameRect {
            x: column * self.frame_size,
            y: row_index * self.frame_size,
            width: self.frame_size,
            height: self.frame_size,
        }
    }
}
```

- [ ] **Step 4: Run sprite tests**

Run:

```bash
cargo test sprite -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit sprite loader**

Run:

```bash
git add src/sprite.rs
git commit -m "feat: validate pet sprite sheet"
```

---

### Task 5: Alpha Blitting Renderer Core

**Files:**
- Modify: `src/renderer.rs`

- [ ] **Step 1: Write failing alpha-blit tests**

Replace `src/renderer.rs` with:

```rust
use image::RgbaImage;

use crate::sprite::FrameRect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlitOptions {
    pub dest_x: i32,
    pub dest_y: i32,
    pub flip_x: bool,
}

pub fn clear_rgba(frame: &mut [u8]) {
    frame.fill(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_sets_every_byte_to_zero() {
        let mut frame = vec![255; 16];
        clear_rgba(&mut frame);
        assert_eq!(frame, vec![0; 16]);
    }

    #[test]
    fn blit_copies_opaque_source_pixel() {
        let source = RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 255]));
        let mut frame = vec![0; 4 * 2 * 2];
        blit_frame(
            &source,
            FrameRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            &mut frame,
            2,
            2,
            BlitOptions {
                dest_x: 1,
                dest_y: 1,
                flip_x: false,
            },
        );
        assert_eq!(&frame[12..16], &[10, 20, 30, 255]);
    }

    #[test]
    fn blit_alpha_blends_translucent_source_pixel() {
        let source = RgbaImage::from_pixel(1, 1, image::Rgba([100, 0, 0, 128]));
        let mut frame = vec![0; 4];
        frame.copy_from_slice(&[0, 0, 100, 255]);
        blit_frame(
            &source,
            FrameRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            &mut frame,
            1,
            1,
            BlitOptions {
                dest_x: 0,
                dest_y: 0,
                flip_x: false,
            },
        );
        assert_eq!(frame[3], 255);
        assert!(frame[0] >= 49 && frame[0] <= 51);
        assert!(frame[2] >= 49 && frame[2] <= 51);
    }
}
```

- [ ] **Step 2: Run renderer tests to verify they fail**

Run:

```bash
cargo test renderer -- --nocapture
```

Expected: FAIL with missing `blit_frame`.

- [ ] **Step 3: Implement alpha blit**

Add this implementation above the test module in `src/renderer.rs`:

```rust
pub fn blit_frame(
    source: &RgbaImage,
    rect: FrameRect,
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    options: BlitOptions,
) {
    for local_y in 0..rect.height {
        let dest_y = options.dest_y + local_y as i32;
        if dest_y < 0 || dest_y >= frame_height as i32 {
            continue;
        }

        for local_x in 0..rect.width {
            let dest_x = options.dest_x + local_x as i32;
            if dest_x < 0 || dest_x >= frame_width as i32 {
                continue;
            }

            let source_x = if options.flip_x {
                rect.x + rect.width - 1 - local_x
            } else {
                rect.x + local_x
            };
            let source_y = rect.y + local_y;
            let src = source.get_pixel(source_x, source_y).0;
            if src[3] == 0 {
                continue;
            }

            let offset = ((dest_y as u32 * frame_width + dest_x as u32) * 4) as usize;
            alpha_blend_pixel(src, &mut frame[offset..offset + 4]);
        }
    }
}

fn alpha_blend_pixel(src: [u8; 4], dst: &mut [u8]) {
    let src_alpha = src[3] as f32 / 255.0;
    let dst_alpha = dst[3] as f32 / 255.0;
    let out_alpha = src_alpha + dst_alpha * (1.0 - src_alpha);

    if out_alpha <= f32::EPSILON {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }

    for channel in 0..3 {
        let src_channel = src[channel] as f32 / 255.0;
        let dst_channel = dst[channel] as f32 / 255.0;
        let out = (src_channel * src_alpha
            + dst_channel * dst_alpha * (1.0 - src_alpha))
            / out_alpha;
        dst[channel] = (out * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    dst[3] = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}
```

- [ ] **Step 4: Run renderer tests**

Run:

```bash
cargo test renderer -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit renderer core**

Run:

```bash
git add src/renderer.rs
git commit -m "feat: add alpha blitting renderer core"
```

---

### Task 6: Bundle Resource Lookup and App Packaging

**Files:**
- Modify: `src/bundle.rs`
- Create: `packaging/Info.plist`
- Create: `scripts/build_app.sh`

- [ ] **Step 1: Write failing bundle path tests**

Replace `src/bundle.rs` with:

```rust
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "DesktopPet";
pub const SPRITE_FILE_NAME: &str = "pet_spritesheet.png";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePaths {
    pub sprite_sheet: PathBuf,
}

pub fn resource_paths_from_executable(executable_path: &Path) -> ResourcePaths {
    let resources = resources_dir_from_executable(executable_path);
    ResourcePaths {
        sprite_sheet: resources.join(SPRITE_FILE_NAME),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_binary_resolves_to_contents_resources() {
        let executable =
            Path::new("/Applications/DesktopPet.app/Contents/MacOS/desktop-pet");
        let paths = resource_paths_from_executable(executable);
        assert_eq!(
            paths.sprite_sheet,
            PathBuf::from("/Applications/DesktopPet.app/Contents/Resources/pet_spritesheet.png")
        );
    }

    #[test]
    fn development_binary_resolves_to_assets_directory() {
        let executable = Path::new("/repo/target/debug/desktop-pet");
        let paths = resource_paths_from_executable(executable);
        assert_eq!(paths.sprite_sheet, PathBuf::from("assets/pet_spritesheet.png"));
    }
}
```

- [ ] **Step 2: Run bundle tests to verify they fail**

Run:

```bash
cargo test bundle -- --nocapture
```

Expected: FAIL with missing `resources_dir_from_executable`.

- [ ] **Step 3: Implement resource path lookup**

Add this implementation above the test module in `src/bundle.rs`:

```rust
pub fn current_resource_paths() -> std::io::Result<ResourcePaths> {
    let executable = std::env::current_exe()?;
    Ok(resource_paths_from_executable(&executable))
}

fn resources_dir_from_executable(executable_path: &Path) -> PathBuf {
    let components: Vec<_> = executable_path.components().collect();
    let has_app_bundle = components
        .iter()
        .any(|component| component.as_os_str().to_string_lossy().ends_with(".app"));

    if has_app_bundle {
        if let Some(macos_dir) = executable_path.parent() {
            if let Some(contents_dir) = macos_dir.parent() {
                return contents_dir.join("Resources");
            }
        }
    }

    PathBuf::from("assets")
}
```

- [ ] **Step 4: Add Info.plist**

Write `packaging/Info.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>desktop-pet</string>
  <key>CFBundleIdentifier</key>
  <string>dev.tattran.desktop-pet</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>DesktopPet</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
```

- [ ] **Step 5: Add app bundle build script**

Write `scripts/build_app.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT_DIR/dist/DesktopPet.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

cargo build --release

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$ROOT_DIR/target/release/desktop-pet" "$MACOS_DIR/desktop-pet"
cp "$ROOT_DIR/packaging/Info.plist" "$CONTENTS_DIR/Info.plist"
cp "$ROOT_DIR/assets/pet_spritesheet.png" "$RESOURCES_DIR/pet_spritesheet.png"

chmod +x "$MACOS_DIR/desktop-pet"

echo "Built $APP_DIR"
```

Run:

```bash
chmod +x scripts/build_app.sh
```

- [ ] **Step 6: Run bundle tests**

Run:

```bash
cargo test bundle -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit bundle packaging files**

Run:

```bash
git add src/bundle.rs packaging/Info.plist scripts/build_app.sh
git commit -m "feat: add macOS app bundle packaging"
```

---

### Task 7: Personalized Sprite Asset

**Files:**
- Create: `assets/pet_spritesheet.png`
- Create: `assets/pet_spritesheet_source.png`

- [ ] **Step 1: Generate sprite sheet concept with imagegen**

Use built-in `image_gen` with this prompt:

```text
Use case: stylized-concept
Asset type: macOS desktop pet sprite sheet
Primary request: Create a polished pixel-art sprite sheet for a small personalized desktop companion.
Scene/backdrop: perfectly flat solid #00ff00 chroma-key background for background removal.
Subject: one small warm intelligent maker/developer buddy, friendly but not childish, readable silhouette, subtle expressive eyes, compact body, no extra characters.
Style/medium: crisp pixel art, game sprite sheet, 64x64 pixel frame feel, clean edges, limited but tasteful palette, no anti-aliased blur.
Composition/framing: 3 rows by 4 columns, equal-size frames, generous padding inside each frame. Row 1 idle frames, row 2 walking right frames, row 3 sleeping frames.
Lighting/mood: cozy, calm, premium, not distracting.
Color palette: warm neutral base with one distinctive accent color; do not use #00ff00 in the subject.
Constraints: background must be one uniform #00ff00 with no shadows, gradients, texture, floor plane, reflections, or lighting variation. No text, no watermark, no border, no cast shadow.
Avoid: multiple characters, cropped frames, inconsistent frame size, noisy background, realistic fur, photorealism.
```

Save the generated source image as `assets/pet_spritesheet_source.png`.

- [ ] **Step 2: Remove chroma key**

Run:

```bash
python "${CODEX_HOME:-$HOME/.codex}/skills/.system/imagegen/scripts/remove_chroma_key.py" \
  --input assets/pet_spritesheet_source.png \
  --out assets/pet_spritesheet.png \
  --auto-key border \
  --soft-matte \
  --transparent-threshold 12 \
  --opaque-threshold 220 \
  --despill
```

Expected: `assets/pet_spritesheet.png` exists and has alpha transparency.

- [ ] **Step 3: Validate dimensions**

Run:

```bash
python - <<'PY'
from PIL import Image
img = Image.open("assets/pet_spritesheet.png")
print(img.size, img.mode)
assert img.size == (256, 192), img.size
assert img.mode == "RGBA", img.mode
assert img.getpixel((0, 0))[3] == 0
PY
```

Expected: prints `(256, 192) RGBA` and exits 0.

- [ ] **Step 4: Commit sprite asset**

Run:

```bash
git add assets/pet_spritesheet.png assets/pet_spritesheet_source.png
git commit -m "feat: add personalized pet sprite sheet"
```

---

### Task 8: App Runtime Wiring

**Files:**
- Modify: `src/app.rs`
- Modify: `src/main.rs`
- Modify: `src/renderer.rs`

- [ ] **Step 1: Extend renderer with runtime frame drawing**

Append this to `src/renderer.rs`:

```rust
use pixels::{Pixels, SurfaceTexture};
use winit::window::Window;

use crate::sprite::SpriteSheet;

pub struct PetRenderer<'window> {
    pixels: Pixels<'window>,
    surface_width: u32,
    surface_height: u32,
}

impl<'window> PetRenderer<'window> {
    pub fn new(window: &'window Window, width: u32, height: u32) -> Result<Self, pixels::Error> {
        let surface_texture = SurfaceTexture::new(width, height, window);
        let pixels = Pixels::new(width, height, surface_texture)?;
        Ok(Self {
            pixels,
            surface_width: width,
            surface_height: height,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), pixels::Error> {
        self.surface_width = width;
        self.surface_height = height;
        self.pixels.resize_surface(width, height)?;
        self.pixels.resize_buffer(width, height)?;
        Ok(())
    }

    pub fn draw(
        &mut self,
        sprite_sheet: &SpriteSheet,
        rect: crate::sprite::FrameRect,
        flip_x: bool,
    ) -> Result<(), pixels::Error> {
        let frame = self.pixels.frame_mut();
        clear_rgba(frame);
        blit_frame(
            sprite_sheet.image(),
            rect,
            frame,
            self.surface_width,
            self.surface_height,
            BlitOptions {
                dest_x: 0,
                dest_y: 0,
                flip_x,
            },
        );
        self.pixels.render()
    }
}
```

- [ ] **Step 2: Implement app runtime state**

Replace `src/app.rs` with:

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{error, warn};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::bundle::current_resource_paths;
use crate::menu_bar::MenuBarController;
use crate::pet::{Direction, Pet, PetState};
use crate::physics::{Bounds, Physics, Vec2};
use crate::renderer::PetRenderer;
use crate::sprite::{SpriteRow, SpriteSheet};
use crate::window_macos::apply_desktop_pet_window_behavior;

const FRAME_SIZE: u32 = 64;
const WINDOW_SCALE: u32 = 2;
const WINDOW_SIZE: u32 = FRAME_SIZE * WINDOW_SCALE;

pub struct DesktopPetApp {
    window: Option<Arc<Window>>,
    renderer: Option<PetRenderer<'static>>,
    sprite_sheet: Option<SpriteSheet>,
    pet: Pet,
    physics: Physics,
    last_tick: Instant,
    menu_bar: Option<MenuBarController>,
}

impl DesktopPetApp {
    pub fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            sprite_sheet: None,
            pet: Pet::new_with_seed(fastrand::u64(..)),
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
                    max_x: 800.0,
                    max_y: 600.0,
                },
            },
            last_tick: Instant::now(),
            menu_bar: None,
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = WindowAttributes::default()
            .with_title("DesktopPet")
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_inner_size(LogicalSize::new(WINDOW_SIZE as f64, WINDOW_SIZE as f64));

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                error!("failed to create window: {err}");
                event_loop.exit();
                return;
            }
        };

        if let Err(err) = apply_desktop_pet_window_behavior(&window) {
            warn!("failed to apply macOS window behavior: {err}");
        }

        self.update_bounds_from_window(&window);
        window.set_outer_position(PhysicalPosition::new(
            self.physics.position.x as i32,
            self.physics.position.y as i32,
        ));

        let renderer_window: &'static Window = unsafe { std::mem::transmute(window.as_ref()) };
        let renderer = match PetRenderer::new(renderer_window, WINDOW_SIZE, WINDOW_SIZE) {
            Ok(renderer) => renderer,
            Err(err) => {
                error!("failed to initialize renderer: {err}");
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window);
        self.renderer = Some(renderer);
    }

    fn load_assets(&mut self, event_loop: &ActiveEventLoop) {
        let paths = match current_resource_paths() {
            Ok(paths) => paths,
            Err(err) => {
                error!("failed to resolve resources: {err}");
                event_loop.exit();
                return;
            }
        };

        match SpriteSheet::load(&paths.sprite_sheet, FRAME_SIZE) {
            Ok(sprite_sheet) => self.sprite_sheet = Some(sprite_sheet),
            Err(err) => {
                error!("failed to load sprite sheet {}: {err:?}", paths.sprite_sheet.display());
                event_loop.exit();
            }
        }
    }

    fn update_bounds_from_window(&mut self, window: &Window) {
        if let Some(monitor) = window.current_monitor().or_else(|| window.primary_monitor()) {
            let position = monitor.position();
            let size = monitor.size();
            self.physics.bounds = Bounds {
                min_x: position.x as f32,
                min_y: position.y as f32,
                max_x: (position.x + size.width as i32) as f32,
                max_y: (position.y + size.height as i32) as f32,
            };
            self.physics.clamp_to_bounds();
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick).min(Duration::from_millis(100));
        self.last_tick = now;

        let pet_tick = self.pet.tick(dt);
        self.physics.velocity.x = pet_tick.speed_x;
        self.physics.update(dt.as_secs_f32());

        if let Some(window) = &self.window {
            window.set_outer_position(PhysicalPosition::new(
                self.physics.position.x as i32,
                self.physics.position.y as i32,
            ));
            window.request_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let Some(sprite_sheet) = self.sprite_sheet.as_ref() else {
            return;
        };

        let row = match self.pet.state() {
            PetState::Idle => SpriteRow::Idle,
            PetState::Walk => SpriteRow::WalkRight,
            PetState::Sleep => SpriteRow::Sleep,
        };
        let flip_x = self.pet.state() == PetState::Walk && self.pet.direction() == Direction::Left;
        let rect = sprite_sheet.frame_rect(row, self.pet.frame_index());

        if let Err(err) = renderer.draw(sprite_sheet, rect, flip_x) {
            error!("render failed: {err}");
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
        if self.sprite_sheet.is_none() {
            self.load_assets(event_loop);
        }
        if self.window.is_none() {
            self.create_window(event_loop);
        }
        if self.menu_bar.is_none() {
            self.menu_bar = MenuBarController::new();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.tick();
        event_loop.set_control_flow(ControlFlow::wait_duration(Duration::from_millis(16)));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(err) = renderer.resize(size.width.max(1), size.height.max(1)) {
                        error!("resize failed: {err}");
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = self.window.clone() {
                    self.update_bounds_from_window(&window);
                }
            }
            _ => {}
        }
    }
}
```

- [ ] **Step 3: Run compile check**

Run:

```bash
cargo check
```

Expected: FAIL until Task 9 adds macOS behavior modules; PASS after Task 9.

- [ ] **Step 4: Keep runtime changes staged for Task 9 integration**

Run:

```bash
git diff -- src/app.rs src/renderer.rs src/main.rs
```

Expected: diff includes runtime wiring only.

---

### Task 9: macOS Window Behavior and Menu Bar

**Files:**
- Modify: `src/window_macos.rs`
- Modify: `src/menu_bar.rs`

- [ ] **Step 1: Implement cross-platform compile fallbacks**

Replace `src/window_macos.rs` with:

```rust
use std::fmt;

use winit::window::{Window, WindowLevel};

#[derive(Debug)]
pub struct WindowTweaksError {
    message: String,
}

impl WindowTweaksError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for WindowTweaksError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for WindowTweaksError {}

pub fn apply_desktop_pet_window_behavior(window: &Window) -> Result<(), WindowTweaksError> {
    window.set_window_level(WindowLevel::AlwaysOnTop);
    window
        .set_cursor_hittest(false)
        .map_err(|err| WindowTweaksError::new(format!("cursor hittest failed: {err}")))?;

    apply_platform_window_behavior(window)
}

#[cfg(not(target_os = "macos"))]
fn apply_platform_window_behavior(_window: &Window) -> Result<(), WindowTweaksError> {
    Ok(())
}
```

Replace `src/menu_bar.rs` with:

```rust
#[derive(Debug)]
pub struct MenuBarController;

impl MenuBarController {
    #[cfg(not(target_os = "macos"))]
    pub fn new() -> Option<Self> {
        None
    }

    #[cfg(target_os = "macos")]
    pub fn new() -> Option<Self> {
        macos::create_menu_bar()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::MenuBarController;

    pub fn create_menu_bar() -> Option<MenuBarController> {
        Some(MenuBarController)
    }
}
```

- [ ] **Step 2: Run compile check**

Run:

```bash
cargo check
```

Expected: PASS on non-macOS, or PASS on macOS before AppKit internals are added.

- [ ] **Step 3: Add macOS AppKit window behavior**

Append this macOS implementation to `src/window_macos.rs`:

```rust
#[cfg(target_os = "macos")]
fn apply_platform_window_behavior(window: &Window) -> Result<(), WindowTweaksError> {
    use objc2::rc::Retained;
    use objc2_app_kit::{NSView, NSWindowCollectionBehavior};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::platform::macos::WindowExtMacOS;

    window.set_has_shadow(false);

    let handle = window
        .window_handle()
        .map_err(|err| WindowTweaksError::new(format!("window handle unavailable: {err}")))?;

    let RawWindowHandle::AppKit(appkit_handle) = handle.as_raw() else {
        return Err(WindowTweaksError::new("window is not an AppKit window"));
    };

    let ns_view = appkit_handle.ns_view.as_ptr();
    let ns_view: Retained<NSView> = unsafe { Retained::retain(ns_view.cast()) }
        .ok_or_else(|| WindowTweaksError::new("failed to retain NSView"))?;
    let ns_window = ns_view
        .window()
        .ok_or_else(|| WindowTweaksError::new("NSView has no NSWindow"))?;

    ns_window.setHasShadow(false);
    ns_window.setIgnoresMouseEvents(true);
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary,
    );

    Ok(())
}
```

- [ ] **Step 4: Add macOS menu bar controller**

Replace `src/menu_bar.rs` with:

```rust
#[cfg(not(target_os = "macos"))]
#[derive(Debug)]
pub struct MenuBarController;

#[cfg(not(target_os = "macos"))]
impl MenuBarController {
    pub fn new() -> Option<Self> {
        None
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct MenuBarController {
    _status_item: objc2::rc::Retained<objc2_app_kit::NSStatusItem>,
    _menu: objc2::rc::Retained<objc2_app_kit::NSMenu>,
}

#[cfg(target_os = "macos")]
impl MenuBarController {
    pub fn new() -> Option<Self> {
        use objc2::{sel, MainThreadMarker};
        use objc2_app_kit::{
            NSApplication, NSMenu, NSMenuItem, NSStatusBar, NSVariableStatusItemLength,
        };
        use objc2_foundation::NSString;

        let mtm = MainThreadMarker::new()?;
        let status_bar = NSStatusBar::systemStatusBar();
        let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);
        status_item.setTitle(Some(&NSString::from_str("DP")));

        let menu = NSMenu::initWithTitle(mtm.alloc(), &NSString::from_str("DesktopPet"));
        let quit_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str("Quit DesktopPet"),
                Some(sel!(terminate:)),
                &NSString::from_str("q"),
            )
        };

        let app = NSApplication::sharedApplication(mtm);
        unsafe {
            quit_item.setTarget(Some(app.as_ref()));
        }
        menu.addItem(&quit_item);
        status_item.setMenu(Some(&menu));

        Some(Self {
            _status_item: status_item,
            _menu: menu,
        })
    }
}
```

Run:

```bash
cargo check
```

Expected: PASS.

- [ ] **Step 5: Commit runtime and macOS integration**

Run:

```bash
git add src/app.rs src/renderer.rs src/window_macos.rs src/menu_bar.rs src/main.rs
git commit -m "feat: wire desktop pet runtime"
```

---

### Task 10: Build, Bundle, and Verify

**Files:**
- Modify: `scripts/verify.sh`
- Read: `docs/superpowers/specs/2026-05-25-desktop-pet-design.md`

- [ ] **Step 1: Extend verification script with bundle build**

Replace `scripts/verify.sh` with:

```bash
#!/usr/bin/env bash
set -euo pipefail

cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
./scripts/build_app.sh

test -x dist/DesktopPet.app/Contents/MacOS/desktop-pet
test -f dist/DesktopPet.app/Contents/Info.plist
test -f dist/DesktopPet.app/Contents/Resources/pet_spritesheet.png
```

- [ ] **Step 2: Run full automated verification**

Run:

```bash
./scripts/verify.sh
```

Expected: PASS.

- [ ] **Step 3: Inspect bundle metadata**

Run:

```bash
/usr/libexec/PlistBuddy -c "Print :LSUIElement" dist/DesktopPet.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" dist/DesktopPet.app/Contents/Info.plist
```

Expected output:

```text
true
dev.tattran.desktop-pet
```

- [ ] **Step 4: Manual smoke launch**

Run:

```bash
open dist/DesktopPet.app
```

Manual checks:

- No Dock icon appears.
- App does not appear in Cmd+Tab.
- Pet appears in a transparent borderless window.
- Clicks on the pet area reach the app underneath.
- Pet remains visible when switching Spaces.
- Pet appears with at least one fullscreen app where macOS permits auxiliary windows.
- Menu bar `Quit` exits the app.

- [ ] **Step 5: Measure idle resources**

Run:

```bash
ps -axo pid,comm,%cpu,rss | rg 'desktop-pet|DesktopPet'
```

Expected: CPU trends near idle after animation settles. RSS is recorded in the final report; do not block V1 on the original 30 MB target until measured.

- [ ] **Step 6: Commit verification script**

Run:

```bash
git add scripts/verify.sh
git commit -m "chore: verify desktop pet bundle"
```

---

## Self-Review

Spec coverage:

- Personalized single pet: Task 7.
- `idle`, `walk`, `sleep`: Task 3.
- Transparent, borderless, always-on-top, click-through window: Tasks 8 and 9.
- Hidden Dock/Cmd+Tab with `LSUIElement=true`: Task 6 and Task 10.
- Cross-Space/fullscreen auxiliary behavior: Task 9 and Task 10 manual smoke.
- Menu bar Quit: Task 9 and Task 10.
- Primary-display bounds handling: Tasks 2 and 8.
- Sprite validation and alpha output: Tasks 4 and 7.
- Performance measurement: Task 10.

The plan intentionally leaves full multi-monitor roaming, settings UI, auto-update, multi-pet, drag/feed, and persistent mood outside V1.
