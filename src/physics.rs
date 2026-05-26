#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Physics {
    pub position: Vec2,
    pub velocity: Vec2,
    pub size: Vec2,
    pub bounds: Bounds,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhysicsStep {
    pub bounced_x: bool,
    pub bounced_y: bool,
    pub stopped_x: bool,
    pub stopped_y: bool,
}

impl Physics {
    pub fn update(&mut self, dt_seconds: f32) -> PhysicsStep {
        self.position.x += self.velocity.x * dt_seconds;
        self.position.y += self.velocity.y * dt_seconds;

        let max_position = self.effective_max_position();
        let stopped_x = max_position.x == self.bounds.min_x;
        let stopped_y = max_position.y == self.bounds.min_y;
        let hit_x =
            !stopped_x && (self.position.x < self.bounds.min_x || self.position.x > max_position.x);
        let hit_y =
            !stopped_y && (self.position.y < self.bounds.min_y || self.position.y > max_position.y);

        self.clamp_to_bounds_with(max_position);

        let mut step = PhysicsStep::default();

        if stopped_x {
            self.velocity.x = 0.0;
            step.stopped_x = true;
        } else if hit_x {
            self.velocity.x = -self.velocity.x;
            step.bounced_x = true;
        }
        if stopped_y {
            self.velocity.y = 0.0;
            step.stopped_y = true;
        } else if hit_y {
            self.velocity.y = -self.velocity.y;
            step.bounced_y = true;
        }

        step
    }

    pub fn clamp_to_bounds(&mut self) {
        self.clamp_to_bounds_with(self.effective_max_position());
    }

    fn effective_max_position(&self) -> Vec2 {
        let max_x = (self.bounds.max_x - self.size.x).max(self.bounds.min_x);
        let max_y = (self.bounds.max_y - self.size.y).max(self.bounds.min_y);

        Vec2 { x: max_x, y: max_y }
    }

    fn clamp_to_bounds_with(&mut self, max_position: Vec2) {
        self.position.x = self.position.x.clamp(self.bounds.min_x, max_position.x);
        self.position.y = self.position.y.clamp(self.bounds.min_y, max_position.y);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn intersects(&self, other: &Rect) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
    }
}

impl From<Bounds> for Rect {
    fn from(bounds: Bounds) -> Self {
        Rect {
            min: Vec2 { x: bounds.min_x, y: bounds.min_y },
            max: Vec2 { x: bounds.max_x, y: bounds.max_y },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn physics() -> Physics {
        Physics {
            position: Vec2 { x: 10.0, y: 20.0 },
            velocity: Vec2 { x: 40.0, y: -10.0 },
            size: Vec2 { x: 64.0, y: 64.0 },
            bounds: Bounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 200.0,
                max_y: 200.0,
            },
        }
    }

    #[test]
    fn update_moves_position_by_velocity_times_delta() {
        let mut physics = physics();
        physics.update(0.5);
        assert_eq!(physics.position, Vec2 { x: 30.0, y: 15.0 });
    }

    #[test]
    fn clamp_keeps_pet_inside_bounds_using_sprite_size() {
        let mut physics = physics();
        physics.position = Vec2 { x: 190.0, y: -20.0 };
        physics.clamp_to_bounds();
        assert_eq!(physics.position, Vec2 { x: 136.0, y: 0.0 });
    }

    #[test]
    fn update_bounces_velocity_when_hitting_horizontal_edge() {
        let mut physics = physics();
        physics.position = Vec2 { x: 135.0, y: 20.0 };
        physics.velocity = Vec2 { x: 40.0, y: 0.0 };
        let step = physics.update(1.0);
        assert_eq!(physics.position.x, 136.0);
        assert_eq!(physics.velocity.x, -40.0);
        assert!(step.bounced_x);
        assert!(!step.bounced_y);
    }

    #[test]
    fn update_bounces_velocity_when_hitting_vertical_edge() {
        let mut physics = physics();
        physics.position = Vec2 { x: 10.0, y: 135.0 };
        physics.velocity = Vec2 { x: 0.0, y: 40.0 };
        let step = physics.update(1.0);
        assert_eq!(physics.position.y, 136.0);
        assert_eq!(physics.velocity.y, -40.0);
        assert!(!step.bounced_x);
        assert!(step.bounced_y);
    }

    #[test]
    fn update_stops_velocity_on_axes_too_small_for_pet() {
        let mut physics = physics();
        physics.bounds = Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 50.0,
            max_y: 50.0,
        };
        let step = physics.update(1.0);
        assert_eq!(physics.position, Vec2 { x: 0.0, y: 0.0 });
        assert_eq!(physics.velocity, Vec2 { x: 0.0, y: 0.0 });
        assert_eq!(
            step,
            PhysicsStep {
                bounced_x: false,
                bounced_y: false,
                stopped_x: true,
                stopped_y: true,
            }
        );
    }

    #[test]
    fn rect_intersects_returns_true_for_overlap() {
        let a = Rect { min: Vec2 { x: 0.0, y: 0.0 }, max: Vec2 { x: 10.0, y: 10.0 } };
        let b = Rect { min: Vec2 { x: 5.0, y: 5.0 }, max: Vec2 { x: 15.0, y: 15.0 } };
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn rect_intersects_returns_false_for_disjoint() {
        let a = Rect { min: Vec2 { x: 0.0, y: 0.0 }, max: Vec2 { x: 10.0, y: 10.0 } };
        let b = Rect { min: Vec2 { x: 20.0, y: 20.0 }, max: Vec2 { x: 30.0, y: 30.0 } };
        assert!(!a.intersects(&b));
    }

    #[test]
    fn rect_intersects_returns_false_for_touch_only() {
        let a = Rect { min: Vec2 { x: 0.0, y: 0.0 }, max: Vec2 { x: 10.0, y: 10.0 } };
        let b = Rect { min: Vec2 { x: 10.0, y: 10.0 }, max: Vec2 { x: 20.0, y: 20.0 } };
        assert!(!a.intersects(&b));
    }

    #[test]
    fn rect_from_bounds_round_trips() {
        let bounds = Bounds { min_x: 0.0, min_y: 0.0, max_x: 100.0, max_y: 50.0 };
        let rect: Rect = bounds.into();
        assert_eq!(rect.min, Vec2 { x: 0.0, y: 0.0 });
        assert_eq!(rect.max, Vec2 { x: 100.0, y: 50.0 });
    }
}
