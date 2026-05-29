# SP4-B Notification Model + Unix Socket + CLI Trigger Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let local processes trigger an ambient pet reaction via `happy-cappy notify ...` over a Unix socket; the pet swaps to a kind/animation for a TTL. Core stays generic (no agent names); `label`/`body` are carried + logged, not rendered.

**Architecture:** A pure `notification` module holds the `NotificationEvent` model, presets, clamps, UTF-8-safe caps, and the wire parser. The runtime gains a `notification: Option<NotificationState>` resolved **once** at `set_notification` (via a new dynamic resolver helper), a `Notifying` behavior mode that pins the resolved animation, priority-based preemption, and a TTL that counts down in every state (including hidden). A `control_socket` module binds `~/Library/Application Support/Happy Cappy/control.sock` at startup (before GUI init) and forwards parsed events into the winit loop via `EventLoopProxy<AppCommand>`. The same binary's `notify` subcommand is the client.

**Tech Stack:** Rust 2021, `serde`/`serde_json`, **`clap` (new dep, derive)**, `std::os::unix::net`, existing `winit` `EventLoopProxy<AppCommand>`. Depends on **SP4-A** (one-shot completion signal, `is_lifecycle`, `set_selected_animation`, `current_fallback`).

Spec: `docs/superpowers/specs/2026-05-29-sp4b-notification-trigger-design.md`

---

## File Structure

- Create `src/notification.rs`: `NotificationEvent`, presets (`preset_for`), `clamp_priority`/`clamp_ttl`, `truncate_text`, `parse_notify_line`, `NotifyParseError`, and the `clap` CLI types (`Cli`/`Command`/`NotifyArgs`).
- Create `src/control_socket.rs`: `control_socket_path`, `bind_control_socket -> BindOutcome`, `spawn_listener`, `send_notify`.
- Modify `src/pet/resolver.rs`: add `lookup_with_fallback_dynamic`.
- Modify `src/pet/runtime.rs`: `NotificationState`, `notification` field, `set_notification`/`clear_notification`, `BehaviorMode::Notifying`, refresh pin, TTL countdown, one-shot clear.
- Modify `src/app.rs`: `AppCommand::Notify(NotificationEvent)` + handler.
- Modify `src/settings.rs`: `app_support_dir()` helper (reused by socket).
- Modify `src/main.rs`: parse `Cli`; client path vs GUI + socket bootstrap.
- Modify `src/lib.rs`: `pub mod notification;`, `pub mod control_socket;`.
- Modify `assets/manifests/happy_cappy.json`: add `notify-*` animations (reuse existing frames).
- Modify `Cargo.toml`: add `clap`.

---

### Task 1: `notification` module — event model, presets, caps, wire parser

**Files:**
- Create: `src/notification.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Register the module**

In `src/lib.rs`, add (keep alphabetical-ish ordering near `micro_action`):

```rust
pub mod notification;
```

- [ ] **Step 2: Write the failing tests**

Create `src/notification.rs` with only the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_event() {
        let ev = parse_notify_line(r#"{ "kind": "running" }"#).unwrap();
        assert_eq!(ev.kind, "running");
        assert_eq!(ev.animation_name, None);
        assert_eq!(ev.ttl_ms, None);
        assert_eq!(ev.priority, None);
    }

    #[test]
    fn parses_full_event() {
        let ev = parse_notify_line(
            r#"{ "kind": "failed", "animation_name": "notify-failed",
                 "label": "L", "body": "B", "ttl_ms": 5000, "priority": 70 }"#,
        )
        .unwrap();
        assert_eq!(ev.kind, "failed");
        assert_eq!(ev.animation_name.as_deref(), Some("notify-failed"));
        assert_eq!(ev.label.as_deref(), Some("L"));
        assert_eq!(ev.ttl_ms, Some(5000));
        assert_eq!(ev.priority, Some(70));
    }

    #[test]
    fn rejects_empty_kind() {
        assert!(matches!(
            parse_notify_line(r#"{ "kind": "" }"#),
            Err(NotifyParseError::MissingKind)
        ));
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_notify_line("not json").is_err());
    }

    #[test]
    fn rejects_oversized_line() {
        let big = format!(r#"{{ "kind": "running", "body": "{}" }}"#, "x".repeat(MAX_LINE_BYTES));
        assert!(matches!(parse_notify_line(&big), Err(NotifyParseError::TooLong)));
    }

    #[test]
    fn rejects_overlong_kind_identifier() {
        let k = "k".repeat(KIND_MAX_BYTES + 1);
        let line = format!(r#"{{ "kind": "{k}" }}"#);
        assert!(matches!(parse_notify_line(&line), Err(NotifyParseError::FieldTooLong("kind"))));
    }

    #[test]
    fn truncates_body_at_utf8_char_boundary_without_panic() {
        // "é" is 2 bytes; build a body that straddles the cap boundary.
        let body = "é".repeat(TEXT_MAX_BYTES); // 2*cap bytes of 2-byte chars
        let line = format!(r#"{{ "kind": "message", "body": {} }}"#, serde_json::to_string(&body).unwrap());
        let ev = parse_notify_line(&line).unwrap();
        let out = ev.body.unwrap();
        assert!(out.len() <= TEXT_MAX_BYTES);
        assert!(out.is_char_boundary(out.len())); // valid String, no mid-codepoint cut
    }

    #[test]
    fn preset_unknown_kind_uses_message_defaults() {
        assert_eq!(preset_for("totally-new"), preset_for("message"));
    }

    #[test]
    fn preset_priority_ordering_attention_outranks_informational() {
        let (run, _) = preset_for("running");
        let (succ, _) = preset_for("succeeded");
        let (review, _) = preset_for("needs-review");
        let (fail, _) = preset_for("failed");
        assert!(run < succ && succ < review && review < fail);
    }

    #[test]
    fn clamps_priority_and_ttl() {
        assert_eq!(clamp_priority(-5), PRIORITY_MIN);
        assert_eq!(clamp_priority(999), PRIORITY_MAX);
        assert_eq!(clamp_ttl(0), TTL_MIN_MS);
        assert_eq!(clamp_ttl(u64::MAX), TTL_MAX_MS);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib notification::tests 2>&1 | head -20`
