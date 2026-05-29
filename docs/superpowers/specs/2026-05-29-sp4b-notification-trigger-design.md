# SP4-B — Notification Model + Unix Socket + CLI Trigger — Design

> Part of the multi-pet platform roadmap (umbrella: `2026-05-26-pet-manifest-refactor-design.md`).
> Sub-project 4, second of three specs. **Depends on SP4-A** (animation lifecycle engine).
> - SP4-A: animation engine (manifest v2, one-shot, fallback, loop_start, per-frame ms).
> - **SP4-B (this spec):** generic local notification model + Unix-socket transport + `notify` CLI subcommand. Pet reacts by **changing animation** for the notification's TTL. `label`/`body` are carried + logged but **not rendered**.
> - SP4-C: speech-bubble UI rendering `label`/`body` (deferred; separate spec).

## 1. Context & intent

Give external local processes a way to poke the running pet so it acts as an ambient status indicator. The core model stays **generic** — it does not bind to Claude/Codex/build-tool names. Well-known dev-agent event kinds are wired as presets and exercised by a first integration test; concrete adapters (Claude Code, Codex, build scripts) live on top later and are out of scope here.

Explicitly **not** in SP4-B: macOS notification mirroring, cloud/Hermes triggers, any text/overlay UI, speech bubbles, `NSView`/`NSPanel`/`NSWindow` additions.

## 2. Event model (`src/notification.rs`, pure Rust)

```rust
pub struct NotificationEvent {
    pub kind: String,                  // open string: running|succeeded|failed|needs-review|message|...
    pub animation_name: Option<String>,// override; None -> "notify-<kind>"
    pub label: Option<String>,         // stored + logged, NOT rendered (SP4-C)
    pub body: Option<String>,          // stored + logged, NOT rendered (SP4-C)
    pub ttl_ms: Option<u64>,           // None -> preset default for kind
    pub priority: Option<i32>,         // None -> preset default for kind
}
```

`kind` is an **open string** to stay generic. A preset table in code supplies defaults for known kinds; unknown kinds fall back to the `message` preset:

| kind | default animation | priority | default TTL | one-shot? |
|---|---|---|---|---|
| `running` | `notify-running` | 10 | 180 s | loop |
| `message` | `notify-message` | 20 | 10 s | loop |
| `needs-review` | `notify-review` | 30 | 120 s | loop |
| `succeeded` | `notify-success` | 40 | 8 s | one-shot |
| `failed` | `notify-failed` | 50 | 30 s | loop |
| *(unknown)* | `notify-message` | 20 | 10 s | loop |

(one-shot vs loop is a property of the manifest animation, per SP4-A — the table column documents the intended authoring, not a model field.)

### Animation resolution

The requested animation name is `animation_name` if set, else `notify-<kind>`. The runtime resolves it against the manifest using SP4-A's `lookup_with_fallback` with the chain:

```
[ requested, "notify-<kind>", "notify-message", "notify-running", "idle" ]
```

So a pet missing `notify-*` animations still does something sensible (eventually `idle`). Custom pets may define their own `notify-*` animations.

## 3. Runtime state (`src/pet/runtime.rs`)

Add a field `notification: Option<NotificationState>` alongside the existing `action_override`:

```rust
struct NotificationState {
    animation_name: String,   // resolved name
    remaining: Duration,      // TTL countdown
    priority: i32,
}
```

### 3.1 Lifecycle (in `tick`)

- Decrement `remaining` by `dt`; when it hits zero, clear the notification.
- If the resolved animation is `one_shot` (SP4-A) and the engine reports it **completed** before the TTL elapses, clear the notification immediately (a "success" plays its celebration once and is done). Looping animations run for the full TTL.
- Producers extend a long-running state by re-sending (see preemption: equal priority = latest-wins, which resets the TTL).

### 3.2 Preemption (decided)

When a new `NotificationEvent` arrives while one is active:

- **higher** priority → replaces current, resets TTL;
- **equal** priority → latest-wins (replaces, resets TTL);
- **lower** priority → ignored; the active notification continues.

### 3.3 Behavior-mode priority (decided: interaction always wins)

`refresh_behavior_mode` priority order becomes:

```
Hidden > Dragging > Hovered > Notifying > Action(micro) > Walking > Default
```

- A new `BehaviorMode::Notifying` is selected only when `notification.is_some()` and no higher state applies.
- Dragging/hovering takes over immediately (shows drag/hover animation); when interaction ends, if TTL remains the notify animation resumes.
- Notification preempts micro-actions (Nap/CheerUp) and walking/idle.
- `resolve_animation_chain` gains a `Notifying` branch returning the resolved-name chain from §2.

## 4. Transport: Unix socket + `notify` CLI (single binary)

### 4.1 Socket server (GUI process)

