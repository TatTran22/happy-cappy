#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT_DIR/dist/DesktopPet.app"

cargo fmt --manifest-path "$ROOT_DIR/Cargo.toml" --check
cargo test --manifest-path "$ROOT_DIR/Cargo.toml"
cargo clippy --manifest-path "$ROOT_DIR/Cargo.toml" --all-targets -- -D warnings
cargo build --manifest-path "$ROOT_DIR/Cargo.toml" --release
"$ROOT_DIR/scripts/build_app.sh"

test -x "$APP_DIR/Contents/MacOS/desktop-pet"
test -f "$APP_DIR/Contents/Info.plist"
test -f "$APP_DIR/Contents/Resources/pet_spritesheet.png"

if command -v codesign >/dev/null 2>&1; then
  codesign --verify --deep --strict "$APP_DIR"
fi
