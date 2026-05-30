//! Pure-Rust speech-bubble placement geometry (SP4-C). Works entirely in the
//! app's winit/Quartz logical space: primary-display top-left origin, **Y-DOWN**,
//! points — the same space as `physics`, `workspace`, and `move_window_to_pet`.

use crate::physics::{Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailSide {
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BubblePlacement {
    /// Top-left of the bubble, in Y-down logical points.
    pub origin: Vec2,
    pub tail: TailSide,
    /// Tail-tip X measured from `origin.x`, in points.
    pub tail_x: f32,
}

/// Gap between the bubble and the pet.
pub const GAP: f32 = 6.0;
/// Inset kept from the visible-frame edges.
pub const INSET: f32 = 8.0;
/// Corner radius reserved so the tail never overruns a rounded corner.
pub const CORNER_RADIUS: f32 = 11.0;
/// Half-width of the tail-triangle base.
pub const TAIL_HALF: f32 = 7.0;

/// Place a bubble of `size` = `(width, height)` above `pet`, within the
/// `visible` frame. Flips below when there isn't room above, and clamps
/// horizontally to the visible frame with `INSET`. All inputs/outputs are in
/// Y-down logical points.
pub fn place_bubble(pet: Rect, size: (f32, f32), visible: Rect) -> BubblePlacement {
    let (w, h) = size;
    let pet_center_x = (pet.min.x + pet.max.x) * 0.5;

    // Horizontal: center on the pet, then clamp into [min+inset, max-inset-w].
    let min_x = visible.min.x + INSET;
    let max_x = (visible.max.x - INSET - w).max(min_x);
    let origin_x = (pet_center_x - w * 0.5).clamp(min_x, max_x);

    // Vertical: default above (smaller Y); flip below if it crosses the top.
    let above_y = pet.min.y - GAP - h;
    let (origin_y, tail) = if above_y >= visible.min.y + INSET {
        (above_y, TailSide::Down)
    } else {
        let below_y = pet.max.y + GAP;
        if below_y + h <= visible.max.y - INSET {
            (below_y, TailSide::Up)
        } else {
            // Neither above nor below fully fits: prefer above, clamp inward.
            (above_y.max(visible.min.y + INSET), TailSide::Down)
        }
    };

    // Tail points at the pet center, clamped within the rounded body.
    let tail_min = CORNER_RADIUS + TAIL_HALF;
    let tail_max = (w - CORNER_RADIUS - TAIL_HALF).max(tail_min);
    let tail_x = (pet_center_x - origin_x).clamp(tail_min, tail_max);

    BubblePlacement {
        origin: Vec2 {
            x: origin_x,
            y: origin_y,
        },
        tail,
        tail_x,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Rect {
        Rect {
            min: Vec2 { x: min_x, y: min_y },
            max: Vec2 { x: max_x, y: max_y },
        }
    }

    // A roomy screen with the pet comfortably in the middle.
    fn screen() -> Rect {
        rect(0.0, 0.0, 1000.0, 800.0)
    }

    #[test]
    fn defaults_above_with_down_tail() {
        // pet 64x64 at (468,400); bubble 200x80.
        let pet = rect(468.0, 400.0, 532.0, 464.0);
        let p = place_bubble(pet, (200.0, 80.0), screen());
        assert_eq!(p.tail, TailSide::Down);
        // bottom edge = pet.min.y - GAP => origin.y = 400 - 6 - 80 = 314
        assert_eq!(p.origin.y, 314.0);
        // centered: pet center x = 500 => origin.x = 500 - 100 = 400
        assert_eq!(p.origin.x, 400.0);
        // tail points at pet center: 500 - 400 = 100
        assert_eq!(p.tail_x, 100.0);
    }

    #[test]
    fn flips_below_when_no_room_above() {
        // pet near the top edge: above would land at negative y.
        let pet = rect(468.0, 10.0, 532.0, 74.0);
        let p = place_bubble(pet, (200.0, 80.0), screen());
        assert_eq!(p.tail, TailSide::Up);
        // below: origin.y = pet.max.y + GAP = 74 + 6 = 80
        assert_eq!(p.origin.y, 80.0);
    }

    #[test]
    fn clamps_to_left_edge_with_inset() {
        let pet = rect(0.0, 400.0, 64.0, 464.0); // pet center x = 32
        let p = place_bubble(pet, (200.0, 80.0), screen());
        // origin.x clamped to INSET (8), not 32 - 100 = -68
        assert_eq!(p.origin.x, INSET);
        // tail still points at pet center 32 => 32 - 8 = 24, within body
        assert_eq!(p.tail_x, 24.0);
    }

    #[test]
    fn clamps_to_right_edge_with_inset() {
        let pet = rect(936.0, 400.0, 1000.0, 464.0); // center x = 968
        let p = place_bubble(pet, (200.0, 80.0), screen());
        // max origin.x = 1000 - 8 - 200 = 792
        assert_eq!(p.origin.x, 792.0);
        // tail wants 968 - 792 = 176, clamped to max = 200 - 11 - 7 = 182 -> 176 < 182 ok
        assert_eq!(p.tail_x, 176.0);
    }

    #[test]
    fn tail_x_is_clamped_within_rounded_body() {
        // Pet far right so the tail target would exceed the body; expect clamp.
        let pet = rect(990.0, 400.0, 1000.0, 464.0); // center x = 995
        let p = place_bubble(pet, (200.0, 80.0), screen());
        // origin.x clamped to 792; raw tail = 995 - 792 = 203; max = 200-11-7 = 182
        assert_eq!(p.tail_x, 182.0);
    }
}
