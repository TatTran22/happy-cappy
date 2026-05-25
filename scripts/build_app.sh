#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT_DIR/dist/DesktopPet.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
SPRITE_PATH="$ROOT_DIR/assets/pet_spritesheet.png"

if [[ ! -f "$SPRITE_PATH" ]]; then
  echo "Missing sprite asset: $SPRITE_PATH" >&2
  echo "Create assets/pet_spritesheet.png before building the app bundle." >&2
  exit 1
fi

cargo build --manifest-path "$ROOT_DIR/Cargo.toml" --release

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$ROOT_DIR/target/release/desktop-pet" "$MACOS_DIR/desktop-pet"
cp "$ROOT_DIR/packaging/Info.plist" "$CONTENTS_DIR/Info.plist"
cp "$SPRITE_PATH" "$RESOURCES_DIR/pet_spritesheet.png"

chmod +x "$MACOS_DIR/desktop-pet"

if command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - "$MACOS_DIR/desktop-pet"
  codesign --force --sign - "$APP_DIR"
fi

echo "Built $APP_DIR"
