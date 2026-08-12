use anyhow::{Result, bail};
use rusqlite::{Connection, params};

/// Shared SQLite pragmas for serve/import (WAL + busy wait so auth/UI can overlap writes).
///
/// Busy timeout is applied first. WAL is best-effort: a hot rollback journal or another
/// process holding the DB (e.g. Next.js) can make `journal_mode=WAL` fail; callers still
/// get a usable connection.
pub fn configure_connection(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA busy_timeout = 15000;
        PRAGMA synchronous = NORMAL;
        PRAGMA temp_store = MEMORY;
        PRAGMA cache_size = -200000;
        PRAGMA foreign_keys = ON;
        "#,
    )?;
    // journal_mode returns a row; may fail if another connection holds a lock.
    match conn.query_row("PRAGMA journal_mode = WAL", [], |row| {
        row.get::<_, String>(0)
    }) {
        Ok(mode) => {
            if !mode.eq_ignore_ascii_case("wal") {
                eprintln!("warning: journal_mode is {mode} (wanted wal)");
            }
        }
        Err(err) => {
            eprintln!(
                "warning: could not enable WAL ({err}); continuing with current journal mode"
            );
        }
    }
    Ok(())
}

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

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )?;
    Ok(exists)
}

/// Create every table and index required by a current vault.
pub fn ensure_vault_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    ensure_accounts_schema(conn)?;
    // Contacts DDL defines `handles`, the FK target of conversations, participants,
    // messages, and tapbacks (messages.sql) plus account_handles (accounts.sql).
    // Apply it before the tables that reference handles.
    conn.execute_batch(CONTACTS_TABLES_DDL)?;
    conn.execute_batch(MESSAGE_TABLES_DDL)?;
    conn.execute_batch(STAGING_TABLES_DDL)?;
    ensure_attachment_missing_reason_columns(conn)?;
    ensure_messages_fts(conn)?;
    Ok(())
}

/// Ensure the complete current vault schema exists.
pub fn ensure_messages_schema(conn: &Connection) -> Result<()> {
    ensure_vault_schema(conn)
}

/// Marker that current FTS sync trigger definitions are installed.
pub const MESSAGES_FTS_TRIGGERS_META_KEY: &str = "messages_fts_triggers_v1";

/// Contentless FTS5 index over message body/subject plus attachment text.
fn ensure_messages_fts(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "messages")? {
        return Ok(());
    }
    conn.execute_batch(FTS_VIRTUAL_DDL)?;

    let triggers_ready: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM schema_meta WHERE key = ?1",
        params![MESSAGES_FTS_TRIGGERS_META_KEY],
        |row| row.get(0),
    )?;
    if !triggers_ready {
        install_messages_fts_triggers(conn)?;
    }

    Ok(())
}

/// Drop FTS sync triggers (used during bulk promote so inserts skip per-row indexing).
pub(crate) fn drop_messages_fts_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(DROP_MESSAGES_FTS_TRIGGERS_SQL)?;
    conn.execute(
        "DELETE FROM schema_meta WHERE key = ?1",
        params![MESSAGES_FTS_TRIGGERS_META_KEY],
    )?;
    Ok(())
}

/// Install FTS sync triggers and mark them ready in `schema_meta`.
pub(crate) fn install_messages_fts_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(DROP_MESSAGES_FTS_TRIGGERS_SQL)?;
    conn.execute_batch(CREATE_MESSAGES_FTS_TRIGGERS_SQL)?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES (?1, '1')",
        params![MESSAGES_FTS_TRIGGERS_META_KEY],
    )?;
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

/// Drop secondary `messages` indexes during bulk promote (transactional with the promote tx).
pub(crate) fn drop_messages_secondary_indexes(conn: &Connection) -> Result<()> {
    for (name, _) in MESSAGES_SECONDARY_INDEX_DDL {
        conn.execute(&format!("DROP INDEX IF EXISTS {name}"), [])?;
    }
    Ok(())
}

