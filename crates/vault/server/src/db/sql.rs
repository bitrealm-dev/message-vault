//! Shared SQLite query helpers.

/// Max ids per `IN (...)` bind list (SQLite's default variable limit is 999).
pub const SQLITE_IN_CHUNK: usize = 400;

pub fn in_placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}
