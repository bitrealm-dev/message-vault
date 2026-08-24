//! Schema management for the vault and accounts databases.
//!
//! Serve and import open their database connections through
//! [`crate::db::engine`] pools (shared pragmas for SQLite) and ensure the
//! schema with `ensure_vault_schema` / `ensure_accounts_schema`. DDL lives in
//! the SQL files embedded at compile time; the functions here apply and
//! evolve it. SQLite and Postgres each have their own DDL variants
//! (`schema/sql/*.sql` and `schema/sql/pg_*.sql`).
//!
//! Schema changes are versioned with `PRAGMA user_version` on SQLite (see
//! [`SCHEMA_VERSION`]). The rule is: any schema change requires a fresh
//! reload of data, so an out-of-date database is rebuilt empty from the
//! embedded DDL instead of being patched in place. Postgres has no
//! user_version pragma; its DDL is idempotent (`IF NOT EXISTS`) and is
//! applied directly.

use anyhow::Result;
use sqlx::AnyConnection;

use crate::db::dialect;
use crate::db::engine::DbEngine;

/// Baseline DDL lives in `schema/sql/` (shared with the web app via sync script).
const ACCOUNTS_DDL: &str = include_str!("../../../../../schema/sql/accounts.sql");
const MESSAGE_TABLES_DDL: &str = include_str!("../../../../../schema/sql/messages.sql");
const STAGING_TABLES_DDL: &str = include_str!("../../../../../schema/sql/staging.sql");
const CONTACTS_TABLES_DDL: &str = include_str!("../../../../../schema/sql/contacts.sql");
const FTS_VIRTUAL_DDL: &str = include_str!("../../../../../schema/sql/fts_virtual.sql");
const DROP_MESSAGES_FTS_TRIGGERS_SQL: &str =
    include_str!("../../../../../schema/sql/fts_triggers_drop.sql");
const CREATE_MESSAGES_FTS_TRIGGERS_SQL: &str =
    include_str!("../../../../../schema/sql/fts_triggers_create.sql");

/// Postgres DDL variants; the FTS index and its sync triggers are SQLite-only
/// (the Postgres full-text twin is a separate concern).
const PG_ACCOUNTS_DDL: &str = include_str!("../../../../../schema/sql/pg_accounts.sql");
const PG_MESSAGE_TABLES_DDL: &str = include_str!("../../../../../schema/sql/pg_messages.sql");
const PG_STAGING_TABLES_DDL: &str = include_str!("../../../../../schema/sql/pg_staging.sql");
const PG_CONTACTS_TABLES_DDL: &str = include_str!("../../../../../schema/sql/pg_contacts.sql");

/// Current vault schema version, stamped into each SQLite database with
/// `PRAGMA user_version`. Bump this whenever any `schema/sql/*.sql` file
/// changes; a database at any other version is rebuilt empty (see
/// [`migrate_vault_schema`]).
pub const SCHEMA_VERSION: i64 = 1;

/// Bring the database to [`SCHEMA_VERSION`].
///
/// A database already at the current version is left untouched. Anything else
/// — a fresh file, a pre-versioning vault, or one stamped by a different
/// server — is rebuilt empty and stamped; the user re-imports afterwards.
///
/// The only kind of migration is a full rebuild: schema changes require a
/// fresh reload of data, never in-place column patches.
async fn migrate_vault_schema(conn: &mut AnyConnection) -> Result<()> {
    let version = user_version(conn).await?;
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    if version > SCHEMA_VERSION {
        eprintln!(
            "warning: vault schema is version {version}, newer than this server's {SCHEMA_VERSION}; rebuilding empty (re-import your data)"
        );
        rebuild_vault_schema(conn).await?;
    } else {
        if has_user_tables(conn).await? {
            eprintln!(
                "warning: vault schema is version {version}; rebuilding empty at version {SCHEMA_VERSION} (re-import your data)"
            );
        }
        rebuild_vault_schema(conn).await?;
    }
    stamp_user_version(conn, SCHEMA_VERSION).await?;
    Ok(())
}

/// The `user_version` pragma value stamped by [`migrate_vault_schema`].
async fn user_version(conn: &mut AnyConnection) -> Result<i64> {
    Ok(sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *conn)
        .await?)
}

