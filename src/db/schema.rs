use anyhow::Result;
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

const MESSAGE_TABLES_DDL: &str = r#"
CREATE TABLE conversations (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    chat_identifier TEXT NOT NULL,
    service TEXT,
    conversation_type TEXT NOT NULL,
    group_title TEXT,
    exported_at TEXT,
    source_file TEXT NOT NULL,
    UNIQUE(account_id, chat_identifier)
);

CREATE INDEX ix_conversations_account_id ON conversations (account_id);

CREATE TABLE participants (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    handle TEXT NOT NULL,
    name_hint TEXT,
    UNIQUE(conversation_id, handle)
);

CREATE INDEX ix_participants_handle ON participants (handle);

CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    guid TEXT,
    timestamp TEXT NOT NULL,
    timestamp_utc TEXT,
    is_from_me INTEGER NOT NULL,
    sender TEXT,
    subject TEXT,
    body TEXT,
    is_announcement INTEGER NOT NULL DEFAULT 0,
    is_reply INTEGER NOT NULL DEFAULT 0,
    thread_originator_guid TEXT,
    thread_originator_part INTEGER,
    num_replies INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL,
    content_key TEXT,
    duplicate_of INTEGER REFERENCES messages(id) ON DELETE SET NULL
);

CREATE INDEX ix_messages_conversation_timestamp
    ON messages (conversation_id, timestamp);
CREATE INDEX ix_messages_conversation_source_timestamp
    ON messages (conversation_id, source, timestamp);
CREATE INDEX ix_messages_account_id ON messages (account_id);
CREATE UNIQUE INDEX ix_messages_account_source_guid
    ON messages (account_id, source, guid)
    WHERE guid IS NOT NULL AND guid != '';
CREATE INDEX ix_messages_content_key
    ON messages (content_key)
    WHERE content_key IS NOT NULL AND content_key != '';
CREATE INDEX ix_messages_duplicate_of
    ON messages (duplicate_of)
    WHERE duplicate_of IS NOT NULL;

CREATE TABLE attachments (
    id INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    path TEXT,
    original_name TEXT,
    mime_type TEXT,
    is_sticker INTEGER NOT NULL DEFAULT 0,
    transcription TEXT,
    sha256 TEXT,
    assets_path TEXT,
    size_bytes INTEGER,
    derived_sha256 TEXT,
    derived_assets_path TEXT,
    derived_mime_type TEXT
);

CREATE INDEX ix_attachments_sha256 ON attachments (sha256);
CREATE INDEX ix_attachments_message_id ON attachments (message_id);

CREATE TABLE tapbacks (
    id INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    part_index INTEGER NOT NULL DEFAULT 0,
    kind TEXT NOT NULL,
    emoji TEXT,
    is_from_me INTEGER NOT NULL,
    sender TEXT
);

CREATE INDEX ix_tapbacks_message_id ON tapbacks (message_id);
CREATE INDEX ix_messages_source ON messages (source);
"#;

const STAGING_TABLES_DDL: &str = r#"
CREATE TABLE staging_conversations (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    chat_identifier TEXT NOT NULL,
    service TEXT,
    conversation_type TEXT NOT NULL,
    group_title TEXT,
    exported_at TEXT,
    source_file TEXT NOT NULL,
    UNIQUE(account_id, chat_identifier)
);

CREATE TABLE staging_participants (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES staging_conversations(id) ON DELETE CASCADE,
    handle TEXT NOT NULL,
    name_hint TEXT,
    UNIQUE(conversation_id, handle)
);