Expected: FAIL — module items not defined.

- [ ] **Step 4: Implement the module**

Prepend to `src/notification.rs` (above the test module):

```rust
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Maximum bytes accepted for one socket event line.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// Identifier caps (over-length -> reject the event).
pub const KIND_MAX_BYTES: usize = 64;
pub const ANIMATION_NAME_MAX_BYTES: usize = 64;
/// Free-text caps (over-length -> truncate at a char boundary).
pub const TEXT_MAX_BYTES: usize = 1024;
pub const PRIORITY_MIN: i32 = 0;
pub const PRIORITY_MAX: i32 = 100;
pub const TTL_MIN_MS: u64 = 1;
pub const TTL_MAX_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub kind: String,
    #[serde(default)]
    pub animation_name: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub ttl_ms: Option<u64>,
    #[serde(default)]
    pub priority: Option<i32>,
}

/// Default `(priority, ttl_ms)` for a kind. Unknown kinds use the `message` preset.
/// Priorities order attention states (needs-review, failed) above informational ones.
pub fn preset_for(kind: &str) -> (i32, u64) {
    match kind {
        "running" => (10, 180_000),
        "message" => (20, 10_000),
        "succeeded" => (30, 8_000),
        "needs-review" => (80, 120_000),
        "failed" => (90, 30_000),
        _ => (20, 10_000), // message preset
    }
}

pub fn clamp_priority(p: i32) -> i32 {
    p.clamp(PRIORITY_MIN, PRIORITY_MAX)
}

pub fn clamp_ttl(ms: u64) -> u64 {
    ms.clamp(TTL_MIN_MS, TTL_MAX_MS)
}

/// Truncate free text to the largest UTF-8 char boundary at or below `cap` bytes.
pub fn truncate_text(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        return s.to_string();
    }
    let mut end = cap;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[derive(Debug)]
pub enum NotifyParseError {
    TooLong,
    MissingKind,
    FieldTooLong(&'static str),
    Json(serde_json::Error),
}

impl fmt::Display for NotifyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong => write!(f, "event line exceeds {MAX_LINE_BYTES} bytes"),
            Self::MissingKind => write!(f, "event is missing a non-empty 'kind'"),
            Self::FieldTooLong(field) => write!(f, "field '{field}' exceeds its length cap"),
            Self::Json(e) => write!(f, "invalid event JSON: {e}"),
        }
    }
}

impl Error for NotifyParseError {}

impl From<serde_json::Error> for NotifyParseError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Parse + bound one wire line into a `NotificationEvent`.
/// Identifiers over their cap reject the event; free text is truncated at a char boundary.
pub fn parse_notify_line(line: &str) -> Result<NotificationEvent, NotifyParseError> {
    if line.len() > MAX_LINE_BYTES {
        return Err(NotifyParseError::TooLong);
    }
    let mut ev: NotificationEvent = serde_json::from_str(line)?;
    if ev.kind.is_empty() {
        return Err(NotifyParseError::MissingKind);
    }
    if ev.kind.len() > KIND_MAX_BYTES {
        return Err(NotifyParseError::FieldTooLong("kind"));
    }
    if let Some(a) = &ev.animation_name {
        if a.len() > ANIMATION_NAME_MAX_BYTES {
            return Err(NotifyParseError::FieldTooLong("animation_name"));
        }
    }
    ev.label = ev.label.map(|s| truncate_text(&s, TEXT_MAX_BYTES));
    ev.body = ev.body.map(|s| truncate_text(&s, TEXT_MAX_BYTES));
    Ok(ev)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib notification::tests`
Expected: PASS (10 tests).

- [ ] **Step 6: Commit**

```bash
git add src/notification.rs src/lib.rs
git commit -m "feat(notification): event model, presets, clamps, UTF-8-safe wire parser"
```

---

### Task 2: `clap` CLI types (`notify` subcommand)

**Files:**
- Modify: `Cargo.toml`, `src/notification.rs`

- [ ] **Step 1: Add the `clap` dependency**

In `Cargo.toml` under `[dependencies]`, add:

