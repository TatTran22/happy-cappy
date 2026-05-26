use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::micro_action::{ActionOverride, MicroAction};

pub mod manifest;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationGroup {
    Idle,
    Blink,
    Happy,
    Curious,
    Sleepy,
    HoverCalm,
    HoverCheerful,
    HoverLively,
    WalkRight,
    Drag,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorIntent {
    Idle,
    ChaseHorizontal { direction: Direction },
    AvoidHorizontal { direction: Direction },
    AvoidRectHorizontal { direction: Direction },
}

#[derive(Debug)]
pub struct Pet {
    state: PetState,
    direction: Direction,
    frame_index: usize,
    frame_elapsed: Duration,
    state_elapsed: Duration,
    walk_distance_remaining: f32,
    completed_walk_cycles: u32,
    personality: Personality,
    behavior_mode: BehaviorMode,
    animation_group: AnimationGroup,
    expression_index: usize,
    expression_elapsed: Duration,
    movement_speed_multiplier: f32,
    hover_intensity: f32,
    action_override: Option<ActionOverride>,
    hovered: bool,
    dragging: bool,
    hidden: bool,
    intent: BehaviorIntent,
}

const FRAME_COUNT: usize = 4;
const IDLE_FRAME_MS: u64 = 200;
const WALK_FRAME_MS: u64 = 100;
const SLEEP_FRAME_MS: u64 = 500;
const WALK_SPEED: f32 = 45.0;
const WALK_DISTANCE: f32 = 120.0;

impl Default for Pet {
    fn default() -> Self {
        Self::new()
    }
}

impl Pet {
    pub fn new() -> Self {
        Self::new_with_seed(0)
    }

    pub fn new_with_seed(seed: u64) -> Self {
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
            animation_group: AnimationGroup::Idle,
            expression_index: 0,
            expression_elapsed: Duration::ZERO,
            movement_speed_multiplier: 1.0,
            hover_intensity: 1.0,
            action_override: None,
            hovered: false,
            dragging: false,
            hidden: false,
            intent: BehaviorIntent::Idle,
        }
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

    pub fn current_animation_group(&self) -> AnimationGroup {
        self.animation_group
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

        if self.hidden {
            self.refresh_behavior_mode();
            return PetTick {
                state: self.state,
                frame_index: self.frame_index,
                speed_x: 0.0,
            };
        }

        self.frame_elapsed += dt;
        if !self.dragging {
            self.state_elapsed += dt;
            self.expression_elapsed += dt;
        }

        self.advance_animation();
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
        }
    }

    fn advance_animation(&mut self) {
        let frame_duration = self.frame_duration();
        while self.frame_elapsed >= frame_duration {
            self.frame_elapsed -= frame_duration;
            self.frame_index = (self.frame_index + 1) % FRAME_COUNT;
        }
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
            PetState::Idle => Duration::from_millis(IDLE_FRAME_MS),
            PetState::Walk => Duration::from_millis(WALK_FRAME_MS),
            PetState::Sleep => Duration::from_millis(SLEEP_FRAME_MS),
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
        } else if self.action_override.is_some() {
            BehaviorMode::Action
        } else if self.state == PetState::Walk && self.movement_enabled() {
            BehaviorMode::Walking
        } else {
            BehaviorMode::Default
        };

