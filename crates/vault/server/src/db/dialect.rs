//! SQL dialect helpers for queries that cannot be written portably.

use crate::db::engine::DbEngine;

/// Case-insensitive substring match (`%term%` patterns).
pub fn like_ci(engine: DbEngine) -> &'static str {
    match engine {
        DbEngine::Sqlite => "LIKE ? COLLATE NOCASE",
        DbEngine::Postgres => "ILIKE ?",
    }
}

/// Current timestamp in the format SQLite's `datetime('now')` produces
/// (`YYYY-MM-DD HH:MM:SS`, UTC), so both engines write identical values.
pub fn now_utc_sql(engine: DbEngine) -> &'static str {
    match engine {
        DbEngine::Sqlite => "datetime('now')",
        DbEngine::Postgres => "to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')",
    }
}

/// Engine for a live connection, for db-module code that has no `DbEngine` in scope.
pub fn engine_of(conn: &sqlx::AnyConnection) -> DbEngine {
    if conn.backend_name() == "PostgreSQL" {
        DbEngine::Postgres
    } else {
        DbEngine::Sqlite
    }
}
