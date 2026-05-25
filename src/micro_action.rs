use crate::pet::AnimationGroup;
use std::time::Duration;

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

impl ActionOverride {
    pub fn new(action: MicroAction) -> Self {
        Self {
            action,
            remaining: action.duration(),
        }
    }

    pub fn remaining(&self) -> Duration {
        self.remaining
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

    pub fn tick(&mut self, dt: Duration) -> bool {
        self.remaining = self.remaining.saturating_sub(dt);
        self.remaining.is_zero()
    }
}

impl MicroAction {
    fn duration(self) -> Duration {
        match self {
            Self::Nap => Duration::from_secs(30),
            Self::CheerUp => Duration::from_secs(8),
        }
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
