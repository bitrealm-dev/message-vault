use anyhow::{Result, bail};
use rusqlite::{Connection, params};

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
    first_name TEXT,
    last_name TEXT,
    exclude INTEGER NOT NULL DEFAULT 0,
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

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )?;
    Ok(exists)
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
        [column],
        |row| row.get(0),
    )?;
    Ok(n > 0)
}

/// Fresh-start: drop legacy single-tenant vault tables lacking `account_id`.
fn wipe_legacy_vault_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;
        DROP TABLE IF EXISTS tapbacks;
        DROP TABLE IF EXISTS attachments;
        DROP TABLE IF EXISTS messages;
        DROP TABLE IF EXISTS participants;
        DROP TABLE IF EXISTS conversations;
        DROP TABLE IF EXISTS staging_tapbacks;
        DROP TABLE IF EXISTS staging_attachments;
        DROP TABLE IF EXISTS staging_messages;
        DROP TABLE IF EXISTS staging_participants;
        DROP TABLE IF EXISTS staging_conversations;
        DROP TABLE IF EXISTS contact_label_members;
        DROP TABLE IF EXISTS contact_labels;
        DROP TABLE IF EXISTS contact_group_members;
        DROP TABLE IF EXISTS contact_groups;
        DROP TABLE IF EXISTS contact_handles;
        DROP TABLE IF EXISTS contacts;
        DROP TABLE IF EXISTS trashed_handles;
        DROP TABLE IF EXISTS trashed_conversations;
        DROP TABLE IF EXISTS trashed_contacts;
        PRAGMA foreign_keys = ON;
        "#,
    )?;
    Ok(())
}

/// Ensure multi-account vault schema. Wipes legacy tables when `conversations` lacks `account_id`.
pub fn ensure_vault_schema(conn: &Connection) -> Result<()> {
    if table_exists(conn, "conversations")?
        && !table_has_column(conn, "conversations", "account_id")?
    {
        wipe_legacy_vault_tables(conn)?;
    }

    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    ensure_accounts_schema(conn)?;

    let has_conversations = table_exists(conn, "conversations")?;
    if !has_conversations {
        conn.execute_batch(MESSAGE_TABLES_DDL)?;
    }

    let has_contacts = table_exists(conn, "contacts")?;
    if !has_contacts {
        conn.execute_batch(CONTACTS_TABLES_DDL)?;
    }

    migrate_contact_groups_to_labels(conn)?;

    Ok(())
}

/// Rename legacy contact_groups* tables to contact_labels*.
fn migrate_contact_groups_to_labels(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "contact_groups")? || table_exists(conn, "contact_labels")? {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;
        ALTER TABLE contact_groups RENAME TO contact_labels;
        ALTER TABLE contact_group_members RENAME TO contact_label_members;
        ALTER TABLE contact_label_members RENAME COLUMN group_id TO label_id;
        PRAGMA foreign_keys = ON;
        "#,
    )?;
    Ok(())
}

/// Create production message tables if they do not already exist (for append on a fresh DB).
/// Migrates older schemas that lack `messages.source` / cross-source dedupe columns.
/// Marker for the one-time FTS5 backfill of existing messages.
pub const MESSAGES_FTS_BACKFILL_META_KEY: &str = "messages_fts_backfill_v1";

pub fn ensure_messages_schema(conn: &Connection) -> Result<()> {
    ensure_vault_schema(conn)?;

    let exists = table_exists(conn, "conversations")?;
    if !exists {
        conn.execute_batch(MESSAGE_TABLES_DDL)?;
    } else {
        migrate_messages_source(conn)?;
        migrate_messages_dedupe_columns(conn)?;
        migrate_messages_account_guid(conn)?;
        migrate_delete_performance_indexes(conn)?;
    }
    ensure_messages_fts(conn)?;
    Ok(())
}

