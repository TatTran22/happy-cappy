use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Maximum bytes accepted for one socket event line.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// Identifier caps (over-length -> reject the event).
pub const KIND_MAX_BYTES: usize = 64;
pub const ANIMATION_NAME_MAX_BYTES: usize = 64;
/// Free-text caps (over-length -> truncate at a char boundary).
pub const TEXT_MAX_BYTES: usize = 1024;
pub const PRIORITY_MIN: i32 = 0;
pub const PRIORITY_MAX: i32 = 100;
pub const TTL_MIN_MS: u64 = 1;
pub const TTL_MAX_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub kind: String,
    #[serde(default)]
    pub animation_name: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub ttl_ms: Option<u64>,
    #[serde(default)]
    pub priority: Option<i32>,
}

/// Default `(priority, ttl_ms)` for a kind. Unknown kinds use the `message` preset.
/// Priorities order attention states (needs-review, failed) above informational ones.
pub fn preset_for(kind: &str) -> (i32, u64) {
    match kind {
        "running" => (10, 180_000),
        "message" => (20, 10_000),
        "succeeded" => (30, 8_000),
        "needs-review" => (80, 120_000),
        "failed" => (90, 30_000),
        _ => (20, 10_000), // message preset
    }
}

pub fn clamp_priority(p: i32) -> i32 {
    p.clamp(PRIORITY_MIN, PRIORITY_MAX)
}

pub fn clamp_ttl(ms: u64) -> u64 {
    ms.clamp(TTL_MIN_MS, TTL_MAX_MS)
}

/// Truncate free text to the largest UTF-8 char boundary at or below `cap` bytes.
pub fn truncate_text(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        return s.to_string();
    }
    let mut end = cap;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[derive(Debug)]
pub enum NotifyParseError {
    TooLong,
    MissingKind,
    FieldTooLong(&'static str),
    Json(serde_json::Error),
}

impl fmt::Display for NotifyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong => write!(f, "event line exceeds {MAX_LINE_BYTES} bytes"),
            Self::MissingKind => write!(f, "event is missing a non-empty 'kind'"),
            Self::FieldTooLong(field) => write!(f, "field '{field}' exceeds its length cap"),
            Self::Json(e) => write!(f, "invalid event JSON: {e}"),
        }
    }
}

impl Error for NotifyParseError {}

impl From<serde_json::Error> for NotifyParseError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Parse + bound one wire line into a `NotificationEvent`.
/// Identifiers over their cap reject the event; free text is truncated at a char boundary.
pub fn parse_notify_line(line: &str) -> Result<NotificationEvent, NotifyParseError> {
    if line.len() > MAX_LINE_BYTES {
        return Err(NotifyParseError::TooLong);
    }
    let mut ev: NotificationEvent = serde_json::from_str(line)?;
    if ev.kind.is_empty() {
        return Err(NotifyParseError::MissingKind);
    }
    if ev.kind.len() > KIND_MAX_BYTES {
        return Err(NotifyParseError::FieldTooLong("kind"));
    }
    if let Some(a) = &ev.animation_name {
        if a.len() > ANIMATION_NAME_MAX_BYTES {
            return Err(NotifyParseError::FieldTooLong("animation_name"));
        }
    }
    ev.label = ev.label.map(|s| truncate_text(&s, TEXT_MAX_BYTES));
    ev.body = ev.body.map(|s| truncate_text(&s, TEXT_MAX_BYTES));
    Ok(ev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_event() {
        let ev = parse_notify_line(r#"{ "kind": "running" }"#).unwrap();
        assert_eq!(ev.kind, "running");
        assert_eq!(ev.animation_name, None);
        assert_eq!(ev.ttl_ms, None);
        assert_eq!(ev.priority, None);
    }

    #[test]
    fn parses_full_event() {
        let ev = parse_notify_line(
            r#"{ "kind": "failed", "animation_name": "notify-failed",
                 "label": "L", "body": "B", "ttl_ms": 5000, "priority": 70 }"#,
        )
        .unwrap();
        assert_eq!(ev.kind, "failed");
        assert_eq!(ev.animation_name.as_deref(), Some("notify-failed"));
        assert_eq!(ev.label.as_deref(), Some("L"));
        assert_eq!(ev.ttl_ms, Some(5000));
        assert_eq!(ev.priority, Some(70));
    }

    #[test]
    fn rejects_empty_kind() {
        assert!(matches!(
            parse_notify_line(r#"{ "kind": "" }"#),
            Err(NotifyParseError::MissingKind)
        ));
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_notify_line("not json").is_err());
    }

    #[test]
    fn rejects_oversized_line() {
        let big = format!(
            r#"{{ "kind": "running", "body": "{}" }}"#,
            "x".repeat(MAX_LINE_BYTES)
        );
        assert!(matches!(
            parse_notify_line(&big),
            Err(NotifyParseError::TooLong)
        ));
    }

    #[test]
    fn rejects_overlong_kind_identifier() {
        let k = "k".repeat(KIND_MAX_BYTES + 1);
        let line = format!(r#"{{ "kind": "{k}" }}"#);
        assert!(matches!(
            parse_notify_line(&line),
            Err(NotifyParseError::FieldTooLong("kind"))
        ));
    }

    #[test]
    fn truncates_body_at_utf8_char_boundary_without_panic() {
        let body = "é".repeat(TEXT_MAX_BYTES); // 2-byte chars; total 2*cap bytes
        let line = format!(
            r#"{{ "kind": "message", "body": {} }}"#,
            serde_json::to_string(&body).unwrap()
        );
        let ev = parse_notify_line(&line).unwrap();
        let out = ev.body.unwrap();
        assert!(out.len() <= TEXT_MAX_BYTES);
        assert!(out.is_char_boundary(out.len()));
    }

    #[test]
    fn preset_unknown_kind_uses_message_defaults() {
        assert_eq!(preset_for("totally-new"), preset_for("message"));
    }

    #[test]
    fn preset_priority_ordering_attention_outranks_informational() {
        let (run, _) = preset_for("running");
        let (succ, _) = preset_for("succeeded");
        let (review, _) = preset_for("needs-review");
        let (fail, _) = preset_for("failed");
        assert!(run < succ && succ < review && review < fail);
    }

    #[test]
    fn clamps_priority_and_ttl() {
        assert_eq!(clamp_priority(-5), PRIORITY_MIN);
        assert_eq!(clamp_priority(999), PRIORITY_MAX);
        assert_eq!(clamp_ttl(0), TTL_MIN_MS);
        assert_eq!(clamp_ttl(u64::MAX), TTL_MAX_MS);
    }
}
