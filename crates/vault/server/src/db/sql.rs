//! Shared SQLite query helpers.

use std::collections::HashMap;

use rusqlite::{Connection, Row, params_from_iter};

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

/// Load child rows for a set of parent ids, grouped by the parent id in column 0.
///
/// `build_sql` receives the `IN (...)` placeholder list for the current chunk;
/// `map_row` turns each row into its parent id and value.
pub fn group_rows_by_id<T, E>(
    conn: &Connection,
    ids: &[i64],
    build_sql: impl Fn(&str) -> String,
    map_row: impl Fn(&Row<'_>) -> rusqlite::Result<(i64, T)>,
) -> Result<HashMap<i64, Vec<T>>, E>
where
    E: From<rusqlite::Error>,
{
    fold_in_id_chunks(ids, |chunk| {
        let sql = build_sql(&in_placeholders(chunk.len()));
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(chunk.iter().copied()), &map_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    })
}