/// Contentless FTS5 index over message body/subject plus attachment text.
fn ensure_messages_fts(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "messages")? {
        return Ok(());
    }
    // schema_meta may not exist yet on older DBs that only ran message DDL.
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

    // Recreate sync triggers so definition updates apply cleanly.
    conn.execute_batch(
        r#"
        DROP TRIGGER IF EXISTS messages_fts_ai;
        DROP TRIGGER IF EXISTS messages_fts_ad;
        DROP TRIGGER IF EXISTS messages_fts_au;
        DROP TRIGGER IF EXISTS attachments_fts_ai;
        DROP TRIGGER IF EXISTS attachments_fts_ad;
        DROP TRIGGER IF EXISTS attachments_fts_au;

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
        "#,
    )?;

    backfill_messages_fts(conn)?;
    Ok(())
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

fn migrate_messages_source(conn: &Connection) -> Result<()> {
    if !table_has_column(conn, "messages", "source")? {
        conn.execute_batch(
            r#"
            ALTER TABLE messages ADD COLUMN source TEXT NOT NULL DEFAULT 'default';
            DROP INDEX IF EXISTS ix_messages_guid;
            CREATE INDEX IF NOT EXISTS ix_messages_conversation_source_timestamp
                ON messages (conversation_id, source, timestamp);
            "#,
        )?;
    }
    Ok(())
}

/// Denormalize `account_id` onto messages and scope GUID uniqueness per account.
fn migrate_messages_account_guid(conn: &Connection) -> Result<()> {
    if !table_has_column(conn, "messages", "account_id")? {
        conn.execute_batch(
            "ALTER TABLE messages ADD COLUMN account_id TEXT REFERENCES accounts(id);",
        )?;
        conn.execute_batch(
            r#"
            UPDATE messages
            SET account_id = (
                SELECT c.account_id FROM conversations c
                WHERE c.id = messages.conversation_id
            )
            WHERE account_id IS NULL;
            "#,
        )?;
        let orphans: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE account_id IS NULL OR account_id = ''",
            [],
            |row| row.get(0),
        )?;
        if orphans > 0 {
            bail!(
                "messages.account_id migration found {orphans} orphan message(s) without a conversation account"
            );
        }
    }

    conn.execute_batch(
        r#"
        DROP INDEX IF EXISTS ix_messages_source_guid;
        CREATE INDEX IF NOT EXISTS ix_messages_account_id ON messages (account_id);
        CREATE UNIQUE INDEX IF NOT EXISTS ix_messages_account_source_guid
            ON messages (account_id, source, guid)
            WHERE guid IS NOT NULL AND guid != '';
        "#,
    )?;
    Ok(())
}

fn migrate_messages_dedupe_columns(conn: &Connection) -> Result<()> {
    if !table_has_column(conn, "messages", "content_key")? {
        conn.execute_batch("ALTER TABLE messages ADD COLUMN content_key TEXT;")?;
    }
    if !table_has_column(conn, "messages", "duplicate_of")? {
        conn.execute_batch(
            "ALTER TABLE messages ADD COLUMN duplicate_of INTEGER REFERENCES messages(id) ON DELETE SET NULL;",
        )?;
    }
    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS ix_messages_content_key
            ON messages (content_key)
            WHERE content_key IS NOT NULL AND content_key != '';
        CREATE INDEX IF NOT EXISTS ix_messages_duplicate_of
            ON messages (duplicate_of)
            WHERE duplicate_of IS NOT NULL;
        "#,
    )?;
    Ok(())
}

fn migrate_delete_performance_indexes(conn: &Connection) -> Result<()> {
    // CASCADE deletes on messages are O(n²) without message_id indexes on child tables.
    // Group discovery filters participants by handle.
    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS ix_attachments_message_id ON attachments (message_id);
        CREATE INDEX IF NOT EXISTS ix_tapbacks_message_id ON tapbacks (message_id);
        CREATE INDEX IF NOT EXISTS ix_messages_source ON messages (source);
        CREATE INDEX IF NOT EXISTS ix_participants_handle ON participants (handle);
        "#,
    )?;
    Ok(())
}

