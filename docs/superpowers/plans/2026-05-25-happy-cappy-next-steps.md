# Happy Cappy Next Steps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the current verified local Happy Cappy bundle into a smoke-tested, less intrusive, more expressive desktop companion.

**Architecture:** Keep the existing Rust `winit + pixels + AppKit` architecture. Add only small pure-model changes for focus mode and micro-actions, keep AppKit-specific UI wiring in the existing macOS modules, and document the current transparent-window fallback instead of pretending per-pixel click-through is guaranteed.

**Tech Stack:** Rust 2021, `winit 0.30.13`, `pixels 0.17.1`, `objc2`, native macOS app bundle, Bash verification scripts, repo-local smoke checklist.

---

## Scope Check

This plan covers all next-step recommendations from the code review:

- Runtime smoke and cleanup of stale bundle artifacts.
- Desktop-friendly interaction polish through a practical Focus Mode and explicit documentation of the full-frame interaction fallback.
- Product depth through small session micro-actions: Nap and Cheer Up.
- Updated verification and smoke documentation.

This plan intentionally does not attempt full per-pixel pass-through via global mouse polling. The current `winit + AppKit` window receives events at the rectangular window level when interactive. A reliable per-pixel pass-through implementation would require deeper platform experimentation. The implementation here gives users a real low-friction escape hatch: Focus Mode makes the pet ignore mouse input completely while staying visible and controllable from the menu bar.

## File Structure

- Modify `scripts/build_app.sh`: remove stale `dist/DesktopPet.app` during bundle assembly.
- Create `scripts/smoke_app.sh`: repeatable local smoke launcher and static bundle checks.
- Modify `docs/superpowers/plans/2026-05-25-happy-cappy-smoke.md`: add Focus Mode and micro-action smoke items.
- Create `docs/superpowers/reports/2026-05-25-happy-cappy-smoke-results.md`: manual smoke result log created during execution.
- Modify `src/lib.rs`: export a new pure `micro_action` module.
- Create `src/micro_action.rs`: pure model for Nap and Cheer Up action overrides.
- Modify `src/settings.rs`: persist `focus_mode` with serde defaults.
- Modify `src/app.rs`: add focus mode, micro-action commands, window pass-through synchronization, and test hooks.
- Modify `src/menu_bar.rs`: add Focus Mode, Nap, and Cheer Up menu commands.
- Modify `src/window_macos.rs`: add Focus Mode, Nap, and Cheer Up to the pet context menu.
- Modify `src/command_target_macos.rs`: no structural change expected; verify new command tags flow through `command_from_tag`.
- Modify `src/settings_window_macos.rs`: add a Focus Mode toggle button and keep its title synchronized.
- Modify `src/pet.rs`: add action override priority into the behavior model.
- Modify `README.md`: document Focus Mode, micro-actions, and the transparent-window limitation.

---

### Task 1: Harden Bundle Cleanup And Smoke Launch Workflow

**Files:**
- Modify: `scripts/build_app.sh`
- Create: `scripts/smoke_app.sh`
- Modify: `docs/superpowers/plans/2026-05-25-happy-cappy-smoke.md`

- [ ] **Step 1: Update bundle cleanup in `scripts/build_app.sh`**

Add a legacy bundle variable after `APP_DIR`:

```bash
LEGACY_APP_DIR="$ROOT_DIR/dist/DesktopPet.app"
```

Replace the existing bundle cleanup:

```bash
rm -rf "$APP_DIR"
```

with:

```bash
rm -rf "$APP_DIR" "$LEGACY_APP_DIR"
```

