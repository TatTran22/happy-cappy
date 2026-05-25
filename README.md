# Happy Cappy

Happy Cappy is a native macOS desktop companion: a small capybara pet that lives above your desktop, reacts to hover, can be dragged around, and stays controllable from the menu bar.

## Features

- Capybara sprite pet with idle, blink, happy, curious, sleepy, hover, walk, and drag animations.
- Drag the pet to any visible position and persist its location.
- Hover reactions with Calm, Cheerful, and Lively personality presets.
- Right-click the pet for Settings, Hide/Show, and Reset Position controls.
- Native macOS menu bar item (`HC`) and settings panel.
- Hide the pet while keeping the menu bar app running.
- Focus Mode keeps Happy Cappy visible while passing mouse input through to apps underneath.
- Nap and Cheer Up actions from the menu bar and pet context menu.
- Local JSON settings under `~/Library/Application Support/Happy Cappy/settings.json`.

## Interaction Notes

Interactive mode captures mouse input across the pet window frame so hover, drag, and right-click controls stay reliable. Transparent pixels are alpha-tested for pet actions, but macOS still routes events to the window frame. Use Focus Mode when you want Happy Cappy to stay visible without intercepting clicks.

## Requirements

- macOS 11 or newer
- Rust stable toolchain

## Build

```bash
./scripts/build_app.sh
```

The app bundle is created at:

```text
dist/Happy Cappy.app
```

## Run

```bash
open "dist/Happy Cappy.app"
```

Use the `HC` menu bar item to open Settings, reset the pet, hide/show it, or quit.

## Verify

```bash
./scripts/verify.sh
```

This runs formatting, tests, clippy, release build, bundle assembly, and codesign verification when `codesign` is available.

## Assets

The runtime sprite sheet is `assets/happy_cappy_spritesheet.png`. The source image generated for the asset workflow is retained at `assets/happy_cappy_spritesheet_source.png`.

## License

No license has been granted yet.