CREATE TABLE staging_messages (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES staging_conversations(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    guid TEXT,
    timestamp TEXT NOT NULL,
    timestamp_utc TEXT,
    is_from_me INTEGER NOT NULL,
    sender TEXT,
    subject TEXT,
    body TEXT,
    is_announcement INTEGER NOT NULL DEFAULT 0,
    is_reply INTEGER NOT NULL DEFAULT 0,
    thread_originator_guid TEXT,
    thread_originator_part INTEGER,
    num_replies INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL
);

CREATE INDEX ix_staging_messages_conversation_timestamp
    ON staging_messages (conversation_id, timestamp);
CREATE INDEX ix_staging_messages_account_id ON staging_messages (account_id);
CREATE UNIQUE INDEX ix_staging_messages_account_source_guid
    ON staging_messages (account_id, source, guid)
    WHERE guid IS NOT NULL AND guid != '';

CREATE TABLE staging_attachments (
    id INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES staging_messages(id) ON DELETE CASCADE,
    path TEXT,
    original_name TEXT,
    mime_type TEXT,
    is_sticker INTEGER NOT NULL DEFAULT 0,
    transcription TEXT,
    sha256 TEXT,
    assets_path TEXT,
    size_bytes INTEGER,
    derived_sha256 TEXT,
    derived_assets_path TEXT,
    derived_mime_type TEXT
);

CREATE INDEX ix_staging_attachments_sha256 ON staging_attachments (sha256);
CREATE INDEX ix_staging_attachments_message_id ON staging_attachments (message_id);

CREATE TABLE staging_tapbacks (
    id INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES staging_messages(id) ON DELETE CASCADE,
    part_index INTEGER NOT NULL DEFAULT 0,
    kind TEXT NOT NULL,
    emoji TEXT,
    is_from_me INTEGER NOT NULL,
    sender TEXT
);

CREATE INDEX ix_staging_tapbacks_message_id ON staging_tapbacks (message_id);
"#;

const CONTACTS_TABLES_DDL: &str = r#"
CREATE TABLE contacts (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    preferred_name TEXT,
    preferred_handle TEXT
);

CREATE INDEX ix_contacts_account_id ON contacts (account_id);

CREATE TABLE contact_handles (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    handle TEXT NOT NULL,
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    PRIMARY KEY (account_id, handle)
);

CREATE INDEX ix_contact_handles_contact_id
    ON contact_handles (contact_id);

CREATE TABLE contact_labels (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    UNIQUE(account_id, name)
);

CREATE TABLE contact_label_members (
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    label_id INTEGER NOT NULL REFERENCES contact_labels(id) ON DELETE CASCADE,
    PRIMARY KEY (contact_id, label_id)
);

CREATE TABLE trashed_handles (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    handle TEXT NOT NULL,
    trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (account_id, handle)
);

CREATE TABLE trashed_conversations (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    conversation_id INTEGER NOT NULL,
    trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (account_id, conversation_id)
);

CREATE TABLE trashed_contacts (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    contact_id INTEGER NOT NULL,
    trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (account_id, contact_id)
);
"#;

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
        params![column],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

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

    if !table_exists(conn, "conversations")? {
        conn.execute_batch(MESSAGE_TABLES_DDL)?;
    }
    if !table_exists(conn, "staging_conversations")? {
        conn.execute_batch(STAGING_TABLES_DDL)?;
    }
    if !table_exists(conn, "contacts")? {
        conn.execute_batch(CONTACTS_TABLES_DDL)?;
    }

    ensure_messages_fts(conn)?;
    Ok(())
}

/// Ensure the complete current vault schema exists.
pub fn ensure_messages_schema(conn: &Connection) -> Result<()> {
    ensure_vault_schema(conn)
}

/// Marker for the one-time FTS5 backfill of messages present before trigger setup.
pub const MESSAGES_FTS_BACKFILL_META_KEY: &str = "messages_fts_backfill_v1";
/// Marker that current FTS sync trigger definitions are installed.
pub const MESSAGES_FTS_TRIGGERS_META_KEY: &str = "messages_fts_triggers_v1";