- [ ] **Step 2: Create `scripts/smoke_app.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT_DIR/dist/Happy Cappy.app"
INFO_PLIST="$APP_DIR/Contents/Info.plist"
EXECUTABLE="$APP_DIR/Contents/MacOS/happy-cappy"
SPRITE="$APP_DIR/Contents/Resources/happy_cappy_spritesheet.png"

"$ROOT_DIR/scripts/build_app.sh"

test -x "$EXECUTABLE"
test -f "$INFO_PLIST"
test -f "$SPRITE"
test ! -e "$ROOT_DIR/dist/DesktopPet.app"

/usr/libexec/PlistBuddy -c "Print :CFBundleName" "$INFO_PLIST" | grep -qx "Happy Cappy"
/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" "$INFO_PLIST" | grep -qx "happy-cappy"
/usr/libexec/PlistBuddy -c "Print :LSUIElement" "$INFO_PLIST" | grep -qx "true"

if command -v codesign >/dev/null 2>&1; then
  codesign --verify --deep --strict "$APP_DIR"
fi

open "$APP_DIR"

cat <<'CHECKLIST'

Manual smoke checklist:
- No Dock icon and no Cmd+Tab entry.
- Menu bar item title is HC.
- Settings opens from menu bar.
- Personality changes apply on hover.
- Drag persists after quit and relaunch.
- Right-click visible pet pixels opens context menu.
- Focus Mode makes clicks pass through the pet window while the pet remains visible.
- Disable Focus Mode from menu bar restores hover, drag, and right-click.
- Nap shows a sleepy action and pauses walking.
- Cheer Up shows a happy action.
- Hide Pet hides only the pet; Show Pet restores it.
- Reset Position returns the pet to a visible safe location.

Record results in docs/superpowers/reports/2026-05-25-happy-cappy-smoke-results.md.
CHECKLIST
```

- [ ] **Step 3: Make the smoke script executable**

Run:

```bash
chmod +x scripts/smoke_app.sh
```

- [ ] **Step 4: Extend the smoke checklist document**

Append these items to `docs/superpowers/plans/2026-05-25-happy-cappy-smoke.md`:

```markdown
- Enable Focus Mode from the menu bar; confirm clicks pass through the pet while it remains visible.
- Disable Focus Mode from the menu bar; confirm hover, drag, and right-click work again.
- Trigger Nap; confirm the pet switches to a sleepy expression and stops walking temporarily.
- Trigger Cheer Up; confirm the pet switches to a happy expression temporarily.
```

- [ ] **Step 5: Run static smoke workflow checks**

Run:

```bash
bash -n scripts/build_app.sh
bash -n scripts/smoke_app.sh
./scripts/build_app.sh
test ! -e "dist/DesktopPet.app"
```

Expected: all commands exit 0, and only `dist/Happy Cappy.app` remains under `dist/`.

- [ ] **Step 6: Commit**

```bash
git add scripts/build_app.sh scripts/smoke_app.sh docs/superpowers/plans/2026-05-25-happy-cappy-smoke.md
git commit -m "chore: harden Happy Cappy smoke workflow"
```

---

### Task 2: Add Focus Mode To Settings And Runtime Commands

**Files:**
- Modify: `src/settings.rs`
- Modify: `src/app.rs`
- Modify: `src/menu_bar.rs`

- [ ] **Step 1: Add failing settings tests**

Add tests to `src/settings.rs`:

```rust
#[test]
fn defaults_keep_focus_mode_off() {
    let settings = AppSettings::default();

    assert!(!settings.focus_mode);
}

#[test]
fn partial_settings_load_defaults_focus_mode_to_off() {
    let root = std::env::temp_dir().join(format!(
        "happy-cappy-settings-focus-default-{}",
        fastrand::u64(..)
    ));
    let path = root.join("settings.json");
    fs::create_dir_all(&root).unwrap();
    fs::write(&path, br#"{"personality":"calm"}"#).unwrap();

    let settings = AppSettings::load_from(&path).unwrap();

    assert_eq!(settings.personality, Personality::Calm);
    assert!(!settings.focus_mode);
}
```

Run:

```bash
cargo test --manifest-path Cargo.toml settings::tests::defaults_keep_focus_mode_off settings::tests::partial_settings_load_defaults_focus_mode_to_off
```

Expected: fail because `focus_mode` does not exist yet.

- [ ] **Step 2: Add `focus_mode` to `AppSettings`**

Add this field to `AppSettings`:

```rust
#[serde(default = "default_focus_mode")]
pub focus_mode: bool,
```

Add it to `Default`:

```rust
focus_mode: false,
```

Add the default function near the existing default helpers:

```rust
fn default_focus_mode() -> bool {
    false
}
```

- [ ] **Step 3: Add command mapping tests**

In `src/menu_bar.rs`, extend `command_tags_map_to_app_commands`:

```rust
assert_eq!(
    command_from_tag(MENU_TAG_FOCUS_MODE),
    Some(AppCommand::ToggleFocusMode)
);
assert_eq!(command_from_tag(MENU_TAG_NAP), Some(AppCommand::Nap));
assert_eq!(command_from_tag(MENU_TAG_CHEER_UP), Some(AppCommand::CheerUp));
```

Run:

```bash
cargo test --manifest-path Cargo.toml menu_bar::tests::command_tags_map_to_app_commands
```

Expected: fail because the new constants and commands do not exist yet.

- [ ] **Step 4: Add runtime command variants**

Add to `AppCommand` in `src/app.rs`:

```rust
SetFocusMode(bool),
ToggleFocusMode,
Nap,
CheerUp,
```

Add constants to `src/menu_bar.rs`:

```rust
pub const MENU_TAG_FOCUS_MODE: isize = 1005;
pub const MENU_TAG_NAP: isize = 1006;
pub const MENU_TAG_CHEER_UP: isize = 1007;
```

Extend `command_from_tag`:

```rust
MENU_TAG_FOCUS_MODE => Some(AppCommand::ToggleFocusMode),
MENU_TAG_NAP => Some(AppCommand::Nap),
MENU_TAG_CHEER_UP => Some(AppCommand::CheerUp),
```

- [ ] **Step 5: Add app command tests**

Add tests to `src/app.rs`:

```rust
#[test]
fn non_quit_command_toggles_focus_mode() {
    let mut app = DesktopPetApp::new_for_test();
    app.settings_path = None;

    assert!(app.handle_non_quit_command_for_test(AppCommand::ToggleFocusMode));

    assert!(app.settings_for_test().focus_mode);
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
```

Run:

```bash
cargo test --manifest-path Cargo.toml app::tests::non_quit_command_toggles_focus_mode app::tests::set_focus_mode_command_sets_exact_value
```

Expected: fail until command handling is implemented.

- [ ] **Step 6: Implement focus mode command handling**

Import `set_pet_window_mouse_passthrough` in `src/app.rs`:

```rust
window_macos::{apply_desktop_pet_window_behavior, set_pet_window_mouse_passthrough},
```

Add this method to `DesktopPetApp`:

```rust
fn set_focus_mode(&mut self, focus_mode: bool) {
    self.settings.focus_mode = focus_mode;
    if focus_mode {
        self.interaction = InteractionState::default();
        self.pet.set_hovered(false);
        self.pet.set_dragging(false);
    }
    self.sync_window_passthrough();
    self.sync_settings_window();
    self.sync_menu_bar();
    self.save_settings();
}

fn sync_window_passthrough(&self) {
    let Some(window) = &self.window else {
        return;
    };
    if let Err(error) = set_pet_window_mouse_passthrough(window, self.settings.focus_mode) {
        warn!("failed to update focus mode mouse pass-through: {error}");
    }
}
```

Call `self.sync_window_passthrough();` near the end of `apply_settings`.

Add cases to `handle_non_quit_command`:

```rust
AppCommand::SetFocusMode(focus_mode) => self.set_focus_mode(focus_mode),
AppCommand::ToggleFocusMode => self.set_focus_mode(!self.settings.focus_mode),
```

Leave `Nap` and `CheerUp` as no-op command cases for now:

```rust
AppCommand::Nap | AppCommand::CheerUp => {}
```

- [ ] **Step 7: Run focused tests**

Run:

```bash
cargo test --manifest-path Cargo.toml settings::tests::defaults_keep_focus_mode_off settings::tests::partial_settings_load_defaults_focus_mode_to_off
cargo test --manifest-path Cargo.toml menu_bar::tests::command_tags_map_to_app_commands
cargo test --manifest-path Cargo.toml app::tests::non_quit_command_toggles_focus_mode app::tests::set_focus_mode_command_sets_exact_value
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src/settings.rs src/app.rs src/menu_bar.rs
git commit -m "feat: add Happy Cappy focus mode state"
```

