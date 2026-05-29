use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::micro_action::{ActionOverride, MicroAction};
use crate::pet::manifest::PetManifest;
use crate::pet::resolver::{lookup_with_fallback, resolve_animation_chain};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Personality {
    Calm,
    Cheerful,
    Lively,
}

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
    pub oneshot_completed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorIntent {
    Idle,
    ChaseHorizontal { direction: Direction },
    AvoidHorizontal { direction: Direction },
    AvoidRectHorizontal { direction: Direction },
}

#[derive(Debug)]
pub struct PetRuntime {
    state: PetState,
    direction: Direction,
    frame_index: usize,
    frame_elapsed: Duration,
    state_elapsed: Duration,
    walk_distance_remaining: f32,
    completed_walk_cycles: u32,
    personality: Personality,
    behavior_mode: BehaviorMode,
    expression_index: usize,
    expression_elapsed: Duration,
    movement_speed_multiplier: f32,
    hover_intensity: f32,
    action_override: Option<ActionOverride>,
    hovered: bool,
    dragging: bool,
    hidden: bool,
    intent: BehaviorIntent,
    manifest: PetManifest,
    current_animation_name: String,
    /// Edge-trigger guard: set once a one-shot animation reaches its held final
    /// frame, so the completion signal fires exactly once until the animation
    /// (re)starts at frame 0.
    oneshot_held: bool,
    /// Active notification, if any.  Set by `set_notification`, cleared by `clear_notification`.
    notification: Option<NotificationState>,
    /// Test-only pin: when set, `refresh_behavior_mode` will not overwrite
    /// `current_animation_name`, allowing tests to drive a specific animation.
    #[cfg(test)]
    pinned_animation_name: Option<String>,
}

#[derive(Debug, Clone)]
struct NotificationState {
    animation_name: String,
    remaining: Duration,
    priority: i32,
    #[allow(dead_code)] // carried for SP4-C (not rendered in SP4-B)
    label: Option<String>,
    #[allow(dead_code)]
    body: Option<String>,
}

const IDLE_STATE_MS: u64 = 200;
const WALK_STATE_MS: u64 = 100;
const SLEEP_STATE_MS: u64 = 500;
const WALK_SPEED: f32 = 45.0;
const WALK_DISTANCE: f32 = 120.0;

impl Default for PetRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl PetRuntime {
    pub fn new() -> Self {
        Self::new_with_manifest(PetManifest::load_embedded_happy_cappy())
    }

    pub fn new_with_seed(seed: u64) -> Self {
        Self::new_with_manifest_and_seed(PetManifest::load_embedded_happy_cappy(), seed)
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

    pub fn personality(&self) -> Personality {
        self.personality
    }

    pub fn behavior_mode(&self) -> BehaviorMode {
        self.behavior_mode
    }

    pub fn current_animation_name(&self) -> &str {
        &self.current_animation_name
    }

    pub fn current_sprite_index(&self) -> u32 {
        let anim = self
            .manifest
            .animations
            .get(&self.current_animation_name)
            .or_else(|| self.manifest.animations.get("idle"))
            .expect("manifest validation guarantees 'idle' exists");
        anim.sprite_index(self.frame_index)
    }

    pub fn frame_size(&self) -> (u32, u32) {
        (self.manifest.frame.width, self.manifest.frame.height)
    }

    pub fn manifest(&self) -> &PetManifest {
        &self.manifest
    }

    pub fn apply_personality(&mut self, personality: Personality) {
        self.personality = personality;
        self.refresh_behavior_mode();
    }

    pub fn set_movement_speed_multiplier(&mut self, multiplier: f32) {
        self.movement_speed_multiplier = multiplier.clamp(0.0, 3.0);
        self.refresh_behavior_mode();
    }

    pub fn set_hover_intensity(&mut self, intensity: f32) {
        self.hover_intensity = intensity.clamp(0.0, 3.0);
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        self.refresh_behavior_mode();
    }

    pub fn set_dragging(&mut self, dragging: bool) {
        self.dragging = dragging;
        self.refresh_behavior_mode();
    }

    pub fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
        self.refresh_behavior_mode();
    }

    pub fn intent(&self) -> BehaviorIntent {
        self.intent
    }

    pub fn set_intent(&mut self, intent: BehaviorIntent) {
        self.intent = intent;
        if let BehaviorIntent::AvoidRectHorizontal { direction } = intent {
            self.direction = direction;
            if matches!(self.state, PetState::Idle | PetState::Sleep) {
                self.enter_walk();
            } else {
                // Mid-Walk: keep walking, but with full distance in the new direction
                // so a brief intent flip doesn't immediately terminate the segment.
                self.walk_distance_remaining = WALK_DISTANCE;
            }
        }
    }

    pub fn start_micro_action(&mut self, action: MicroAction) {
        self.action_override = Some(ActionOverride::new(action));
        self.refresh_behavior_mode();
    }

    pub fn clear_micro_action(&mut self) {
        self.action_override = None;
        self.refresh_behavior_mode();
    }

    pub fn set_notification(&mut self, event: &crate::notification::NotificationEvent) {
        let (default_priority, default_ttl) = crate::notification::preset_for(&event.kind);
        let priority =
            crate::notification::clamp_priority(event.priority.unwrap_or(default_priority));
        let ttl_ms = crate::notification::clamp_ttl(event.ttl_ms.unwrap_or(default_ttl));

        // Preemption: a new notification with LOWER priority than the active one is ignored.
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
        let (resolved, _) =
            crate::pet::resolver::lookup_with_fallback_dynamic(&self.manifest, &chain);

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
        self.notification
            .as_ref()
            .map(|n| n.animation_name.as_str())
    }