const DROP_MESSAGES_FTS_TRIGGERS_SQL: &str = r#"
DROP TRIGGER IF EXISTS messages_fts_ai;
DROP TRIGGER IF EXISTS messages_fts_ad;
DROP TRIGGER IF EXISTS messages_fts_au;
DROP TRIGGER IF EXISTS attachments_fts_ai;
DROP TRIGGER IF EXISTS attachments_fts_ad;
DROP TRIGGER IF EXISTS attachments_fts_au;
"#;

const CREATE_MESSAGES_FTS_TRIGGERS_SQL: &str = r#"
CREATE TRIGGER messages_fts_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, body, subject, attachment_text)
    VALUES (
        new.id,
        coalesce(new.body, ''),
        coalesce(new.subject, ''),
        (
            SELECT coalesce(
                group_concat(
                    trim(coalesce(original_name, '') || ' ' || coalesce(transcription, '')),
                    ' '
                ),
                ''
            )
            FROM attachments
            WHERE message_id = new.id
        )
    );
END;

CREATE TRIGGER messages_fts_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, body, subject, attachment_text)
    VALUES ('delete', old.id, coalesce(old.body, ''), coalesce(old.subject, ''), '');
END;

CREATE TRIGGER messages_fts_au AFTER UPDATE OF body, subject ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, body, subject, attachment_text)
    VALUES ('delete', old.id, coalesce(old.body, ''), coalesce(old.subject, ''), '');
    INSERT INTO messages_fts(rowid, body, subject, attachment_text)
    VALUES (
        new.id,
        coalesce(new.body, ''),
        coalesce(new.subject, ''),
        (
            SELECT coalesce(
                group_concat(
                    trim(coalesce(original_name, '') || ' ' || coalesce(transcription, '')),
                    ' '
                ),
                ''
            )
            FROM attachments
            WHERE message_id = new.id
        )
    );
END;

CREATE TRIGGER attachments_fts_ai AFTER INSERT ON attachments BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, body, subject, attachment_text)
    SELECT 'delete', m.id, coalesce(m.body, ''), coalesce(m.subject, ''), ''
    FROM messages m WHERE m.id = new.message_id;
    INSERT INTO messages_fts(rowid, body, subject, attachment_text)
    SELECT
        m.id,
        coalesce(m.body, ''),
        coalesce(m.subject, ''),
        (
            SELECT coalesce(
                group_concat(
                    trim(coalesce(a.original_name, '') || ' ' || coalesce(a.transcription, '')),
                    ' '
                ),
                ''
            )
            FROM attachments a
            WHERE a.message_id = m.id
        )
    FROM messages m WHERE m.id = new.message_id;
END;

CREATE TRIGGER attachments_fts_ad AFTER DELETE ON attachments BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, body, subject, attachment_text)
    SELECT 'delete', m.id, coalesce(m.body, ''), coalesce(m.subject, ''), ''
    FROM messages m WHERE m.id = old.message_id;
    INSERT INTO messages_fts(rowid, body, subject, attachment_text)
    SELECT
        m.id,
        coalesce(m.body, ''),
        coalesce(m.subject, ''),
        (
            SELECT coalesce(
                group_concat(
                    trim(coalesce(a.original_name, '') || ' ' || coalesce(a.transcription, '')),
                    ' '
                ),
                ''
            )
            FROM attachments a
            WHERE a.message_id = m.id
        )
    FROM messages m WHERE m.id = old.message_id;
END;

CREATE TRIGGER attachments_fts_au AFTER UPDATE OF original_name, transcription ON attachments BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, body, subject, attachment_text)
    SELECT 'delete', m.id, coalesce(m.body, ''), coalesce(m.subject, ''), ''
    FROM messages m WHERE m.id = new.message_id;
    INSERT INTO messages_fts(rowid, body, subject, attachment_text)
    SELECT
        m.id,
        coalesce(m.body, ''),
        coalesce(m.subject, ''),
        (
            SELECT coalesce(
                group_concat(
                    trim(coalesce(a.original_name, '') || ' ' || coalesce(a.transcription, '')),
                    ' '
                ),
                ''
            )
            FROM attachments a
            WHERE a.message_id = m.id
        )
    FROM messages m WHERE m.id = new.message_id;
