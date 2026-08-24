//! SQL dialect helpers for queries that cannot be written portably.

use crate::db::engine::DbEngine;

/// Case-insensitive substring match fragment (`%term%` patterns).
///
/// The `?` placeholder form is **only** for the renumber-pass fragments
/// consumed by the Task 5 SqlParam renumberer, which rewrites `?` to the
/// right `$n`; nothing else may use it. Hand-numbered SQL must use
/// [`like_ci_numbered`] instead — sqlx Any does no client-side placeholder
/// rewriting, so a bare `?` is invalid on Postgres.
pub fn like_ci(engine: DbEngine) -> &'static str {
    match engine {
        DbEngine::Sqlite => "LIKE ? COLLATE NOCASE",
        DbEngine::Postgres => "ILIKE ?",
    }
}

/// Case-insensitive substring match with an explicit numbered placeholder
/// (`%term%` patterns), for SQL that hand-numbers its binds. `n` is the
/// 1-based index of the pattern argument in the statement.
pub fn like_ci_numbered(engine: DbEngine, n: usize) -> String {
    match engine {
        DbEngine::Sqlite => format!("LIKE ${n} COLLATE NOCASE"),
        DbEngine::Postgres => format!("ILIKE ${n}"),
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

/// The transaction-begin statement for each engine. SQLite uses IMMEDIATE so
/// overlapping writers fail fast instead of deadlocking; Postgres has no
/// equivalent statement-level mode and uses a plain BEGIN.
pub fn begin_immediate_sql(engine: DbEngine) -> &'static str {
    match engine {
        DbEngine::Sqlite => "BEGIN IMMEDIATE TRANSACTION",
        DbEngine::Postgres => "BEGIN",
    }
}

/// Aggregate many values into one column with U+001F separators (the format
/// the export pipeline expects). SQLite uses GROUP_CONCAT, Postgres string_agg.
pub fn group_concat_unit_separator(engine: DbEngine, col: &str) -> String {
    match engine {
        DbEngine::Sqlite => format!("GROUP_CONCAT({col}, char(31))"),
        DbEngine::Postgres => format!("string_agg({col}, chr(31))"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_ci_fragment_is_stable() {
        assert_eq!(like_ci(DbEngine::Sqlite), "LIKE ? COLLATE NOCASE");
        assert_eq!(like_ci(DbEngine::Postgres), "ILIKE ?");
    }

    #[test]
    fn like_ci_numbered_emits_engine_placeholders() {
        assert_eq!(
            like_ci_numbered(DbEngine::Sqlite, 1),
            "LIKE $1 COLLATE NOCASE"
        );
        assert_eq!(like_ci_numbered(DbEngine::Postgres, 1), "ILIKE $1");
        assert_eq!(
            like_ci_numbered(DbEngine::Sqlite, 3),
            "LIKE $3 COLLATE NOCASE"
        );
        assert_eq!(like_ci_numbered(DbEngine::Postgres, 3), "ILIKE $3");
    }

    #[test]
    fn begin_immediate_sql_emits_engine_statements() {
        assert_eq!(
            begin_immediate_sql(DbEngine::Sqlite),
            "BEGIN IMMEDIATE TRANSACTION"
        );
        assert_eq!(begin_immediate_sql(DbEngine::Postgres), "BEGIN");
    }

    #[test]
    fn group_concat_unit_separator_emits_engine_aggregates() {
        assert_eq!(
            group_concat_unit_separator(DbEngine::Sqlite, "val"),
            "GROUP_CONCAT(val, char(31))"
        );
        assert_eq!(
            group_concat_unit_separator(DbEngine::Postgres, "val"),
            "string_agg(val, chr(31))"
        );
        assert_eq!(
            group_concat_unit_separator(DbEngine::Sqlite, "cl.name"),
            "GROUP_CONCAT(cl.name, char(31))"
        );
        assert_eq!(
            group_concat_unit_separator(DbEngine::Postgres, "cl.name"),
            "string_agg(cl.name, chr(31))"
        );
    }
}
