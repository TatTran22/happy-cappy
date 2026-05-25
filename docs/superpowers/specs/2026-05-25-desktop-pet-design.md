# Desktop Pet macOS V1 Design

Date: 2026-05-25

## Context

`/Users/tattran/Projects/desktop-pet` is currently an empty project workspace. The starting technical brief is `/Users/tattran/Downloads/desktop-pet-spec.md`.

This design keeps the original product direction: a native macOS desktop pet that floats above work windows, stays out of the user's way, and uses a personalized pixel-art character. The important change is that V1 is an `.app` bundle from the beginning, because the core behavior depends on macOS bundle metadata and AppKit window behavior.

## V1 Scope

V1 ships a minimal native macOS app bundle with:

- One personalized pixel-art pet.
- Three states: `idle`, `walk`, and `sleep`.
- Transparent, borderless window.
- Always-on-top display.
- Click-through interaction so mouse input reaches apps underneath.
- Hidden Dock and Cmd+Tab presence through `LSUIElement=true`.
- Cross-Space/fullscreen auxiliary window behavior.
- Minimal menu bar status item with `Quit`.
- Basic primary-display bounds handling and response to resolution/scale changes.

V1 does not include settings UI, auto-update, multi-pet, drag/feed interactions, persistent mood, or full multi-monitor roaming.

## Reviewed Technical Baseline

The original spec listed dependency versions that are now stale. Use the current stable versions checked during design review:

- `winit = "0.30.13"` for window creation, event loop, transparent window, window level, and cursor hit-test control.
- `pixels = "0.17.1"` for GPU-backed pixel framebuffer rendering through `wgpu` and Metal.
- `image = "0.25"` with PNG support for sprite sheet decoding.
- `fastrand = "2"` for lightweight random state transitions.
- `env_logger = "0.11"` and `log = "0.4"` for development logging.
- `objc2 = "0.6"` and `objc2-app-kit = "0.3"` for macOS behavior not fully covered by `winit`.

`winit` should be used first where it provides stable APIs: `Window::set_window_level(WindowLevel::AlwaysOnTop)` and `Window::set_cursor_hittest(false)`. AppKit interop stays small and focused on `NSApplication` activation policy fallback, `NSWindow.collectionBehavior`, status item, and bundle-specific behavior.

## Architecture

The application is a small Rust native app:

```text
DesktopPet.app
  Info.plist                 # LSUIElement=true, app metadata
  Contents/MacOS/desktop-pet # Rust binary
  Contents/Resources/        # sprite sheet and optional app icon
```

Source modules:

- `main.rs`: initialize logging, create `EventLoop`, run the app.
- `app.rs`: implement `winit::application::ApplicationHandler`, own runtime state, window, renderer, pet model, and timers.
- `window_macos.rs`: macOS-only window and app tweaks behind `cfg(target_os = "macos")`.
- `menu_bar.rs`: macOS status item with a single `Quit` action.
- `pet.rs`: state machine, animation state, transition timing.
- `physics.rs`: position, velocity, bounds clamping, edge bounce.
- `sprite.rs`: sprite sheet loading, validation, frame lookup, horizontal flip for `walk-left`.
- `renderer.rs`: `pixels` integration, transparent clear, alpha blit, present.
- `bundle.rs`: resolve resource paths in app bundle and development mode.

## Runtime Flow

1. App launches from `DesktopPet.app`.
2. `bundle.rs` locates `Resources/pet_spritesheet.png`.
3. `sprite.rs` validates grid size and frame dimensions.
4. `winit` creates a borderless transparent window sized for the pet.
5. `window_macos.rs` applies always-on-top, click-through, no shadow, cross-Space, and fullscreen auxiliary behavior.
6. `menu_bar.rs` creates a status item with `Quit`.
7. `app.rs` ticks pet behavior using `ControlFlow::WaitUntil` instead of spin-waiting.
8. `pet.rs` updates state, animation frame, and movement intent.
9. `physics.rs` applies velocity and clamps to current display bounds.
10. `renderer.rs` clears the transparent buffer, alpha-blits the current frame, and calls `pixels.render()`.

The app requests redraws only when animation or position changes. Sleep and idle states use lower animation rates to keep idle CPU low.

## Window Behavior

The app uses a layered approach:

1. Prefer `winit` APIs for cross-platform concepts available on macOS:
   - `with_decorations(false)`
   - `with_transparent(true)`
   - `set_window_level(WindowLevel::AlwaysOnTop)`
   - `set_cursor_hittest(false)`
2. Use AppKit for macOS-only behavior:
   - `NSWindow.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]`
   - remove window shadow
   - accessory activation policy fallback if bundle metadata is insufficient in development mode