END;
"#;

/// Contentless FTS5 index over message body/subject plus attachment text.
fn ensure_messages_fts(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "messages")? {
        return Ok(());
    }
    // Keep FTS setup self-contained for callers that initialize message storage directly.
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            body,
            subject,
            attachment_text,
            content='',
            tokenize='unicode61 remove_diacritics 2'
        );
        "#,
    )?;

    let triggers_ready: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM schema_meta WHERE key = ?1",
        params![MESSAGES_FTS_TRIGGERS_META_KEY],
        |row| row.get(0),
    )?;
    if !triggers_ready {
        install_messages_fts_triggers(conn)?;
    }

    backfill_messages_fts(conn)?;
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

fn backfill_messages_fts(conn: &Connection) -> Result<()> {
    let already: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM schema_meta WHERE key = ?1",
        params![MESSAGES_FTS_BACKFILL_META_KEY],
        |row| row.get(0),
    )?;
    if already {
        return Ok(());
    }

    // Clear any partial index before a full rebuild.
    conn.execute_batch(
        r#"
        INSERT INTO messages_fts(messages_fts) VALUES('delete-all');
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
        FROM messages m;
        "#,
    )?;
    conn.execute(
        "INSERT INTO schema_meta (key, value) VALUES (?1, '1')",
        params![MESSAGES_FTS_BACKFILL_META_KEY],
    )?;
    Ok(())
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
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS accounts (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE COLLATE NOCASE,
            read_only INTEGER NOT NULL DEFAULT 0,
            password_hash TEXT,
            preferred_name TEXT,
            hanko_user_id TEXT
        );

        CREATE TABLE IF NOT EXISTS account_emails (
            account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            email TEXT NOT NULL UNIQUE COLLATE NOCASE,
            is_primary INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (account_id, email)
        );

        CREATE UNIQUE INDEX IF NOT EXISTS ix_account_emails_one_primary
            ON account_emails(account_id)
            WHERE is_primary = 1;

        CREATE TABLE IF NOT EXISTS account_phones (
            account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            phone TEXT NOT NULL,
            PRIMARY KEY (account_id, phone)
        );

        CREATE TABLE IF NOT EXISTS account_api_tokens (
            account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
            token_hash TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS account_prefs (
            account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            PRIMARY KEY (account_id, key)
        );

        CREATE TABLE IF NOT EXISTS schema_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;

    if !column_exists(conn, "accounts", "hanko_user_id")? {
        conn.execute_batch("ALTER TABLE accounts ADD COLUMN hanko_user_id TEXT")?;
    }
    conn.execute_batch(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS ix_accounts_hanko_user_id
            ON accounts(hanko_user_id)
            WHERE hanko_user_id IS NOT NULL AND hanko_user_id != ''
        "#,
    )?;
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
                INSERT INTO conversations (
                    account_id, chat_identifier, service, conversation_type,
                    group_title, exported_at, source_file
                ) VALUES (?1, '+15555550100', 'SMS', 'individual', NULL, NULL, 't.json')
                "#,
                params![id],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn fresh_vault_has_complete_current_schema() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_vault_schema(&conn).unwrap();
        let contract: serde_json::Value =
            serde_json::from_str(include_str!("../../fixtures/schema/current-schema.json")).unwrap();

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
            ["id", "account_id", "preferred_name", "preferred_handle"]
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
        assert!(
            columns("staging_attachments")
                .iter()
                .any(|c| c == "size_bytes")
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
                    account_id, chat_identifier, service, conversation_type,
                    group_title, exported_at, source_file
                ) VALUES (?1, '+15555550100', 'SMS', 'individual', NULL, NULL, 't.json')
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