        self.animation_group = match self.behavior_mode {
            BehaviorMode::Hidden => AnimationGroup::Idle,
            BehaviorMode::Dragging => AnimationGroup::Drag,
            BehaviorMode::Hovered => match self.personality {
                Personality::Calm => AnimationGroup::HoverCalm,
                Personality::Cheerful => AnimationGroup::HoverCheerful,
                Personality::Lively => AnimationGroup::HoverLively,
            },
            BehaviorMode::Action => self
                .action_override
                .map(|action| action.animation_group())
                .unwrap_or_else(|| self.default_expression_group()),
            BehaviorMode::Walking => AnimationGroup::WalkRight,
            BehaviorMode::Default => self.default_expression_group(),
        };
    }

    fn default_expression_group(&self) -> AnimationGroup {
        match self.expression_index % 5 {
            0 => AnimationGroup::Idle,
            1 => AnimationGroup::Blink,
            2 => AnimationGroup::Happy,
            3 => AnimationGroup::Curious,
            _ => AnimationGroup::Sleepy,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_idle_on_frame_zero() {
        let pet = Pet::new();
        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(pet.frame_index(), 0);
    }

    #[test]
    fn cheerful_is_default_personality() {
        let pet = Pet::new();
        assert_eq!(pet.personality(), Personality::Cheerful);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
    }

    #[test]
    fn personality_changes_hover_group() {
        let mut pet = Pet::new();

        pet.apply_personality(Personality::Calm);
        pet.set_hovered(true);
        assert_eq!(pet.current_animation_group(), AnimationGroup::HoverCalm);

        pet.apply_personality(Personality::Cheerful);
        assert_eq!(pet.current_animation_group(), AnimationGroup::HoverCheerful);

        pet.apply_personality(Personality::Lively);
        assert_eq!(pet.current_animation_group(), AnimationGroup::HoverLively);
    }

    #[test]
    fn dragging_overrides_hover_and_movement() {
        let mut pet = Pet::new();
        pet.set_hovered(true);
        pet.set_dragging(true);

        let tick = pet.tick(Duration::from_millis(100));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Dragging);
        assert_eq!(pet.current_animation_group(), AnimationGroup::Drag);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn dragging_pauses_autonomous_state_progression() {
        let mut pet = Pet::new();
        pet.set_dragging(true);

        let tick = pet.tick(Duration::from_secs(10));

        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Dragging);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn dragging_pauses_walk_distance_consumption() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Walk);
        pet.set_dragging(true);

        pet.tick(Duration::from_secs_f32(WALK_DISTANCE / WALK_SPEED));

        assert_eq!(pet.state(), PetState::Walk);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Dragging);
    }

    #[test]
    fn expression_loop_advances_without_requiring_walk() {
        let mut pet = Pet::new();
        let first = pet.current_animation_group();
        pet.tick(Duration::from_secs(3));
        let second = pet.current_animation_group();

        assert_ne!(first, second);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
    }

    #[test]
    fn movement_speed_zero_disables_walk_speed() {
        let mut pet = Pet::new();
        pet.set_movement_speed_multiplier(0.0);
        pet.force_state_for_test(PetState::Walk);

        let tick = pet.tick(Duration::from_millis(16));

        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn movement_speed_zero_prevents_entering_stuck_walk_state() {
        let mut pet = Pet::new();
        pet.set_movement_speed_multiplier(0.0);

        let tick = pet.tick(Duration::from_secs(5));

        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn movement_speed_multiplier_controls_walk_completion_distance() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Walk);
        pet.set_movement_speed_multiplier(2.0);

        pet.tick(Duration::from_secs_f32(WALK_DISTANCE / (WALK_SPEED * 2.0)));

        assert_eq!(pet.state(), PetState::Idle);
    }

    #[test]
    fn movement_speed_update_refreshes_behavior_immediately() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Walk);
        assert_eq!(pet.behavior_mode(), BehaviorMode::Walking);

        pet.set_movement_speed_multiplier(0.0);

        assert_eq!(pet.behavior_mode(), BehaviorMode::Default);
        assert_eq!(pet.current_animation_group(), AnimationGroup::Idle);
    }

    #[test]
    fn nap_micro_action_uses_sleepy_group_and_stops_movement() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Walk);

        pet.start_micro_action(MicroAction::Nap);
        let tick = pet.tick(Duration::from_millis(16));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Action);
        assert_eq!(pet.current_animation_group(), AnimationGroup::Sleepy);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn cheer_up_micro_action_uses_happy_group_temporarily() {
        let mut pet = Pet::new();

        pet.start_micro_action(MicroAction::CheerUp);
        pet.tick(Duration::from_secs(7));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Action);
        assert_eq!(pet.current_animation_group(), AnimationGroup::Happy);

        pet.tick(Duration::from_secs(1));

        assert_eq!(pet.behavior_mode(), BehaviorMode::Walking);
        assert_eq!(pet.current_animation_group(), AnimationGroup::WalkRight);
    }

    #[test]
    fn hover_overrides_micro_action_until_hover_ends() {
        let mut pet = Pet::new();

        pet.start_micro_action(MicroAction::CheerUp);
        pet.set_hovered(true);

        assert_eq!(pet.behavior_mode(), BehaviorMode::Hovered);
        assert_eq!(pet.current_animation_group(), AnimationGroup::HoverCheerful);

        pet.set_hovered(false);

        assert_eq!(pet.behavior_mode(), BehaviorMode::Action);
        assert_eq!(pet.current_animation_group(), AnimationGroup::Happy);
    }

    #[test]
    fn idle_animation_advances_every_200ms() {
        let mut pet = Pet::new();
        let tick = pet.tick(Duration::from_millis(200));
        assert_eq!(tick.frame_index, 1);
        assert_eq!(tick.state, PetState::Idle);
    }

    #[test]
    fn idle_transitions_to_walk_after_threshold() {
        let mut pet = Pet::new_with_seed(1);
        pet.tick(Duration::from_secs(5));
        assert_eq!(pet.state(), PetState::Walk);
    }

    #[test]
    fn idle_transitions_to_walk_after_incremental_expression_ticks() {
        let mut pet = Pet::new_with_seed(1);

        for _ in 0..25 {
            pet.tick(Duration::from_millis(200));
        }

        assert_eq!(pet.state(), PetState::Walk);
    }

    #[test]
    fn forced_walk_preserves_walk_distance_invariant() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Walk);

        let tick = pet.tick(Duration::from_millis(1));

        assert_eq!(pet.state(), PetState::Walk);
        assert_eq!(tick.state, PetState::Walk);
        assert_eq!(tick.speed_x, WALK_SPEED);
    }

    #[test]
    fn walk_speed_sign_follows_seed_direction() {
        let mut right_pet = Pet::new_with_seed(0);
        right_pet.force_state_for_test(PetState::Walk);
        assert_eq!(right_pet.direction(), Direction::Right);
        assert_eq!(right_pet.tick(Duration::ZERO).speed_x, WALK_SPEED);

        let mut left_pet = Pet::new_with_seed(1);
        left_pet.force_state_for_test(PetState::Walk);
        assert_eq!(left_pet.direction(), Direction::Left);
        assert_eq!(left_pet.tick(Duration::ZERO).speed_x, -WALK_SPEED);
    }

    #[test]
    fn turn_around_reverses_walk_speed_direction() {
        let mut pet = Pet::new_with_seed(0);
        pet.force_state_for_test(PetState::Walk);

        pet.turn_around();

        assert_eq!(pet.direction(), Direction::Left);
        assert_eq!(pet.tick(Duration::ZERO).speed_x, -WALK_SPEED);
    }

    #[test]
    fn walk_returns_to_idle_after_configured_distance() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Walk);

        let tick = pet.tick(Duration::from_secs_f32(WALK_DISTANCE / WALK_SPEED));

        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(tick.state, PetState::Idle);
        assert_eq!(tick.frame_index, 0);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn sleep_returns_to_idle_after_12_seconds() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Sleep);

        let tick = pet.tick(Duration::from_secs(12));

        assert_eq!(pet.state(), PetState::Idle);
        assert_eq!(tick.state, PetState::Idle);
        assert_eq!(tick.frame_index, 0);
        assert_eq!(tick.speed_x, 0.0);
    }

    #[test]
    fn sleep_is_naturally_reachable_after_two_walk_cycles() {
        let mut pet = Pet::new();

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
        let mut pet = Pet::new();

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
        let mut pet = Pet::new_with_seed(1);

        let tick = pet.tick(Duration::from_secs(5));

        assert_eq!(pet.state(), PetState::Walk);
        assert_eq!(tick.state, PetState::Walk);
        assert_eq!(tick.frame_index, 0);
        assert_eq!(tick.speed_x, -WALK_SPEED);
    }

    #[test]
    fn sleep_uses_slow_animation_rate() {
        let mut pet = Pet::new();
        pet.force_state_for_test(PetState::Sleep);
        pet.tick(Duration::from_millis(499));
        assert_eq!(pet.frame_index(), 0);
        pet.tick(Duration::from_millis(1));
        assert_eq!(pet.frame_index(), 1);
    }

    #[test]
    fn set_intent_stores_intent() {
        let mut pet = Pet::new_with_seed(0);
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
        let pet = Pet::new_with_seed(0);
        assert_eq!(pet.intent(), BehaviorIntent::Idle);
    }

    #[test]
    fn set_intent_avoid_rect_interrupts_idle_into_walk() {
        let mut pet = Pet::new_with_seed(0);
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
        let mut pet = Pet::new_with_seed(0); // seed 0 starts Direction::Right
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
        let mut pet = Pet::new_with_seed(0); // seed 0 starts Direction::Right
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
}
