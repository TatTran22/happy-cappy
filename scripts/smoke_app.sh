#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT_DIR/dist/Happy Cappy.app"
LEGACY_APP_DIR="$ROOT_DIR/dist/DesktopPet.app"
INFO_PLIST="$APP_DIR/Contents/Info.plist"
EXECUTABLE="$APP_DIR/Contents/MacOS/happy-cappy"
SPRITE="$APP_DIR/Contents/Resources/happy_cappy_spritesheet.png"
PLIST_BUDDY="/usr/libexec/PlistBuddy"
RECORD_RESULTS_PATH="$ROOT_DIR/docs/superpowers/plans/2026-05-25-happy-cappy-smoke.md"

"$ROOT_DIR/scripts/build_app.sh"

if [[ ! -x "$EXECUTABLE" ]]; then
  echo "Missing executable: $EXECUTABLE" >&2
  exit 1
fi

if [[ ! -f "$INFO_PLIST" ]]; then
  echo "Missing Info.plist: $INFO_PLIST" >&2
  exit 1
fi

if [[ ! -f "$SPRITE" ]]; then
  echo "Missing sprite: $SPRITE" >&2
  exit 1
fi

if [[ -e "$LEGACY_APP_DIR" ]]; then
  echo "Legacy app bundle still exists: $LEGACY_APP_DIR" >&2
  exit 1
fi

bundle_name="$("$PLIST_BUDDY" -c "Print :CFBundleName" "$INFO_PLIST")"
bundle_executable="$("$PLIST_BUDDY" -c "Print :CFBundleExecutable" "$INFO_PLIST")"
lsui_element="$("$PLIST_BUDDY" -c "Print :LSUIElement" "$INFO_PLIST")"

if [[ "$bundle_name" != "Happy Cappy" ]]; then
  echo "Unexpected CFBundleName: $bundle_name" >&2
  exit 1
fi

if [[ "$bundle_executable" != "happy-cappy" ]]; then
  echo "Unexpected CFBundleExecutable: $bundle_executable" >&2
  exit 1
fi

if [[ "$lsui_element" != "true" ]]; then
  echo "Unexpected LSUIElement: $lsui_element" >&2
  exit 1
fi

if command -v codesign >/dev/null 2>&1; then
  codesign --verify --deep --strict "$APP_DIR"
fi

open "$APP_DIR"

cat <<CHECKLIST
Manual smoke checklist:
- Confirm there is no Dock or Cmd+Tab entry.
- Confirm the menu bar item title is HC.
- Enable Focus Mode from the menu bar; confirm clicks pass through the pet while it remains visible.
- Disable Focus Mode from the menu bar; confirm hover, drag, and right-click work again.
- Trigger Nap; confirm the pet switches to a sleepy expression and stops walking temporarily.
- Trigger Cheer Up; confirm the pet switches to a happy expression temporarily.
- Record results in: $RECORD_RESULTS_PATH
CHECKLIST
