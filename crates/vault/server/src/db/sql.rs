//! Shared SQL query helpers.

use std::collections::HashMap;

use sqlx::AnyConnection;

/// Max ids per `IN (...)` bind list (SQLite's default variable limit is 999).
pub const SQLITE_IN_CHUNK: usize = 400;

/// Comma-separated `?` placeholders for an `IN (...)` list of length `n`.
pub fn in_placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

/// Comma-separated `(?, ?)` placeholders for a VALUES list of length `n`.
pub fn pair_placeholders(n: usize) -> String {
    vec!["(?, ?)"; n].join(",")
}

/// Run `query_chunk` on successive slices of `ids` and group the results by id.
/// Each chunk keeps binds under the engine bind limit; `SQLITE_IN_CHUNK` (400)
/// stays as the chunk size for both engines.
///
/// # Errors
///
/// Returns whatever error `query_chunk` returns.
pub async fn fold_in_id_chunks<T, E>(
    conn: &mut AnyConnection,
    ids: &[i64],
    mut query_chunk: impl FnMut(&mut AnyConnection, &[i64]) -> Result<Vec<(i64, T)>, E>,
) -> Result<HashMap<i64, Vec<T>>, E> {
    let mut map = HashMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    for chunk in ids.chunks(SQLITE_IN_CHUNK) {
        for (id, row) in query_chunk(conn, chunk)? {
            map.entry(id).or_default().push(row);
        }
    }
    Ok(map)
}