---

### Task 3: Add Pure Micro-Action Model

**Files:**
- Create: `src/micro_action.rs`
- Modify: `src/lib.rs`
- Modify: `src/pet.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Export the new module**

Add to `src/lib.rs`:

```rust
pub mod micro_action;
```

- [ ] **Step 2: Create `src/micro_action.rs` with tests and implementation**

```rust
use std::time::Duration;

use crate::pet::AnimationGroup;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroAction {
    Nap,
    CheerUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionOverride {
    action: MicroAction,
    remaining: Duration,
}

const NAP_DURATION: Duration = Duration::from_secs(30);
const CHEER_UP_DURATION: Duration = Duration::from_secs(8);

impl ActionOverride {
    pub fn new(action: MicroAction) -> Self {
        let remaining = match action {
            MicroAction::Nap => NAP_DURATION,
            MicroAction::CheerUp => CHEER_UP_DURATION,
        };

        Self { action, remaining }
    }

    pub fn action(&self) -> MicroAction {
        self.action
    }

    pub fn remaining(&self) -> Duration {
        self.remaining
    }

    pub fn tick(&mut self, dt: Duration) -> bool {
        self.remaining = self.remaining.checked_sub(dt).unwrap_or(Duration::ZERO);
        self.remaining.is_zero()
    }

    pub fn animation_group(&self) -> AnimationGroup {
        match self.action {
            MicroAction::Nap => AnimationGroup::Sleepy,
            MicroAction::CheerUp => AnimationGroup::Happy,
        }
    }

    pub fn disables_movement(&self) -> bool {
        matches!(self.action, MicroAction::Nap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nap_last_30_seconds_and_uses_sleepy_group() {
        let action = ActionOverride::new(MicroAction::Nap);

        assert_eq!(action.remaining(), Duration::from_secs(30));
        assert_eq!(action.animation_group(), AnimationGroup::Sleepy);
        assert!(action.disables_movement());
    }

    #[test]
    fn cheer_up_last_8_seconds_and_uses_happy_group() {
        let action = ActionOverride::new(MicroAction::CheerUp);

        assert_eq!(action.remaining(), Duration::from_secs(8));
        assert_eq!(action.animation_group(), AnimationGroup::Happy);
        assert!(!action.disables_movement());
    }

    #[test]
    fn tick_reports_completion() {
        let mut action = ActionOverride::new(MicroAction::CheerUp);

        assert!(!action.tick(Duration::from_secs(7)));
        assert_eq!(action.remaining(), Duration::from_secs(1));
        assert!(action.tick(Duration::from_secs(1)));
        assert_eq!(action.remaining(), Duration::ZERO);
    }
}
```

- [ ] **Step 3: Run micro-action tests**

Run:

```bash
cargo test --manifest-path Cargo.toml micro_action
```

Expected: pass.

- [ ] **Step 4: Add failing pet tests**

Add tests to `src/pet.rs`:

```rust
#[test]
fn nap_micro_action_uses_sleepy_group_and_stops_movement() {
    let mut pet = Pet::new();
    pet.force_state_for_test(PetState::Walk);

    pet.start_micro_action(crate::micro_action::MicroAction::Nap);
    let tick = pet.tick(Duration::from_millis(16));

    assert_eq!(pet.behavior_mode(), BehaviorMode::Action);
    assert_eq!(pet.current_animation_group(), AnimationGroup::Sleepy);
    assert_eq!(tick.speed_x, 0.0);
}

#[test]
fn cheer_up_micro_action_uses_happy_group_temporarily() {
    let mut pet = Pet::new();

    pet.start_micro_action(crate::micro_action::MicroAction::CheerUp);
    assert_eq!(pet.current_animation_group(), AnimationGroup::Happy);

    pet.tick(Duration::from_secs(8));

    assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
}

#[test]
fn hover_overrides_micro_action_until_hover_ends() {
    let mut pet = Pet::new();

    pet.start_micro_action(crate::micro_action::MicroAction::CheerUp);
    pet.set_hovered(true);
    assert_eq!(pet.behavior_mode(), BehaviorMode::Hovered);
    assert_eq!(pet.current_animation_group(), AnimationGroup::HoverCheerful);

    pet.set_hovered(false);
    assert_eq!(pet.behavior_mode(), BehaviorMode::Action);
    assert_eq!(pet.current_animation_group(), AnimationGroup::Happy);
}
```

Run:

```bash
cargo test --manifest-path Cargo.toml pet::tests::nap_micro_action_uses_sleepy_group_and_stops_movement pet::tests::cheer_up_micro_action_uses_happy_group_temporarily pet::tests::hover_overrides_micro_action_until_hover_ends
```

Expected: fail until `Pet` supports action overrides.

- [ ] **Step 5: Implement pet action override support**

In `src/pet.rs`, import:

```rust
use crate::micro_action::{ActionOverride, MicroAction};
```

Add behavior mode:

```rust
Action,
```

Add a field to `Pet`:

```rust
action_override: Option<ActionOverride>,
```

Initialize it in `new_with_seed`:

```rust
action_override: None,
```

Add public methods:

```rust
pub fn start_micro_action(&mut self, action: MicroAction) {
    self.action_override = Some(ActionOverride::new(action));
    self.refresh_behavior_mode();
}

pub fn clear_micro_action(&mut self) {
    self.action_override = None;
    self.refresh_behavior_mode();
}
```

At the start of `tick`, after the hidden early-return and before state advancement, add:

```rust
if let Some(action) = &mut self.action_override {
    if action.tick(dt) {
        self.action_override = None;
    }
}
```

Update `speed_x` before checking `self.state`:

```rust
if self
    .action_override
    .as_ref()
    .is_some_and(ActionOverride::disables_movement)
{
    return 0.0;
}
```

Update `refresh_behavior_mode` priority:

```rust
} else if self.action_override.is_some() {
    BehaviorMode::Action
```

Update `animation_group` selection:

```rust
BehaviorMode::Action => self
    .action_override
    .map(|action| action.animation_group())
    .unwrap_or_else(|| self.default_expression_group()),
```

- [ ] **Step 6: Wire app commands to pet actions**

In `src/app.rs`, import:

```rust
use crate::micro_action::MicroAction;
```

Replace the no-op command cases:

```rust
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
```

Update `next_tick_interval`:

```rust
crate::pet::BehaviorMode::Action => TARGET_FRAME_TIME,
```

- [ ] **Step 7: Add app command tests for micro-actions**

Add tests to `src/app.rs`:

```rust
#[test]
fn nap_command_starts_sleepy_action() {
    let mut app = DesktopPetApp::new_for_test();
    app.settings_path = None;

    assert!(app.handle_non_quit_command_for_test(AppCommand::Nap));

    assert_eq!(app.pet.behavior_mode(), crate::pet::BehaviorMode::Action);
    assert_eq!(
        app.pet.current_animation_group(),
        crate::pet::AnimationGroup::Sleepy
    );
}

#[test]
fn cheer_up_command_starts_happy_action() {
    let mut app = DesktopPetApp::new_for_test();
    app.settings_path = None;

    assert!(app.handle_non_quit_command_for_test(AppCommand::CheerUp));

    assert_eq!(app.pet.behavior_mode(), crate::pet::BehaviorMode::Action);
    assert_eq!(
        app.pet.current_animation_group(),
        crate::pet::AnimationGroup::Happy
    );
}
```

- [ ] **Step 8: Run focused tests**

Run:

```bash
cargo test --manifest-path Cargo.toml micro_action
cargo test --manifest-path Cargo.toml pet::tests::nap_micro_action_uses_sleepy_group_and_stops_movement pet::tests::cheer_up_micro_action_uses_happy_group_temporarily pet::tests::hover_overrides_micro_action_until_hover_ends
cargo test --manifest-path Cargo.toml app::tests::nap_command_starts_sleepy_action app::tests::cheer_up_command_starts_happy_action
```

Expected: all pass.

- [ ] **Step 9: Commit**

```bash
git add src/lib.rs src/micro_action.rs src/pet.rs src/app.rs
git commit -m "feat: add Happy Cappy micro actions"
```

---

### Task 4: Wire Focus Mode And Micro-Actions Into Native UI

**Files:**
- Modify: `src/menu_bar.rs`
- Modify: `src/window_macos.rs`
- Modify: `src/settings_window_macos.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Update menu bar state synchronization**

In `src/menu_bar.rs`, add fields to `MenuBarController`:

```rust
focus_mode_item: objc2::rc::Retained<objc2_app_kit::NSMenuItem>,
```

Create menu items in `MenuBarController::new`:

```rust
let focus_mode_item = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
        NSMenuItem::alloc(mtm),
        focus_mode_title(false),
        None,
        ns_string!(""),
    )
};
let nap_item = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
        NSMenuItem::alloc(mtm),
        ns_string!("Nap"),
        None,
        ns_string!(""),
    )
};
let cheer_up_item = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
        NSMenuItem::alloc(mtm),
        ns_string!("Cheer Up"),
        None,
        ns_string!(""),
    )
};
```

Set tags:

```rust
focus_mode_item.setTag(MENU_TAG_FOCUS_MODE);
nap_item.setTag(MENU_TAG_NAP);
cheer_up_item.setTag(MENU_TAG_CHEER_UP);
```

Include them in the target/action loop:

```rust
for item in [
    &settings_item,
    &show_hide_item,
    &reset_item,
    &focus_mode_item,
    &nap_item,
    &cheer_up_item,
    &quit_item,
] {
    unsafe {
        item.setTarget(Some(target_object));
        item.setAction(Some(
            crate::command_target_macos::CommandTarget::command_selector(),
        ));
    }
}
```

Add them to the menu before Quit:

```rust
menu.addItem(&settings_item);
menu.addItem(&show_hide_item);
menu.addItem(&reset_item);
menu.addItem(&focus_mode_item);
menu.addItem(&nap_item);
menu.addItem(&cheer_up_item);
menu.addItem(&quit_item);
```

Replace `sync_pet_visibility` with:

```rust
pub fn sync_runtime_state(&self, pet_visible: bool, focus_mode: bool) {
    self.show_hide_item.setTitle(show_hide_title(pet_visible));
    self.focus_mode_item.setTitle(focus_mode_title(focus_mode));
}
```

Add:

```rust
#[cfg(target_os = "macos")]
fn focus_mode_title(focus_mode: bool) -> &'static objc2_foundation::NSString {
    use objc2_foundation::ns_string;

    if focus_mode {
        ns_string!("Disable Focus Mode")
    } else {
        ns_string!("Enable Focus Mode")
    }
}
```

For the non-macOS stub, replace `sync_pet_visibility` with:

```rust
pub fn sync_runtime_state(&self, _pet_visible: bool, _focus_mode: bool) {}
```

- [ ] **Step 2: Update app menu synchronization**

In `src/app.rs`, replace:

```rust
menu_bar.sync_pet_visibility(self.pet_visible);
```

with:

```rust
menu_bar.sync_runtime_state(self.pet_visible, self.settings.focus_mode);
```

- [ ] **Step 3: Add context menu items**

In `src/window_macos.rs`, create these items in `show_pet_context_menu`:

```rust
let focus = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
        NSMenuItem::alloc(mtm),
        ns_string!("Enable Focus Mode"),
        Some(crate::command_target_macos::CommandTarget::command_selector()),
        ns_string!(""),
    )
};
let nap = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
        NSMenuItem::alloc(mtm),
        ns_string!("Nap"),
        Some(crate::command_target_macos::CommandTarget::command_selector()),
        ns_string!(""),
    )
};
let cheer_up = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
        NSMenuItem::alloc(mtm),
        ns_string!("Cheer Up"),
        Some(crate::command_target_macos::CommandTarget::command_selector()),
        ns_string!(""),
    )
};
```

Set tags and targets:

```rust
focus.setTag(crate::menu_bar::MENU_TAG_FOCUS_MODE);
nap.setTag(crate::menu_bar::MENU_TAG_NAP);
cheer_up.setTag(crate::menu_bar::MENU_TAG_CHEER_UP);
unsafe {
    focus.setTarget(Some(target_object));
    nap.setTarget(Some(target_object));
    cheer_up.setTarget(Some(target_object));
}
```

Add them before reset:

```rust
menu.addItem(&settings);
menu.addItem(&hide);
menu.addItem(&focus);
menu.addItem(&nap);
menu.addItem(&cheer_up);
menu.addItem(&reset);
```

- [ ] **Step 4: Add a Focus Mode button to settings panel**

In `src/settings_window_macos.rs`, add field:

```rust
focus_mode_button: Retained<NSButton>,
```

In `add_buttons`, create the button:

```rust
let focus_mode = unsafe {
    NSButton::buttonWithTitle_target_action(
        focus_mode_title(false),
        Some(target_object),
        Some(CommandTarget::command_selector()),
        mtm,
    )
};
focus_mode.setFrame(rect(MARGIN_X, 64.0, 132.0, 30.0));
focus_mode.setTag(MENU_TAG_FOCUS_MODE as NSInteger);
content_view.addSubview(&focus_mode);
```

Return both buttons from `add_buttons`:

```rust
fn add_buttons(
    content_view: &NSView,
    mtm: MainThreadMarker,
    target_object: &AnyObject,
    pet_visible: bool,
    focus_mode: bool,
) -> (Retained<NSButton>, Retained<NSButton>) {
    ...
    set_focus_mode_title(&focus_mode, focus_mode);
    ...
    (show_hide, focus_mode)
}
```

Add helpers:

```rust
fn set_focus_mode_title(button: &NSButton, focus_mode: bool) {
    button.setTitle(focus_mode_title(focus_mode));
}

fn focus_mode_title(focus_mode: bool) -> &'static NSString {
    if focus_mode {
        ns_string!("Disable Focus")
    } else {
        ns_string!("Enable Focus")
    }
}
```

Update `sync_settings`:

```rust
set_show_hide_title(&self.show_hide_button, settings.pet_visible);
set_focus_mode_title(&self.focus_mode_button, settings.focus_mode);
```

- [ ] **Step 5: Run UI compile checks**

Run:

```bash
cargo fmt --manifest-path Cargo.toml
cargo test --manifest-path Cargo.toml menu_bar::tests::command_tags_map_to_app_commands
cargo check --manifest-path Cargo.toml
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src/menu_bar.rs src/window_macos.rs src/settings_window_macos.rs src/app.rs
git commit -m "feat: wire Happy Cappy focus and actions into menus"
```

---

### Task 5: Document The Runtime Behavior And User-Facing Scope

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-05-25-happy-cappy-smoke.md`

- [ ] **Step 1: Update README features**

Add bullets under `## Features`:

```markdown
- Focus Mode keeps Happy Cappy visible while passing mouse input through to apps underneath.
- Nap and Cheer Up actions from the menu bar and pet context menu.
```

Add a short `## Interaction Notes` section before `## Requirements`:

```markdown
## Interaction Notes

Interactive mode captures mouse input across the pet window frame so hover, drag, and right-click controls stay reliable. Transparent pixels are alpha-tested for pet actions, but macOS still routes events to the window frame. Use Focus Mode when you want Happy Cappy to stay visible without intercepting clicks.
```

- [ ] **Step 2: Update manual smoke checklist**

Ensure `docs/superpowers/plans/2026-05-25-happy-cappy-smoke.md` includes:

```markdown
- Confirm interactive mode still supports hover, drag, and right-click.
- Enable Focus Mode; confirm clicks pass through to a window behind Happy Cappy.
- Disable Focus Mode from the menu bar; confirm interactions return.
- Trigger Nap and confirm the pet shows the sleepy action.
- Trigger Cheer Up and confirm the pet shows the happy action.
```

- [ ] **Step 3: Commit**

```bash
git add README.md docs/superpowers/plans/2026-05-25-happy-cappy-smoke.md
git commit -m "docs: document Happy Cappy interaction modes"
```

---

### Task 6: Full Verification And Manual Smoke Report

**Files:**
- Create: `docs/superpowers/reports/2026-05-25-happy-cappy-smoke-results.md`

- [ ] **Step 1: Run automated verification**

Run:

```bash
./scripts/verify.sh
```

Expected:

- `cargo fmt --check` exits 0.
- `cargo test` exits 0.
- `cargo clippy --all-targets -- -D warnings` exits 0.
- Release build exits 0.
- `dist/Happy Cappy.app` is assembled.
- `codesign --verify --deep --strict` exits 0 when `codesign` is available.

- [ ] **Step 2: Run smoke launcher**

Run:

```bash
./scripts/smoke_app.sh
```

Expected:

- Static bundle checks exit 0.
- `dist/DesktopPet.app` does not exist.
- Happy Cappy launches.

- [ ] **Step 3: Create manual smoke report**

Create `docs/superpowers/reports/2026-05-25-happy-cappy-smoke-results.md`:

```markdown
# Happy Cappy Smoke Results

Date: 2026-05-25
Bundle: `dist/Happy Cappy.app`

## Automated Checks

- `./scripts/verify.sh`: PASS
- `./scripts/smoke_app.sh` static checks: PASS

## Manual Checks

- No Dock icon and no Cmd+Tab entry: PASS
- Menu bar item title is `HC`: PASS
- Settings opens from menu bar: PASS
- Personality changes apply on hover: PASS
- Drag persists after quit and relaunch: PASS
- Right-click visible pet pixels opens context menu: PASS
- Interactive mode supports hover, drag, and right-click: PASS
- Focus Mode passes clicks through to apps underneath: PASS
- Menu bar disables Focus Mode and restores interactions: PASS
- Nap shows sleepy action and pauses walking temporarily: PASS
- Cheer Up shows happy action temporarily: PASS
- Hide Pet keeps menu bar app alive: PASS
- Show Pet restores the pet: PASS
- Reset Position returns the pet to a visible safe location: PASS

## Notes

- Interactive mode intentionally uses full-frame window event capture for reliable controls.
- Focus Mode is the supported click-through mode.
```

If any item fails, mark it `FAIL` and add one sentence under `## Notes` with the observed behavior and exact reproduction step.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/reports/2026-05-25-happy-cappy-smoke-results.md
git commit -m "test: record Happy Cappy runtime smoke"
```

---

### Task 7: Final Repository Sanity Check

**Files:**
- No source edits expected.

- [ ] **Step 1: Check git state**

Run:

```bash
git status --short
git log --oneline -6
```

Expected:

- `git status --short` prints nothing.
- Recent commits include:
  - `chore: harden Happy Cappy smoke workflow`
  - `feat: add Happy Cappy focus mode state`
  - `feat: add Happy Cappy micro actions`
  - `feat: wire Happy Cappy focus and actions into menus`
  - `docs: document Happy Cappy interaction modes`
  - `test: record Happy Cappy runtime smoke`

- [ ] **Step 2: Confirm final bundle**

Run:

```bash
test -x "dist/Happy Cappy.app/Contents/MacOS/happy-cappy"
test -f "dist/Happy Cappy.app/Contents/Resources/happy_cappy_spritesheet.png"
test ! -e "dist/DesktopPet.app"
du -sh "dist/Happy Cappy.app"
```

Expected:

- All `test` commands exit 0.
- `du` prints a bundle size.

## Self-Review

- Runtime smoke is covered by Tasks 1 and 6.
- Stale `DesktopPet.app` cleanup is covered by Task 1 and verified again in Task 7.
- Desktop-friendly interaction is covered by Focus Mode in Tasks 2 and 4, with transparent-window limitation documentation in Task 5.
- Product depth is covered by Nap and Cheer Up in Tasks 3 and 4.
- Automated and manual verification are covered by Task 6.
- No plan step relies on a placeholder or an unspecified file path.
