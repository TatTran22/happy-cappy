//! Pure-Rust speech-bubble content model (SP4-C). No platform dependencies.

/// Per-kind accent for the bubble's dot, derived from a notification `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleAccent {
    Running,
    Message,
    Succeeded,
    NeedsReview,
    Failed,
}

/// What the bubble renders. Constructible directly by any producer; SP4-C
/// derives it from the active notification, but a future producer (e.g. a
/// Hermes agent message) may build one too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BubbleContent {
    pub title: Option<String>,
    pub body: Option<String>,
    pub accent: BubbleAccent,
}

impl BubbleAccent {
    /// Map a notification `kind` to an accent. Unknown kinds borrow `Message`
    /// (mirrors `notification::preset_for`'s default).
    pub fn for_kind(kind: &str) -> Self {
        match kind {
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "needs-review" => Self::NeedsReview,
            "failed" => Self::Failed,
            _ => Self::Message,
        }
    }

    /// Dot color as straight-alpha sRGB components in `[0, 1]`.
    pub fn rgba(self) -> (f32, f32, f32, f32) {
        match self {
            Self::Running => (0.243, 0.482, 0.839, 1.0),   // #3E7BD6
            Self::Message => (0.541, 0.565, 0.612, 1.0),   // #8A909C
            Self::Succeeded => (0.243, 0.608, 0.310, 1.0), // #3E9B4F
            Self::NeedsReview => (0.878, 0.639, 0.180, 1.0), // #E0A32E
            Self::Failed => (0.898, 0.282, 0.302, 1.0),    // #E5484D
        }
    }

    /// `needs-review` / `failed` use a larger dot to draw the eye.
    pub fn emphasized(self) -> bool {
        matches!(self, Self::NeedsReview | Self::Failed)
    }
}

impl BubbleContent {
    /// Build content from a notification's `kind` + raw `label`/`body`. Each
    /// text field is trimmed; empty/whitespace-only becomes `None`. Returns
    /// `None` when BOTH title and body are empty (no bubble is shown).
    pub fn from_parts(kind: &str, label: Option<&str>, body: Option<&str>) -> Option<Self> {
        let clean = |s: Option<&str>| -> Option<String> {
            s.map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        };
        let title = clean(label);
        let body = clean(body);
        if title.is_none() && body.is_none() {
            return None;
        }
        Some(Self {
            title,
            body,
            accent: BubbleAccent::for_kind(kind),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_empty_yields_none() {
        assert_eq!(BubbleContent::from_parts("running", None, None), None);
        assert_eq!(
            BubbleContent::from_parts("running", Some("   "), Some("\t")),
            None
        );
    }

    #[test]
    fn title_only_keeps_title_drops_body() {
        let c = BubbleContent::from_parts("succeeded", Some("  Done  "), Some("   ")).unwrap();
        assert_eq!(c.title.as_deref(), Some("Done"));
        assert_eq!(c.body, None);
        assert_eq!(c.accent, BubbleAccent::Succeeded);
    }

    #[test]
    fn body_only_keeps_body_drops_title() {
        let c = BubbleContent::from_parts("message", None, Some("hello")).unwrap();
        assert_eq!(c.title, None);
        assert_eq!(c.body.as_deref(), Some("hello"));
        assert_eq!(c.accent, BubbleAccent::Message);
    }

    #[test]
    fn accent_maps_known_kinds_and_falls_back_to_message() {
        assert_eq!(BubbleAccent::for_kind("running"), BubbleAccent::Running);
        assert_eq!(BubbleAccent::for_kind("message"), BubbleAccent::Message);
        assert_eq!(BubbleAccent::for_kind("succeeded"), BubbleAccent::Succeeded);
        assert_eq!(
            BubbleAccent::for_kind("needs-review"),
            BubbleAccent::NeedsReview
        );
        assert_eq!(BubbleAccent::for_kind("failed"), BubbleAccent::Failed);
        assert_eq!(BubbleAccent::for_kind("deploy"), BubbleAccent::Message);
    }

    #[test]
    fn needs_review_and_failed_are_emphasized() {
        assert!(BubbleAccent::NeedsReview.emphasized());
        assert!(BubbleAccent::Failed.emphasized());
        assert!(!BubbleAccent::Running.emphasized());
        assert!(!BubbleAccent::Message.emphasized());
        assert!(!BubbleAccent::Succeeded.emphasized());
    }

    #[test]
    fn rgba_is_in_unit_range_and_opaque() {
        for accent in [
            BubbleAccent::Running,
            BubbleAccent::Message,
            BubbleAccent::Succeeded,
            BubbleAccent::NeedsReview,
            BubbleAccent::Failed,
        ] {
            let (r, g, b, a) = accent.rgba();
            for ch in [r, g, b, a] {
                assert!((0.0..=1.0).contains(&ch));
            }
            assert_eq!(a, 1.0);
        }
    }
}
