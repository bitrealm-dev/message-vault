//! Fixed UTC offset parsing (`UTC±HH:MM`).

use chrono::FixedOffset;

/// Parse a fixed UTC offset string into [`FixedOffset`].
///
/// Accepted forms:
/// - `UTC` → +00:00
/// - `UTC+00:00`, `UTC-05:00`, `UTC+05:30`, `UTC+05:45`
///
/// IANA names are rejected.
pub fn parse_utc_offset(raw: &str) -> Result<FixedOffset, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("empty UTC offset".into());
    }
    let upper = s.to_ascii_uppercase();
    if upper == "UTC" || upper == "Z" {
        return FixedOffset::east_opt(0).ok_or_else(|| "invalid UTC offset".into());
    }
    let rest = upper
        .strip_prefix("UTC")
        .ok_or_else(|| format!("expected UTC offset like UTC-05:00, got {raw:?}"))?;
    if rest.is_empty() {
        return FixedOffset::east_opt(0).ok_or_else(|| "invalid UTC offset".into());
    }
    let (sign, body) = match rest.chars().next() {
        Some('+') => (1i32, &rest[1..]),
        Some('-') => (-1i32, &rest[1..]),
        _ => {
            return Err(format!("expected UTC±HH:MM (e.g. UTC-05:00), got {raw:?}"));
        }
    };
    let (hours, minutes) = parse_hh_mm(body)?;
    if hours > 14 || (hours == 14 && minutes > 0) {
        return Err(format!("UTC offset out of range: {raw:?}"));
    }
    if minutes >= 60 {
        return Err(format!("invalid minutes in UTC offset: {raw:?}"));
    }
    let secs = sign * (hours * 3600 + minutes * 60);
    FixedOffset::east_opt(secs).ok_or_else(|| format!("invalid UTC offset: {raw:?}"))
}

fn parse_hh_mm(body: &str) -> Result<(i32, i32), String> {
    let parts: Vec<&str> = body.split(':').collect();
    match parts.as_slice() {
        [hh] => {
            let hours: i32 = hh
                .parse()
                .map_err(|_| format!("invalid hours in UTC offset: {body:?}"))?;
            Ok((hours, 0))
        }
        [hh, mm] => {
            let hours: i32 = hh
                .parse()
                .map_err(|_| format!("invalid hours in UTC offset: {body:?}"))?;
            let minutes: i32 = mm
                .parse()
                .map_err(|_| format!("invalid minutes in UTC offset: {body:?}"))?;
            Ok((hours, minutes))
        }
        _ => Err(format!("expected HH or HH:MM in UTC offset, got {body:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_utc_and_offsets() {
        assert_eq!(parse_utc_offset("UTC").unwrap().local_minus_utc(), 0);
        assert_eq!(parse_utc_offset("UTC+00:00").unwrap().local_minus_utc(), 0);
        assert_eq!(
            parse_utc_offset("UTC-05:00").unwrap().local_minus_utc(),
            -5 * 3600
        );
        assert_eq!(
            parse_utc_offset("UTC+05:30").unwrap().local_minus_utc(),
            5 * 3600 + 30 * 60
        );
        assert_eq!(
            parse_utc_offset("UTC+05:45").unwrap().local_minus_utc(),
            5 * 3600 + 45 * 60
        );
        assert_eq!(
            parse_utc_offset("UTC+14:00").unwrap().local_minus_utc(),
            14 * 3600
        );
    }

    #[test]
    fn rejects_iana() {
        assert!(parse_utc_offset("America/New_York").is_err());
        assert!(parse_utc_offset("Not/AZone").is_err());
    }
}