```toml
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 2: Write the failing tests**

Add to `src/notification.rs` tests module:

```rust
    use clap::Parser;

    #[test]
    fn cli_parses_notify_with_kind() {
        let cli = Cli::try_parse_from(["happy-cappy", "notify", "--kind", "running"]).unwrap();
        let Some(Command::Notify(args)) = cli.command else { panic!("expected notify") };
        assert_eq!(args.kind, "running");
        let ev = args.to_event();
        assert_eq!(ev.kind, "running");
        assert_eq!(ev.ttl_ms, None);
    }

    #[test]
    fn cli_notify_converts_ttl_seconds_to_ms() {
        let cli = Cli::try_parse_from(["happy-cappy", "notify", "--kind", "failed", "--ttl", "30"]).unwrap();
        let Some(Command::Notify(args)) = cli.command else { panic!() };
        assert_eq!(args.to_event().ttl_ms, Some(30_000));
    }

    #[test]
    fn cli_no_subcommand_is_none() {
        let cli = Cli::try_parse_from(["happy-cappy"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn cli_notify_requires_kind() {
        assert!(Cli::try_parse_from(["happy-cappy", "notify"]).is_err());
    }

    #[test]
    fn cli_notify_rejects_nonnumeric_ttl() {
        assert!(Cli::try_parse_from(["happy-cappy", "notify", "--kind", "x", "--ttl", "soon"]).is_err());
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib notification::tests::cli_ 2>&1 | head -20`
Expected: FAIL — `Cli`/`Command`/`NotifyArgs` not defined.

- [ ] **Step 4: Implement the CLI types**

Add to `src/notification.rs` (above the test module):

```rust
#[derive(clap::Parser, Debug)]
#[command(name = "happy-cappy", about = "Happy Cappy desktop companion")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Send a notification to the running pet over its control socket.
    Notify(NotifyArgs),
}

#[derive(clap::Args, Debug)]
pub struct NotifyArgs {
    /// Event kind, e.g. running | succeeded | failed | needs-review | message (open string).
    #[arg(long)]
    pub kind: String,
    /// Explicit animation name override (else `notify-<kind>`).
    #[arg(long)]
    pub animation: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long)]
    pub body: Option<String>,
    /// Time-to-live in seconds (converted to ms).
    #[arg(long)]
    pub ttl: Option<u64>,
    #[arg(long)]
    pub priority: Option<i32>,
}

impl NotifyArgs {
    pub fn to_event(&self) -> NotificationEvent {
        NotificationEvent {
            kind: self.kind.clone(),
            animation_name: self.animation.clone(),
            label: self.label.clone(),
            body: self.body.clone(),
            ttl_ms: self.ttl.map(|s| s.saturating_mul(1000)),
            priority: self.priority,
        }
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib notification::tests`
Expected: PASS (CLI tests + Task 1 tests).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/notification.rs
git commit -m "feat(cli): clap notify subcommand types + event conversion"
```

---

### Task 3: Dynamic resolver helper

**Files:**
- Modify: `src/pet/resolver.rs`

- [ ] **Step 1: Write the failing tests**

Add to `src/pet/resolver.rs` tests module (it already has `fixture_manifest(names)`):

```rust
    #[test]
    fn dynamic_lookup_returns_first_present_name() {
        let manifest = fixture_manifest(&["idle", "notify-running"]);
        let (name, _) = lookup_with_fallback_dynamic(
            &manifest,
            &["notify-deploy", "notify-running", "notify-message", "idle"],
        );
        assert_eq!(name, "notify-running");
    }

    #[test]
    fn dynamic_lookup_falls_back_to_idle() {
        let manifest = fixture_manifest(&["idle"]);
        let (name, _) = lookup_with_fallback_dynamic(&manifest, &["notify-running", "notify-message"]);
        assert_eq!(name, "idle");
    }

    #[test]
    fn dynamic_lookup_honors_runtime_string_first() {
        let manifest = fixture_manifest(&["idle", "notify-custom"]);
        let requested = format!("notify-{}", "custom");
        let (name, _) = lookup_with_fallback_dynamic(&manifest, &[requested.as_str(), "idle"]);
        assert_eq!(name, "notify-custom");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib pet::resolver::tests::dynamic_lookup_`
Expected: FAIL — `lookup_with_fallback_dynamic` not defined.

- [ ] **Step 3: Implement the dynamic helper**

Add to `src/pet/resolver.rs` (next to `lookup_with_fallback`):

```rust
/// Like `lookup_with_fallback`, but accepts runtime `&str` names (e.g. `notify-<kind>`,
/// a CLI-supplied `animation_name`) and returns an owned resolved name. The `&'static`
/// version stays for the enum-driven behavior chains.
pub fn lookup_with_fallback_dynamic<'a>(
    manifest: &'a PetManifest,
    chain: &[&str],
) -> (String, &'a Animation) {
    for &name in chain {
        if let Some(anim) = manifest.animations.get(name) {
            return (name.to_string(), anim);
        }
    }
    let idle = manifest
        .animations
        .get("idle")
        .expect("manifest validation guarantees 'idle' exists");
    ("idle".to_string(), idle)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib pet::resolver::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/pet/resolver.rs
git commit -m "feat(resolver): dynamic lookup_with_fallback for runtime animation names"
```

---

### Task 4: Runtime `NotificationState` — set/clear/preempt (resolve-once)

**Files:**
- Modify: `src/pet/runtime.rs`

- [ ] **Step 1: Write the failing tests**

Add to the runtime tests module. These use a fixture manifest that includes `notify-*` animations:

```rust
fn notify_fixture() -> PetRuntime {
    use crate::pet::manifest::{Animation, FrameGeometry, PetManifest};
    use std::collections::BTreeMap;
    let mut animations = BTreeMap::new();
    animations.insert("idle".to_string(), Animation::from_indices(&[0, 1, 2, 3]));
    animations.insert("notify-running".to_string(), Animation::from_indices(&[4, 5]));
    animations.insert("notify-failed".to_string(), Animation::from_indices(&[6, 7]));
    let manifest = PetManifest {
        manifest_version: 1,
        id: "fixture".into(),
        display_name: "Fixture".into(),
        spritesheet_path: "x.png".into(),
        frame: FrameGeometry { width: 16, height: 16, columns: 8, rows: 1 },
        animations,
    };
    PetRuntime::new_with_manifest(manifest)
}

fn event(kind: &str) -> crate::notification::NotificationEvent {
    crate::notification::NotificationEvent {
        kind: kind.to_string(),
        animation_name: None,
        label: None,
        body: None,
        ttl_ms: None,
        priority: None,
    }
}

#[test]
fn fresh_runtime_has_no_notification() {
    // Pet swap builds a fresh runtime (app::activate_pet), so this is the swap-clears invariant.
    assert_eq!(PetRuntime::new().notification_animation(), None);
}

#[test]
fn set_notification_resolves_kind_animation() {
    let mut pet = notify_fixture();
    pet.set_notification(&event("running"));
    assert_eq!(pet.notification_animation(), Some("notify-running"));
}

#[test]
fn set_notification_falls_back_when_animation_absent() {
    let mut pet = notify_fixture(); // has no notify-review
    pet.set_notification(&event("needs-review"));
    // chain: notify-review -> notify-message(absent) -> notify-running(present)
    assert_eq!(pet.notification_animation(), Some("notify-running"));
}

#[test]
fn higher_priority_preempts_lower() {
    let mut pet = notify_fixture();
    pet.set_notification(&event("running")); // priority 10
    pet.set_notification(&event("failed")); // priority 90 -> preempts
    assert_eq!(pet.notification_animation(), Some("notify-failed"));
}

#[test]
fn lower_priority_is_ignored() {
    let mut pet = notify_fixture();
    pet.set_notification(&event("failed")); // 90
    pet.set_notification(&event("running")); // 10 -> ignored
    assert_eq!(pet.notification_animation(), Some("notify-failed"));
}

#[test]
fn explicit_priority_is_clamped() {
    let mut pet = notify_fixture();
    let mut ev = event("running");
    ev.priority = Some(10_000);
    pet.set_notification(&ev);
    pet.set_notification(&event("failed")); // 90 < clamped 100 -> ignored
    assert_eq!(pet.notification_animation(), Some("notify-running"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib pet::runtime::tests::set_notification 2>&1 | head -20`
Expected: FAIL — `notification` field / methods not defined.

- [ ] **Step 3: Add the field, struct, and methods**

In `src/pet/runtime.rs`, add the struct (near `PetRuntime`):

```rust
#[derive(Debug, Clone)]
struct NotificationState {
    animation_name: String,
    remaining: Duration,
    priority: i32,
    #[allow(dead_code)] // carried for logging + SP4-C; not rendered in SP4-B
    label: Option<String>,
    #[allow(dead_code)]
    body: Option<String>,
}
```

Add `notification: Option<NotificationState>,` to the `PetRuntime` struct fields, and initialize `notification: None,` in **every** `PetRuntime` constructor that builds the struct literal (`new_with_manifest` and `new_with_manifest_and_seed` — grep `Self {` / struct-literal sites in this file).

Add the public methods (place near `start_micro_action`):

```rust
    pub fn set_notification(&mut self, event: &crate::notification::NotificationEvent) {
        let (default_priority, default_ttl) = crate::notification::preset_for(&event.kind);
        let priority = crate::notification::clamp_priority(event.priority.unwrap_or(default_priority));
        let ttl_ms = crate::notification::clamp_ttl(event.ttl_ms.unwrap_or(default_ttl));

        // Preemption: lower priority than the active notification is ignored.
        if let Some(active) = &self.notification {
            if priority < active.priority {
                return;
            }
        }

        let requested = event
            .animation_name
            .clone()
            .unwrap_or_else(|| format!("notify-{}", event.kind));
        let notify_kind = format!("notify-{}", event.kind);
        let chain: [&str; 5] = [
            requested.as_str(),
            notify_kind.as_str(),
            "notify-message",
            "notify-running",
            "idle",
        ];
        let (resolved, _) = crate::pet::resolver::lookup_with_fallback_dynamic(&self.manifest, &chain);

        self.notification = Some(NotificationState {
            animation_name: resolved,
            remaining: Duration::from_millis(ttl_ms),
            priority,
            label: event.label.clone(),
            body: event.body.clone(),
        });
        self.refresh_behavior_mode();
    }

    pub fn clear_notification(&mut self) {
        self.notification = None;
        self.refresh_behavior_mode();
    }

    #[cfg(test)]
    pub fn notification_animation(&self) -> Option<&str> {
        self.notification.as_ref().map(|n| n.animation_name.as_str())
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib pet::runtime::tests`
Expected: PASS — set/resolve/preempt tests + all prior runtime tests. (Selection/animation not wired yet; `refresh_behavior_mode` still ignores `notification` — that is Task 5. These tests only assert on stored state.)

- [ ] **Step 5: Commit**

```bash
git add src/pet/runtime.rs
git commit -m "feat(runtime): NotificationState with resolve-once + priority preemption"
```

---

### Task 5: `BehaviorMode::Notifying` + pinned animation selection

**Files:**
- Modify: `src/pet/runtime.rs`

- [ ] **Step 1: Write the failing tests**

Add to the runtime tests module:

```rust
#[test]
fn notifying_pins_resolved_animation() {
    let mut pet = notify_fixture();
    pet.set_notification(&event("running"));
    assert_eq!(pet.behavior_mode(), BehaviorMode::Notifying);
    assert_eq!(pet.current_animation_name(), "notify-running");
}

#[test]
fn drag_and_hover_outrank_notification() {
    let mut pet = notify_fixture();
    pet.set_notification(&event("running"));
    pet.set_hovered(true);
    assert_ne!(pet.behavior_mode(), BehaviorMode::Notifying);
    pet.set_hovered(false);
    assert_eq!(pet.behavior_mode(), BehaviorMode::Notifying); // resumes while TTL remains
    pet.set_dragging(true);
    assert_eq!(pet.behavior_mode(), BehaviorMode::Dragging);
}

#[test]
fn notification_outranks_micro_action_and_walk() {
    let mut pet = notify_fixture();
    pet.start_micro_action(crate::micro_action::MicroAction::CheerUp);
    pet.set_notification(&event("running"));
    assert_eq!(pet.behavior_mode(), BehaviorMode::Notifying);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib pet::runtime::tests::notifying_ pet::runtime::tests::drag_and_hover pet::runtime::tests::notification_outranks 2>&1 | head -20`
Expected: FAIL — `BehaviorMode::Notifying` does not exist.

- [ ] **Step 3: Add the mode and wire selection**

Add the variant to `BehaviorMode`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorMode {
    Default,
    Action,
    Hovered,
    Dragging,
    Walking,
    Hidden,
    Notifying,
}
```

In `refresh_behavior_mode`, update the priority chain to insert `Notifying` between `Hovered` and `Action`, and pin the notification's animation when in that mode. Replace the body so it reads:

```rust
    fn refresh_behavior_mode(&mut self) {
        self.behavior_mode = if self.hidden {
            BehaviorMode::Hidden
        } else if self.dragging {
            BehaviorMode::Dragging
        } else if self.hovered {
            BehaviorMode::Hovered
        } else if self.notification.is_some() {
            BehaviorMode::Notifying
        } else if self.action_override.is_some() {
            BehaviorMode::Action
        } else if self.state == PetState::Walk && self.movement_enabled() {
            BehaviorMode::Walking
        } else {
            BehaviorMode::Default
        };

        // Notifying pins the name resolved once in set_notification (no re-resolution here).
        if self.behavior_mode == BehaviorMode::Notifying {
            if let Some(name) = self.notification.as_ref().map(|n| n.animation_name.clone()) {
                self.set_selected_animation(&name);
            }
            return;
        }

        let chain = resolve_animation_chain(
            self.behavior_mode,
            self.personality,
            self.expression_index,
            self.action_override.map(|a| a.action()),
        );
        let (name, _) = lookup_with_fallback(&self.manifest, chain);
        self.set_selected_animation(name);
    }
```

(`set_selected_animation` from SP4-A handles the lifecycle entry-reset.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib pet::runtime::tests`
Expected: PASS — pin + priority tests and all prior tests.

- [ ] **Step 5: Commit**

```bash
git add src/pet/runtime.rs
git commit -m "feat(runtime): Notifying behavior mode pins resolved notification animation"
```

---

### Task 6: TTL countdown (incl. hidden) + one-shot completion clears notification

**Files:**
- Modify: `src/pet/runtime.rs`

- [ ] **Step 1: Write the failing tests**

Add to the runtime tests module:

```rust
fn notify_oneshot_fixture() -> PetRuntime {
    use crate::pet::manifest::{Animation, Frame, FrameGeometry, PetManifest};
    use std::collections::BTreeMap;
    let mut animations = BTreeMap::new();
    animations.insert("idle".to_string(), Animation::from_indices(&[0, 1, 2, 3]));
    animations.insert("notify-running".to_string(), Animation::from_indices(&[4, 5]));
    animations.insert(
        "notify-success".to_string(),
        Animation {
            frames: vec![Frame { index: 6, ms: Some(50) }, Frame { index: 7, ms: Some(50) }],
            loop_start: None,
            fallback: Some("idle".to_string()),
            one_shot: true,
        },
    );
    let manifest = PetManifest {
        manifest_version: 1,
        id: "fixture".into(),
        display_name: "Fixture".into(),
        spritesheet_path: "x.png".into(),
        frame: FrameGeometry { width: 16, height: 16, columns: 8, rows: 1 },
        animations,
    };
    PetRuntime::new_with_manifest(manifest)
}

#[test]
fn ttl_expires_and_clears_notification() {
    let mut pet = notify_fixture();
    let mut ev = event("running");
    ev.ttl_ms = Some(100);
    pet.set_notification(&ev);
    pet.tick(Duration::from_millis(60));
    assert!(pet.notification_animation().is_some());
    pet.tick(Duration::from_millis(60)); // total 120 > 100
    assert_eq!(pet.notification_animation(), None);
}

#[test]
fn ttl_counts_down_while_hidden() {
    let mut pet = notify_fixture();
    let mut ev = event("running");
    ev.ttl_ms = Some(100);
    pet.set_notification(&ev);
    pet.set_hidden(true);
    pet.tick(Duration::from_millis(120));
    assert_eq!(pet.notification_animation(), None, "TTL must keep counting while hidden");
}

#[test]
fn one_shot_notification_clears_on_completion_before_ttl() {
    let mut pet = notify_oneshot_fixture();
    let mut ev = crate::notification::NotificationEvent {
        kind: "succeeded".to_string(),
        animation_name: Some("notify-success".to_string()),
        label: None,
        body: None,
        ttl_ms: Some(60_000), // long TTL; one-shot should end it sooner
        priority: None,
    };
    ev.kind = "succeeded".to_string();
    pet.set_notification(&ev);
    assert_eq!(pet.current_animation_name(), "notify-success");
    pet.tick(Duration::from_millis(50)); // frame 0 done
    let t = pet.tick(Duration::from_millis(50)); // final frame full duration -> completion
    assert!(t.oneshot_completed);
    assert_eq!(pet.notification_animation(), None, "one-shot completion clears the notification");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib pet::runtime::tests::ttl_ pet::runtime::tests::one_shot_notification 2>&1 | head -20`
Expected: FAIL — no TTL countdown / completion handling yet.

- [ ] **Step 3: Add TTL countdown and completion handling in `tick`**

In `tick`, immediately after the `action_override` block and **before** the `if self.hidden` early-return, add:

```rust
        // Notification TTL counts down in every state (hidden / drag / hover included).
        if let Some(n) = self.notification.as_mut() {
            n.remaining = n.remaining.saturating_sub(dt);
            if n.remaining.is_zero() {
                self.notification = None;
                self.refresh_behavior_mode();
            }
        }
```

The `advance_animation()` call (now returns `bool` from SP4-A) is captured; right after it (before the final `refresh_behavior_mode()`), add the one-shot clear:

```rust
        let oneshot_completed = self.advance_animation();
        // One-shot notify animation finished -> notification owner clears itself
        // (does NOT consult the manifest `fallback`). Ordering: advance -> consume -> refresh.
        if oneshot_completed && self.notification.is_some() {
            self.notification = None;
        }
```

(The final `self.refresh_behavior_mode();` then drops `Notifying` and the behavior chain resumes.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib pet::runtime::tests`
Expected: PASS — TTL expiry, hidden countdown, one-shot clear, all prior tests.

- [ ] **Step 5: Commit**

```bash
git add src/pet/runtime.rs
git commit -m "feat(runtime): notification TTL countdown (incl hidden) + one-shot clear"
```

---

### Task 7: `AppCommand::Notify` + handler

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Write the failing test**

Add to the `app` tests module, following the existing pattern (`DesktopPetApp::new_for_test()`, `app.settings_path = None`, direct `app.pet` access — see the `non_quit_command_*` tests):

```rust
    #[test]
    fn notify_command_enters_notifying_mode() {
        let mut app = DesktopPetApp::new_for_test();
        app.settings_path = None;
        let ev = crate::notification::NotificationEvent {
            kind: "running".to_string(),
            animation_name: None,
            label: None,
            body: None,
            ttl_ms: None,
            priority: None,
        };
        // Returns true (non-quit). Asserting on the behavior MODE (not a specific animation)
        // keeps this independent of whether the bundled manifest defines notify-running yet.
        assert!(app.handle_non_quit_command_for_test(AppCommand::Notify(ev)));
        assert_eq!(app.pet.behavior_mode(), crate::pet::BehaviorMode::Notifying);
    }
```

No new accessor is needed: `app.pet` is reachable from the in-module tests (other tests already call `app.pet.behavior_mode()`), and `new_for_test()` builds the pet in the constructor (no `resumed()` required).

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib app::tests::notify_command 2>&1 | head -20`
Expected: FAIL — `AppCommand::Notify` not defined.

- [ ] **Step 3: Add the variant and handler**

Add to the `AppCommand` enum in `src/app.rs`:

```rust
    Notify(crate::notification::NotificationEvent),
```

In `handle_non_quit_command`, add an arm mirroring the `Nap` handler:

```rust
            AppCommand::Notify(event) => {
                self.pet.set_notification(&event);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
```

`AppCommand` must remain usable as the winit user-event type. `NotificationEvent` already derives `Debug`/`Clone`; confirm `AppCommand` still compiles (it does not require `Copy`).

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --lib app::tests::notify_command`
Expected: PASS (the pet enters `Notifying`; with the fixture/bundled manifest the animation resolves through the fallback chain).

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): handle AppCommand::Notify by setting a pet notification"
```

---

### Task 8: `app_support_dir()` helper

**Files:**
- Modify: `src/settings.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `settings` tests module:

```rust
    #[test]
    fn app_support_dir_is_happy_cappy_root() {
        let path = app_support_dir().unwrap();
        assert!(path.ends_with("Library/Application Support/Happy Cappy"));
    }

    #[test]
    fn settings_and_pets_paths_live_under_app_support_dir() {
        let root = app_support_dir().unwrap();
        assert_eq!(default_settings_path().unwrap(), root.join("settings.json"));
        assert_eq!(custom_pets_dir().unwrap(), root.join("pets"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib settings::tests::app_support_dir settings::tests::settings_and_pets 2>&1 | head -20`
Expected: FAIL — `app_support_dir` not defined.

- [ ] **Step 3: Add the helper and reuse it**

In `src/settings.rs`, add:

```rust
pub fn app_support_dir() -> Result<PathBuf, SettingsError> {
    let home = std::env::var_os("HOME").ok_or(SettingsError::MissingHomeDirectory)?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Happy Cappy"))
}
```

Refactor `default_settings_path` and `custom_pets_dir` to build on it:

```rust
pub fn default_settings_path() -> Result<PathBuf, SettingsError> {
    Ok(app_support_dir()?.join("settings.json"))
}

pub fn custom_pets_dir() -> Result<PathBuf, SettingsError> {
    Ok(app_support_dir()?.join("pets"))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib settings::tests`
Expected: PASS — new tests + existing path tests (`custom_pets_dir_lives_under_happy_cappy_app_support` etc.).

- [ ] **Step 5: Commit**

```bash
git add src/settings.rs
git commit -m "refactor(settings): shared app_support_dir() helper"
```

---

### Task 9: `control_socket` — bind seam, stale cleanup, single-instance

**Files:**
- Create: `src/control_socket.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Register the module**

In `src/lib.rs`, add:

```rust
pub mod control_socket;
```

- [ ] **Step 2: Write the failing tests**

Create `src/control_socket.rs` with the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn binds_clean_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("control.sock");
        match bind_control_socket(&path) {
            BindOutcome::Bound(_listener) => {}
            other => panic!("expected Bound, got {other:?}"),
        }
    }

    #[test]
    fn detects_already_running() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let _live = match bind_control_socket(&path) {
            BindOutcome::Bound(l) => l, // keep the listener alive
            other => panic!("expected Bound, got {other:?}"),
        };
        match bind_control_socket(&path) {
            BindOutcome::AlreadyRunning => {}
            other => panic!("expected AlreadyRunning, got {other:?}"),
        }
    }

    #[test]
    fn rebinds_over_stale_socket() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("control.sock");
        // Create a stale socket file by binding then dropping the listener.
        drop(match bind_control_socket(&path) {
            BindOutcome::Bound(l) => l,
            other => panic!("expected Bound, got {other:?}"),
        });
        // File may linger; a fresh bind should detect it's dead and rebind.
        match bind_control_socket(&path) {
            BindOutcome::Bound(_) => {}
            other => panic!("expected Bound after stale cleanup, got {other:?}"),
        }
    }

    #[test]
    fn roundtrips_event_through_send_and_parse() {
        use crate::notification::NotificationEvent;
        let dir = tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let listener = match bind_control_socket(&path) {
            BindOutcome::Bound(l) => l,
            other => panic!("{other:?}"),
        };
        let ev = NotificationEvent {
            kind: "running".into(),
            animation_name: None,
            label: Some("hi".into()),
            body: None,
            ttl_ms: Some(1000),
            priority: None,
        };
        let send_path = path.clone();
        let ev_clone = ev.clone();
        let sender = std::thread::spawn(move || send_notify(&send_path, &ev_clone).unwrap());
        let (stream, _) = listener.accept().unwrap();
        let got = read_event(stream).unwrap();
        sender.join().unwrap();
        assert_eq!(got, ev);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib control_socket::tests 2>&1 | head -20`
Expected: FAIL — items not defined.

- [ ] **Step 4: Implement bind + send + read (no thread yet)**

Prepend to `src/control_socket.rs`:

```rust
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use crate::notification::{parse_notify_line, NotificationEvent, MAX_LINE_BYTES};

#[derive(Debug)]
pub enum BindOutcome {
    Bound(UnixListener),
    AlreadyRunning,
    Failed(std::io::Error),
}

pub fn control_socket_path() -> Result<PathBuf, crate::settings::SettingsError> {
    Ok(crate::settings::app_support_dir()?.join("control.sock"))
}

fn set_socket_perms(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o600);
        let _ = std::fs::set_permissions(path, perms);
    }
}

/// Bind the control socket. On a pre-existing path, probe whether a live instance
/// holds it (→ `AlreadyRunning`) or it is a stale file (→ unlink + rebind).
/// Returns an outcome rather than exiting, so tests never call `process::exit`.
pub fn bind_control_socket(path: &Path) -> BindOutcome {
    match UnixListener::bind(path) {
        Ok(listener) => {
            set_socket_perms(path);
            BindOutcome::Bound(listener)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => match UnixStream::connect(path) {
            Ok(_) => BindOutcome::AlreadyRunning,
            Err(_) => {
                let _ = std::fs::remove_file(path);
                match UnixListener::bind(path) {
                    Ok(listener) => {
                        set_socket_perms(path);
                        BindOutcome::Bound(listener)
                    }
                    Err(e) => BindOutcome::Failed(e),
                }
            }
        },
        Err(e) => BindOutcome::Failed(e),
    }
}

/// Read + parse a single bounded event line from a connection.
pub fn read_event(stream: UnixStream) -> Result<NotificationEvent, Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream).take(MAX_LINE_BYTES as u64 + 1);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(parse_notify_line(line.trim_end())?)
}

/// Client: connect and write one JSON event line.
pub fn send_notify(path: &Path, event: &NotificationEvent) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(path)?;
    let mut json = serde_json::to_string(event).expect("NotificationEvent serializes");
    json.push('\n');
    stream.write_all(json.as_bytes())
}
```

`BufReader::take` needs `std::io::Read` in scope via the `Read` trait — `BufReader` + `.take()` comes from `std::io::Read`; add `use std::io::Read;` to the import list.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib control_socket::tests`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add src/control_socket.rs src/lib.rs
git commit -m "feat(control_socket): bind seam, stale cleanup, bounded read, client send"
```

---

### Task 10: Listener thread + `main.rs` wiring (client vs GUI + degraded mode)

**Files:**
- Modify: `src/control_socket.rs`, `src/main.rs`

- [ ] **Step 1: Add the listener thread spawner**

In `src/control_socket.rs`, add (no unit test — exercised by the manual smoke in Task 13 and the `read_event` test above):

```rust
use log::warn;
use winit::event_loop::EventLoopProxy;

use crate::app::AppCommand;

/// Spawn a background thread that accepts connections and forwards parsed
/// events into the winit loop. The thread only ever *sends* events.
pub fn spawn_listener(listener: UnixListener, proxy: EventLoopProxy<AppCommand>) {
    std::thread::spawn(move || {
        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => match read_event(stream) {
                    Ok(event) => {
                        let _ = proxy.send_event(AppCommand::Notify(event));
                    }
                    Err(e) => warn!("control socket: dropping bad event line: {e}"),
                },
                Err(e) => warn!("control socket: accept error: {e}"),
            }
        }
    });
}
```

- [ ] **Step 2: Rewrite `main.rs` for client vs GUI**

Replace `src/main.rs` with:

```rust
use clap::Parser;
use happy_cappy::app::{AppCommand, DesktopPetApp};
use happy_cappy::control_socket::{
    bind_control_socket, control_socket_path, send_notify, spawn_listener, BindOutcome,
};
use happy_cappy::notification::{Cli, Command};
use log::warn;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Client path: `happy-cappy notify ...` sends one event to the running app and exits.
    let cli = Cli::parse();
    if let Some(Command::Notify(args)) = cli.command {
        let path = control_socket_path()?;
        return match send_notify(&path, &args.to_event()) {
            Ok(()) => Ok(()),
            Err(e) => {
                eprintln!("happy-cappy: could not reach a running pet ({e}). Is the app open?");
                std::process::exit(1);
            }
        };
    }

    // Server/GUI path.
    let event_loop = EventLoop::<AppCommand>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();

    // Control socket: bind BEFORE GUI init so the single-instance check happens first.
    match control_socket_path() {
        Ok(path) => {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match bind_control_socket(&path) {
                BindOutcome::Bound(listener) => spawn_listener(listener, proxy.clone()),
                BindOutcome::AlreadyRunning => {
                    eprintln!("happy-cappy: another instance is already running.");
                    std::process::exit(0);
                }
                BindOutcome::Failed(e) => {
                    warn!("control socket unavailable ({e}); continuing without external triggers");
                }
            }
        }
        Err(e) => warn!("cannot resolve control socket path ({e}); external triggers disabled"),
    }

    let mut app = DesktopPetApp::new(proxy);
    event_loop.run_app(&mut app)?;
    Ok(())
}
```

- [ ] **Step 3: Build and verify the binary**

Run: `cargo build`
Expected: compiles (clap `Parser` in scope, `Cli`/`Command` re-exported from `notification`).

Run: `cargo run -- notify --kind running 2>&1 | head -5`
Expected: with no app running, prints `could not reach a running pet ...` and exits non-zero.

- [ ] **Step 4: Run the full suite**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/control_socket.rs src/main.rs
git commit -m "feat(control_socket): listener thread + main.rs client/GUI + degraded mode"
```

---

### Task 11: Bundled manifest `notify-*` animations (reuse frames)

**Files:**
- Modify: `assets/manifests/happy_cappy.json`

- [ ] **Step 1: Write the failing test**

Add to `src/pet/manifest.rs` tests:

```rust
#[test]
fn bundled_manifest_defines_notify_animations() {
    let manifest = PetManifest::load_embedded_happy_cappy();
    for name in ["notify-running", "notify-success", "notify-failed", "notify-review", "notify-message"] {
        assert!(manifest.animations.contains_key(name), "missing {name}");
    }
    // notify-success is a one-shot.
    assert!(manifest.animations["notify-success"].one_shot);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib pet::manifest::tests::bundled_manifest_defines_notify_animations`
Expected: FAIL — animations missing.

- [ ] **Step 3: Add `notify-*` animations reusing existing sprite indices**

In `assets/manifests/happy_cappy.json`, add these entries to the `animations` object (the sheet is 4×10 = 40 frames; these reuse existing rows — `happy` 8-11, `curious` 12-15, `sleepy` 16-19, `blink` 4-7):

```jsonc
    "notify-running":  { "frames": [12, 13, 14, 15] },
    "notify-success":  { "frames": [{ "index": 8, "ms": 90 }, { "index": 9, "ms": 90 },
                                    { "index": 10, "ms": 120 }, { "index": 11, "ms": 260 }],
                         "oneShot": true, "fallback": "idle" },
    "notify-failed":   { "frames": [16, 17, 18, 19] },
    "notify-review":   { "frames": [12, 13, 14, 15] },
    "notify-message":  { "frames": [4, 5, 6, 7] }
```

(Mapping is illustrative per the spec — adjust indices to taste, but keep every index `< 40` and `notify-success` a one-shot.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib pet::manifest::tests`
Expected: PASS — bundled manifest still parses/validates and now defines `notify-*`. (Required-key validation is unchanged; `notify-*` are not required.)

- [ ] **Step 5: Commit**

```bash
git add assets/manifests/happy_cappy.json src/pet/manifest.rs
git commit -m "feat(assets): bundled notify-* animations reusing existing frames"
```

---

### Task 12: Dev-agent preset integration test (generic)

**Files:**
- Modify: `src/pet/runtime.rs`

- [ ] **Step 1: Write the test**

Add to the runtime tests module (uses the bundled manifest — generic, no agent names in code):

```rust
#[test]
fn dev_agent_flow_running_then_succeeded_transitions_animation() {
    let mut pet = PetRuntime::new(); // bundled manifest with notify-*
    pet.set_notification(&event("running"));
    assert_eq!(pet.current_animation_name(), "notify-running");

    // Build "succeeds": a one-shot success preempts (priority 30 > running 10) and,
    // after playing once, clears -> behavior chain resumes.
    let mut ok = event("succeeded");
    ok.animation_name = Some("notify-success".to_string());
    pet.set_notification(&ok);
    assert_eq!(pet.current_animation_name(), "notify-success");

    // Advance through the one-shot (4 frames: 90+90+120+260 ms).
    pet.tick(Duration::from_millis(90));
    pet.tick(Duration::from_millis(90));
    pet.tick(Duration::from_millis(120));
    let t = pet.tick(Duration::from_millis(260));
    assert!(t.oneshot_completed);
    assert_eq!(pet.notification_animation(), None);
    assert_ne!(pet.behavior_mode(), BehaviorMode::Notifying);
}
```

- [ ] **Step 2: Run the test to verify it passes**

Run: `cargo test --lib pet::runtime::tests::dev_agent_flow`
Expected: PASS (this is an end-to-end model check on top of Tasks 4–6 + 11; if it fails, the failure points at a real gap).

- [ ] **Step 3: Commit**

```bash
git add src/pet/runtime.rs
git commit -m "test(runtime): generic dev-agent running->succeeded notification flow"
```

---

### Task 13: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Full suite + lint + format**

Run: `cargo test`
Expected: PASS.

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: clean.

Run: `cargo fmt --check`
Expected: clean (run `cargo fmt` if not).

Run: `./scripts/verify.sh`
Expected: PASS.

- [ ] **Step 2: Manual smoke (real artifact paths — binary is not on PATH)**

Build: `cargo build --release` (then `BIN=target/release/happy-cappy`) — or `./scripts/build_app.sh` (then `BIN="dist/Happy Cappy.app/Contents/MacOS/happy-cappy"`).

Launch the app (`"$BIN"` with no args, or open the bundle). From a second terminal:

```bash
"$BIN" notify --kind running --label "Building…" --ttl 30   # pet -> notify-running
"$BIN" notify --kind succeeded                              # success plays once, returns to idle
"$BIN" notify --kind failed --body "3 tests failed"         # failed preempts running (higher priority)
```

Verify: dragging the pet during a notification shows the drag animation and the notify animation resumes on release; quitting and re-running `"$BIN" notify ...` with no app open prints a clear error and exits non-zero; starting a second app instance exits with "another instance is already running".

- [ ] **Step 3: Confirm no regressions**

Verify SP2/SP3 surfaces still work: menu bar **Pet ▸** quick-swap, the Pet Library picker, the Settings window, focus mode, Nap/Cheer Up, and pet drag all behave as before, and swapping pets while a notification is active clears the notification (the pet shows the new pet's idle, not the prior notify animation).

---

## Self-Review notes

- **Spec coverage:** event model + presets + caps + parser (Task 1, §2/§6), CLI (Task 2, §4.3), dynamic resolver (Task 3, §2/§9), NotificationState + resolve-once + preempt (Task 4, §2/§3.2), Notifying pin + priority (Task 5, §3.3), TTL incl hidden + one-shot clear ordering (Task 6, §3.1/§3.1a), AppCommand wiring (Task 7, §4.1), app_support_dir (Task 8, §4.2), bind seam + stale + single-instance (Task 9, §4.2), listener + main client/GUI + degraded (Task 10, §4.1/§4.3/§6), bundled notify-* (Task 11, §5), dev-agent preset test (Task 12, §7), exit criteria + smoke (Task 13, §8). Pet-swap-clears (§3.4) is the `fresh_runtime_has_no_notification` invariant (Task 4) + Task 13 regression check (activate_pet builds a fresh `PetRuntime`).
- **Type consistency:** `NotificationEvent`, `parse_notify_line`, `preset_for`, `clamp_priority`/`clamp_ttl`, `truncate_text`, `Cli`/`Command`/`NotifyArgs::to_event`, `lookup_with_fallback_dynamic`, `NotificationState`, `set_notification`/`clear_notification`/`notification_animation`, `BehaviorMode::Notifying`, `BindOutcome`/`bind_control_socket`/`control_socket_path`/`read_event`/`send_notify`/`spawn_listener`, `AppCommand::Notify`, `app_support_dir` — each defined where first used and reused verbatim downstream. Depends on SP4-A's `PetTick.oneshot_completed`, `set_selected_animation` (entry-reset), `is_lifecycle`, `Animation::from_indices`.
- **Ordering guard:** the one-shot clear sits between `advance_animation()` and the final `refresh_behavior_mode()` in `tick` (SP4-A §5.3 ordering); the TTL countdown sits before the hidden early-return so it never stalls.
