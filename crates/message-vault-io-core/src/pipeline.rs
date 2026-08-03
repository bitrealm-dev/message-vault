//! Thin helpers shared by exporter CLIs and in-process runners.
//!
//! Kept free of `anyhow` so GUI/core stay lightweight; callers map `String`
//! errors at the edge when needed.

use message_csv::DateRange;

/// Result of a successful exporter [`crate`]-style `run`: human-readable log lines.
#[derive(Debug, Default)]
pub struct RunResult {
    pub messages: Vec<String>,
}

/// Parse optional start/end date strings into a [`DateRange`].
pub fn parse_date_range(
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Result<DateRange, String> {
    DateRange::parse(start_date, end_date).map_err(|e| format!("invalid date range: {e}"))
}

/// Parse optional start/end dates with an optional timezone name (iMazing path).
pub fn parse_date_range_tz(
    start_date: Option<&str>,
    end_date: Option<&str>,
    timezone: Option<&str>,
) -> Result<DateRange, String> {
    DateRange::parse_optional_tz(start_date, end_date, timezone)
        .map_err(|e| format!("invalid date range: {e}"))
}

/// Filesystem-safe stem from a display name or handle (alnum / `-` / `_` / `+`).
pub fn name_stem(value: &str) -> String {
    let raw: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '+' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if raw.is_empty() || raw.chars().all(|c| c == '_') {
        "unknown".to_string()
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_stem_sanitizes() {
        assert_eq!(name_stem("Alice Bob"), "Alice_Bob");
        assert_eq!(name_stem("+15555550100"), "+15555550100");
        assert_eq!(name_stem("!!!"), "unknown");
        assert_eq!(name_stem(""), "unknown");
    }

    #[test]
    fn parse_date_range_rejects_bad() {
        let err = parse_date_range(Some("not-a-date"), None).unwrap_err();
        assert!(err.starts_with("invalid date range:"));
    }
}
