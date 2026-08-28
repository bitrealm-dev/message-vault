//! Shared SQL query helpers.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use sqlx::AnyConnection;
use sqlx::any::AnyRow;

use super::engine::DbEngine;

/// Max ids per `IN (...)` bind list (SQLite's default variable limit is 999).
pub const SQLITE_IN_CHUNK: usize = 400;

/// SQLite default `SQLITE_MAX_VARIABLE_NUMBER`. Multi-row `INSERT` chunks
/// must keep `columns × rows` at or below this.
pub const SQLITE_MAX_VARIABLES: usize = 999;

/// Postgres protocol bind-parameter cap. Multi-row `INSERT` chunks must
/// keep `columns × rows` at or below this.
pub const POSTGRES_MAX_VARIABLES: usize = 65_535;

/// Practical Postgres `INSERT … VALUES` row cap. The protocol allows
/// thousands of rows; 1000 is where Docker round-trips flatten out
/// without building a half-megabyte statement.
pub const POSTGRES_INSERT_MAX_ROWS: usize = 1000;

/// Largest row count whose binds fit in one statement for `engine`.
/// SQLite: `columns × rows ≤ 999`. Postgres: `columns × rows ≤ 65_535`
/// and at most [`POSTGRES_INSERT_MAX_ROWS`] rows.
pub fn max_rows_for_bind_limit(engine: DbEngine, columns: usize) -> usize {
    if columns == 0 {
        return 0;
    }
    let bind_cap = match engine {
        DbEngine::Sqlite => SQLITE_MAX_VARIABLES,
        DbEngine::Postgres => POSTGRES_MAX_VARIABLES,
    };
    let by_binds = bind_cap / columns;
    match engine {
        DbEngine::Sqlite => by_binds,
        DbEngine::Postgres => by_binds.min(POSTGRES_INSERT_MAX_ROWS),
    }
}

/// Hand-numbered `VALUES` tuples: `($1,$2,$3),($4,$5,$6)` for `row_count` rows
/// of `col_count` columns. sqlx Any does no placeholder rewriting.
pub fn values_tuples(row_count: usize, col_count: usize) -> String {
    (0..row_count)
        .map(|row| {
            let start = row * col_count + 1;
            let inner = (start..start + col_count)
                .map(|i| format!("${i}"))
                .collect::<Vec<_>>()
                .join(",");
            format!("({inner})")
        })
        .collect::<Vec<_>>()
        .join(",")
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::engine::DbEngine;

    #[test]
    fn max_rows_for_bind_limit_respects_sqlite_999() {
        assert_eq!(max_rows_for_bind_limit(DbEngine::Sqlite, 18), 55);
        assert_eq!(max_rows_for_bind_limit(DbEngine::Sqlite, 10), 99);
        assert_eq!(max_rows_for_bind_limit(DbEngine::Sqlite, 6), 166);
        assert_eq!(max_rows_for_bind_limit(DbEngine::Sqlite, 0), 0);
    }

    #[test]
    fn max_rows_for_bind_limit_caps_postgres_at_1000() {
        assert_eq!(max_rows_for_bind_limit(DbEngine::Postgres, 18), 1000);
        assert_eq!(max_rows_for_bind_limit(DbEngine::Postgres, 10), 1000);
        assert_eq!(max_rows_for_bind_limit(DbEngine::Postgres, 6), 1000);
        assert_eq!(max_rows_for_bind_limit(DbEngine::Postgres, 0), 0);
        // 70 columns: 65_535 / 70 = 936, so the bind cap wins over 1000.
        assert_eq!(max_rows_for_bind_limit(DbEngine::Postgres, 70), 936);
        assert!(1000 * 18 < POSTGRES_MAX_VARIABLES);
    }

    #[test]
    fn values_tuples_numbers_placeholders_across_rows() {
        assert_eq!(values_tuples(2, 3), "($1,$2,$3),($4,$5,$6)");
        assert_eq!(values_tuples(1, 2), "($1,$2)");
        assert_eq!(values_tuples(0, 3), "");
    }
}
