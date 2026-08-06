//! Inclusive start / exclusive end day filters (`YYYY-MM-DD`).

use chrono::{FixedOffset, Local, NaiveDate, TimeZone};

use crate::parse_utc_offset;

/// Message timestamp window: `[start, end)` in Unix seconds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DateRange {
    /// Inclusive lower bound (local/tz midnight), if set.
    pub start_secs: Option<i64>,
    /// Exclusive upper bound (local/tz midnight), if set.
    pub end_secs: Option<i64>,
}

impl DateRange {
    /// Parse optional `YYYY-MM-DD` bounds using the host local timezone.
    pub fn parse(start: Option<&str>, end: Option<&str>) -> Result<Self, String> {
        Self::parse_with(
            |date| {
                Local
                    .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight"))
                    .single()
                    .map(|dt| dt.timestamp())
                    .ok_or_else(|| format!("ambiguous or invalid local midnight for {date}"))
            },
            start,
            end,
        )
    }

    /// Parse optional `YYYY-MM-DD` bounds in a fixed UTC offset.
    pub fn parse_in_offset(
        start: Option<&str>,
        end: Option<&str>,
        offset: FixedOffset,
    ) -> Result<Self, String> {
        Self::parse_with(
            |date| {
                offset
                    .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight"))
                    .single()
                    .map(|dt| dt.timestamp())
                    .ok_or_else(|| format!("ambiguous or invalid midnight for {date} in {offset}"))
            },
            start,
            end,
        )
    }

    /// Parse bounds in a UTC offset string when provided; otherwise host local.
    ///
    /// `tz_name` accepts fixed offsets like `UTC-05:00` (see [`parse_utc_offset`]).
    pub fn parse_optional_tz(
        start: Option<&str>,
        end: Option<&str>,
        tz_name: Option<&str>,
    ) -> Result<Self, String> {
        match tz_name.map(str::trim).filter(|s| !s.is_empty()) {
            None => Self::parse(start, end),
            Some(name) => {
                let offset = parse_utc_offset(name)?;
                Self::parse_in_offset(start, end, offset)
            }
        }
    }

    fn parse_with(
        midnight_secs: impl Fn(NaiveDate) -> Result<i64, String>,
        start: Option<&str>,
        end: Option<&str>,
    ) -> Result<Self, String> {
        let start_secs = match start.map(str::trim).filter(|s| !s.is_empty()) {
            None => None,
            Some(s) => Some(midnight_secs(parse_ymd(s)?)?),
        };
        let end_secs = match end.map(str::trim).filter(|s| !s.is_empty()) {
            None => None,
            Some(s) => Some(midnight_secs(parse_ymd(s)?)?),
        };
        if let (Some(s), Some(e)) = (start_secs, end_secs)
            && s >= e
        {
            return Err("start-date must be before end-date (end is exclusive)".into());
        }
        Ok(Self {
            start_secs,
            end_secs,
        })
    }

    pub fn is_unbounded(&self) -> bool {
        self.start_secs.is_none() && self.end_secs.is_none()
    }

    pub fn contains_secs(&self, secs: i64) -> bool {
        if let Some(start) = self.start_secs
            && secs < start
        {
            return false;
        }
        if let Some(end) = self.end_secs
            && secs >= end
        {
            return false;
        }
        true
    }

    pub fn contains_secs_f64(&self, secs: f64) -> bool {
        if !secs.is_finite() {
            return false;
        }
        self.contains_secs(secs.floor() as i64)
    }
}

fn parse_ymd(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("invalid date '{value}' (expected YYYY-MM-DD)"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    #[test]
    fn blank_is_unbounded() {
        let range = DateRange::parse(None, None).unwrap();
        assert!(range.is_unbounded());
        assert!(range.contains_secs(0));
        assert!(range.contains_secs(i64::MAX / 2));
    }

    #[test]
    fn inclusive_start_exclusive_end_utc() {
        let range =
            DateRange::parse_in_offset(Some("2020-01-01"), Some("2020-01-03"), utc()).unwrap();
        // 2020-01-01 00:00:00 UTC
        assert!(range.contains_secs(1_577_836_800));
        // 2020-01-02 12:00:00 UTC
        assert!(range.contains_secs(1_577_966_400));
        // 2020-01-03 00:00:00 UTC — exclusive end
        assert!(!range.contains_secs(1_578_009_600));
        // 2019-12-31 23:59:59 UTC
        assert!(!range.contains_secs(1_577_836_799));
    }

    #[test]
    fn start_must_precede_end() {
        let err =
            DateRange::parse_in_offset(Some("2020-01-02"), Some("2020-01-02"), utc()).unwrap_err();
        assert!(err.contains("before end-date"));
    }

    #[test]
    fn rejects_bad_date() {
        assert!(DateRange::parse(Some("2020/01/01"), None).is_err());
        assert!(DateRange::parse_optional_tz(None, Some("nope"), Some("UTC")).is_err());
    }

    #[test]
    fn unknown_offset() {
        assert!(
            DateRange::parse_optional_tz(Some("2020-01-01"), None, Some("America/New_York"))
                .is_err()
        );
    }

    #[test]
    fn f64_floors_toward_contains() {
        let range =
            DateRange::parse_in_offset(Some("2020-01-01"), Some("2020-01-02"), utc()).unwrap();
        assert!(range.contains_secs_f64(1_577_836_800.9));
        assert!(!range.contains_secs_f64(1_577_923_200.0)); // 2020-01-02 00:00 UTC
    }

    #[test]
    fn optional_tz_accepts_utc_offset() {
        let range =
            DateRange::parse_optional_tz(Some("2020-01-01"), Some("2020-01-02"), Some("UTC-05:00"))
                .unwrap();
        assert!(!range.is_unbounded());
    }
}
