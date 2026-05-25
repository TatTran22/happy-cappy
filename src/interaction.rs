use image::RgbaImage;

use crate::physics::Vec2;
use crate::sprite::FrameRect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButtonKind {
    Left,
    Right,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InteractionEvent {
    HoverChanged(bool),
    DragStarted { pointer: Vec2 },
    DragMoved { delta: Vec2 },
    DragEnded { pointer: Vec2 },
    ContextMenuRequested { pointer: Vec2 },
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InteractionState {
    hovered: bool,
    dragging: bool,
    last_pointer: Option<Vec2>,
}

impl InteractionState {
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub fn is_hovered(&self) -> bool {
        self.hovered
    }

    pub fn pointer_moved(
        &mut self,
        pointer: Vec2,
        hit_visible_pixel: bool,
    ) -> Vec<InteractionEvent> {
        let mut events = Vec::new();

        if self.dragging {
            if let Some(previous) = self.last_pointer {
                events.push(InteractionEvent::DragMoved {
                    delta: Vec2 {
                        x: pointer.x - previous.x,
                        y: pointer.y - previous.y,
                    },
                });
            }
            self.last_pointer = Some(pointer);
            return events;
        }

        if self.hovered != hit_visible_pixel {
            self.hovered = hit_visible_pixel;
            events.push(InteractionEvent::HoverChanged(hit_visible_pixel));
        }
        self.last_pointer = Some(pointer);
        events
    }

    pub fn mouse_pressed(
        &mut self,
        pointer: Vec2,
        button: MouseButtonKind,
        hit_visible_pixel: bool,
    ) -> Vec<InteractionEvent> {
        if !hit_visible_pixel {
            return Vec::new();
        }

        self.last_pointer = Some(pointer);
        match button {
            MouseButtonKind::Left => {
                self.dragging = true;
                vec![InteractionEvent::DragStarted { pointer }]
            }
            MouseButtonKind::Right => vec![InteractionEvent::ContextMenuRequested { pointer }],
            MouseButtonKind::Other => Vec::new(),
        }
    }

    pub fn mouse_released(
        &mut self,
        pointer: Vec2,
        button: MouseButtonKind,
        hit_visible_pixel: bool,
    ) -> Vec<InteractionEvent> {
        if button != MouseButtonKind::Left || !self.dragging {
            return Vec::new();
        }

        self.dragging = false;
        self.last_pointer = Some(pointer);

        let mut events = vec![InteractionEvent::DragEnded { pointer }];
        if self.hovered != hit_visible_pixel {
            self.hovered = hit_visible_pixel;
            events.push(InteractionEvent::HoverChanged(hit_visible_pixel));
        }
        events
    }
}

pub fn alpha_hit_test(source: &RgbaImage, rect: FrameRect, point: Vec2) -> bool {
    alpha_hit_test_with_flip(source, rect, point, false)
}

pub fn alpha_hit_test_with_flip(
    source: &RgbaImage,
    rect: FrameRect,
    point: Vec2,
    flip_x: bool,
) -> bool {
    if !point.x.is_finite() || !point.y.is_finite() || point.x < 0.0 || point.y < 0.0 {
        return false;
    }

    let local_x = point.x.floor() as u32;
    let local_y = point.y.floor() as u32;
    if local_x >= rect.width || local_y >= rect.height {
        return false;
    }

    let source_x = if flip_x {
        rect.x
            .checked_add(rect.width)
            .and_then(|x| x.checked_sub(1))
            .and_then(|x| x.checked_sub(local_x))
    } else {
        rect.x.checked_add(local_x)
    };
    let Some(source_x) = source_x else {
        return false;
    };
    let Some(source_y) = rect.y.checked_add(local_y) else {
        return false;
    };
    if source_x >= source.width() || source_y >= source.height() {
        return false;
    }

    source.get_pixel(source_x, source_y).0[3] > 16
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn image() -> RgbaImage {
        let mut image = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 0]));
        image.put_pixel(2, 1, Rgba([10, 20, 30, 255]));
        image
    }

    fn rect() -> FrameRect {
        FrameRect {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        }
    }

    #[test]
    fn alpha_hit_test_accepts_visible_pixel() {
        assert!(alpha_hit_test(&image(), rect(), Vec2 { x: 2.0, y: 1.0 }));
    }

    #[test]
    fn alpha_hit_test_rejects_transparent_pixel() {
        assert!(!alpha_hit_test(&image(), rect(), Vec2 { x: 0.0, y: 0.0 }));
    }

    #[test]
    fn alpha_hit_test_rejects_out_of_bounds() {
        assert!(!alpha_hit_test(&image(), rect(), Vec2 { x: 9.0, y: 9.0 }));
    }

    #[test]
    fn alpha_hit_test_rejects_non_finite_points() {
        let visible_origin = RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255]));

        assert!(!alpha_hit_test(
            &visible_origin,
            rect(),
            Vec2 {
                x: f32::NAN,
                y: 0.0
            }
        ));
        assert!(!alpha_hit_test(
            &visible_origin,
            rect(),
            Vec2 {
                x: 0.0,
                y: f32::INFINITY
            }
        ));
    }

    #[test]
    fn alpha_hit_test_with_flip_samples_mirrored_pixel() {
        assert!(alpha_hit_test_with_flip(
            &image(),
            rect(),
            Vec2 { x: 1.0, y: 1.0 },
            true
        ));
        assert!(!alpha_hit_test_with_flip(
            &image(),
            rect(),
            Vec2 { x: 1.0, y: 1.0 },
            false
        ));
    }

    #[test]
    fn pointer_move_emits_hover_change_once() {
        let mut state = InteractionState::default();

        assert_eq!(
            state.pointer_moved(Vec2 { x: 2.0, y: 1.0 }, true),
            vec![InteractionEvent::HoverChanged(true)]
        );
        assert!(state
            .pointer_moved(Vec2 { x: 2.0, y: 1.0 }, true)
            .is_empty());
        assert_eq!(
            state.pointer_moved(Vec2 { x: 0.0, y: 0.0 }, false),
            vec![InteractionEvent::HoverChanged(false)]
        );
    }

    #[test]
    fn left_press_on_visible_pixel_starts_drag() {
        let mut state = InteractionState::default();

        let events = state.mouse_pressed(Vec2 { x: 2.0, y: 1.0 }, MouseButtonKind::Left, true);

        assert_eq!(
            events,
            vec![InteractionEvent::DragStarted {
                pointer: Vec2 { x: 2.0, y: 1.0 }
            }]
        );
    }

    #[test]
    fn transparent_press_returns_no_events() {
        let mut state = InteractionState::default();

        let events = state.mouse_pressed(Vec2 { x: 0.0, y: 0.0 }, MouseButtonKind::Left, false);

        assert!(events.is_empty());
        assert!(!state.is_dragging());
    }

    #[test]
    fn right_press_on_visible_pixel_requests_context_menu() {
        let mut state = InteractionState::default();

        let events = state.mouse_pressed(Vec2 { x: 2.0, y: 1.0 }, MouseButtonKind::Right, true);

        assert_eq!(
            events,
            vec![InteractionEvent::ContextMenuRequested {
                pointer: Vec2 { x: 2.0, y: 1.0 }
            }]
        );
    }

    #[test]
    fn dragging_move_reports_delta_and_release_ends_drag() {
        let mut state = InteractionState::default();
        state.pointer_moved(Vec2 { x: 2.0, y: 1.0 }, true);
        state.mouse_pressed(Vec2 { x: 2.0, y: 1.0 }, MouseButtonKind::Left, true);

        assert_eq!(
            state.pointer_moved(Vec2 { x: 5.0, y: 4.0 }, true),
            vec![InteractionEvent::DragMoved {
                delta: Vec2 { x: 3.0, y: 3.0 }
            }]
        );
        assert_eq!(
            state.mouse_released(Vec2 { x: 5.0, y: 4.0 }, MouseButtonKind::Left, true),
            vec![InteractionEvent::DragEnded {
                pointer: Vec2 { x: 5.0, y: 4.0 }
            }]
        );
    }

    #[test]
    fn release_after_drag_reconciles_hover_state() {
        let mut state = InteractionState::default();
        state.pointer_moved(Vec2 { x: 2.0, y: 1.0 }, true);
        state.mouse_pressed(Vec2 { x: 2.0, y: 1.0 }, MouseButtonKind::Left, true);

        assert_eq!(
            state.mouse_released(Vec2 { x: 0.0, y: 0.0 }, MouseButtonKind::Left, false),
            vec![
                InteractionEvent::DragEnded {
                    pointer: Vec2 { x: 0.0, y: 0.0 }
                },
                InteractionEvent::HoverChanged(false)
            ]
        );
        assert!(!state.is_dragging());
        assert!(!state.is_hovered());
    }
}