3. Use bundle metadata for final app identity:
   - `LSUIElement=true` in `Info.plist` to hide from Dock and Cmd+Tab.

Development builds may still show different Dock behavior when launched with `cargo run`. The acceptance test for Dock/Cmd+Tab behavior must use the `.app` bundle.

## Pet Behavior

States:

- `idle`: low-frequency breathing/blink animation; transition to `walk` after 2-5 seconds.
- `walk`: move 30-60 px/sec for 50-200 px, then return to `idle`.
- `sleep`: entered after long idle time; low-frequency animation; wakes after 10-20 seconds.

Movement stays within the visible primary-display bounds. On bounds changes, the pet is clamped into the new safe rect.

## Personalized Asset Direction

Use the `imagegen` skill to create a custom pixel-art sprite sheet instead of a generic placeholder or free asset.

Initial asset target:

- Sprite sheet with transparent final output.
- Frame size: `64x64`.
- Rows:
  - Row 0: `idle`, 4 frames.
  - Row 1: `walk-right`, 4 frames.
  - Row 2: `sleep`, 4 frames.
- `walk-left` is produced in code by horizontal flipping `walk-right`.
- Style: small, warm, intelligent maker/developer companion; polished pixel art; readable silhouette; not childish; no text or watermark.

Generation workflow:

1. Generate a sprite sheet on a flat chroma-key background with `imagegen`.
2. Remove the key color locally to produce an alpha PNG.
3. Validate transparent corners, clean edges, consistent frame size, and stable silhouette.
4. If the generated sheet has inconsistent frame alignment, use it as concept art and create a cleaned sheet before wiring it into the app.

## Error Handling

- Missing or invalid sprite sheet: fail fast with a clear log message.
- GPU or `pixels` initialization failure: fail fast with a clear log message.
- Render surface loss: try to recreate surface on resize/scale events; exit only if recovery fails.
- AppKit tweak failure: log warning and continue when the app remains usable; manual verification decides if the build passes.
- Display bounds unavailable: fall back to current window monitor, then primary monitor, then a conservative default rect.

## Performance Targets

The original numbers remain useful goals, but they are measurement targets rather than hard MVP gates:

- Idle CPU target: under 1% when pet is idle or asleep.
- RAM target: under 30 MB if practical with `wgpu`/Metal initialization overhead.
- Startup target: under 200 ms if practical for release bundle launch.
- Binary size target: keep release artifact small, but do not block MVP on a strict 5 MB cap before measuring real `wgpu` dependency impact.
- Frame cadence: smooth 60 FPS while walking; support high-refresh displays opportunistically without burning CPU when idle.

## Verification

Automated checks:

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release`

Unit tests:

- State transitions for `idle`, `walk`, and `sleep`.
- Physics clamp and edge bounce.
- Sprite sheet grid validation and frame lookup.
- Resource path resolution in development mode.

Manual macOS smoke test:

- App launches from `.app`.
- No Dock icon and no Cmd+Tab entry.
- Pet window is transparent and borderless.
- Clicks pass through pet/window to the app underneath.
- Pet stays visible across Spaces.
- Pet appears with fullscreen apps where macOS permits auxiliary windows.
- Menu bar `Quit` exits the app.
- Activity Monitor/Instruments captures idle CPU and memory.

Asset verification:

- Sprite sheet dimensions match grid.
- Alpha channel is present.
- Transparent corners are fully transparent.
- No chroma-key fringe is visible at normal scale.
- Animation frames remain aligned and visually coherent.

## Risks

- Fullscreen and Space behavior can vary with macOS, Stage Manager, and fullscreen games. Treat this as a manual smoke requirement and document limitations.
- Generated sprite sheets may need cleanup before animation looks good.
- `wgpu` initialization may exceed the original memory and binary-size goals. Measure before optimizing.
- Development-mode launch behavior differs from bundled app behavior for Dock/Cmd+Tab.
- AppKit interop can break across crate version changes. Keep AppKit calls isolated in `window_macos.rs` and `menu_bar.rs`.

## References

- winit 0.30.13 docs: https://docs.rs/crate/winit/latest
- winit Window API: https://docs.rs/winit/latest/x86_64-apple-darwin/winit/window/struct.Window.html
- pixels 0.17.1 docs: https://docs.rs/crate/pixels/latest
- objc2 0.6.4 docs: https://docs.rs/crate/objc2/latest
- objc2-app-kit 0.3.2 docs: https://docs.rs/crate/objc2-app-kit/latest
- Apple LSUIElement: https://developer.apple.com/documentation/bundleresources/information-property-list/lsuielement
- Apple NSWindow collection behavior: https://developer.apple.com/documentation/appkit/nswindow/collectionbehavior-swift.struct