/// Delete all production messages (and cascaded rows) for one import source within one account.
pub fn delete_messages_for_source(
    conn: &Connection,
    account_id: &str,
    source: &str,
) -> Result<u64> {
    // Ensure indexes exist even if caller skipped ensure_messages_schema somehow.
    migrate_delete_performance_indexes(conn)?;

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

fn index_exists(conn: &Connection, name: &str) -> Result<bool> {
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'index' AND name = ?1",
        [name],
        |row| row.get(0),
    )?;
    Ok(exists)
}

/// Ensure staging tables exist (idempotent). Migrates older staging schemas in place.
pub fn ensure_staging_schema(conn: &Connection) -> Result<()> {
    ensure_accounts_schema(conn)?;
    if !table_exists(conn, "staging_conversations")? {
        conn.execute_batch(STAGING_TABLES_DDL)?;
        return Ok(());
    }

    // Older staging lacked account_id on messages, or used a global GUID unique index.
    if table_exists(conn, "staging_messages")?
        && (!table_has_column(conn, "staging_messages", "account_id")?
            || index_exists(conn, "ix_staging_messages_source_guid")?)
    {
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            DROP TABLE IF EXISTS staging_tapbacks;
            DROP TABLE IF EXISTS staging_attachments;
            DROP TABLE IF EXISTS staging_messages;
            DROP TABLE IF EXISTS staging_participants;
            DROP TABLE IF EXISTS staging_conversations;
            "#,
        )?;
        conn.execute_batch(STAGING_TABLES_DDL)?;
    }
    Ok(())
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

/// Wipe and recreate all staging tables (emergency / tests). Prefer
/// [`reset_staging_for_account`] for normal imports.
#[allow(dead_code)]
pub fn recreate_staging(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        DROP TABLE IF EXISTS staging_tapbacks;
        DROP TABLE IF EXISTS staging_attachments;
        DROP TABLE IF EXISTS staging_messages;
        DROP TABLE IF EXISTS staging_participants;
        DROP TABLE IF EXISTS staging_conversations;
        "#,
    )?;
    conn.execute_batch(STAGING_TABLES_DDL)?;
    Ok(())
}

/// True when contacts tables match the current multi-account handle-based schema.
pub fn contacts_schema_ready(conn: &Connection) -> Result<bool> {
    let has_handles: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'contact_handles'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_handles {
        return Ok(false);
    }

    let mut stmt = conn.prepare("PRAGMA table_info(contacts)")?;
    let cols: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(cols.iter().any(|c| c == "account_id") && cols.iter().any(|c| c == "preferred_handle"))
}

/// Create contacts tables if they do not already exist.
pub fn ensure_contacts_schema(conn: &Connection) -> Result<()> {
    ensure_vault_schema(conn)?;
    if !table_exists(conn, "contacts")? {
        conn.execute_batch(CONTACTS_TABLES_DDL)?;
    }
    Ok(())
}

/// Web login accounts and per-account vault owner profile tables.
/// Marker for the one-time migration that locks existing accounts by default.
pub const ACCOUNTS_DEFAULT_READ_ONLY_META_KEY: &str = "accounts_default_read_only_v1";

