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
//! user_version pragma; its idempotent DDL (`IF NOT EXISTS`) runs once
//! behind a `schema_meta` marker gate (see [`VAULT_SCHEMA_META_KEY`]).

use anyhow::Result;
use sqlx::AnyConnection;
use sqlx::Connection;

use crate::db::dialect;
use crate::db::engine::DbEngine;

/// Baseline DDL lives in `schema/sql/` (shared with the web app via sync script).
const ACCOUNTS_DDL: &str = include_str!("../../../../../schema/sql/accounts.sql");
const MESSAGE_TABLES_DDL: &str = include_str!("../../../../../schema/sql/messages.sql");
const STAGING_TABLES_DDL: &str = include_str!("../../../../../schema/sql/staging.sql");
const CONTACTS_TABLES_DDL: &str = include_str!("../../../../../schema/sql/contacts.sql");
const SAVED_SEARCHES_DDL: &str = include_str!("../../../../../schema/sql/saved_searches.sql");
const FTS_VIRTUAL_DDL: &str = include_str!("../../../../../schema/sql/fts_virtual.sql");
const DROP_MESSAGES_FTS_TRIGGERS_SQL: &str =
    include_str!("../../../../../schema/sql/fts_triggers_drop.sql");
const CREATE_MESSAGES_FTS_TRIGGERS_SQL: &str =
    include_str!("../../../../../schema/sql/fts_triggers_create.sql");
/// Postgres FTS twin of `FTS_VIRTUAL_DDL` + `CREATE_MESSAGES_FTS_TRIGGERS_SQL`:
/// the `search_tsv` column, GIN index, sync functions, and triggers (all
/// idempotent).
const FTS_POSTGRES_DDL: &str = include_str!("../../../../../schema/sql/fts_postgres.sql");
const DROP_MESSAGES_FTS_TRIGGERS_PG_SQL: &str =
    include_str!("../../../../../schema/sql/fts_postgres_drop.sql");

/// Current vault schema version, stamped into each SQLite database with
/// `PRAGMA user_version`. Bump this whenever any `schema/sql/*.sql` file
/// changes; a database at any other version is rebuilt empty (see
/// [`migrate_vault_schema`]).
pub const SCHEMA_VERSION: i64 = 7;

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
    execute_batch(conn, SAVED_SEARCHES_DDL).await?;
    ensure_messages_fts(conn).await?;
    Ok(())
}

/// The Postgres DDL that creates the vault's own tables, in the order the
/// vault installs it — transpiled from the SQLite originals (see
/// [`crate::db::pg_ddl`]). The installer, the rebuild's drop list, and the
/// drift guard all read this one value, so a DDL file cannot reach one of
/// them and miss the others.
fn pg_vault_table_ddl() -> &'static crate::db::pg_ddl::PgDdl {
    static DDL: std::sync::OnceLock<crate::db::pg_ddl::PgDdl> = std::sync::OnceLock::new();
    DDL.get_or_init(|| {
        crate::db::pg_ddl::transpile(&[
            ACCOUNTS_DDL,
            // Contacts before messages: the messages DDL references contact tables.
            CONTACTS_TABLES_DDL,
            MESSAGE_TABLES_DDL,
            STAGING_TABLES_DDL,
            SAVED_SEARCHES_DDL,
        ])
    })
}

