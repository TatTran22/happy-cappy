# Happy Cappy Upgrade Design

Date: 2026-05-25

## Context

`/Users/tattran/Projects/desktop-pet` is a native Rust macOS desktop pet app. The current V1 app already has a transparent always-on-top pet window, `winit + pixels` rendering, a basic pet state machine, physics, sprite-sheet loading, bundle assembly, and a menu bar `Quit` item.

This upgrade turns the app into **Happy Cappy**, a capybara desktop companion with direct interaction, settings, persisted preferences, and richer behavior. The design intentionally keeps the existing Rust/macOS architecture and extends it with small focused modules rather than replacing the runtime with a larger UI framework.

## Goals

- Rename the app identity to **Happy Cappy**.
- Replace the current generic pet with a capybara named Happy Cappy.
- Let the user drag the pet to any valid screen position.
- Save the pet's position and settings across restarts.
- Run hover actions when the pointer is over visible pet pixels.
- Rotate default expressions when not hovered or dragged.
- Add a right-click pet menu with `Settings`, `Hide Pet`, and `Reset Position`.
- Keep the app running when the pet is hidden, with `Show Pet` available from the menu bar.
- Add app settings with three personality presets and advanced behavior controls.
- Use the `imagegen` skill for project-bound sprite assets, following the same built-in image generation plus chroma-key cleanup workflow used in V1.

## Non-Goals

- Multi-pet support.
- A large dashboard-style settings app.
- Auto-update.
- Network features.
- Sound effects.
- Complex AI behavior or persistent mood simulation.
- Guaranteed per-pixel click-through on every macOS version and display mode.

## Product Scope

Happy Cappy ships one capybara pet with three selectable personality presets:

- `Calm`: slower expression changes, subtle hover breathing, lower movement.
- `Cheerful`: balanced expression changes, happy hover bob or wave-like pose.
- `Lively`: more frequent reactions, stronger hover wiggle or bounce.

The default behavior is expression-first. When the user is not interacting with the pet, it cycles through readable expressions such as blink, happy, curious, sleepy, and idle. Movement remains available through settings, but it should not dominate the default experience.

The pet supports:

- Hover reaction while the pointer is over visible sprite pixels.
- Left-button drag from visible sprite pixels.
- Right-click context menu from visible sprite pixels.
- `Hide Pet`, which hides only the pet window and keeps the menu bar app alive.
- `Show Pet`, available from the menu bar after hiding.
- `Reset Position`, available from both settings and menus.

## Settings

Settings are available from both:

- Menu bar item.
- Pet right-click context menu.

The settings UI is a small native macOS panel. It should feel like a utility app control surface, not a marketing page or dashboard.

Controls:

- Personality segmented control: `Calm`, `Cheerful`, `Lively`.
- Scale slider.
- Movement speed slider.
- Hover intensity slider.
- Monitor behavior selector: `Current Display` and `Primary Display`.
- Start at login checkbox as a stretch setting. The UI must show this control only after the implementation has working native support.
- Buttons: `Show Pet`, `Hide Pet`, `Reset Position`, `Quit Happy Cappy`.

Start-at-login is not required for the first implementation plan. If it requires risky entitlement, signing, or packaging changes, omit the checkbox instead of shipping a disabled or misleading control. All other settings are required scope.

## Architecture

Existing modules stay in place and gain focused responsibilities:

- `app.rs`: runtime orchestration, window lifecycle, menu commands, settings window lifecycle, show/hide behavior, and config application.
- `pet.rs`: personality-aware state machine, default expression loop, hover state, drag pause, and movement intent.
- `physics.rs`: position updates, bounds clamp, and drag-end clamp.
- `sprite.rs`: larger Happy Cappy sprite-sheet validation and frame lookup by animation group.
- `renderer.rs`: unchanged rendering core, with support for the expanded frame groups.
- `menu_bar.rs`: menu bar actions for Settings, Show/Hide Pet, Reset Position, and Quit.
- `window_macos.rs`: native window behavior, event capture, best-effort transparent-area pass-through, context menu plumbing, and show/hide window behavior.
- `bundle.rs`: resource paths for the renamed bundle and sprite assets.

New modules:

- `settings.rs`: config data model, defaults, load/save, validation, migration for missing fields, and path resolution.
- `interaction.rs`: hover, drag, right-click gesture state, alpha hit-test, and interaction events emitted to `app.rs`.
- `settings_window_macos.rs`: native macOS settings panel and controls.

The module boundary is important: `interaction.rs` decides what happened, `pet.rs` decides how the pet should behave, and `app.rs` applies window/config side effects.

## Configuration Model

Persist settings under the user's application support directory for Happy Cappy.

Required fields:

- `personality`: `calm`, `cheerful`, or `lively`.
- `scale`: bounded numeric value.
- `movement_speed`: bounded numeric value.
- `hover_intensity`: bounded numeric value.
- `monitor_behavior`: `current_display` or `primary_display`.
- `pet_visible`: boolean.
- `last_position`: optional x/y plus enough display identity to restore safely.

Load behavior:

1. Load config on launch.
2. Fill missing fields from defaults.
3. Clamp out-of-range numeric values.
4. Clamp restored position into the active display bounds.
5. Save normalized config after user changes or drag-end.

If the config file is missing or corrupt, the app should log a warning, start with defaults, and overwrite only after the next explicit settings or position change.

## Interaction Design

The current app is click-through by default. This upgrade changes the model to interaction-aware hit testing:

- Visible sprite pixels receive mouse events.
- Transparent pixels should pass through to apps underneath when the macOS layer allows it.
- If stable per-pixel pass-through is not possible with the current AppKit/winit integration, fallback to receiving events within the full pet window frame and document that limitation.