    pub fn turn_around(&mut self) {
        self.direction = match self.direction {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        };
    }

    pub fn tick(&mut self, dt: Duration) -> PetTick {
        let had_action = self.action_override.is_some();
        if self
            .action_override
            .as_mut()
            .is_some_and(|action| action.tick(dt))
        {
            self.clear_micro_action();
        }

        // Notification TTL counts down in every state (hidden / drag / hover included),
        // so a stale notification never lingers behind an obscuring state.
        if let Some(n) = self.notification.as_mut() {
            n.remaining = n.remaining.saturating_sub(dt);
        }
        if self
            .notification
            .as_ref()
            .is_some_and(|n| n.remaining.is_zero())
        {
            self.notification = None;
            self.refresh_behavior_mode();
        }

        if self.hidden {
            self.refresh_behavior_mode();
            return PetTick {
                state: self.state,
                frame_index: self.frame_index,
                speed_x: 0.0,
                oneshot_completed: false,
            };
        }

        self.frame_elapsed += dt;
        if !self.dragging {
            self.state_elapsed += dt;
            self.expression_elapsed += dt;
        }

        let oneshot_completed = self.advance_animation();

        // One-shot notify animation finished -> the notification owner clears itself
        // (the manifest `fallback` is NOT consulted on the notification path). Ordering
        // per SP4-A 5.3: advance -> owner consumes -> refresh (the trailing refresh drops Notifying).
        if oneshot_completed && self.notification.is_some() {
            self.notification = None;
        }

        if !self.dragging {
            self.advance_state(dt);
        }

        if !had_action
            && !self.hovered
            && !self.dragging
            && self.expression_elapsed >= self.expression_interval()
        {
            self.expression_index = self.expression_index.wrapping_add(1);
            self.expression_elapsed = Duration::ZERO;
        }

        self.refresh_behavior_mode();

        PetTick {
            state: self.state,
            frame_index: self.frame_index,
            speed_x: self.speed_x(),
            oneshot_completed,
        }
    }

    fn current_animation(&self) -> &crate::pet::manifest::Animation {
        self.manifest
            .animations
            .get(&self.current_animation_name)
            .or_else(|| self.manifest.animations.get("idle"))
            .expect("manifest validation guarantees 'idle' exists")
    }

    pub fn current_fallback(&self) -> String {
        self.current_animation()
            .fallback
            .clone()
            .unwrap_or_else(|| "idle".to_string())
    }

    fn frame_duration_for(&self, pos: usize) -> Duration {
        if let Some(ms) = self.current_animation().frame_ms(pos) {
            Duration::from_millis(ms as u64)
        } else {
            self.frame_duration()
        }
    }

    fn advance_animation(&mut self) -> bool {
        let (frame_count, loop_start, one_shot) = {
            let anim = self.current_animation();
            (
                anim.frame_count().max(1),
                anim.loop_start.unwrap_or(0),
                anim.one_shot,
            )
        };
        let mut completed = false;
        loop {
            let frame_duration = self.frame_duration_for(self.frame_index);
            if frame_duration.is_zero() {
                break; // ms:0 is rejected by manifest validation; guard against test-injected/raw frames
            }
            if self.frame_elapsed < frame_duration {
                break;
            }
            self.frame_elapsed -= frame_duration;
            if one_shot && self.frame_index + 1 >= frame_count {
                // Final frame shown for its full duration. Hold it; report completion
                // exactly once (edge-triggered) until the owner switches animation away.
                completed = !self.oneshot_held;
                self.oneshot_held = true;
                self.frame_elapsed = Duration::ZERO;
                break;
            }
            let next = self.frame_index + 1;
            self.frame_index = if next >= frame_count {
                loop_start
            } else {
                next
            };
        }
        completed
    }

