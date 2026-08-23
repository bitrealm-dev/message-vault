//! Schema management for the vault and accounts databases.
//!
//! Serve and import open their SQLite connections through `open_configured`
//! (shared pragmas) and ensure the schema with `ensure_vault_schema` /
//! `ensure_accounts_schema`. DDL lives in the SQL files embedded at compile
//! time; the functions here apply and evolve it.

use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, params};

/// Shared SQLite settings for serve and import.
///
/// A busy timeout is applied first so overlapping auth and UI writes wait
/// instead of failing immediately. Write-ahead logging (SQLite's extra log of
/// recent writes, often called WAL) is best-effort: a hot rollback journal or
/// another process holding the database can make `journal_mode=WAL` fail;
/// callers still get a usable connection.
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
    // journal_mode returns a row; it may fail if another connection holds a lock.
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
                "warning: could not enable write-ahead logging ({err}); continuing with current journal mode"
            );
        }
    }
    Ok(())
}

/// Open `path` and apply [`configure_connection`].
///
/// # Errors
///
/// Returns an error when the database file cannot be opened or configured.
pub fn open_configured(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    configure_connection(&conn)?;
    Ok(conn)
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

/// Create every table and index required by a current vault.
///
/// # Errors
///
/// Returns an error when a DDL statement fails.
pub fn ensure_vault_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    ensure_accounts_schema(conn)?;
    // Older vaults used `contact_labels`. Rename those tables before CREATE
    // so a restart picks up the current names.
    migrate_contact_labels_to_groups(conn)?;
    // Contacts DDL defines `handles`, the FK target of conversations, participants,
    // messages, and tapbacks (messages.sql) plus account_handles (accounts.sql).
    // Apply it before the tables that reference handles.
    conn.execute_batch(CONTACTS_TABLES_DDL)?;
    conn.execute_batch(MESSAGE_TABLES_DDL)?;
    conn.execute_batch(STAGING_TABLES_DDL)?;
    ensure_messages_fts(conn)?;
    Ok(())
}

/// Marker that current full-text search (FTS) sync trigger definitions are installed.
pub const MESSAGES_FTS_TRIGGERS_META_KEY: &str = "messages_fts_triggers_v1";

/// Contentless full-text search index over message body/subject plus attachment text.
fn ensure_messages_fts(conn: &Connection) -> Result<()> {
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

/// Drop full-text search sync triggers (used during bulk promote so inserts skip
/// per-row indexing).
///
/// # Errors
///
/// Returns an error when the drop statements fail.
pub(crate) fn drop_messages_fts_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(DROP_MESSAGES_FTS_TRIGGERS_SQL)?;
    conn.execute(
        "DELETE FROM schema_meta WHERE key = ?1",
        params![MESSAGES_FTS_TRIGGERS_META_KEY],
    )?;
    Ok(())
}

/// Install full-text search sync triggers and mark them ready in `schema_meta`.
///
/// # Errors
///
/// Returns an error when the trigger SQL or metadata write fails.
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

/// Drop secondary `messages` indexes during bulk promote (same transaction as
/// the promote inserts).
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
pub(crate) fn index_messages_fts_from_promote_map(
    conn: &Connection,
    min_new_message_id: i64,
) -> Result<u64> {
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
        FROM (
            SELECT DISTINCT prod_id FROM _promote_msg_map WHERE prod_id > ?1
        ) mm
        JOIN messages m ON m.id = mm.prod_id
        "#,
        params![min_new_message_id],
    )?;
    Ok(u64::try_from(n).unwrap_or(0))
}

/// Message ids for one source within one account, bound as `?1` = source, `?2` = account.
const MESSAGE_IDS_FOR_SOURCE: &str = "SELECT m.id FROM messages m \
     JOIN conversations c ON c.id = m.conversation_id \
     WHERE m.source = ?1 AND c.account_id = ?2";

/// Delete all production messages (and cascaded rows) for one import source within one account.
///
/// # Errors
///
/// Returns an error when a delete or update statement fails.
pub fn delete_messages_for_source(
    conn: &Connection,
    account_id: &str,
    source: &str,
) -> Result<u64> {
    conn.execute(
        &format!("DELETE FROM attachments WHERE message_id IN ({MESSAGE_IDS_FOR_SOURCE})"),
        params![source, account_id],
    )?;
    conn.execute(
        &format!("DELETE FROM tapbacks WHERE message_id IN ({MESSAGE_IDS_FOR_SOURCE})"),
        params![source, account_id],
    )?;
    conn.execute(
        &format!(
            "UPDATE messages SET duplicate_of = NULL
             WHERE duplicate_of IN ({MESSAGE_IDS_FOR_SOURCE})"
        ),
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

/// Clear one account's staging rows (the temporary import area). Child rows
/// are removed by CASCADE. Other accounts are untouched.
///
/// # Errors
///
/// Returns an error when schema setup or the delete fails.
pub fn reset_staging_for_account(conn: &Connection, account_id: &str) -> Result<()> {
    ensure_vault_schema(conn)?;
    conn.execute(
        "DELETE FROM staging_conversations WHERE account_id = ?1",
        params![account_id],
    )?;
    Ok(())
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )?;
    Ok(exists)
}

/// Rename leftover `contact_labels` tables from vaults created before groups.
fn migrate_contact_labels_to_groups(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "contact_labels")? {
        return Ok(());
    }
    if table_exists(conn, "contact_groups")? {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;
        ALTER TABLE contact_labels RENAME TO contact_groups;
        ALTER TABLE contact_label_members RENAME TO contact_group_members;
        ALTER TABLE contact_group_members RENAME COLUMN label_id TO group_id;
        PRAGMA foreign_keys = ON;
        "#,
    )?;
    Ok(())
}