pub fn ensure_accounts_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS accounts (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE COLLATE NOCASE,
            read_only INTEGER NOT NULL DEFAULT 1
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

        CREATE TABLE IF NOT EXISTS vault_owners (
            account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
            first_name TEXT NOT NULL DEFAULT '',
            last_name TEXT NOT NULL DEFAULT '',
            display_name TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS vault_owner_phones (
            account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            phone TEXT NOT NULL,
            PRIMARY KEY (account_id, phone)
        );

        CREATE TABLE IF NOT EXISTS vault_owner_emails (
            account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            email TEXT NOT NULL,
            PRIMARY KEY (account_id, email)
        );

        CREATE TABLE IF NOT EXISTS account_api_tokens (
            account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
            token TEXT NOT NULL UNIQUE,
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
    migrate_legacy_accounts_email(conn)?;
    migrate_vault_owner_name_columns(conn)?;
    migrate_accounts_default_read_only(conn)?;
    Ok(())
}

/// One-time: lock every existing account. Later unlocks are preserved.
fn migrate_accounts_default_read_only(conn: &Connection) -> Result<()> {
    let already: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM schema_meta WHERE key = ?1",
        params![ACCOUNTS_DEFAULT_READ_ONLY_META_KEY],
        |row| row.get(0),
    )?;
    if already {
        return Ok(());
    }
    conn.execute("UPDATE accounts SET read_only = 1", [])?;
    conn.execute(
        "INSERT INTO schema_meta (key, value) VALUES (?1, '1')",
        params![ACCOUNTS_DEFAULT_READ_ONLY_META_KEY],
    )?;
    Ok(())
}

fn migrate_vault_owner_name_columns(conn: &Connection) -> Result<()> {
    let has_first_name: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('vault_owners') WHERE name = 'first_name'",
        [],
        |row| row.get(0),
    )?;
    if has_first_name {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        ALTER TABLE vault_owners ADD COLUMN first_name TEXT NOT NULL DEFAULT '';
        ALTER TABLE vault_owners ADD COLUMN last_name TEXT NOT NULL DEFAULT '';
        UPDATE vault_owners
        SET first_name = trim(display_name)
        WHERE first_name = '' OR first_name IS NULL;
        "#,
    )?;
    Ok(())
}

/// Drop legacy `accounts.email` column; emails live in `account_emails`.
fn migrate_legacy_accounts_email(conn: &Connection) -> Result<()> {
    let has_email: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('accounts') WHERE name = 'email'",
        [],
        |row| row.get(0),
    )?;
    if !has_email {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;

        INSERT OR IGNORE INTO account_emails (account_id, email, is_primary)
        SELECT id, email, 1 FROM accounts
        WHERE email IS NOT NULL AND trim(email) != '';

        CREATE TABLE accounts_new (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE COLLATE NOCASE,
            read_only INTEGER NOT NULL DEFAULT 1
        );
        INSERT INTO accounts_new (id, username, read_only)
            SELECT id, username, read_only FROM accounts;
        DROP TABLE accounts;
        ALTER TABLE accounts_new RENAME TO accounts;

        PRAGMA foreign_keys = ON;
        "#,
    )?;
    Ok(())
}