/// Every table name the embedded Postgres DDL creates, parsed from the DDL
/// itself so the rebuild's drop list cannot drift from what the vault
/// installs.
///
/// A SQLite database file belongs to the vault alone, but a Postgres schema
/// may be shared with another application. The rebuild therefore names the
/// vault's own tables instead of sweeping `current_schema()`.
fn pg_vault_table_names() -> Vec<&'static str> {
    const CREATE_TABLE: &str = "CREATE TABLE IF NOT EXISTS ";
    let mut names: Vec<&'static str> = Vec::new();
    for ddl in &pg_vault_table_ddl().files {
        for line in ddl.lines() {
            let Some(rest) = line.trim_start().strip_prefix(CREATE_TABLE) else {
                continue;
            };
            let name = rest
                .trim_start()
                .split(|c: char| c.is_whitespace() || c == '(' || c == ';')
                .next()
                .unwrap_or_default();
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Drop the vault's own tables in the current schema. Postgres twin of
/// [`rebuild_vault_schema`]: a vault stamped with an older marker is
/// rebuilt empty rather than patched in place.
///
/// Only the tables [`pg_vault_table_names`] lists are dropped, and each is
/// schema-qualified, so a vault sharing its schema with another application
/// rebuilds its own data without touching the neighbour's.
///
/// `CASCADE` takes the FTS triggers and foreign keys down with their
/// tables; the sync functions are recreated with `CREATE OR REPLACE`.
async fn drop_pg_user_tables(conn: &mut AnyConnection) -> Result<()> {
    // `::text` because the Any driver has no mapping for Postgres's `name`.
    let schema: String = sqlx::query_scalar("SELECT current_schema()::text")
        .fetch_one(&mut *conn)
        .await?;
    let schema = schema.replace('"', "\"\"");
    for table in pg_vault_table_names() {
        sqlx::query(&format!(
            "DROP TABLE IF EXISTS \"{schema}\".\"{table}\" CASCADE"
        ))
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Apply the Postgres DDL variants. The DDL is idempotent (`IF NOT EXISTS`),
/// so applying it again is a no-op.
async fn apply_postgres_vault_ddl(conn: &mut AnyConnection) -> Result<()> {
    // Installed vaults skip straight past this (one marker lookup per
    // request instead of re-running the DDL batch).
    if pg_vault_schema_ready(&mut *conn).await? {
        return Ok(());
    }
    // One-time install: the advisory lock serializes concurrent
    // first-touches (the trigger drop/create pair is not race-safe), and
    // the re-check under the lock turns a waiter into a no-op.
    let mut tx = conn.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(VAULT_SCHEMA_LOCK_ID)
        .execute(&mut *tx)
        .await?;
    if !pg_vault_schema_ready(&mut tx).await? {
        // A vault carrying an older marker (or none, with tables present)
        // is rebuilt empty — the same contract SQLite's user_version
        // gives. Re-importing is the migration.
        if table_exists(&mut tx, "vault_imports").await? {
            eprintln!(
                "warning: vault schema predates {VAULT_SCHEMA_META_KEY}; rebuilding empty (re-import your data)"
            );
            drop_pg_user_tables(&mut tx).await?;
        }
        // Same ordering as the SQLite variant: contacts before messages.
        for ddl in &pg_vault_table_ddl().files {
            execute_batch(&mut tx, ddl).await?;
        }
        // Post-hoc FKs last: they reference tables created across the DDL
        // sequence (see `pg_ddl` rule 4).
        execute_batch(&mut tx, &pg_vault_table_ddl().deferred_fks).await?;
        // FTS last, same as the SQLite variant: the tsvector column, GIN
        // index, and sync triggers all target tables created above.
        ensure_messages_fts(&mut tx).await?;
        sqlx::query(
            "INSERT INTO schema_meta (key, value) VALUES ($1, '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(VAULT_SCHEMA_META_KEY)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// True when the one-time Postgres DDL marker is present. Also false when
/// `schema_meta` itself does not exist yet (pre-install).
async fn pg_vault_schema_ready(conn: &mut AnyConnection) -> Result<bool> {
    if !table_exists(&mut *conn, "schema_meta").await? {
        return Ok(false);
    }
    let ready: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_meta WHERE key = $1")
        .bind(VAULT_SCHEMA_META_KEY)
        .fetch_one(&mut *conn)
        .await?;
    Ok(ready > 0)
}

/// Create every table and index required by a current vault.
///
/// SQLite is versioned with `PRAGMA user_version` and rebuilt when the stamp
/// does not match; Postgres gates its one-time idempotent install behind a
/// `schema_meta` marker (see [`VAULT_SCHEMA_META_KEY`]) so repeated ensures
/// cost one marker lookup instead of re-running the DDL.
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

/// Marker that the one-time Postgres vault DDL install has completed.
/// Bumped with the schema: a vault holding an older marker is rebuilt
/// empty, matching SQLite's `user_version` behaviour.
pub const VAULT_SCHEMA_META_KEY: &str = "vault_schema_v2";

/// Advisory lock id serializing the one-time Postgres DDL install so two
/// concurrent first-touches cannot interleave the trigger drop/create pair
/// (arbitrary but unique within this database).
const VAULT_SCHEMA_LOCK_ID: i64 = 0x4D56_0001;

/// Full-text search index over message body/subject plus attachment text:
/// contentless FTS5 virtual table with sync triggers on SQLite, a `search_tsv`
/// tsvector column with GIN index and sync triggers on Postgres.
async fn ensure_messages_fts(conn: &mut AnyConnection) -> Result<()> {
    if dialect::engine_of(conn) == DbEngine::Postgres {
        // Postgres has no `CREATE TRIGGER IF NOT EXISTS`, so installing means
        // dropping the six sync triggers and recreating them. That may only
        // run when the marker says they are missing: every schema ensure
        // (each import's reset_staging_for_account) would otherwise drop and
        // recreate the triggers behind a concurrent writer, a silent desync
        // window for rows written in between. install_messages_fts_triggers
        // writes the marker, drop_messages_fts_triggers deletes it.
        let triggers_ready: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM schema_meta WHERE key = $1")
                .bind(MESSAGES_FTS_TRIGGERS_META_KEY)
                .fetch_one(&mut *conn)
                .await?;
        if triggers_ready == 0 {
            install_messages_fts_triggers(conn).await?;
        }
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
/// per-row indexing). On Postgres this is the drop half of the trigger install
/// (the promote path disables triggers instead — see
/// [`disable_fts_triggers_pg`]).
///
/// # Errors
///
/// Returns an error when the drop statements fail.
pub(crate) async fn drop_messages_fts_triggers(conn: &mut AnyConnection) -> Result<()> {
    if dialect::engine_of(conn) == DbEngine::Postgres {
        execute_batch(conn, DROP_MESSAGES_FTS_TRIGGERS_PG_SQL).await?;
    } else {
        execute_batch(conn, DROP_MESSAGES_FTS_TRIGGERS_SQL).await?;
    }
    sqlx::query("DELETE FROM schema_meta WHERE key = $1")
        .bind(MESSAGES_FTS_TRIGGERS_META_KEY)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Install full-text search sync triggers and mark them ready in `schema_meta`.
/// On Postgres the trigger statements are made idempotent by dropping first,
/// exactly like the SQLite path.
///
/// # Errors
///
/// Returns an error when the trigger SQL or metadata write fails.
pub(crate) async fn install_messages_fts_triggers(conn: &mut AnyConnection) -> Result<()> {
    if dialect::engine_of(conn) == DbEngine::Postgres {
        execute_batch(conn, DROP_MESSAGES_FTS_TRIGGERS_PG_SQL).await?;
        execute_batch(conn, FTS_POSTGRES_DDL).await?;
    } else {
        execute_batch(conn, DROP_MESSAGES_FTS_TRIGGERS_SQL).await?;
        execute_batch(conn, CREATE_MESSAGES_FTS_TRIGGERS_SQL).await?;
    }
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

/// Disable the six Postgres FTS sync triggers by name during bulk promote, so
/// per-row FTS sync work is skipped (Postgres has no per-statement "don't run
/// triggers" mode; SQLite drops its FTS triggers instead — see
/// [`drop_messages_fts_triggers`]). Only the FTS triggers are touched: FK
/// constraint triggers stay enabled, so a staging row that violates a foreign
/// key still fails loudly, and the statements need only table ownership (no
/// superuser). The bulk vector fill runs afterwards via
/// [`index_messages_fts_from_promote_map`], then
/// [`enable_fts_triggers_pg`] restores the triggers. Disabling and re-enabling
/// are transactional, so a failed promote rolls the disable back.
///
/// # Errors
///
/// Returns an error when a disable statement fails.
pub(crate) async fn disable_fts_triggers_pg(conn: &mut AnyConnection) -> Result<()> {
    sqlx::query("ALTER TABLE messages DISABLE TRIGGER messages_fts_ai")
        .execute(&mut *conn)
        .await?;
    sqlx::query("ALTER TABLE messages DISABLE TRIGGER messages_fts_au")
        .execute(&mut *conn)
        .await?;
    sqlx::query("ALTER TABLE messages DISABLE TRIGGER messages_fts_ad")
        .execute(&mut *conn)
        .await?;
    sqlx::query("ALTER TABLE attachments DISABLE TRIGGER attachments_fts_ai")
        .execute(&mut *conn)
        .await?;
    sqlx::query("ALTER TABLE attachments DISABLE TRIGGER attachments_fts_ad")
        .execute(&mut *conn)
        .await?;
    sqlx::query("ALTER TABLE attachments DISABLE TRIGGER attachments_fts_au")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Re-enable the six Postgres FTS sync triggers disabled by
/// [`disable_fts_triggers_pg`], by the same names.
///
/// # Errors
///
/// Returns an error when an enable statement fails.
pub(crate) async fn enable_fts_triggers_pg(conn: &mut AnyConnection) -> Result<()> {
    sqlx::query("ALTER TABLE messages ENABLE TRIGGER messages_fts_ai")
        .execute(&mut *conn)
        .await?;
    sqlx::query("ALTER TABLE messages ENABLE TRIGGER messages_fts_au")
        .execute(&mut *conn)
        .await?;
    sqlx::query("ALTER TABLE messages ENABLE TRIGGER messages_fts_ad")
        .execute(&mut *conn)
        .await?;
    sqlx::query("ALTER TABLE attachments ENABLE TRIGGER attachments_fts_ai")
        .execute(&mut *conn)
        .await?;
    sqlx::query("ALTER TABLE attachments ENABLE TRIGGER attachments_fts_ad")
        .execute(&mut *conn)
        .await?;
    sqlx::query("ALTER TABLE attachments ENABLE TRIGGER attachments_fts_au")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Bulk-index promoted messages (joined via temp `_promote_msg_map`).
/// Call after attachment rows exist so `attachment_text` is complete.
/// SQLite inserts into the contentless `messages_fts` table; Postgres fills
/// the `messages.search_tsv` tsvector instead.
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
    if dialect::engine_of(conn) == DbEngine::Postgres {
        let n = sqlx::query(
            r#"
            UPDATE messages SET search_tsv = fts.vec
            FROM (
                SELECT mm.prod_id,
                       to_tsvector('simple',
                           coalesce(m.body, '') || ' ' || coalesce(m.subject, '') || ' ' || coalesce(a.attachment_text, '')) AS vec
                FROM (SELECT DISTINCT prod_id FROM _promote_msg_map WHERE prod_id > $1) mm
                JOIN messages m ON m.id = mm.prod_id
                LEFT JOIN (
                    SELECT message_id,
                           string_agg(trim(coalesce(original_name, '') || ' ' || coalesce(transcription, '')), ' ') AS attachment_text
                    FROM attachments
                    GROUP BY message_id
                ) a ON a.message_id = mm.prod_id
            ) fts
            WHERE messages.id = fts.prod_id
            "#,
        )
        .bind(min_new_message_id)
        .execute(&mut *conn)
        .await?;
        return Ok(n.rows_affected());
    }
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
/// that implies). On Postgres the one-time DDL install is gated by the
/// [`VAULT_SCHEMA_META_KEY`] marker.
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
///
/// Branches on the engine: `pg_catalog.pg_tables` for Postgres, `sqlite_master`
/// for SQLite. Used by [`crate::process_assets::run`] to skip the account
/// sweep on a database that has no vault schema yet.
///
/// The Postgres lookup is restricted to `current_schema()` — the schema the
/// vault reads, writes, and rebuilds — so a same-named table in another
/// schema of the same database never stands in for the vault's own.
pub async fn table_exists(conn: &mut AnyConnection, name: &str) -> Result<bool> {
    let found: i64 = if dialect::engine_of(conn) == DbEngine::Postgres {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM pg_catalog.pg_tables
             WHERE tablename = $1 AND schemaname = current_schema()",
        )
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
/// ordinary statements end with `;` at end of line, and the only multi-line
/// statements are trigger bodies (ending in a line ending with `END;`, or
/// ending on the same line they start), Postgres `DO $$` blocks (ending in a
/// line ending with `$$;`), and Postgres `CREATE OR REPLACE FUNCTION … AS $$`
/// blocks (ending in a line that starts with `$$`).
pub fn split_ddl(batch: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_trigger = false;
    let mut in_do_block = false;
    let mut in_function = false;
    for line in batch.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        let starts_trigger = trimmed.starts_with("CREATE TRIGGER");
        let starts_do_block = trimmed.starts_with("DO $$");
        let starts_function = trimmed.starts_with("CREATE OR REPLACE FUNCTION");
        if starts_trigger {
            in_trigger = true;
        }
        if starts_do_block {
            in_do_block = true;
        }
        if starts_function {
            in_function = true;
        }
        current.push_str(line);
        current.push('\n');
        if in_trigger {
            // Multi-line trigger bodies end with `END;`; a one-line trigger
            // (e.g. `EXECUTE FUNCTION`) ends with `;` on its own line.
            if trimmed.ends_with("END;") || (starts_trigger && trimmed.ends_with(';')) {
                statements.push(current.trim_end().to_string());
                current.clear();
                in_trigger = false;
            }
        } else if in_do_block {
            if trimmed.ends_with("$$;") {
                statements.push(current.trim_end().to_string());
                current.clear();
                in_do_block = false;
            }
        } else if in_function {
            // The body's closing delimiter is its own `$$` line, e.g.
            // `$$ LANGUAGE plpgsql;`.
            if trimmed.starts_with("$$") && trimmed.ends_with(';') {
                statements.push(current.trim_end().to_string());
                current.clear();
                in_function = false;
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

    /// Search hits via the Postgres `search_tsv` vector (`messages_fts` has no
    /// Postgres twin — the tsvector lives on `messages`).
    async fn pg_fts_hits(conn: &mut AnyConnection, term: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM messages WHERE search_tsv @@ plainto_tsquery('simple', $1)",
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
                "password_hash",
                "preferred_name",
                "is_admin",
                "disabled",
                "can_import",
                "can_export",
                "can_delete"
            ]
        );
        assert_eq!(
            column_names(&mut *conn, "contacts").await,
            [
                "id",
                "account_id",
                "preferred_name",
                "origin",
                "created_at",
                "last_modified"
            ]
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
                "service",
                "origin",
                "created_at",
                "last_modified"
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
    async fn one_running_import_per_account() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        ensure_vault_schema(&mut conn).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ('acct', 'alice')")
            .execute(&mut *conn)
            .await
            .unwrap();

        let insert = r#"
            INSERT INTO vault_imports (
                account_id, source, mode, status, started_at,
                message_count, attachment_count, bytes_uploaded
            ) VALUES ('acct', 'imessage', 'append', $1, '2026-08-30T00:00:00Z', 0, 0, 0)
        "#;

        sqlx::query(insert)
            .bind("running")
            .execute(&mut *conn)
            .await
            .expect("first running session inserts");

        let second = sqlx::query(insert)
            .bind("running")
            .execute(&mut *conn)
            .await;
        assert!(second.is_err(), "a second running session must be rejected");

        // A finished session does not occupy the slot.
        sqlx::query(insert)
            .bind("completed")
            .execute(&mut *conn)
            .await
            .expect("a completed session is not covered by the partial index");
    }

    #[tokio::test]
    async fn vault_imports_carries_the_session_columns() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        ensure_vault_schema(&mut conn).await.unwrap();
        sqlx::query(
            "SELECT stage, staging_dir, device_id, form_json, source_fingerprint
             FROM vault_imports WHERE 1 = 0",
        )
        .fetch_optional(&mut *conn)
        .await
        .expect("session columns exist");
    }

    #[tokio::test]
    async fn fresh_accounts_default_to_full_permissions() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        ensure_accounts_schema(&mut conn).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'fresh')")
            .bind(A1)
            .execute(&mut *conn)
            .await
            .unwrap();
        let row: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT is_admin, can_import, can_export, can_delete FROM accounts WHERE id = $1",
        )
        .bind(A1)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(row, (0, 1, 1, 1));
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

    /// The `messages_fts_stays_in_sync` twin for Postgres: the sync triggers
    /// keep `search_tsv` in step with message and attachment edits. Skips
    /// unless `MV_TEST_POSTGRES_URL` is set.
    #[tokio::test]
    async fn messages_fts_stays_in_sync_pg() {
        let Some(url) = crate::pg_test_url() else {
            return;
        };
        let _pg_guard = crate::acquire_pg_test_lock().await;
        sqlx::any::install_default_drivers();
        let pool = sqlx::any::AnyPoolOptions::new()
            .connect(&url)
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        ensure_vault_schema(&mut conn).await.unwrap();
        // The Postgres test database is shared across runs, so clear anything
        // a previous run left behind (the account FKs cascade to handles,
        // conversations, messages, attachments, and tapbacks).
        sqlx::query("DELETE FROM accounts WHERE id = $1")
            .bind(A1)
            .execute(&mut *conn)
            .await
            .unwrap();
        // The shared identity sequence may hand out message ids inside the
        // search-parity corpus range (keys 1..=15): that test binds its keys
        // as explicit ids, so clear the range (the PG_TEST_LOCK above
        // serializes us against the other gated tests).
        sqlx::query("DELETE FROM messages WHERE id BETWEEN 1 AND 15")
            .execute(&mut *conn)
            .await
            .unwrap();

        // One account + conversation, mirroring the SQLite test's setup.
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(A1)
            .execute(&mut *conn)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550100', '+15555550100', 'phone', 'phone')
             RETURNING id",
        )
        .bind(A1)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let conversation_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type,
                group_title, exported_at, source_file
            ) VALUES ($1, $2, 'individual', NULL, NULL, 't.json')
            RETURNING id
            "#,
        )
        .bind(A1)
        .bind(handle_id)
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

        assert_eq!(pg_fts_hits(&mut conn, "vault").await, 1);
        assert_eq!(pg_fts_hits(&mut conn, "secret").await, 1);

        sqlx::query("UPDATE messages SET body = 'goodbye' WHERE id = $1")
            .bind(message_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        assert_eq!(pg_fts_hits(&mut conn, "vault").await, 0);
        assert_eq!(pg_fts_hits(&mut conn, "goodbye").await, 1);

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
        assert_eq!(pg_fts_hits(&mut conn, "goodbye").await, 0);
    }

    /// The `old_vault_rebuilds_empty_at_current_version` twin for Postgres: a
    /// vault stamped with a stale [`VAULT_SCHEMA_META_KEY`] is rebuilt empty
    /// by [`drop_pg_user_tables`] rather than patched in place, so the new
    /// session columns land on an already-installed vault too. Skips unless
    /// `MV_TEST_POSTGRES_URL` is set.
    #[tokio::test]
    async fn stale_postgres_marker_rebuilds_vault_schema_empty() {
        let Some(url) = crate::pg_test_url() else {
            return;
        };
        let _pg_guard = crate::acquire_pg_test_lock().await;
        sqlx::any::install_default_drivers();
        let pool = sqlx::any::AnyPoolOptions::new()
            .connect(&url)
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        ensure_vault_schema(&mut conn).await.unwrap();
        // The Postgres test database is shared across runs, so clear anything
        // a previous run left behind (the account FK cascades to handles,
        // conversations, messages, attachments, and tapbacks).
        sqlx::query("DELETE FROM accounts WHERE id = $1")
            .bind(A1)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(A1)
            .execute(&mut *conn)
            .await
            .unwrap();

        // Roll the marker back to what a vault installed before this schema
        // change would carry, simulating the upgrade scenario the rebuild
        // path exists for.
        sqlx::query("DELETE FROM schema_meta WHERE key = $1")
            .bind(VAULT_SCHEMA_META_KEY)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO schema_meta (key, value) VALUES ('vault_schema_v1', '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        ensure_vault_schema(&mut conn).await.unwrap();

        let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(accounts, 0, "a stale marker rebuilds the vault empty");

        let ready: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_meta WHERE key = $1")
            .bind(VAULT_SCHEMA_META_KEY)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(ready, 1, "the rebuild stamps the current marker");

        sqlx::query(
            "SELECT stage, staging_dir, device_id, form_json, source_fingerprint
             FROM vault_imports WHERE 1 = 0",
        )
        .fetch_optional(&mut *conn)
        .await
        .expect("the rebuilt Postgres vault carries the session columns");
    }

    /// A vault sharing its Postgres schema with another application rebuilds
    /// its own tables and leaves the neighbour's alone. Skips unless
    /// `MV_TEST_POSTGRES_URL` is set.
    #[tokio::test]
    async fn postgres_rebuild_spares_tables_the_vault_does_not_own() {
        let Some(url) = crate::pg_test_url() else {
            return;
        };
        let _pg_guard = crate::acquire_pg_test_lock().await;
        sqlx::any::install_default_drivers();
        let pool = sqlx::any::AnyPoolOptions::new()
            .connect(&url)
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        ensure_vault_schema(&mut conn).await.unwrap();

        // A co-tenant application's table, sitting in the same schema.
        sqlx::query("DROP TABLE IF EXISTS mv_test_neighbour")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE mv_test_neighbour (id BIGINT PRIMARY KEY, note TEXT)")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO mv_test_neighbour (id, note) VALUES (1, 'keep me')")
            .execute(&mut *conn)
            .await
            .unwrap();

        // Roll the marker back so the next ensure takes the rebuild path.
        sqlx::query("DELETE FROM schema_meta WHERE key = $1")
            .bind(VAULT_SCHEMA_META_KEY)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO schema_meta (key, value) VALUES ('vault_schema_v1', '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        ensure_vault_schema(&mut conn).await.unwrap();

        let note: Option<String> =
            sqlx::query_scalar("SELECT note FROM mv_test_neighbour WHERE id = 1")
                .fetch_optional(&mut *conn)
                .await
                .expect("a table the vault does not own survives the rebuild")
                .flatten();
        assert_eq!(
            note.as_deref(),
            Some("keep me"),
            "the neighbour's rows survive the rebuild too"
        );

        // The vault's own tables were still rebuilt.
        let ready: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_meta WHERE key = $1")
            .bind(VAULT_SCHEMA_META_KEY)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(ready, 1, "the rebuild stamps the current marker");

        sqlx::query("DROP TABLE IF EXISTS mv_test_neighbour")
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    /// The drop list is read out of the embedded DDL, so it covers every
    /// table the vault installs and nothing else.
    #[test]
    fn pg_vault_table_names_match_the_embedded_ddl() {
        let names = pg_vault_table_names();
        for expected in [
            "accounts",
            "account_api_tokens",
            "schema_meta",
            "vault_imports",
            "vault_import_issues",
            "contacts",
            "handles",
            "trashed_conversations",
            "conversations",
            "messages",
            "attachments",
            "message_tags",
            "staging_messages",
        ] {
            assert!(
                names.contains(&expected),
                "{expected} missing from {names:?}"
            );
        }
        let declared = pg_vault_table_ddl()
            .files
            .iter()
            .flat_map(|ddl| ddl.lines())
            .filter(|line| line.trim_start().starts_with("CREATE TABLE"))
            .count();
        assert_eq!(
            names.len(),
            declared,
            "every CREATE TABLE in the Postgres DDL is on the drop list"
        );
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

    #[test]
    fn split_ddl_keeps_do_blocks_intact() {
        let fks = &pg_vault_table_ddl().deferred_fks;
        let stmts = split_ddl(fks);
        assert_eq!(stmts.len(), 1, "the deferred FKs must be one DO block");
        assert!(
            stmts[0].starts_with("DO $$"),
            "unexpected split: {}",
            stmts[0]
        );
        assert!(stmts[0].ends_with("$$;"), "DO block must end in $$;");
    }

    #[test]
    fn split_ddl_keeps_pg_function_bodies_intact() {
        let ddl = include_str!("../../../../../schema/sql/fts_postgres.sql");
        let stmts = split_ddl(ddl);
        // Column + GIN index + two sync functions + six one-line triggers.
        assert_eq!(stmts.len(), 10, "unexpected split of fts_postgres.sql");
        let mut functions = 0;
        let mut triggers = 0;
        for stmt in &stmts {
            if stmt.starts_with("CREATE OR REPLACE FUNCTION") {
                functions += 1;
                assert!(
                    stmt.ends_with("$$ LANGUAGE plpgsql;"),
                    "function must end in $$ LANGUAGE plpgsql;: {stmt}"
                );
            } else if stmt.starts_with("CREATE TRIGGER") {
                triggers += 1;
                assert!(
                    stmt.ends_with("EXECUTE FUNCTION messages_fts_sync();")
                        || stmt.ends_with("EXECUTE FUNCTION attachments_fts_sync();"),
                    "unexpected split: {stmt}"
                );
            } else {
                assert!(stmt.ends_with(';'), "statement must end with ;: {stmt}");
            }
        }
        assert_eq!(functions, 2);
        assert_eq!(triggers, 6);
        let drop = split_ddl(include_str!(
            "../../../../../schema/sql/fts_postgres_drop.sql"
        ));
        assert_eq!(drop.len(), 6, "six sync triggers to drop");
        for stmt in drop {
            assert!(
                stmt.starts_with("DROP TRIGGER IF EXISTS"),
                "unexpected split: {stmt}"
            );
        }
    }
}