Pointer behavior:

- `HoverEnter`: enter hover mode and run the personality-specific hover animation.
- `HoverLeave`: return to the default expression loop.
- `DragStart`: begin dragging only from visible sprite pixels.
- `DragMove`: move the window with the pointer; pause autonomous movement and expression transitions that conflict with dragging.
- `DragEnd`: clamp into display bounds, save position, and return to default behavior.
- `ContextMenu`: right-click visible sprite pixels to open `Settings`, `Hide Pet`, `Reset Position`.

While hidden, the pet window should not render or tick at active animation cadence. The menu bar controller remains alive.

## Pet Behavior

The pet state model expands from `Idle`, `Walk`, and `Sleep` into behavior modes and animation groups.

Behavior modes:

- `Default`: expression loop based on personality.
- `Hovered`: hover action based on personality and intensity.
- `Dragging`: held/drag animation with autonomous movement paused.
- `Walking`: optional movement if movement speed is above zero.
- `Hidden`: pet window not visible.

Animation groups:

- `idle`
- `blink`
- `happy`
- `curious`
- `sleepy`
- `hover-calm`
- `hover-cheerful`
- `hover-lively`
- `walk-right`
- `drag`

`walk-left` can continue to be generated by horizontal flipping `walk-right`.

Each personality controls:

- Expression selection weights.
- Expression transition interval.
- Hover animation group.
- Hover frame cadence.
- Movement cadence and probability.

## Asset Workflow

Use the `imagegen` skill for the Happy Cappy sprite assets, matching the V1 approach.

Default workflow:

1. Use built-in `image_gen` to generate a capybara pixel-art sprite-sheet source on a flat chroma-key background.
2. Use `#00ff00` as the chroma-key background. If a generation introduces green into the subject, discard it and regenerate with `#ff00ff`.
3. Keep the subject fully separated from the background with no shadow, reflection, watermark, or text.
4. Copy the selected generated image into the workspace as a source artifact.
5. Run the installed chroma-key removal helper from the imagegen skill to produce an alpha PNG.
6. Validate alpha channel, transparent corners, frame dimensions, silhouette alignment, and lack of key-color fringe.
7. Save the final project-bound asset under `assets/`.

Initial final files:

- `assets/happy_cappy_spritesheet.png`: transparent final sprite sheet consumed by the app.
- `assets/happy_cappy_spritesheet_source.png`: source image retained for traceability.

If generated frames are visually inconsistent, use the output as concept art and produce a cleaned sheet before wiring it into runtime. Do not leave a project-referenced sprite only under `$CODEX_HOME/generated_images`.

## Bundle And App Identity

Rename visible app identity:

- `CFBundleName`: `Happy Cappy`
- Menu title and quit action: `Quit Happy Cappy`
- Window title: `Happy Cappy`
- Bundle output path: `dist/Happy Cappy.app`

The executable is `happy-cappy`, matching the public project and bundle identity.

## Error Handling

- Missing sprite asset: fail fast with a clear log message.
- Invalid sprite dimensions: fail fast with expected grid details.
- Config load failure: warn and use defaults.
- Config save failure: warn, keep runtime changes active, and continue.
- Settings window creation failure: warn and keep menu/pet usable.
- Context menu creation failure: warn and keep drag/hover usable.
- Start-at-login setup failure: show a warning in logs and leave the setting off.
- Display bounds unavailable: fall back to current monitor, then primary monitor, then the existing conservative bounds.

## Testing And Verification

Automated checks:

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release`
- `scripts/build_app.sh`
- App bundle verification already covered by `scripts/verify.sh`

Unit tests:

- Config defaults, missing-field normalization, corrupt-config fallback, bounds clamping.
- Personality timing and expression selection invariants.
- Hover mode overrides default expression and exits cleanly.
- Drag mode pauses movement, clamps on release, and emits save intent.
- Alpha hit-test distinguishes transparent and visible pixels.
- Sprite-sheet validation for the expanded Happy Cappy grid.
- Menu command mapping for show/hide/reset/settings.

Manual macOS smoke:

- App launches as Happy Cappy.
- Menu bar shows Settings, Show/Hide Pet, Reset Position, Quit.
- Settings can be opened from menu bar and right-click context menu.
- Personality changes apply immediately.
- Hover action changes with personality.
- Pet can be dragged and stays where released.
- Position persists after quitting and relaunching.
- Right-click `Hide Pet` hides only the pet.
- Menu bar `Show Pet` restores the pet.
- `Reset Position` moves the pet back into a safe visible location.
- Transparent window behavior remains acceptable; any fallback from per-pixel pass-through is documented.

## Risks

- Per-pixel transparent-area pass-through may be brittle with `winit` and AppKit. The fallback is full pet-window event capture.
- Native settings and context menus require AppKit interop. Keep unsafe code isolated and small.
- Start-at-login may require packaging/signing decisions beyond the current ad hoc bundle. Hide or defer the checkbox if it cannot be supported cleanly.
- Generated sprite sheets may need cleanup before frame alignment is good enough for runtime animation.
- Expanded settings add more state. Config normalization and narrow tests are required to avoid bad persisted values breaking launch.

## Implementation Order

1. Rename bundle/menu identity to Happy Cappy without changing behavior.
2. Add settings config model and persistence.
3. Add personality-aware pet behavior tests and implementation.
4. Add interaction state and alpha hit-test tests.
5. Change window mouse behavior to support hover, drag, and right-click.
6. Add menu bar and context menu commands.
7. Add native settings window.
8. Generate and validate Happy Cappy assets using `imagegen`.
9. Wire expanded sprite sheet and final runtime animations.
10. Run automated verification and manual macOS smoke.
