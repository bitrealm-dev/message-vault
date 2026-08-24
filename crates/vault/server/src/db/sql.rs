//! Shared SQL query helpers.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use sqlx::AnyConnection;
use sqlx::any::AnyRow;

/// Max ids per `IN (...)` bind list (SQLite's default variable limit is 999).
pub const SQLITE_IN_CHUNK: usize = 400;

/// Comma-separated hand-numbered `$N` placeholders for an `IN (...)` list of
/// length `n`, starting at 1-based index `start` (the index of the first
/// placeholder in the full statement). sqlx Any does no placeholder rewriting,
/// so the numbers must be explicit.
pub fn in_placeholders(start: usize, n: usize) -> String {
    (start..start + n)
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Load child rows for a set of parent ids, grouped by the parent id in
/// column 0 of each row. `build_sql` receives the `$N` placeholder list for
/// the current chunk of ids (starting at `$1`); `map_row` extracts the parent
/// id and value from each row.
///
/// # Errors
///
/// Returns a database error when a statement fails.
pub async fn group_rows_by_id<T, E>(
    conn: &mut AnyConnection,
    ids: &[i64],
    build_sql: impl Fn(&str) -> String,
    map_row: impl Fn(&AnyRow) -> Result<(i64, T), sqlx::Error>,
) -> Result<HashMap<i64, Vec<T>>, E>
where
    E: From<sqlx::Error>,
{
    let mut map: HashMap<i64, Vec<T>> = HashMap::new();
    for chunk in ids.chunks(SQLITE_IN_CHUNK) {
        let sql = build_sql(&in_placeholders(1, chunk.len()));
        let mut q = sqlx::query(&sql);
        for id in chunk {
            q = q.bind(*id);
        }
        let rows = q.fetch_all(&mut *conn).await?;
        for row in &rows {
            let (id, value) = map_row(row)?;
            map.entry(id).or_default().push(value);
        }
    }
    Ok(map)
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
    mut query_chunk: impl for<'a> FnMut(
        &'a mut AnyConnection,
        &'a [i64],
    ) -> Pin<
        Box<dyn Future<Output = Result<Vec<(i64, T)>, E>> + Send + 'a>,
    >,
) -> Result<HashMap<i64, Vec<T>>, E> {
    let mut map = HashMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    for chunk in ids.chunks(SQLITE_IN_CHUNK) {
        for (id, row) in query_chunk(conn, chunk).await? {
            map.entry(id).or_default().push(row);
        }
    }
    Ok(map)
}