/// Drop and recreate contacts tables (used when overwriting from CSV).
pub fn recreate_contacts(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        DROP TABLE IF EXISTS contact_label_members;
        DROP TABLE IF EXISTS contact_labels;
        DROP TABLE IF EXISTS contact_group_members;
        DROP TABLE IF EXISTS contact_groups;
        DROP TABLE IF EXISTS contact_handles;
        DROP TABLE IF EXISTS contacts;
        DROP TABLE IF EXISTS trashed_handles;
        DROP TABLE IF EXISTS trashed_conversations;
        DROP TABLE IF EXISTS trashed_contacts;
        "#,
    )?;
    ensure_contacts_schema(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A1: &str = "11111111-1111-1111-1111-111111111111";
    const A2: &str = "22222222-2222-2222-2222-222222222222";

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        ensure_messages_schema(&conn).unwrap();
        ensure_staging_schema(&conn).unwrap();
        for (id, user) in [(A1, "alice"), (A2, "bob")] {
            conn.execute(
                "INSERT INTO accounts (id, username, read_only) VALUES (?1, ?2, 0)",
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
    fn same_source_guid_allowed_across_accounts() {
        let conn = setup();
        let c1: i64 = conn
            .query_row(
                "SELECT id FROM conversations WHERE account_id = ?1",
                params![A1],
                |r| r.get(0),
            )
            .unwrap();
        let c2: i64 = conn
            .query_row(
                "SELECT id FROM conversations WHERE account_id = ?1",
                params![A2],
                |r| r.get(0),
            )
            .unwrap();

        for (conv, acct) in [(c1, A1), (c2, A2)] {
            conn.execute(
                r#"
                INSERT INTO messages (
                    conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order
                ) VALUES (?1, ?2, 'sms-backup-restore', 'same-guid', '2020-01-01T00:00:00Z', 0, 0)
                "#,
                params![conv, acct],
            )
            .unwrap();
        }

        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE guid = 'same-guid'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn reset_staging_for_account_leaves_other_accounts() {
        let conn = setup();
        for acct in [A1, A2] {
            conn.execute(
                r#"
                INSERT INTO staging_conversations (
                    account_id, chat_identifier, service, conversation_type,
                    group_title, exported_at, source_file
                ) VALUES (?1, '+15555550100', 'SMS', 'individual', NULL, NULL, 't.json')
                "#,
                params![acct],
            )
            .unwrap();
            let sid = conn.last_insert_rowid();
            conn.execute(
                r#"
                INSERT INTO staging_messages (
                    conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order
                ) VALUES (?1, ?2, 'sms-backup-restore', 'g1', '2020-01-01T00:00:00Z', 0, 0)
                "#,
                params![sid, acct],
            )
            .unwrap();
        }

        reset_staging_for_account(&conn, A1).unwrap();

        let left: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM staging_conversations WHERE account_id = ?1",
                params![A2],
                |r| r.get(0),
            )
            .unwrap();
        let gone: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM staging_conversations WHERE account_id = ?1",
                params![A1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 1);
        assert_eq!(gone, 0);
        let msgs: i64 = conn
            .query_row("SELECT COUNT(*) FROM staging_messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(msgs, 1);
    }

    #[test]
    fn new_accounts_default_to_read_only() {
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
        assert_eq!(read_only, 1);
    }

    #[test]
    fn migrate_locks_existing_accounts_once() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE accounts (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE COLLATE NOCASE,
                read_only INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, 'alice', 0)",
            params![A1],
        )
        .unwrap();

        ensure_accounts_schema(&conn).unwrap();
        let locked: i64 = conn
            .query_row(
                "SELECT read_only FROM accounts WHERE id = ?1",
                params![A1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(locked, 1);

        conn.execute(
            "UPDATE accounts SET read_only = 0 WHERE id = ?1",
            params![A1],
        )
        .unwrap();
        ensure_accounts_schema(&conn).unwrap();
        let still_unlocked: i64 = conn
            .query_row(
                "SELECT read_only FROM accounts WHERE id = ?1",
                params![A1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(still_unlocked, 0);
    }

    #[test]
    fn messages_fts_backfills_once_and_stays_in_sync() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        ensure_messages_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, 'alice', 1)",
            params![A1],
        )
        .unwrap();
        conn.execute(
            r#"
            INSERT INTO conversations (
                account_id, chat_identifier, service, conversation_type,
                group_title, exported_at, source_file
            ) VALUES (?1, '+15555550100', 'SMS', 'individual', NULL, NULL, 't.json')
            "#,
            params![A1],
        )
        .unwrap();
        let cid = conn.last_insert_rowid();
        conn.execute(
            r#"
            INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, sort_order, body, subject
            ) VALUES (?1, ?2, 'sms', 'g1', '2020-01-01T00:00:00Z', 0, 0, 'hello vault', NULL)
            "#,
            params![cid, A1],
        )
        .unwrap();
        let mid = conn.last_insert_rowid();

        // Backfill marker should already be written by ensure_messages_schema.
        let marker: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_meta WHERE key = ?1",
                params![MESSAGES_FTS_BACKFILL_META_KEY],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(marker, 1);

        // Trigger path indexes new inserts.
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'vault'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);

        conn.execute(
            "UPDATE messages SET body = 'goodbye' WHERE id = ?1",
            params![mid],
        )
        .unwrap();
        let after_update: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'vault'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after_update, 0);
        let goodbye: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'goodbye'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(goodbye, 1);

        conn.execute("DELETE FROM messages WHERE id = ?1", params![mid])
            .unwrap();
        let after_delete: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'goodbye'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after_delete, 0);

        // Subsequent ensure must not wipe a user's later index state via re-backfill.
        ensure_messages_schema(&conn).unwrap();
        let still_empty: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'goodbye'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(still_empty, 0);
    }
}
