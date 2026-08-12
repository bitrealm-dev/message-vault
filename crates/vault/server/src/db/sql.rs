//! Shared SQLite query helpers.

use std::collections::HashMap;

/// Max ids per `IN (...)` bind list (SQLite's default variable limit is 999).
pub const SQLITE_IN_CHUNK: usize = 400;

pub fn in_placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

pub fn pair_placeholders(n: usize) -> String {
    vec!["(?, ?)"; n].join(",")
}

pub fn fold_in_id_chunks<T, E>(
    ids: &[i64],
    mut query_chunk: impl FnMut(&[i64]) -> Result<Vec<(i64, T)>, E>,
) -> Result<HashMap<i64, Vec<T>>, E> {
    let mut map = HashMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    for chunk in ids.chunks(SQLITE_IN_CHUNK) {
        for (id, row) in query_chunk(chunk)? {
            map.entry(id).or_default().push(row);
        }
    }
    Ok(map)
}