    fn advance_state(&mut self, dt: Duration) {
        match self.state {
            PetState::Idle if self.state_elapsed >= Duration::from_secs(5) => {
                if !self.movement_enabled() {
                    self.state_elapsed = Duration::ZERO;
                    self.walk_distance_remaining = 0.0;
                } else if self.completed_walk_cycles >= 2 {
                    self.enter_sleep();
                } else {
                    self.enter_walk();
                }
            }
            PetState::Walk if !self.movement_enabled() => {
                self.enter_idle();
            }
            PetState::Walk => {
                self.walk_distance_remaining -= self.effective_walk_speed_abs() * dt.as_secs_f32();
                if self.walk_distance_remaining <= 0.0 {
                    self.completed_walk_cycles += 1;
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
        self.expression_elapsed = Duration::ZERO;
        self.walk_distance_remaining = 0.0;
    }

    fn enter_walk(&mut self) {
        match self.intent {
            BehaviorIntent::ChaseHorizontal { direction }
            | BehaviorIntent::AvoidHorizontal { direction }
            | BehaviorIntent::AvoidRectHorizontal { direction } => {
                self.direction = direction;
            }
            BehaviorIntent::Idle => {}
        }
        self.state = PetState::Walk;
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
        self.state_elapsed = Duration::ZERO;
        self.expression_elapsed = Duration::ZERO;
        self.walk_distance_remaining = WALK_DISTANCE;
    }

    fn enter_sleep(&mut self) {
        self.state = PetState::Sleep;
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
        self.state_elapsed = Duration::ZERO;
        self.expression_elapsed = Duration::ZERO;
        self.walk_distance_remaining = 0.0;
        self.completed_walk_cycles = 0;
    }

    fn frame_duration(&self) -> Duration {
        if self.behavior_mode == BehaviorMode::Hovered {
            return self.hover_frame_duration();
        }

        match self.state {
            PetState::Idle => Duration::from_millis(IDLE_STATE_MS),
            PetState::Walk => Duration::from_millis(WALK_STATE_MS),
            PetState::Sleep => Duration::from_millis(SLEEP_STATE_MS),
        }
    }

    fn speed_x(&self) -> f32 {
        let speed = self.effective_walk_speed_abs();
        if self.hidden
            || self.dragging
            || speed <= 0.0
            || self
                .action_override
                .is_some_and(|action| action.disables_movement())
        {
            return 0.0;
        }
        if self.state != PetState::Walk {
            return 0.0;
        }

        match self.direction {
            Direction::Left => -speed,
            Direction::Right => speed,
        }
    }

    fn effective_walk_speed_abs(&self) -> f32 {
        if !self.movement_enabled() {
            return 0.0;
        }

        WALK_SPEED * self.movement_speed_multiplier
    }

    fn movement_enabled(&self) -> bool {
        self.movement_speed_multiplier > 0.0
            && !self
                .action_override
                .is_some_and(|action| action.disables_movement())
    }

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

        #[cfg(test)]
        if let Some(ref pinned) = self.pinned_animation_name {
            self.current_animation_name = pinned.clone();
            return;
        }

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

    /// Set the current animation name, resetting the cursor when entering a
    /// "lifecycle" animation (loopStart/oneShot) so its intro/one-shot starts at frame 0.
    /// Non-lifecycle name changes preserve the cursor (existing parity behavior).
    fn set_selected_animation(&mut self, name: &str) {
        if name == self.current_animation_name {
            return;
        }
        let is_lifecycle = self
            .manifest
            .animations
            .get(name)
            .map(|a| a.is_lifecycle())
            .unwrap_or(false);
        self.current_animation_name = name.to_string();
        if is_lifecycle {
            self.frame_index = 0;
            self.frame_elapsed = Duration::ZERO;
            self.oneshot_held = false;
        }
    }

    fn expression_interval(&self) -> Duration {
        match self.personality {
            Personality::Calm => Duration::from_secs(5),
            Personality::Cheerful => Duration::from_secs(3),
            Personality::Lively => Duration::from_secs(2),
        }
    }

    fn hover_frame_duration(&self) -> Duration {
        let base_ms = match self.personality {
            Personality::Calm => 220.0,
            Personality::Cheerful => 140.0,
            Personality::Lively => 90.0,
        };
        let divisor = self.hover_intensity.max(0.5);
        Duration::from_millis((base_ms / divisor).round() as u64)
    }

    pub fn new_with_manifest_and_seed(manifest: PetManifest, seed: u64) -> Self {
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
            completed_walk_cycles: 0,
            personality: Personality::Cheerful,
            behavior_mode: BehaviorMode::Default,
            expression_index: 0,
            expression_elapsed: Duration::ZERO,
            movement_speed_multiplier: 1.0,
            hover_intensity: 1.0,
            action_override: None,
            hovered: false,
            dragging: false,
            hidden: false,
            intent: BehaviorIntent::Idle,
            manifest,
            current_animation_name: "idle".to_string(),
            oneshot_held: false,
            notification: None,
            #[cfg(test)]
            pinned_animation_name: None,
        }
    }

    pub fn new_with_manifest(manifest: PetManifest) -> Self {
        Self::new_with_manifest_and_seed(manifest, 0)
    }

    #[cfg(test)]
    fn force_state_for_test(&mut self, state: PetState) {
        self.state = state;
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
        self.state_elapsed = Duration::ZERO;
        self.expression_elapsed = Duration::ZERO;
        self.completed_walk_cycles = 0;
        self.walk_distance_remaining = match state {
            PetState::Walk => WALK_DISTANCE,
            PetState::Idle | PetState::Sleep => 0.0,
        };
        self.refresh_behavior_mode();
    }

    #[cfg(test)]
    pub fn set_current_animation_for_test(&mut self, name: &str) {
        self.pinned_animation_name = Some(name.to_string());
        self.current_animation_name = name.to_string();
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
        self.oneshot_held = false;
    }

    #[cfg(test)]
    pub fn replace_animation_for_test(
        &mut self,
        name: &str,
        animation: crate::pet::manifest::Animation,
    ) {
        self.manifest.animations.insert(name.to_string(), animation);
    }

    #[cfg(test)]
    pub fn set_expression_index_for_test(&mut self, idx: usize) {
        self.expression_index = idx;
    }

    #[cfg(test)]
    pub fn refresh_behavior_mode_for_test(&mut self) {
        self.refresh_behavior_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_idle_on_frame_zero() {
        let pet = PetRuntime::new();
        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(pet.frame_index(), 0);
    }

    #[test]
    fn cheerful_is_default_personality() {
        let pet = PetRuntime::new();
        assert_eq!(pet.personality(), Personality::Cheerful);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
    }

    #[test]
    fn personality_changes_hover_group() {
        let mut pet = PetRuntime::new();

        pet.apply_personality(Personality::Calm);
        pet.set_hovered(true);
        assert_eq!(pet.current_animation_name(), "hover-calm");

        pet.apply_personality(Personality::Cheerful);
        assert_eq!(pet.current_animation_name(), "hover-cheerful");

        pet.apply_personality(Personality::Lively);
        assert_eq!(pet.current_animation_name(), "hover-lively");
    }

    #[test]
    fn dragging_overrides_hover_and_movement() {
        let mut pet = PetRuntime::new();
        pet.set_hovered(true);
        pet.set_dragging(true);

        let tick = pet.tick(Duration::from_millis(100));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Dragging);
        assert_eq!(pet.current_animation_name(), "drag");
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn dragging_pauses_autonomous_state_progression() {
        let mut pet = PetRuntime::new();
        pet.set_dragging(true);

        let tick = pet.tick(Duration::from_secs(10));

        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Dragging);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn dragging_pauses_walk_distance_consumption() {
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Walk);
        pet.set_dragging(true);

        pet.tick(Duration::from_secs_f32(WALK_DISTANCE / WALK_SPEED));

        assert_eq!(pet.state(), PetState::Walk);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Dragging);
    }

    #[test]
    fn expression_loop_advances_without_requiring_walk() {
        let mut pet = PetRuntime::new();
        let first = pet.current_animation_name().to_string();
        pet.tick(Duration::from_secs(3));
        let second = pet.current_animation_name().to_string();

        assert_ne!(first, second);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
    }

    #[test]
    fn movement_speed_zero_disables_walk_speed() {
        let mut pet = PetRuntime::new();
        pet.set_movement_speed_multiplier(0.0);
        pet.force_state_for_test(PetState::Walk);

        let tick = pet.tick(Duration::from_millis(16));

        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn movement_speed_zero_prevents_entering_stuck_walk_state() {
        let mut pet = PetRuntime::new();
        pet.set_movement_speed_multiplier(0.0);

        let tick = pet.tick(Duration::from_secs(5));

        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn movement_speed_multiplier_controls_walk_completion_distance() {
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Walk);
        pet.set_movement_speed_multiplier(2.0);

        pet.tick(Duration::from_secs_f32(WALK_DISTANCE / (WALK_SPEED * 2.0)));

        assert_eq!(pet.state(), PetState::Idle);
    }

    #[test]
    fn movement_speed_update_refreshes_behavior_immediately() {
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Walk);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Walking);

        pet.set_movement_speed_multiplier(0.0);

        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
        assert_eq!(pet.current_animation_name(), "idle");
    }