- Path: `~/Library/Application Support/Happy Cappy/control.sock` (same app-support dir as `pets/` and `settings.json`), permissions `0600`, local-only — no auth (same-user trust boundary).
- On `resumed()`, spawn a `std::thread` that binds a `UnixListener` and loops `accept()`. Each connection: read one line, `parse_notify_line(&str) -> Result<NotificationEvent, _>`, then `proxy.send_event(AppCommand::Notify(event))`.
- The event loop is already `EventLoop::<AppCommand>::with_user_event()`; SP4-B adds `AppCommand::Notify(NotificationEvent)`. `handle_non_quit_command` handles it by calling `self.pet.set_notification(...)` (which applies preemption from §3.2). No event-loop type change.

### 4.2 Bind / stale-socket / single-instance

On bind failure because the path exists:
- try `UnixStream::connect`; if it **succeeds**, another instance is already running → log and exit (free single-instance guard);
- if it **fails** (stale socket from a crash), `unlink` the path and re-bind.

### 4.3 CLI client (`main.rs`)

Inspect `argv` before building the event loop. `happy-cappy notify ...` runs the client path: parse flags with **clap** (added dependency), connect to the socket, write one JSON line, exit. No running instance → clear error to stderr, non-zero exit. With no subcommand, run the GUI as today.

```
happy-cappy notify --kind running --label "Building…" --ttl 180
happy-cappy notify --kind failed  --body "3 tests failed"
happy-cappy notify --kind message --animation notify-message --label "Hi"
```

Flags: `--kind` (required), `--animation`, `--label`, `--body`, `--ttl` (seconds), `--priority`. The client serializes these into a `NotificationEvent` JSON line (the same shape `parse_notify_line` accepts).

## 5. Bundled manifest — add `notify-*` animations

The bundled spritesheet is full (40 frames, all assigned), so `notify-*` animations **reuse existing sprite indices** (no new art) and use SP4-A v2 fields for timing/one-shot. Names to add: `notify-running`, `notify-success` (one-shot), `notify-failed`, `notify-review`, `notify-message`. **Exact frame-index mapping is decided at implementation time.** These are *not* added to the `validate_happy_cappy_required_keys` required list — missing `notify-*` animations fall back per §2.

## 6. Error handling

- Malformed socket line / JSON → log a `warn!`, drop that line, keep the connection loop alive. A bad event never crashes the pet.
- Unknown `kind` → `message` preset (§2), not an error.
- Requested animation absent → fallback chain (§2), not an error.
- CLI client with no running server → stderr message + non-zero exit; nothing is queued (events sent while the app is down are intentionally lost — acceptable for ambient status).

## 7. Testing

- **Model (pure):** kind → default animation/priority/TTL; `animation_name` override; TTL countdown + expiry; one-shot completion clears early; preemption (higher replaces, equal latest-wins, lower ignored).
- **`parse_notify_line`:** valid JSON; missing optional fields; invalid JSON; missing required `kind`.
- **CLI arg parsing (clap):** flag combinations; missing `--kind`; non-numeric `--ttl`/`--priority`.
- **Socket I/O (integration):** bind a `UnixListener` on a `tempfile` socket path, connect a `UnixStream`, send a line, assert the parsed event is delivered; stale-socket cleanup (pre-create a dead socket file → server unlinks + rebinds); single-instance (second bind sees a live connect → exits).
- **`refresh_behavior_mode` priority:** `Notifying` loses to Dragging/Hovered, beats micro-action/Walking/Default.
- **Dev-agent preset integration test (generic):** simulate a "build script" sending `running` then `succeeded`; assert the runtime's resolved animation transitions accordingly. No Claude/Codex names in core.

## 8. Exit criteria

- All new unit + integration tests pass under `cargo test`.
- `cargo clippy --all-targets --all-features -- -D warnings` clean; `cargo fmt --check` clean; `./scripts/verify.sh` passes.
- Manual smoke: launch app; from a second terminal run `happy-cappy notify --kind running` → pet switches to the running animation; run `--kind succeeded` → success animation plays once then returns; `--kind failed` while `running` is active → failed preempts (higher priority); dragging the pet during a notification shows the drag animation and the notify animation resumes on release; sending a notify with no app running prints a clear error.
- No regressions in SP2/SP3 menu, picker, settings, focus mode, micro-actions, or drag.

## 9. Dependencies

| SP4-B needs | From |
|---|---|
| one-shot + `fallback` + per-frame timing + completion signal | SP4-A |
| `lookup_with_fallback`, `PetManifest`, `Animation` | SP1 / SP4-A |
| `EventLoop::<AppCommand>::with_user_event()`, `EventLoopProxy<AppCommand>`, `handle_non_quit_command`, `ApplicationHandler<AppCommand>` | existing (SP1–SP3) |
| `ActionOverride` priority pattern in `refresh_behavior_mode` | existing (mirrored, not refactored) |
| App-support dir resolution (same as `pets/`, `settings.json`) | SP2 |

New dependency: **`clap`** (CLI flag parsing) — a deliberate deviation from the lean-deps style, chosen for robust subcommand handling.
