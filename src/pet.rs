use std::time::Duration;

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

#[derive(Debug)]
pub struct Pet {
    state: PetState,
    direction: Direction,
    frame_index: usize,
    frame_elapsed: Duration,
    state_elapsed: Duration,
    walk_distance_remaining: f32,
    completed_walk_cycles: u32,
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

    pub fn tick(&mut self, dt: Duration) -> PetTick {
        self.state_elapsed += dt;
        self.frame_elapsed += dt;

        self.advance_animation();
        self.advance_state(dt);

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
                if self.completed_walk_cycles >= 2 {
                    self.enter_sleep();
                } else {
                    self.enter_walk();
                }
            }
            PetState::Walk => {
                self.walk_distance_remaining -= WALK_SPEED * dt.as_secs_f32();
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
        self.walk_distance_remaining = 0.0;
    }

    fn enter_walk(&mut self) {
        self.state = PetState::Walk;
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
        self.state_elapsed = Duration::ZERO;
        self.walk_distance_remaining = WALK_DISTANCE;
    }

    fn enter_sleep(&mut self) {
        self.state = PetState::Sleep;
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
        self.state_elapsed = Duration::ZERO;
        self.walk_distance_remaining = 0.0;
        self.completed_walk_cycles = 0;
    }

    fn frame_duration(&self) -> Duration {
        match self.state {
            PetState::Idle => Duration::from_millis(IDLE_FRAME_MS),
            PetState::Walk => Duration::from_millis(WALK_FRAME_MS),
            PetState::Sleep => Duration::from_millis(SLEEP_FRAME_MS),
        }
    }

    fn speed_x(&self) -> f32 {
        if self.state != PetState::Walk {
            return 0.0;
        }

        match self.direction {
            Direction::Left => -WALK_SPEED,
            Direction::Right => WALK_SPEED,
        }
    }

    #[cfg(test)]
    fn force_state_for_test(&mut self, state: PetState) {
        self.state = state;
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
        self.state_elapsed = Duration::ZERO;
        self.completed_walk_cycles = 0;
        self.walk_distance_remaining = match state {
            PetState::Walk => WALK_DISTANCE,
            PetState::Idle | PetState::Sleep => 0.0,
        };
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
}