/// Create current account and vault metadata tables.
///
/// # Errors
///
/// Returns an error when DDL or a column migration fails.
pub fn ensure_accounts_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(ACCOUNTS_DDL)?;
    // Additive migrations for DBs created before expiry/disable columns existed.
    ensure_column(
        conn,
        "account_session_tokens",
        "expires_at",
        "ALTER TABLE account_session_tokens ADD COLUMN expires_at TEXT NOT NULL DEFAULT '0'",
    )?;
    ensure_column(
        conn,
        "account_api_tokens",
        "expires_at",
        "ALTER TABLE account_api_tokens ADD COLUMN expires_at TEXT",
    )?;
    ensure_column(
        conn,
        "account_api_tokens",
        "disabled",
        "ALTER TABLE account_api_tokens ADD COLUMN disabled INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "accounts",
        "guest_status",
        "ALTER TABLE accounts ADD COLUMN guest_status TEXT",
    )?;
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, alter_sql: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut exists = false;
    let mut rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows.by_ref() {
        let Ok(name) = row else {
            continue;
        };
        if name == column {
            exists = true;
            break;
        }
    }
    if !exists {
        conn.execute_batch(alter_sql)?;
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
    fn promote_fts_indexing_covers_only_rows_inserted_by_this_promotion() {
        let conn = setup();
        let insert_message = |id: i64, guid: &str, body: &str| {
            conn.execute(
                r#"
                INSERT INTO messages (
                    id, conversation_id, account_id, source, guid,
                    timestamp, is_from_me, sort_order, body
                ) VALUES (?1, 1, ?2, 'imessage', ?3, '2020-01-01T00:00:00Z', 0, 0, ?4)
                "#,
                params![id, A1, guid, body],
            )
            .unwrap();
        };

        // An earlier import already indexed this row through the insert trigger.
        insert_message(10, "g-existing", "carriedover");
        let max_id_before_promote: i64 = conn
            .query_row("SELECT IFNULL(MAX(id), 0) FROM messages", [], |r| r.get(0))
            .unwrap();

        drop_messages_fts_triggers(&conn).unwrap();
        insert_message(11, "g-new", "freshbody");
        // Append promotion maps existing GUIDs (so child rows find their parent)
        // alongside newly inserted rows, and one production row can be the target
        // of more than one staging row.
        conn.execute_batch(
            r#"
            CREATE TEMP TABLE _promote_msg_map (
                staging_id INTEGER PRIMARY KEY,
                prod_id INTEGER NOT NULL
            );
            INSERT INTO _promote_msg_map (staging_id, prod_id) VALUES (1, 10), (2, 11), (3, 11);
            "#,
        )
        .unwrap();

        let indexed = index_messages_fts_from_promote_map(&conn, max_id_before_promote).unwrap();
        install_messages_fts_triggers(&conn).unwrap();

        assert_eq!(
            indexed, 1,
            "only rows inserted by this promotion may be indexed"
        );
        for (term, expected) in [("carriedover", 1), ("freshbody", 1)] {
            let hits: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH ?1",
                    params![term],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(hits, expected, "unexpected match count for {term}");
        }
    }

    #[test]
    fn fresh_vault_has_complete_current_schema() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_vault_schema(&conn).unwrap();
        let contract: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../tests/fixtures/schema/current-schema.json"
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
                "hanko_user_id",
                "guest_status"
            ]
        );
        assert_eq!(
            columns("contacts"),
            ["id", "account_id", "preferred_name", "last_modified"]
        );
        assert_eq!(columns("contact_groups"), ["id", "account_id", "name"]);
        assert_eq!(columns("contact_group_members"), ["contact_id", "group_id"]);
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
    fn restart_renames_contact_labels_tables() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_vault_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username) VALUES (?1, 'alice')",
            params![A1],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, 'Ada')",
            params![A1],
        )
        .unwrap();
        let contact_id = conn.last_insert_rowid();
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = OFF;
            DROP TABLE contact_group_members;
            DROP TABLE contact_groups;
            CREATE TABLE contact_labels (
                id INTEGER PRIMARY KEY,
                account_id TEXT NOT NULL,
                name TEXT NOT NULL
            );
            CREATE TABLE contact_label_members (
                contact_id INTEGER NOT NULL,
                label_id INTEGER NOT NULL,
                PRIMARY KEY (contact_id, label_id)
            );
            PRAGMA foreign_keys = ON;
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO contact_labels (account_id, name) VALUES (?1, 'Family')",
            params![A1],
        )
        .unwrap();
        let group_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO contact_label_members (contact_id, label_id) VALUES (?1, ?2)",
            params![contact_id, group_id],
        )
        .unwrap();

        ensure_vault_schema(&conn).unwrap();

        assert!(!table_exists(&conn, "contact_labels").unwrap());
        assert!(table_exists(&conn, "contact_groups").unwrap());
        let name: String = conn
            .query_row(
                "SELECT cg.name
                 FROM contact_groups cg
                 JOIN contact_group_members cgm ON cgm.group_id = cg.id
                 WHERE cgm.contact_id = ?1",
                params![contact_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "Family");
    }

    #[test]
    fn guest_status_column_exists_and_defaults_null() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_accounts_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username) VALUES (?1, 'alice')",
            params![A1],
        )
        .unwrap();
        let status: Option<String> = conn
            .query_row(
                "SELECT guest_status FROM accounts WHERE id = ?1",
                params![A1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, None);
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
