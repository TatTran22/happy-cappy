use crate::physics::{Rect, Vec2};

#[derive(Clone, Debug, PartialEq)]
pub struct DisplaySnapshot {
    pub name: Option<String>,
    pub frame: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplaySet {
    pub primary: DisplaySnapshot,
    pub active: DisplaySnapshot,
    pub all: Vec<DisplaySnapshot>,
    pub union: Rect,
}

impl DisplaySet {
    pub fn new(
        primary: DisplaySnapshot,
        active: DisplaySnapshot,
        additional: Vec<DisplaySnapshot>,
    ) -> Self {
        let mut all = vec![primary.clone()];
        if active != primary {
            all.push(active.clone());
        }
        for display in additional {
            if !all.iter().any(|existing| existing == &display) {
                all.push(display);
            }
        }
        let union = union_rect(&all).unwrap_or(primary.frame);

        Self {
            primary,
            active,
            all,
            union,
        }
    }

    pub fn from_all(
        primary: DisplaySnapshot,
        active: DisplaySnapshot,
        all: Vec<DisplaySnapshot>,
    ) -> Option<Self> {
        if all.is_empty() {
            return None;
        }
        let union = union_rect(&all)?;
        Some(Self {
            primary,
            active,
            all,
            union,
        })
    }

    pub fn primary_height(&self) -> f32 {
        self.primary.frame.max.y - self.primary.frame.min.y
    }
}

pub fn display_containing_point(
    displays: &[DisplaySnapshot],
    point: Vec2,
) -> Option<&DisplaySnapshot> {
    displays
        .iter()
        .find(|display| rect_contains_point(display.frame, point))
}

pub fn display_containing_rect_center(
    displays: &[DisplaySnapshot],
    position: Vec2,
    size: Vec2,
) -> Option<&DisplaySnapshot> {
    let center = Vec2 {
        x: position.x + size.x * 0.5,
        y: position.y + size.y * 0.5,
    };
    display_containing_point(displays, center)
}

pub fn clamp_position_to_rect(position: Vec2, size: Vec2, rect: Rect) -> Vec2 {
    let max_x = (rect.max.x - size.x).max(rect.min.x);
    let max_y = (rect.max.y - size.y).max(rect.min.y);

    Vec2 {
        x: position.x.clamp(rect.min.x, max_x),
        y: position.y.clamp(rect.min.y, max_y),
    }
}

pub fn bounds_from_rect(rect: Rect) -> crate::physics::Bounds {
    crate::physics::Bounds {
        min_x: rect.min.x,
        min_y: rect.min.y,
        max_x: rect.max.x,
        max_y: rect.max.y,
    }
}

fn rect_contains_point(rect: Rect, point: Vec2) -> bool {
    point.x >= rect.min.x && point.x <= rect.max.x && point.y >= rect.min.y && point.y <= rect.max.y
}

fn union_rect(displays: &[DisplaySnapshot]) -> Option<Rect> {
    let first = displays.first()?;
    let mut union = first.frame;
    for display in displays.iter().skip(1) {
        union.min.x = union.min.x.min(display.frame.min.x);
        union.min.y = union.min.y.min(display.frame.min.y);
        union.max.x = union.max.x.max(display.frame.max.x);
        union.max.y = union.max.y.max(display.frame.max.y);
    }
    Some(union)
}

#[cfg(target_os = "macos")]
pub fn display_set_from_screens(active_point: Option<Vec2>) -> Option<DisplaySet> {
    use objc2_app_kit::NSScreen;
    use objc2_foundation::MainThreadMarker;

    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    let first = screens.iter().next()?;
    let primary_height = first.frame().size.height as f32;

    let mut snapshots = Vec::new();
    for screen in screens.iter() {
        let frame = screen.frame();
        let min_y = primary_height - (frame.origin.y + frame.size.height) as f32;
        let max_y = primary_height - frame.origin.y as f32;
        snapshots.push(DisplaySnapshot {
            name: Some(screen.localizedName().to_string()),
            frame: Rect {
                min: Vec2 {
                    x: frame.origin.x as f32,
                    y: min_y,
                },
                max: Vec2 {
                    x: (frame.origin.x + frame.size.width) as f32,
                    y: max_y,
                },
            },
        });
    }

    let primary = snapshots.first()?.clone();
    let active = active_point
        .and_then(|point| display_containing_point(&snapshots, point).cloned())
        .unwrap_or_else(|| primary.clone());
    DisplaySet::from_all(primary, active, snapshots)
}

#[cfg(not(target_os = "macos"))]
pub fn display_set_from_screens(_active_point: Option<Vec2>) -> Option<DisplaySet> {
    None
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

    fn display(name: &str, frame: Rect) -> DisplaySnapshot {
        DisplaySnapshot {
            name: Some(name.to_string()),
            frame,
        }
    }

    #[test]
    fn display_set_uses_union_for_all_current_display_drag_bounds() {
        let primary = display("Built-in", rect(0.0, 0.0, 1000.0, 800.0));
        let external = display("External", rect(1000.0, 0.0, 2200.0, 900.0));
        let set = DisplaySet::new(primary.clone(), primary, vec![external]);

        assert_eq!(set.union, rect(0.0, 0.0, 2200.0, 900.0));
    }

    #[test]
    fn display_containing_rect_center_handles_negative_and_vertical_layouts() {
        let primary = display("Built-in", rect(0.0, 0.0, 1000.0, 800.0));
        let left = display("Left", rect(-900.0, 100.0, 0.0, 800.0));
        let top = display("Top", rect(0.0, -700.0, 1000.0, 0.0));
        let displays = vec![primary, left.clone(), top.clone()];

        assert_eq!(
            display_containing_rect_center(
                &displays,
                Vec2 {
                    x: -450.0,
                    y: 300.0
                },
                Vec2 { x: 100.0, y: 100.0 },
            )
            .map(|display| display.name.as_deref()),
            Some(Some("Left")),
        );
        assert_eq!(
            display_containing_rect_center(
                &displays,
                Vec2 {
                    x: 400.0,
                    y: -350.0
                },
                Vec2 { x: 100.0, y: 100.0 },
            )
            .map(|display| display.name.as_deref()),
            Some(Some("Top")),
        );
    }

    #[test]
    fn clamp_position_to_rect_respects_pet_size() {
        assert_eq!(
            clamp_position_to_rect(
                Vec2 { x: 980.0, y: -20.0 },
                Vec2 { x: 128.0, y: 128.0 },
                rect(0.0, 0.0, 1000.0, 800.0),
            ),
            Vec2 { x: 872.0, y: 0.0 },
        );
    }
}