    #[test]
    fn nap_micro_action_uses_sleepy_group_and_stops_movement() {
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Walk);

        pet.start_micro_action(MicroAction::Nap);
        let tick = pet.tick(Duration::from_millis(16));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Action);
        assert_eq!(pet.current_animation_name(), "sleepy");
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn cheer_up_micro_action_uses_happy_group_temporarily() {
        let mut pet = PetRuntime::new();

        pet.start_micro_action(MicroAction::CheerUp);
        pet.tick(Duration::from_secs(7));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Action);
        assert_eq!(pet.current_animation_name(), "happy");

        pet.tick(Duration::from_secs(1));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Walking);
        assert_eq!(pet.current_animation_name(), "walk-right");
    }

    #[test]
    fn hover_overrides_micro_action_until_hover_ends() {
        let mut pet = PetRuntime::new();

        pet.start_micro_action(MicroAction::CheerUp);
        pet.set_hovered(true);

        assert_eq!(pet.behavior_mode(), BehaviorMode::Hovered);
        assert_eq!(pet.current_animation_name(), "hover-cheerful");

        pet.set_hovered(false);

        assert_eq!(pet.behavior_mode(), BehaviorMode::Action);
        assert_eq!(pet.current_animation_name(), "happy");
    }

    #[test]
    fn idle_animation_advances_every_200ms() {
        let mut pet = PetRuntime::new();
        let tick = pet.tick(Duration::from_millis(200));
        assert_eq!(tick.frame_index, 1);
        assert_eq!(tick.state, PetState::Idle);
    }

    #[test]
    fn idle_transitions_to_walk_after_threshold() {
        let mut pet = PetRuntime::new_with_seed(1);
        pet.tick(Duration::from_secs(5));
        assert_eq!(pet.state(), PetState::Walk);
    }

    #[test]
    fn idle_transitions_to_walk_after_incremental_expression_ticks() {
        let mut pet = PetRuntime::new_with_seed(1);

        for _ in 0..25 {
            pet.tick(Duration::from_millis(200));
        }

        assert_eq!(pet.state(), PetState::Walk);
    }

    #[test]
    fn forced_walk_preserves_walk_distance_invariant() {
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Walk);

        let tick = pet.tick(Duration::from_millis(1));

        assert_eq!(pet.state(), PetState::Walk);
        assert_eq!(tick.state, PetState::Walk);
        assert_eq!(tick.speed_x, WALK_SPEED);
    }

    #[test]
    fn walk_speed_sign_follows_seed_direction() {
        let mut right_pet = PetRuntime::new_with_seed(0);
        right_pet.force_state_for_test(PetState::Walk);
        assert_eq!(right_pet.direction(), Direction::Right);
        assert_eq!(right_pet.tick(Duration::ZERO).speed_x, WALK_SPEED);

        let mut left_pet = PetRuntime::new_with_seed(1);
        left_pet.force_state_for_test(PetState::Walk);
        assert_eq!(left_pet.direction(), Direction::Left);
        assert_eq!(left_pet.tick(Duration::ZERO).speed_x, -WALK_SPEED);
    }

    #[test]
    fn turn_around_reverses_walk_speed_direction() {
        let mut pet = PetRuntime::new_with_seed(0);
        pet.force_state_for_test(PetState::Walk);

        pet.turn_around();

        assert_eq!(pet.direction(), Direction::Left);
        assert_eq!(pet.tick(Duration::ZERO).speed_x, -WALK_SPEED);
    }

    #[test]
    fn walk_returns_to_idle_after_configured_distance() {
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Walk);

        let tick = pet.tick(Duration::from_secs_f32(WALK_DISTANCE / WALK_SPEED));

        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(tick.state, PetState::Idle);
        assert_eq!(tick.frame_index, 0);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn sleep_returns_to_idle_after_12_seconds() {
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Sleep);

        let tick = pet.tick(Duration::from_secs(12));

        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(tick.state, PetState::Idle);
        assert_eq!(tick.frame_index, 0);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn sleep_is_naturally_reachable_after_two_walk_cycles() {
        let mut pet = PetRuntime::new();

        for _ in 0..2 {
            pet.tick(Duration::from_secs(5));
            assert_eq!(pet.state(), PetState::Walk);

            pet.tick(Duration::from_secs_f32(WALK_DISTANCE / WALK_SPEED));
            assert_eq!(pet.state(), PetState::Idle);
        }

        let tick = pet.tick(Duration::from_secs(5));

        assert_eq!(pet.state(), PetState::Sleep);
        assert_eq!(tick.state, PetState::Sleep);
        assert_eq!(tick.frame_index, 0);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn naturally_reached_sleep_returns_to_idle_after_12_seconds() {
        let mut pet = PetRuntime::new();

        for _ in 0..2 {
            pet.tick(Duration::from_secs(5));
            pet.tick(Duration::from_secs_f32(WALK_DISTANCE / WALK_SPEED));
        }
        pet.tick(Duration::from_secs(5));
        assert_eq!(pet.state(), PetState::Sleep);

        let tick = pet.tick(Duration::from_secs(12));

        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(tick.state, PetState::Idle);
        assert_eq!(tick.frame_index, 0);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn tick_reports_walk_at_idle_to_walk_boundary() {
        let mut pet = PetRuntime::new_with_seed(1);

        let tick = pet.tick(Duration::from_secs(5));

        assert_eq!(pet.state(), PetState::Walk);
        assert_eq!(tick.state, PetState::Walk);
        assert_eq!(tick.frame_index, 0);
        assert_eq!(tick.speed_x, -WALK_SPEED);
    }

    #[test]
    fn sleep_uses_slow_animation_rate() {
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Sleep);
        pet.tick(Duration::from_millis(499));
        assert_eq!(pet.frame_index(), 0);
        pet.tick(Duration::from_millis(1));
        assert_eq!(pet.frame_index(), 1);
    }

    #[test]
    fn set_intent_stores_intent() {
        let mut pet = PetRuntime::new_with_seed(0);
        pet.set_intent(BehaviorIntent::ChaseHorizontal {
            direction: Direction::Right,
        });
        assert_eq!(
            pet.intent(),
            BehaviorIntent::ChaseHorizontal {
                direction: Direction::Right
            }
        );
    }

    #[test]
    fn default_intent_is_idle() {
        let pet = PetRuntime::new_with_seed(0);
        assert_eq!(pet.intent(), BehaviorIntent::Idle);
    }

    #[test]
    fn set_intent_avoid_rect_interrupts_idle_into_walk() {
        let mut pet = PetRuntime::new_with_seed(0);
        // Force pet into Idle state via a complete tick cycle.
        pet.tick(std::time::Duration::from_millis(0));
        assert_eq!(pet.state(), PetState::Idle);

        pet.set_intent(BehaviorIntent::AvoidRectHorizontal {
            direction: Direction::Left,
        });

        assert_eq!(pet.state(), PetState::Walk);
        assert!(pet.tick(std::time::Duration::ZERO).speed_x < 0.0);
    }

    #[test]
    fn set_intent_avoid_rect_redirects_mid_walk() {
        let mut pet = PetRuntime::new_with_seed(0); // seed 0 starts Direction::Right
        pet.tick(std::time::Duration::ZERO);
        while pet.state() != PetState::Walk {
            pet.tick(std::time::Duration::from_millis(200));
        }
        // Pet is now mid-Walk going Right.
        pet.set_intent(BehaviorIntent::AvoidRectHorizontal {
            direction: Direction::Left,
        });
        assert_eq!(
            pet.state(),
            PetState::Walk,
            "should stay in Walk, not re-enter"
        );
        assert!(
            pet.tick(std::time::Duration::ZERO).speed_x < 0.0,
            "direction should flip to Left immediately"
        );
    }

    #[test]
    fn set_intent_chase_does_not_interrupt_mid_walk() {
        let mut pet = PetRuntime::new_with_seed(0); // seed 0 starts Direction::Right
        pet.tick(std::time::Duration::ZERO);
        while pet.state() != PetState::Walk {
            pet.tick(std::time::Duration::from_millis(200));
        }
        // Pet is now mid-Walk going Right.
        pet.set_intent(BehaviorIntent::ChaseHorizontal {
            direction: Direction::Left,
        });
        // ChaseHorizontal must NOT interrupt: direction stays Right within this walk segment.
        assert!(
            pet.tick(std::time::Duration::ZERO).speed_x > 0.0,
            "Chase intent should be queued, not interrupt mid-walk; direction must remain Right"
        );
    }

    #[test]
    fn current_animation_name_is_idle_at_construction() {
        let pet = PetRuntime::new();
        assert_eq!(pet.current_animation_name(), "idle");
    }

    #[test]
    fn current_animation_name_is_hover_calm_for_calm_hovered() {
        let mut pet = PetRuntime::new();
        pet.apply_personality(Personality::Calm);
        pet.set_hovered(true);
        assert_eq!(pet.current_animation_name(), "hover-calm");
    }

    #[test]
    fn current_animation_name_is_walk_right_when_walking() {
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Walk);
        assert_eq!(pet.current_animation_name(), "walk-right");
    }

    #[test]
    fn current_sprite_index_starts_at_idle_frame_zero() {
        let pet = PetRuntime::new();
        assert_eq!(pet.current_sprite_index(), 0);
    }

    #[test]
    fn current_sprite_index_for_walk_starts_at_thirty_two() {
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Walk);
        assert_eq!(pet.current_sprite_index(), 32);
    }

    #[test]
    fn frame_size_returns_manifest_geometry() {
        let pet = PetRuntime::new();
        assert_eq!(pet.frame_size(), (64, 64));
    }

    #[test]
    fn animation_name_change_does_not_reset_frame_index() {
        // Force into Walk state, advance two full frame_durations (200ms total at 100ms/frame).
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Walk);
        pet.tick(Duration::from_millis(250));
        assert_eq!(pet.frame_index(), 2);
        assert_eq!(pet.current_animation_name(), "walk-right");

        // Engaging hover changes the animation name but must not reset frame_index.
        pet.set_hovered(true);
        assert_eq!(pet.current_animation_name(), "hover-cheerful");
        assert_eq!(pet.frame_index(), 2);

        // The corresponding sprite is hover-cheerful row, column 2 → index 26.
        assert_eq!(pet.current_sprite_index(), 26);
    }

    #[test]
    fn hover_intensity_fractional_value_preserves_rounding_boundary() {
        let mut pet = PetRuntime::new();
        pet.apply_personality(Personality::Cheerful);
        pet.set_hover_intensity(1.3);
        pet.set_hovered(true);
        // base 140 / 1.3 = 107.692..., rounded to 108ms per frame
        pet.tick(Duration::from_millis(107));
        assert_eq!(pet.frame_index(), 0);
        pet.tick(Duration::from_millis(1));
        assert_eq!(pet.frame_index(), 1);
    }

    #[test]
    fn advance_animation_wraps_at_manifest_frame_count_not_fixed_constant() {
        use crate::pet::manifest::{Animation, FrameGeometry, PetManifest};
        use std::collections::BTreeMap;

        // Fixture manifest where "idle" has 6 frames (more than the legacy FRAME_COUNT=4).
        let mut animations = BTreeMap::new();
        animations.insert(
            "idle".to_string(),
            Animation::from_indices(&[0, 1, 2, 3, 4, 5]),
        );
        let manifest = PetManifest {
            manifest_version: 1,
            id: "fixture".into(),
            display_name: "Fixture".into(),
            spritesheet_path: "x.png".into(),
            frame: FrameGeometry {
                width: 16,
                height: 16,
                columns: 6,
                rows: 1,
            },
            animations,
        };
        let mut pet = PetRuntime::new_with_manifest(manifest);

        // Tick 5 idle frames (200ms each). frame_index should land at 5.
        for _ in 0..5 {
            pet.tick(Duration::from_millis(200));
        }
        assert_eq!(pet.frame_index(), 5);
        assert_eq!(pet.current_sprite_index(), 5);

        // One more tick wraps back to 0.
        pet.tick(Duration::from_millis(200));
        assert_eq!(pet.frame_index(), 0);
        assert_eq!(pet.current_sprite_index(), 0);
    }

    #[test]
    fn new_with_manifest_uses_provided_manifest() {
        use crate::pet::manifest::{Animation, FrameGeometry, PetManifest};
        use std::collections::BTreeMap;

        let mut animations = BTreeMap::new();
        animations.insert("idle".to_string(), Animation::from_indices(&[0, 1, 2, 3]));
        let manifest = PetManifest {
            manifest_version: 1,
            id: "custom".to_string(),
            display_name: "Custom".to_string(),
            spritesheet_path: "custom.png".to_string(),
            frame: FrameGeometry {
                width: 32,
                height: 48,
                columns: 4,
                rows: 1,
            },
            animations,
        };

        let pet = PetRuntime::new_with_manifest(manifest);

        assert_eq!(pet.manifest().id, "custom");
        assert_eq!(pet.frame_size(), (32, 48));
    }

    fn lifecycle_fixture(
        anim_name: &str,
        animation: crate::pet::manifest::Animation,
    ) -> PetRuntime {
        use crate::pet::manifest::{Animation, FrameGeometry, PetManifest};
        use std::collections::BTreeMap;
        let mut animations = BTreeMap::new();
        animations.insert("idle".to_string(), Animation::from_indices(&[0, 1, 2, 3]));
        animations.insert(anim_name.to_string(), animation);
        let manifest = PetManifest {
            manifest_version: 1,
            id: "fixture".into(),
            display_name: "Fixture".into(),
            spritesheet_path: "x.png".into(),
            frame: FrameGeometry {
                width: 16,
                height: 16,
                columns: 8,
                rows: 1,
            },
            animations,
        };
        PetRuntime::new_with_manifest(manifest)
    }

    #[test]
    fn per_frame_ms_overrides_runtime_timing() {
        use crate::pet::manifest::{Animation, Frame};
        let timed = Animation {
            frames: vec![
                Frame {
                    index: 0,
                    ms: Some(50),
                },
                Frame {
                    index: 1,
                    ms: Some(50),
                },
            ],
            loop_start: None,
            fallback: None,
            one_shot: false,
        };
        let mut pet = lifecycle_fixture("idle2", timed);
        pet.set_current_animation_for_test("idle2");

        pet.tick(Duration::from_millis(50));
        assert_eq!(pet.frame_index(), 1);
        pet.tick(Duration::from_millis(50));
        assert_eq!(pet.frame_index(), 0); // wrapped (2 frames)
    }

    #[test]
    fn zero_frame_duration_does_not_hang_advance_animation() {
        use crate::pet::manifest::{Animation, Frame};
        // Manifest validation rejects ms:0, but replace_animation_for_test bypasses
        // it. The guard in advance_animation must break instead of spinning forever.
        let mut pet = lifecycle_fixture("idle", Animation::from_indices(&[0, 1, 2, 3]));
        pet.replace_animation_for_test(
            "zero",
            Animation {
                frames: vec![Frame {
                    index: 0,
                    ms: Some(0),
                }],
                loop_start: None,
                fallback: None,
                one_shot: false,
            },
        );
        pet.set_current_animation_for_test("zero");

        // If the guard were missing this would loop forever; reaching the asserts proves it returns.
        pet.tick(Duration::from_millis(100));
        assert_eq!(pet.frame_index(), 0);
    }

    #[test]
    fn loop_start_wraps_to_intro_boundary_not_zero() {
        use crate::pet::manifest::{Animation, Frame};
        let looping = Animation {
            frames: vec![
                Frame {
                    index: 0,
                    ms: Some(50),
                },
                Frame {
                    index: 1,
                    ms: Some(50),
                },
                Frame {
                    index: 2,
                    ms: Some(50),
                },
            ],
            loop_start: Some(1),
            fallback: None,
            one_shot: false,
        };
        let mut pet = lifecycle_fixture("loopy", looping);
        pet.set_current_animation_for_test("loopy"); // cursor at 0
        pet.tick(Duration::from_millis(50)); // -> 1
        pet.tick(Duration::from_millis(50)); // -> 2
        assert_eq!(pet.frame_index(), 2);
        pet.tick(Duration::from_millis(50)); // past last -> loop_start (1), not 0
        assert_eq!(pet.frame_index(), 1);
    }

    #[test]
    fn entering_lifecycle_animation_resets_cursor() {
        use crate::pet::manifest::{Animation, Frame};
        // Default-mode expression slot 2 selects "happy"; make "happy" a lifecycle anim.
        let happy = Animation {
            frames: vec![Frame { index: 5, ms: None }, Frame { index: 6, ms: None }],
            loop_start: Some(1),
            fallback: None,
            one_shot: false,
        };
        let mut pet = lifecycle_fixture("happy", happy);
        // Advance idle a couple frames so frame_index != 0.
        pet.tick(Duration::from_millis(200));
        pet.tick(Duration::from_millis(200));
        assert_ne!(pet.frame_index(), 0);
        // Force selection of the lifecycle "happy" animation via the real chain path.
        pet.set_expression_index_for_test(2);
        pet.refresh_behavior_mode_for_test();
        assert_eq!(pet.current_animation_name(), "happy");
        assert_eq!(pet.frame_index(), 0); // entry-reset fired
    }

    #[test]
    fn entering_non_lifecycle_animation_preserves_cursor() {
        // Parity guard: switching to a plain animation keeps the cursor.
        let mut pet = PetRuntime::new();
        pet.force_state_for_test(PetState::Walk);
        pet.tick(Duration::from_millis(250));
        assert_eq!(pet.frame_index(), 2);
        pet.set_hovered(true);
        assert_eq!(pet.frame_index(), 2); // hover-cheerful is not lifecycle -> preserved
    }

    #[test]
    fn one_shot_completion_fires_after_final_frame_full_duration() {
        use crate::pet::manifest::{Animation, Frame};
        let success = Animation {
            frames: vec![
                Frame {
                    index: 4,
                    ms: Some(50),
                },
                Frame {
                    index: 5,
                    ms: Some(50),
                },
            ],
            loop_start: None,
            fallback: Some("idle".to_string()),
            one_shot: true,
        };
        let mut pet = lifecycle_fixture("success", success);
        pet.set_current_animation_for_test("success"); // frame 0

        let t1 = pet.tick(Duration::from_millis(50)); // frame 0 done -> frame 1
        assert_eq!(pet.frame_index(), 1);
        assert!(!t1.oneshot_completed);

        let t2 = pet.tick(Duration::from_millis(50)); // final frame shown full duration
        assert!(
            t2.oneshot_completed,
            "completion should fire after final frame duration"
        );
        assert_eq!(
            pet.frame_index(),
            1,
            "one-shot holds the last frame (no wrap)"
        );
    }

    #[test]
    fn one_shot_completion_does_not_refire_while_held() {
        use crate::pet::manifest::{Animation, Frame};
        let success = Animation {
            frames: vec![
                Frame {
                    index: 4,
                    ms: Some(50),
                },
                Frame {
                    index: 5,
                    ms: Some(50),
                },
            ],
            loop_start: None,
            fallback: Some("idle".to_string()),
            one_shot: true,
        };
        let mut pet = lifecycle_fixture("success", success);
        pet.set_current_animation_for_test("success");
        pet.tick(Duration::from_millis(50)); // -> frame 1
        let t2 = pet.tick(Duration::from_millis(50)); // completes
        assert!(t2.oneshot_completed);
        let t3 = pet.tick(Duration::from_millis(50)); // held; must NOT re-fire
        assert!(!t3.oneshot_completed);
        assert_eq!(pet.frame_index(), 1);
    }

    #[test]
    fn looping_animation_never_reports_oneshot_completed() {
        let mut pet = PetRuntime::new(); // bundled idle, not one-shot
        let t = pet.tick(Duration::from_millis(200));
        assert!(!t.oneshot_completed);
    }

    #[test]
    fn current_fallback_exposes_manifest_value() {
        use crate::pet::manifest::{Animation, Frame};
        let success = Animation {
            frames: vec![Frame {
                index: 4,
                ms: Some(50),
            }],
            loop_start: None,
            fallback: Some("idle".to_string()),
            one_shot: true,
        };
        let mut pet = lifecycle_fixture("success", success);
        pet.set_current_animation_for_test("success");
        assert_eq!(pet.current_fallback(), "idle");
    }

    fn notify_oneshot_fixture() -> PetRuntime {
        use crate::pet::manifest::{Animation, Frame, FrameGeometry, PetManifest};
        use std::collections::BTreeMap;
        let mut animations = BTreeMap::new();
        animations.insert("idle".to_string(), Animation::from_indices(&[0, 1, 2, 3]));
        animations.insert(
            "notify-running".to_string(),
            Animation::from_indices(&[4, 5]),
        );
        animations.insert(
            "notify-success".to_string(),
            Animation {
                frames: vec![
                    Frame {
                        index: 6,
                        ms: Some(50),
                    },
                    Frame {
                        index: 7,
                        ms: Some(50),
                    },
                ],
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
            frame: FrameGeometry {
                width: 16,
                height: 16,
                columns: 8,
                rows: 1,
            },
            animations,
        };
        PetRuntime::new_with_manifest(manifest)
    }

    fn notify_fixture() -> PetRuntime {
        use crate::pet::manifest::{Animation, FrameGeometry, PetManifest};
        use std::collections::BTreeMap;
        let mut animations = BTreeMap::new();
        animations.insert("idle".to_string(), Animation::from_indices(&[0, 1, 2, 3]));
        animations.insert(
            "notify-running".to_string(),
            Animation::from_indices(&[4, 5]),
        );
        animations.insert(
            "notify-failed".to_string(),
            Animation::from_indices(&[6, 7]),
        );
        let manifest = PetManifest {
            manifest_version: 1,
            id: "fixture".into(),
            display_name: "Fixture".into(),
            spritesheet_path: "x.png".into(),
            frame: FrameGeometry {
                width: 16,
                height: 16,
                columns: 8,
                rows: 1,
            },
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
        // chain: notify-review(absent) -> notify-message(absent) -> notify-running(present)
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
        assert_eq!(
            pet.notification_animation(),
            None,
            "TTL must keep counting while hidden"
        );
    }

    #[test]
    fn one_shot_notification_clears_on_completion_before_ttl() {
        let mut pet = notify_oneshot_fixture();
        let mut ev = event("succeeded");
        ev.animation_name = Some("notify-success".to_string());
        ev.ttl_ms = Some(60_000); // long TTL; one-shot should end it sooner
        pet.set_notification(&ev);
        assert_eq!(pet.current_animation_name(), "notify-success");
        pet.tick(Duration::from_millis(50)); // frame 0 done
        let t = pet.tick(Duration::from_millis(50)); // final frame full duration -> completion
        assert!(t.oneshot_completed);
        assert_eq!(
            pet.notification_animation(),
            None,
            "one-shot completion clears the notification"
        );
    }

    #[test]
    fn dev_agent_flow_running_then_succeeded_transitions_animation() {
        // Generic dev-agent flow on the bundled pet: a "build" goes running -> succeeded.
        // No agent-specific names appear in the core; only generic kinds + the notify-<kind> convention.
        let mut pet = PetRuntime::new(); // bundled manifest with notify-* animations

        pet.set_notification(&event("running"));
        assert_eq!(pet.behavior_mode(), BehaviorMode::Notifying);
        assert_eq!(pet.current_animation_name(), "notify-running");

        // "succeeded" (priority 30) preempts the active "running" (priority 10) and resolves
        // to the one-shot notify-succeeded via the notify-<kind> convention (no explicit animation_name).
        pet.set_notification(&event("succeeded"));
        assert_eq!(pet.current_animation_name(), "notify-succeeded");

        // One tick long enough to play the whole one-shot (90+90+120+260 = 560 ms).
        let t = pet.tick(Duration::from_millis(1000));
        assert!(
            t.oneshot_completed,
            "one-shot should complete within the tick"
        );
        assert_eq!(
            pet.notification_animation(),
            None,
            "completion clears the notification (before its 8s TTL)"
        );
        assert_ne!(
            pet.behavior_mode(),
            BehaviorMode::Notifying,
            "behavior chain resumes after completion"
        );
    }
}