/// Recreate secondary `messages` indexes after bulk promote inserts.
pub(crate) fn create_messages_secondary_indexes(conn: &Connection) -> Result<()> {
    for (_, ddl) in MESSAGES_SECONDARY_INDEX_DDL {
        conn.execute_batch(ddl)?;
    }
    Ok(())
}

/// Bulk-index promoted messages (joined via temp `_promote_msg_map`) into `messages_fts`.
/// Call after attachment rows exist so `attachment_text` is complete.
pub(crate) fn index_messages_fts_from_promote_map(conn: &Connection) -> Result<u64> {
    let n = conn.execute(
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
        FROM messages m
        JOIN _promote_msg_map mm ON mm.prod_id = m.id
        "#,
        [],
    )?;
    Ok(u64::try_from(n).unwrap_or(0))
}

/// Delete all production messages (and cascaded rows) for one import source within one account.
pub fn delete_messages_for_source(
    conn: &Connection,
    account_id: &str,
    source: &str,
) -> Result<u64> {
    conn.execute(
        r#"
        DELETE FROM attachments
        WHERE message_id IN (
            SELECT m.id FROM messages m
            JOIN conversations c ON c.id = m.conversation_id
            WHERE m.source = ?1 AND c.account_id = ?2
        )
        "#,
        params![source, account_id],
    )?;
    conn.execute(
        r#"
        DELETE FROM tapbacks
        WHERE message_id IN (
            SELECT m.id FROM messages m
            JOIN conversations c ON c.id = m.conversation_id
            WHERE m.source = ?1 AND c.account_id = ?2
        )
        "#,
        params![source, account_id],
    )?;
    conn.execute(
        r#"
        UPDATE messages
        SET duplicate_of = NULL
        WHERE duplicate_of IN (
            SELECT m.id FROM messages m
            JOIN conversations c ON c.id = m.conversation_id
            WHERE m.source = ?1 AND c.account_id = ?2
        )
        "#,
        params![source, account_id],
    )?;
    let n = conn.execute(
        r#"
        DELETE FROM messages
        WHERE source = ?1
          AND conversation_id IN (
              SELECT id FROM conversations WHERE account_id = ?2
          )
        "#,
        params![source, account_id],
    )?;
    Ok(n as u64)
}

/// Ensure the complete current vault schema exists, including staging tables.
pub fn ensure_staging_schema(conn: &Connection) -> Result<()> {
    ensure_vault_schema(conn)
}

/// Clear one account's staging rows (CASCADE removes children). Other accounts are untouched.
pub fn reset_staging_for_account(conn: &Connection, account_id: &str) -> Result<()> {
    ensure_staging_schema(conn)?;
    conn.execute(
        "DELETE FROM staging_conversations WHERE account_id = ?1",
        params![account_id],
    )?;
    Ok(())
}

/// Clear one account's staging after a successful promote.
pub fn clear_staging_for_account(conn: &Connection, account_id: &str) -> Result<()> {
    reset_staging_for_account(conn, account_id)
}

/// Ensure the complete current vault schema exists, including contacts tables.
pub fn ensure_contacts_schema(conn: &Connection) -> Result<()> {
    ensure_vault_schema(conn)
}

/// Create current account and vault metadata tables.
pub fn ensure_accounts_schema(conn: &Connection) -> Result<()> {
    migrate_session_and_api_token_tables(conn)?;
    conn.execute_batch(ACCOUNTS_DDL)?;
    ensure_vault_imports_timing_columns(conn)?;
    ensure_vault_import_issues_table(conn)?;
    ensure_named_api_token_scopes_column(conn)?;
    ensure_named_api_token_hint_column(conn)?;
    ensure_named_api_token_last_accessed_column(conn)?;
    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    // pragma_table_info does not accept bound table names; only call with known literals.
    let sql = match table {
        "account_api_tokens" => {
            "SELECT COUNT(*) > 0 FROM pragma_table_info('account_api_tokens') WHERE name = ?1"
        }
        "account_session_tokens" => {
            "SELECT COUNT(*) > 0 FROM pragma_table_info('account_session_tokens') WHERE name = ?1"
        }
        "account_app_passwords" => {
            "SELECT COUNT(*) > 0 FROM pragma_table_info('account_app_passwords') WHERE name = ?1"
        }
        "vault_imports" => "SELECT COUNT(*) > 0 FROM pragma_table_info('vault_imports') WHERE name = ?1",
        "attachments" => "SELECT COUNT(*) > 0 FROM pragma_table_info('attachments') WHERE name = ?1",
        "staging_attachments" => {
            "SELECT COUNT(*) > 0 FROM pragma_table_info('staging_attachments') WHERE name = ?1"
        }
        other => bail!("column_exists: unsupported table {other}"),
    };
    let exists: bool = conn.query_row(sql, params![column], |row| row.get(0))?;
    Ok(exists)
}