async fn stamp_user_version(conn: &mut AnyConnection, version: i64) -> Result<()> {
    sqlx::query(&format!("PRAGMA user_version = {version}"))
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Whether any user table exists. A fresh file has none, so a first run stays
/// quiet instead of warning about a rebuild.
async fn has_user_tables(conn: &mut AnyConnection) -> Result<bool> {
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(tables > 0)
}

/// Drop every user table and recreate the current schema from the embedded
/// DDL. This is the only kind of migration: schema changes require a fresh
/// reload of data, never in-place column patches.
///
/// Foreign keys are turned OFF for the drop loop: SQLite's FK-aware DROP
/// processing cannot handle a schema whose remaining CREATE statements still
/// reference already-dropped tables ("no such table: main.<dropped>"). The
/// constraints themselves have `ON DELETE` actions, so the drops would
/// cascade cleanly; this is a schema-parse limitation, not a data one.
async fn rebuild_vault_schema(conn: &mut AnyConnection) -> Result<()> {
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await?;
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&mut *conn)
    .await?;
    // `IF EXISTS` keeps this safe when an FTS table's shadow tables were
    // already removed with their parent.
    for table in &tables {
        sqlx::query(&format!("DROP TABLE IF EXISTS \"{table}\""))
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await?;
    apply_vault_ddl(conn).await?;
    Ok(())
}

/// Apply the current embedded DDL: accounts, contacts, messages, staging,
/// then the FTS index and its sync triggers.
async fn apply_vault_ddl(conn: &mut AnyConnection) -> Result<()> {
    execute_batch(conn, ACCOUNTS_DDL).await?;
    // Contacts DDL defines `handles`, the FK target of conversations, participants,
    // messages, and tapbacks (messages.sql) plus account_handles (accounts.sql).
    // Apply it before the tables that reference handles.
    execute_batch(conn, CONTACTS_TABLES_DDL).await?;
    execute_batch(conn, MESSAGE_TABLES_DDL).await?;
    execute_batch(conn, STAGING_TABLES_DDL).await?;
    ensure_messages_fts(conn).await?;
    Ok(())
}

/// Apply the Postgres DDL variants. FTS is not part of the Postgres schema
/// yet; the DDL is idempotent (`IF NOT EXISTS`), so applying it again is a
/// no-op.
async fn apply_postgres_vault_ddl(conn: &mut AnyConnection) -> Result<()> {
    execute_batch(conn, PG_ACCOUNTS_DDL).await?;
    // Same ordering as the SQLite variant: contacts before messages.
    execute_batch(conn, PG_CONTACTS_TABLES_DDL).await?;
    execute_batch(conn, PG_MESSAGE_TABLES_DDL).await?;
    execute_batch(conn, PG_STAGING_TABLES_DDL).await?;
    Ok(())
}

/// Create every table and index required by a current vault.
///
/// SQLite is versioned with `PRAGMA user_version` and rebuilt when the stamp
/// does not match; Postgres DDL is idempotent and applied directly.
///
/// # Errors
///
/// Returns an error when a DDL statement fails.
pub async fn ensure_vault_schema(conn: &mut AnyConnection) -> Result<()> {
    if dialect::engine_of(conn) == DbEngine::Postgres {
        return apply_postgres_vault_ddl(conn).await;
    }
    migrate_vault_schema(conn).await
}

/// Marker that current full-text search (FTS) sync trigger definitions are installed.
pub const MESSAGES_FTS_TRIGGERS_META_KEY: &str = "messages_fts_triggers_v1";

/// Contentless full-text search index over message body/subject plus attachment text.
///
/// SQLite-only for now: the Postgres full-text search index is a separate
/// concern.
async fn ensure_messages_fts(conn: &mut AnyConnection) -> Result<()> {
    if dialect::engine_of(conn) == DbEngine::Postgres {
        return Ok(());
    }
    execute_batch(conn, FTS_VIRTUAL_DDL).await?;

    let triggers_ready: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_meta WHERE key = $1")
        .bind(MESSAGES_FTS_TRIGGERS_META_KEY)
        .fetch_one(&mut *conn)
        .await?;
    if triggers_ready == 0 {
        install_messages_fts_triggers(conn).await?;
    }

    Ok(())
}

/// Drop full-text search sync triggers (used during bulk promote so inserts skip
/// per-row indexing).
///
/// # Errors
///
/// Returns an error when the drop statements fail.
pub(crate) async fn drop_messages_fts_triggers(conn: &mut AnyConnection) -> Result<()> {
    execute_batch(conn, DROP_MESSAGES_FTS_TRIGGERS_SQL).await?;
    sqlx::query("DELETE FROM schema_meta WHERE key = $1")
        .bind(MESSAGES_FTS_TRIGGERS_META_KEY)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Install full-text search sync triggers and mark them ready in `schema_meta`.
///
/// # Errors
///
/// Returns an error when the trigger SQL or metadata write fails.
pub(crate) async fn install_messages_fts_triggers(conn: &mut AnyConnection) -> Result<()> {
    execute_batch(conn, DROP_MESSAGES_FTS_TRIGGERS_SQL).await?;
    execute_batch(conn, CREATE_MESSAGES_FTS_TRIGGERS_SQL).await?;
    sqlx::query(
        "INSERT INTO schema_meta (key, value) VALUES ($1, '1')
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(MESSAGES_FTS_TRIGGERS_META_KEY)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Non-unique indexes on `messages` (kept out of bulk promote inserts, then rebuilt).
/// Unique `ix_messages_account_source_guid` stays in place for `INSERT OR IGNORE` dedup.
const MESSAGES_SECONDARY_INDEX_DDL: &[(&str, &str)] = &[
    (
        "ix_messages_conversation_timestamp",
        "CREATE INDEX IF NOT EXISTS ix_messages_conversation_timestamp ON messages (conversation_id, timestamp)",
    ),
    (
        "ix_messages_conversation_source_timestamp",
        "CREATE INDEX IF NOT EXISTS ix_messages_conversation_source_timestamp ON messages (conversation_id, source, timestamp)",
    ),
    (
        "ix_messages_account_id",
        "CREATE INDEX IF NOT EXISTS ix_messages_account_id ON messages (account_id)",
    ),
    (
        "ix_messages_content_key",
        "CREATE INDEX IF NOT EXISTS ix_messages_content_key ON messages (content_key) WHERE content_key IS NOT NULL AND content_key != ''",
    ),
    (
        "ix_messages_duplicate_of",
        "CREATE INDEX IF NOT EXISTS ix_messages_duplicate_of ON messages (duplicate_of) WHERE duplicate_of IS NOT NULL",
    ),
    (
        "ix_messages_import_id",
        "CREATE INDEX IF NOT EXISTS ix_messages_import_id ON messages (import_id) WHERE import_id IS NOT NULL",
    ),
    (
        "ix_messages_source",
        "CREATE INDEX IF NOT EXISTS ix_messages_source ON messages (source)",
    ),
];

/// Drop secondary `messages` indexes during bulk promote (same transaction as
/// the promote inserts).
pub(crate) async fn drop_messages_secondary_indexes(conn: &mut AnyConnection) -> Result<()> {
    for (name, _) in MESSAGES_SECONDARY_INDEX_DDL {
        sqlx::query(&format!("DROP INDEX IF EXISTS {name}"))
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Recreate secondary `messages` indexes after bulk promote inserts.
pub(crate) async fn create_messages_secondary_indexes(conn: &mut AnyConnection) -> Result<()> {
    for (_, ddl) in MESSAGES_SECONDARY_INDEX_DDL {
        sqlx::query(ddl).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Bulk-index promoted messages (joined via temp `_promote_msg_map`) into the
/// full-text search table `messages_fts`.
/// Call after attachment rows exist so `attachment_text` is complete.
///
/// `_promote_msg_map` also targets messages that already existed before this
/// promotion (so attachments and tapbacks can attach to them), and several
/// staging rows can point at one production row. `messages_fts` stores no
/// copy of the message text, so re-indexing an already indexed row writes
/// extra index entries that a later delete does not fully retract.
/// `min_new_message_id` is the highest `messages.id` that existed before this
/// promotion inserted anything; only distinct production ids above it are
/// indexed here.
pub(crate) async fn index_messages_fts_from_promote_map(
    conn: &mut AnyConnection,
    min_new_message_id: i64,
) -> Result<u64> {
    let n = sqlx::query(
        r#"
        INSERT INTO messages_fts(rowid, body, subject, attachment_text)
        SELECT
            m.id,
            coalesce(m.body, ''),
            coalesce(m.subject, ''),
            coalesce((
                SELECT group_concat(
                    trim(coalesce(a.original_name, '') || ' ' || coalesce(a.transcription, '')),
                    ' '
                )
                FROM attachments a
                WHERE a.message_id = m.id
            ), '')
        FROM (
            SELECT DISTINCT prod_id FROM _promote_msg_map WHERE prod_id > $1
        ) mm
        JOIN messages m ON m.id = mm.prod_id
        "#,
    )
    .bind(min_new_message_id)
    .execute(&mut *conn)
    .await?;
    Ok(n.rows_affected())
}

/// Message ids for one source within one account, bound as `$1` = source, `$2` = account.
const MESSAGE_IDS_FOR_SOURCE: &str = "SELECT m.id FROM messages m \
     JOIN conversations c ON c.id = m.conversation_id \
     WHERE m.source = $1 AND c.account_id = $2";

/// Delete all production messages (and cascaded rows) for one import source within one account.
///
/// # Errors
///
/// Returns an error when a delete or update statement fails.
pub async fn delete_messages_for_source(
    conn: &mut AnyConnection,
    account_id: &str,
    source: &str,
) -> Result<u64> {
    sqlx::query(&format!(
        "DELETE FROM attachments WHERE message_id IN ({MESSAGE_IDS_FOR_SOURCE})"
    ))
    .bind(source)
    .bind(account_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query(&format!(
        "DELETE FROM tapbacks WHERE message_id IN ({MESSAGE_IDS_FOR_SOURCE})"
    ))
    .bind(source)
    .bind(account_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query(&format!(
        "UPDATE messages SET duplicate_of = NULL
         WHERE duplicate_of IN ({MESSAGE_IDS_FOR_SOURCE})"
    ))
    .bind(source)
    .bind(account_id)
    .execute(&mut *conn)
    .await?;
    let n = sqlx::query(
        r#"
        DELETE FROM messages
        WHERE source = $1
          AND conversation_id IN (
              SELECT id FROM conversations WHERE account_id = $2
          )
        "#,
    )
    .bind(source)
    .bind(account_id)
    .execute(&mut *conn)
    .await?;
    Ok(n.rows_affected())
}

/// Clear one account's staging rows (the temporary import area). Child rows
/// are removed by CASCADE. Other accounts are untouched.
///
/// # Errors
///
/// Returns an error when schema setup or the delete fails.
pub async fn reset_staging_for_account(conn: &mut AnyConnection, account_id: &str) -> Result<()> {
    ensure_vault_schema(conn).await?;
    sqlx::query("DELETE FROM staging_conversations WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Create current account and vault metadata tables.
///
/// Account tables live in the same database file as the rest of the vault, so
/// the one `user_version` stamp covers them on SQLite. A stamped database
/// needs nothing; anything else gets the full vault schema (with the rebuild
/// that implies). On Postgres the idempotent DDL is applied directly.
///
/// # Errors
///
/// Returns an error when a DDL statement fails.
pub async fn ensure_accounts_schema(conn: &mut AnyConnection) -> Result<()> {
    if dialect::engine_of(conn) == DbEngine::Postgres {
        return ensure_vault_schema(conn).await;
    }
    if user_version(conn).await? != SCHEMA_VERSION {
        ensure_vault_schema(conn).await?;
    }
    Ok(())
}

/// True when `table` exists on this engine.
pub async fn table_exists(conn: &mut AnyConnection, name: &str) -> Result<bool> {
    let found: i64 = if dialect::engine_of(conn) == DbEngine::Postgres {
        sqlx::query_scalar("SELECT COUNT(*) FROM pg_catalog.pg_tables WHERE tablename = $1")
            .bind(name)
            .fetch_one(&mut *conn)
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = $1")
            .bind(name)
            .fetch_one(&mut *conn)
            .await?
    };
    Ok(found > 0)
}

/// Column names of `table` in ordinal order.
#[cfg(test)]
async fn table_columns(conn: &mut AnyConnection, table: &str) -> Result<Vec<String>> {
    if dialect::engine_of(conn) == DbEngine::Postgres {
        return Ok(sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns
             WHERE table_name = $1 ORDER BY ordinal_position",
        )
        .bind(table)
        .fetch_all(&mut *conn)
        .await?);
    }
    Ok(sqlx::query_scalar("SELECT name FROM pragma_table_info($1)")
        .bind(table)
        .fetch_all(&mut *conn)
        .await?)
}

/// True when an index named `name` exists on this engine.
#[cfg(test)]
async fn index_exists(conn: &mut AnyConnection, name: &str) -> Result<bool> {
    let found: i64 = if dialect::engine_of(conn) == DbEngine::Postgres {
        sqlx::query_scalar("SELECT COUNT(*) FROM pg_catalog.pg_indexes WHERE indexname = $1")
            .bind(name)
            .fetch_one(&mut *conn)
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = $1")
            .bind(name)
            .fetch_one(&mut *conn)
            .await?
    };
    Ok(found > 0)
}

/// True when a trigger named `name` exists on this engine.
#[cfg(test)]
async fn trigger_exists(conn: &mut AnyConnection, name: &str) -> Result<bool> {
    let found: i64 = if dialect::engine_of(conn) == DbEngine::Postgres {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.triggers WHERE trigger_name = $1",
        )
        .bind(name)
        .fetch_one(&mut *conn)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = $1",
        )
        .bind(name)
        .fetch_one(&mut *conn)
        .await?
    };
    Ok(found > 0)
}

/// Split a multi-statement DDL batch into individual statements.
///
/// The schema files follow a fixed format: comments are whole `--` lines,
/// ordinary statements end with `;` at end of line, and trigger bodies are
/// the only multi-line statements (each ends with a line ending in `END;`).
pub fn split_ddl(batch: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_trigger = false;
    for line in batch.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        if trimmed.starts_with("CREATE TRIGGER") {
            in_trigger = true;
        }
        current.push_str(line);
        current.push('\n');
        if in_trigger {
            if trimmed.ends_with("END;") {
                statements.push(current.trim_end().to_string());
                current.clear();
                in_trigger = false;
            }
        } else if trimmed.ends_with(';') {
            statements.push(current.trim_end().to_string());
            current.clear();
        }
    }
    debug_assert!(
        current.trim().is_empty(),
        "unterminated DDL statement: {current}"
    );
    statements
}

/// Run every statement in a DDL batch against one connection.
async fn execute_batch(conn: &mut AnyConnection, batch: &str) -> Result<()> {
    for stmt in split_ddl(batch) {
        sqlx::query(&stmt).execute(&mut *conn).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::engine::test_pool;

    const A1: &str = "11111111-1111-1111-1111-111111111111";
    const A2: &str = "22222222-2222-2222-2222-222222222222";

    async fn insert_message(conn: &mut AnyConnection, id: i64, guid: &str, body: &str) {
        sqlx::query(
            r#"
            INSERT INTO messages (
                id, conversation_id, account_id, source, guid,
                timestamp, is_from_me, sort_order, body
            ) VALUES ($1, 1, $2, 'imessage', $3, '2020-01-01T00:00:00Z', 0, 0, $4)
            "#,
        )
        .bind(id)
        .bind(A1)
        .bind(guid)
        .bind(body)
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    async fn conversation_id(conn: &mut AnyConnection, account: &str) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT id FROM conversations WHERE account_id = $1")
            .bind(account)
            .fetch_one(&mut *conn)
            .await
            .unwrap()
    }

    /// Column names of `table`, for contract assertions.
    async fn column_names(conn: &mut AnyConnection, table: &str) -> Vec<String> {
        table_columns(conn, table).await.unwrap()
    }

    async fn fts_hits(conn: &mut AnyConnection, term: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH $1",
        )
        .bind(term)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    }

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir) {
        let (pool, dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        ensure_vault_schema(&mut conn).await.unwrap();
        for (id, user) in [(A1, "alice"), (A2, "bob")] {
            sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, $2)")
                .bind(id)
                .bind(user)
                .execute(&mut *conn)
                .await
                .unwrap();
            let handle_id: i64 = sqlx::query_scalar(
                "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
                 VALUES ($1, '+15555550100', '+15555550100', 'phone', 'phone')
                 RETURNING id",
            )
            .bind(id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
            sqlx::query(
                r#"
                INSERT INTO conversations (
                    account_id, chat_handle_id, conversation_type,
                    group_title, exported_at, source_file
                ) VALUES ($1, $2, 'individual', NULL, NULL, 't.json')
                "#,
            )
            .bind(id)
            .bind(handle_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        }
        (pool, dir)
    }

    #[tokio::test]
    async fn promote_fts_indexing_covers_only_rows_inserted_by_this_promotion() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        // An earlier import already indexed this row through the insert trigger.
        insert_message(&mut conn, 10, "g-existing", "carriedover").await;
        let max_id_before_promote: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM messages")
                .fetch_one(&mut *conn)
                .await
                .unwrap();

        drop_messages_fts_triggers(&mut conn).await.unwrap();
        insert_message(&mut conn, 11, "g-new", "freshbody").await;
        // Append promotion maps existing GUIDs (so child rows find their parent)
        // alongside newly inserted rows, and one production row can be the target
        // of more than one staging row.
        execute_batch(
            &mut conn,
            r#"
            CREATE TEMP TABLE _promote_msg_map (
                staging_id INTEGER PRIMARY KEY,
                prod_id INTEGER NOT NULL
            );
            INSERT INTO _promote_msg_map (staging_id, prod_id) VALUES (1, 10), (2, 11), (3, 11);
            "#,
        )
        .await
        .unwrap();

        let indexed = index_messages_fts_from_promote_map(&mut conn, max_id_before_promote)
            .await
            .unwrap();
        install_messages_fts_triggers(&mut conn).await.unwrap();

        assert_eq!(
            indexed, 1,
            "only rows inserted by this promotion may be indexed"
        );
        for (term, expected) in [("carriedover", 1), ("freshbody", 1)] {
            let hits: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH $1")
                    .bind(term)
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap();
            assert_eq!(hits, expected, "unexpected match count for {term}");
        }
    }

    #[tokio::test]
    async fn fresh_vault_has_complete_current_schema() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        ensure_vault_schema(&mut conn).await.unwrap();
        assert_current_schema_contract(&mut conn).await;
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION, "a fresh vault is stamped at once");
        // Ensuring again on a current vault is a no-op.
        ensure_vault_schema(&mut conn).await.unwrap();
        assert_current_schema_contract(&mut conn).await;
    }

    /// Assert every table, index, trigger, metadata marker, and column the
    /// current schema contract lists is present.
    async fn assert_current_schema_contract(conn: &mut AnyConnection) {
        let contract: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../tests/fixtures/schema/current-schema.json"
        ))
        .unwrap();

        for table in contract["tables"].as_array().unwrap() {
            let table = table.as_str().unwrap();
            assert!(
                table_exists(conn, table).await.unwrap(),
                "missing table {table}"
            );
        }
        for index in contract["indexes"].as_array().unwrap() {
            let index = index.as_str().unwrap();
            assert!(
                index_exists(conn, index).await.unwrap(),
                "missing index {index}"
            );
        }
        for trigger in contract["triggers"].as_array().unwrap() {
            let trigger = trigger.as_str().unwrap();
            assert!(
                trigger_exists(conn, trigger).await.unwrap(),
                "missing trigger {trigger}"
            );
        }
        for key in contract["metadata"].as_array().unwrap() {
            let key = key.as_str().unwrap();
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_meta WHERE key = $1")
                .bind(key)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
            assert!(exists > 0, "missing runtime metadata {key}");
        }

        assert_eq!(
            column_names(&mut *conn, "accounts").await,
            [
                "id",
                "username",
                "read_only",
                "password_hash",
                "preferred_name",
                "hanko_user_id",
                "guest_status"
            ]
        );
        assert_eq!(
            column_names(&mut *conn, "contacts").await,
            ["id", "account_id", "preferred_name", "last_modified"]
        );
        assert_eq!(
            column_names(&mut *conn, "contact_groups").await,
            ["id", "account_id", "name"]
        );
        assert_eq!(
            column_names(&mut *conn, "contact_group_members").await,
            ["contact_id", "group_id"]
        );
        assert_eq!(
            column_names(&mut *conn, "handles").await,
            [
                "id",
                "account_id",
                "raw",
                "normalized",
                "normalized_note",
                "handle_type",
                "service"
            ]
        );
        assert!(
            column_names(&mut *conn, "conversations")
                .await
                .iter()
                .any(|c| c == "chat_handle_id")
        );
        for column in ["account_id", "source", "content_key", "duplicate_of"] {
            assert!(
                column_names(&mut *conn, "messages")
                    .await
                    .iter()
                    .any(|c| c == column)
            );
        }
        assert!(
            column_names(&mut *conn, "staging_messages")
                .await
                .iter()
                .any(|c| c == "account_id")
        );
        assert!(
            column_names(&mut *conn, "attachments")
                .await
                .iter()
                .any(|c| c == "size_bytes")
        );
        assert!(
            column_names(&mut *conn, "attachments")
                .await
                .iter()
                .any(|c| c == "missing_reason")
        );
        assert!(
            column_names(&mut *conn, "staging_attachments")
                .await
                .iter()
                .any(|c| c == "size_bytes")
        );
        assert!(
            column_names(&mut *conn, "staging_attachments")
                .await
                .iter()
                .any(|c| c == "missing_reason")
        );
    }

    #[tokio::test]
    async fn same_source_guid_allowed_across_accounts() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        for (conv, account) in [
            (conversation_id(&mut conn, A1).await, A1),
            (conversation_id(&mut conn, A2).await, A2),
        ] {
            sqlx::query(
                r#"
                INSERT INTO messages (
                    conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order
                ) VALUES ($1, $2, 'sms', 'same-guid', '2020-01-01T00:00:00Z', 0, 0)
                "#,
            )
            .bind(conv)
            .bind(account)
            .execute(&mut *conn)
            .await
            .unwrap();
        }
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE guid = 'same-guid'")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn reset_staging_for_account_leaves_other_accounts() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        for account in [A1, A2] {
            let conversation_id: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO staging_conversations (
                    account_id, chat_handle_id, conversation_type,
                    group_title, exported_at, source_file
                ) VALUES ($1, 1, 'individual', NULL, NULL, 't.json')
                RETURNING id
                "#,
            )
            .bind(account)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
            sqlx::query(
                r#"
                INSERT INTO staging_messages (
                    conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order
                ) VALUES ($1, $2, 'sms', 'g1', '2020-01-01T00:00:00Z', 0, 0)
                "#,
            )
            .bind(conversation_id)
            .bind(account)
            .execute(&mut *conn)
            .await
            .unwrap();
        }

        reset_staging_for_account(&mut conn, A1).await.unwrap();
        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM staging_conversations WHERE account_id = $1")
                .bind(A2)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        let messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM staging_messages")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(remaining, 1);
        assert_eq!(messages, 1);
    }

    #[tokio::test]
    async fn old_vault_rebuilds_empty_at_current_version() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        // A pre-versioning vault from the pre-groups era: contact_labels
        // tables, no user_version stamp.
        execute_batch(
            &mut conn,
            include_str!("../../../../../tests/fixtures/schema/v0-vault.sql"),
        )
        .await
        .unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(A1)
            .execute(&mut *conn)
            .await
            .unwrap();
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Ada') RETURNING id",
        )
        .bind(A1)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let label_id: i64 = sqlx::query_scalar(
            "INSERT INTO contact_labels (account_id, name) VALUES ($1, 'Family') RETURNING id",
        )
        .bind(A1)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query("INSERT INTO contact_label_members (contact_id, label_id) VALUES ($1, $2)")
            .bind(contact_id)
            .bind(label_id)
            .execute(&mut *conn)
            .await
            .unwrap();

        ensure_vault_schema(&mut conn).await.unwrap();

        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION, "old vault must be stamped current");
        assert!(!table_exists(&mut conn, "contact_labels").await.unwrap());
        assert!(
            !table_exists(&mut conn, "contact_label_members")
                .await
                .unwrap()
        );
        assert_current_schema_contract(&mut conn).await;
        let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        let contacts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contacts")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(accounts, 0, "rebuild drops old vault data");
        assert_eq!(contacts, 0, "rebuild drops old vault data");
    }

    #[tokio::test]
    async fn current_version_vault_keeps_data_across_reensure() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        ensure_vault_schema(&mut conn).await.unwrap();
        let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(
            accounts, 2,
            "re-ensuring a current vault must not wipe data"
        );
    }

    #[tokio::test]
    async fn newer_version_vault_rebuilds_to_current() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        stamp_user_version(&mut conn, SCHEMA_VERSION + 1)
            .await
            .unwrap();
        ensure_vault_schema(&mut conn).await.unwrap();
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION, "downgrade rebuilds at current");
        let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(accounts, 0, "downgrade rebuild drops data");
    }

    #[tokio::test]
    async fn guest_status_column_exists_and_defaults_null() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        ensure_accounts_schema(&mut conn).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(A1)
            .execute(&mut *conn)
            .await
            .unwrap();
        let status: Option<String> =
            sqlx::query_scalar("SELECT guest_status FROM accounts WHERE id = $1")
                .bind(A1)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(status, None);
    }

    #[tokio::test]
    async fn fresh_accounts_default_to_writable() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        ensure_accounts_schema(&mut conn).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'fresh')")
            .bind(A1)
            .execute(&mut *conn)
            .await
            .unwrap();
        let read_only: i64 = sqlx::query_scalar("SELECT read_only FROM accounts WHERE id = $1")
            .bind(A1)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(read_only, 0);
    }

    #[tokio::test]
    async fn messages_fts_stays_in_sync() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let conversation_id: i64 =
            sqlx::query_scalar("SELECT id FROM conversations WHERE account_id = $1")
                .bind(A1)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        let message_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body, subject
            ) VALUES ($1, $2, 'sms', 'g1', '2020-01-01T00:00:00Z', 0, 0, 'hello vault', NULL)
            RETURNING id
            "#,
        )
        .bind(conversation_id)
        .bind(A1)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO attachments (message_id, original_name, transcription) VALUES ($1, 'voice.m4a', 'secret phrase')",
        )
        .bind(message_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        assert_eq!(fts_hits(&mut conn, "vault").await, 1);
        assert_eq!(fts_hits(&mut conn, "secret").await, 1);

        sqlx::query("UPDATE messages SET body = 'goodbye' WHERE id = $1")
            .bind(message_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        assert_eq!(fts_hits(&mut conn, "vault").await, 0);
        assert_eq!(fts_hits(&mut conn, "goodbye").await, 1);

        sqlx::query("DELETE FROM attachments WHERE message_id = $1")
            .bind(message_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("DELETE FROM messages WHERE id = $1")
            .bind(message_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        assert_eq!(fts_hits(&mut conn, "goodbye").await, 0);
    }

    #[test]
    fn split_ddl_keeps_trigger_bodies_intact() {
        let create = include_str!("../../../../../schema/sql/fts_triggers_create.sql");
        let drop = include_str!("../../../../../schema/sql/fts_triggers_drop.sql");
        let fts = include_str!("../../../../../schema/sql/fts_virtual.sql");
        assert_eq!(split_ddl(create).len(), 6, "six sync triggers");
        assert_eq!(split_ddl(drop).len(), 6);
        assert_eq!(split_ddl(fts).len(), 1);
        for stmt in split_ddl(create) {
            assert!(
                stmt.starts_with("CREATE TRIGGER"),
                "unexpected split: {stmt}"
            );
        }
        // A statement is never empty and never ends mid-line.
        for stmt in split_ddl(include_str!("../../../../../schema/sql/messages.sql")) {
            assert!(stmt.ends_with(';'), "statement must end with ;: {stmt}");
            assert!(stmt.starts_with("CREATE"), "unexpected split: {stmt}");
        }
    }

    #[test]
    fn split_ddl_skips_comments_and_blanks() {
        let out =
            split_ddl("-- header\nCREATE TABLE a (x INTEGER);\n\nCREATE TABLE b (y INTEGER);\n");
        assert_eq!(
            out,
            vec!["CREATE TABLE a (x INTEGER);", "CREATE TABLE b (y INTEGER);"]
        );
    }
}
