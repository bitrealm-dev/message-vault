//! GO SMS Pro–specific phone helpers (shared sanitize lives in `message-phone`).

use phone::sanitize_number;
use regex::Regex;
use std::sync::LazyLock;

static GV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\(1/\d+\)\s*)?you've got a new voicemail from \((\d{3})\)\s*([\d-]+)")
        .expect("gv regex")
});

/// Extract caller digits from a Google Voice voicemail SMS body.
pub(crate) fn parse_google_voice_voicemail_caller(body: &str) -> Option<String> {
    let caps = GV_RE.captures(body)?;
    let digits = sanitize_number(&format!("{}{}", &caps[1], &caps[2]))?;
    if digits.len() < 10 {
        None
    } else {
        Some(digits)
    }
}