fn ensure_vault_imports_timing_columns(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "vault_imports")? {
        return Ok(());
    }

    for (column, ddl) in [
        ("duration_ms", "ALTER TABLE vault_imports ADD COLUMN duration_ms INTEGER"),
        ("parse_ms", "ALTER TABLE vault_imports ADD COLUMN parse_ms INTEGER"),
        ("convert_ms", "ALTER TABLE vault_imports ADD COLUMN convert_ms INTEGER"),
        ("upload_ms", "ALTER TABLE vault_imports ADD COLUMN upload_ms INTEGER"),
        ("summary_json", "ALTER TABLE vault_imports ADD COLUMN summary_json TEXT"),
    ] {
        if !column_exists(conn, "vault_imports", column)? {
            conn.execute_batch(ddl)?;
        }
    }

    Ok(())
}

fn ensure_attachment_missing_reason_columns(conn: &Connection) -> Result<()> {
    for (table, ddl) in [
        (
            "attachments",
            "ALTER TABLE attachments ADD COLUMN missing_reason TEXT",
        ),
        (
            "staging_attachments",
            "ALTER TABLE staging_attachments ADD COLUMN missing_reason TEXT",
        ),
    ] {
        if table_exists(conn, table)? && !column_exists(conn, table, "missing_reason")? {
            conn.execute_batch(ddl)?;
        }
    }
    Ok(())
}

fn ensure_vault_import_issues_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS vault_import_issues (
            id INTEGER PRIMARY KEY,
            import_id INTEGER NOT NULL REFERENCES vault_imports(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            step TEXT NOT NULL,
            item TEXT NOT NULL,
            reason TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS ix_vault_import_issues_import
            ON vault_import_issues(import_id);
        "#,
    )?;
    Ok(())
}

/// Move legacy session `account_api_tokens` → `account_session_tokens`, then
/// `account_app_passwords` → `account_api_tokens` (named CLI tokens).
fn migrate_session_and_api_token_tables(conn: &Connection) -> Result<()> {
    // Old session table occupied `account_api_tokens` (no `label` column).
    if table_exists(conn, "account_api_tokens")?
        && !column_exists(conn, "account_api_tokens", "label")?
        && !table_exists(conn, "account_session_tokens")?
    {
        conn.execute_batch("ALTER TABLE account_api_tokens RENAME TO account_session_tokens;")?;
    }

    if table_exists(conn, "account_app_passwords")? && !table_exists(conn, "account_api_tokens")? {
        conn.execute_batch("ALTER TABLE account_app_passwords RENAME TO account_api_tokens;")?;
        // Recreate index under the new name when the old one was renamed with the table
        // (SQLite keeps the old index name on RENAME TABLE).
        let has_old_ix: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'index' AND name = 'ix_account_app_passwords_account'",
            [],
            |row| row.get(0),
        )?;
        if has_old_ix {
            conn.execute_batch("DROP INDEX IF EXISTS ix_account_app_passwords_account;")?;
        }
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS ix_account_api_tokens_account ON account_api_tokens(account_id);",
        )?;
    }
    Ok(())
}

/// Older DBs may lack `scopes` on named API tokens.
fn ensure_named_api_token_scopes_column(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "account_api_tokens")? {
        return Ok(());
    }
    // Session-shaped leftover should not happen after migrate; skip if no label.
    if !column_exists(conn, "account_api_tokens", "label")? {
        return Ok(());
    }
    if !column_exists(conn, "account_api_tokens", "scopes")? {
        conn.execute_batch(
            "ALTER TABLE account_api_tokens ADD COLUMN scopes TEXT NOT NULL DEFAULT 'both';",
        )?;
    }
    Ok(())
}

fn ensure_named_api_token_hint_column(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "account_api_tokens")? {
        return Ok(());
    }
    if !column_exists(conn, "account_api_tokens", "label")? {
        return Ok(());
    }
    if !column_exists(conn, "account_api_tokens", "token_hint")? {
        conn.execute_batch(
            "ALTER TABLE account_api_tokens ADD COLUMN token_hint TEXT NOT NULL DEFAULT 'mv-api-..';",
        )?;
    }
    Ok(())
}

fn ensure_named_api_token_last_accessed_column(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "account_api_tokens")? {
        return Ok(());
    }
    if !column_exists(conn, "account_api_tokens", "label")? {
        return Ok(());
    }
    if !column_exists(conn, "account_api_tokens", "last_accessed_at")? {
        conn.execute_batch("ALTER TABLE account_api_tokens ADD COLUMN last_accessed_at TEXT;")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A1: &str = "11111111-1111-1111-1111-111111111111";
    const A2: &str = "22222222-2222-2222-2222-222222222222";

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        ensure_vault_schema(&conn).unwrap();
        for (id, user) in [(A1, "alice"), (A2, "bob")] {
            conn.execute(
                "INSERT INTO accounts (id, username) VALUES (?1, ?2)",
                params![id, user],
            )
            .unwrap();
            conn.execute(
                r#"
                INSERT INTO handles (account_id, raw, normalized, handle_type, service)
                VALUES (?1, '+15555550100', '+15555550100', 'phone', 'phone')
                "#,
                params![id],
            )
            .unwrap();
            let handle_id = conn.last_insert_rowid();
            conn.execute(
                r#"
                INSERT INTO conversations (
                    account_id, chat_handle_id, conversation_type,
                    group_title, exported_at, source_file
                ) VALUES (?1, ?2, 'individual', NULL, NULL, 't.json')
                "#,
                params![id, handle_id],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn fresh_vault_has_complete_current_schema() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_vault_schema(&conn).unwrap();
        let contract: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../fixtures/schema/current-schema.json"
        ))
        .unwrap();

        for table in contract["tables"].as_array().unwrap() {
            let table = table.as_str().unwrap();
            assert!(table_exists(&conn, table).unwrap(), "missing table {table}");
        }
        for index in contract["indexes"].as_array().unwrap() {
            let index = index.as_str().unwrap();
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'index' AND name = ?1",
                    [index],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing index {index}");
        }
        for trigger in contract["triggers"].as_array().unwrap() {
            let trigger = trigger.as_str().unwrap();
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
                    [trigger],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing trigger {trigger}");
        }
        for key in contract["metadata"].as_array().unwrap() {
            let key = key.as_str().unwrap();
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM schema_meta WHERE key = ?1",
                    [key],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing runtime metadata {key}");
        }

        let columns = |table: &str| -> Vec<String> {
            conn.prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |row| row.get(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(
            columns("accounts"),
            [
                "id",
                "username",
                "read_only",
                "password_hash",
                "preferred_name",
                "hanko_user_id"
            ]
        );
        assert_eq!(
            columns("contacts"),
            ["id", "account_id", "preferred_name", "last_modified"]
        );
        assert_eq!(
            columns("handles"),
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
            columns("conversations")
                .iter()
                .any(|c| c == "chat_handle_id")
        );
        for column in ["account_id", "source", "content_key", "duplicate_of"] {
            assert!(columns("messages").iter().any(|c| c == column));
        }
        assert!(
            columns("staging_messages")
                .iter()
                .any(|c| c == "account_id")
        );
        assert!(columns("attachments").iter().any(|c| c == "size_bytes"));
        assert!(columns("attachments").iter().any(|c| c == "missing_reason"));
        assert!(
            columns("staging_attachments")
                .iter()
                .any(|c| c == "size_bytes")
        );
        assert!(
            columns("staging_attachments")
                .iter()
                .any(|c| c == "missing_reason")
        );

        ensure_vault_schema(&conn).unwrap();
    }

    #[test]
    fn same_source_guid_allowed_across_accounts() {
        let conn = setup();
        let conversation = |account: &str| -> i64 {
            conn.query_row(
                "SELECT id FROM conversations WHERE account_id = ?1",
                params![account],
                |row| row.get(0),
            )
            .unwrap()
        };
        for (conv, account) in [(conversation(A1), A1), (conversation(A2), A2)] {
            conn.execute(
                r#"
                INSERT INTO messages (
                    conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order
                ) VALUES (?1, ?2, 'sms', 'same-guid', '2020-01-01T00:00:00Z', 0, 0)
                "#,
                params![conv, account],
            )
            .unwrap();
        }
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE guid = 'same-guid'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn reset_staging_for_account_leaves_other_accounts() {
        let conn = setup();
        for account in [A1, A2] {
            conn.execute(
                r#"
                INSERT INTO staging_conversations (
                    account_id, chat_handle_id, conversation_type,
                    group_title, exported_at, source_file
                ) VALUES (?1, 1, 'individual', NULL, NULL, 't.json')
                "#,
                params![account],
            )
            .unwrap();
            let conversation_id = conn.last_insert_rowid();
            conn.execute(
                r#"
                INSERT INTO staging_messages (
                    conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order
                ) VALUES (?1, ?2, 'sms', 'g1', '2020-01-01T00:00:00Z', 0, 0)
                "#,
                params![conversation_id, account],
            )
            .unwrap();
        }

        reset_staging_for_account(&conn, A1).unwrap();
        let remaining: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM staging_conversations WHERE account_id = ?1",
                params![A2],
                |r| r.get(0),
            )
            .unwrap();
        let messages: i64 = conn
            .query_row("SELECT COUNT(*) FROM staging_messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 1);
        assert_eq!(messages, 1);
    }

    #[test]
    fn fresh_accounts_default_to_writable() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_accounts_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username) VALUES (?1, 'fresh')",
            params![A1],
        )
        .unwrap();
        let read_only: i64 = conn
            .query_row(
                "SELECT read_only FROM accounts WHERE id = ?1",
                params![A1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(read_only, 0);
    }

    #[test]
    fn messages_fts_stays_in_sync() {
        let conn = setup();
        let conversation_id: i64 = conn
            .query_row(
                "SELECT id FROM conversations WHERE account_id = ?1",
                params![A1],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body, subject
            ) VALUES (?1, ?2, 'sms', 'g1', '2020-01-01T00:00:00Z', 0, 0, 'hello vault', NULL)
            "#,
            params![conversation_id, A1],
        )
        .unwrap();
        let message_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO attachments (message_id, original_name, transcription) VALUES (?1, 'voice.m4a', 'secret phrase')",
            params![message_id],
        )
        .unwrap();

        let hits = |term: &str| -> i64 {
            conn.query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH ?1",
                params![term],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert_eq!(hits("vault"), 1);
        assert_eq!(hits("secret"), 1);

        conn.execute(
            "UPDATE messages SET body = 'goodbye' WHERE id = ?1",
            params![message_id],
        )
        .unwrap();
        assert_eq!(hits("vault"), 0);
        assert_eq!(hits("goodbye"), 1);

        conn.execute(
            "DELETE FROM attachments WHERE message_id = ?1",
            params![message_id],
        )
        .unwrap();
        conn.execute("DELETE FROM messages WHERE id = ?1", params![message_id])
            .unwrap();
        assert_eq!(hits("goodbye"), 0);
    }
}
