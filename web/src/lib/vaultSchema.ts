import Database from "better-sqlite3";
import { ensureDbParentDir } from "./paths";

function tableExists(db: Database.Database, name: string): boolean {
  const row = db
    .prepare(
      `SELECT COUNT(*) AS n FROM sqlite_master WHERE type = 'table' AND name = ?`,
    )
    .get(name) as { n: number };
  return row.n > 0;
}

function columnExists(
  db: Database.Database,
  table: string,
  column: string,
): boolean {
  const rows = db.prepare(`PRAGMA table_info(${table})`).all() as Array<{
    name: string;
  }>;
  return rows.some((row) => row.name === column);
}

function ensureAccountColumns(db: Database.Database): void {
  if (!tableExists(db, "accounts")) return;
  if (!columnExists(db, "accounts", "hanko_user_id")) {
    db.exec(`ALTER TABLE accounts ADD COLUMN hanko_user_id TEXT`);
  }
  db.exec(`
    CREATE UNIQUE INDEX IF NOT EXISTS ix_accounts_hanko_user_id
      ON accounts(hanko_user_id)
      WHERE hanko_user_id IS NOT NULL AND hanko_user_id != ''
  `);
}

function ensureImportIdColumns(db: Database.Database): void {
  if (tableExists(db, "messages") && !columnExists(db, "messages", "import_id")) {
    db.exec(
      `ALTER TABLE messages ADD COLUMN import_id INTEGER REFERENCES vault_imports(id) ON DELETE SET NULL`,
    );
  }
  if (
    tableExists(db, "staging_messages") &&
    !columnExists(db, "staging_messages", "import_id")
  ) {
    db.exec(
      `ALTER TABLE staging_messages ADD COLUMN import_id INTEGER REFERENCES vault_imports(id) ON DELETE SET NULL`,
    );
  }
  if (tableExists(db, "messages")) {
    db.exec(`
      CREATE INDEX IF NOT EXISTS ix_messages_import_id
        ON messages (import_id)
        WHERE import_id IS NOT NULL
    `);
  }
}

export function ensureVaultSchema(db: Database.Database): void {
  db.exec(`PRAGMA foreign_keys = ON;`);

  db.exec(`
    CREATE TABLE IF NOT EXISTS accounts (
      id TEXT PRIMARY KEY,
      username TEXT NOT NULL UNIQUE COLLATE NOCASE,
      read_only INTEGER NOT NULL DEFAULT 0,
      password_hash TEXT,
      preferred_name TEXT,
      hanko_user_id TEXT
    );

    CREATE TABLE IF NOT EXISTS schema_meta (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
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

    CREATE TABLE IF NOT EXISTS vault_imports (
      id INTEGER PRIMARY KEY,
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      source TEXT NOT NULL,
      tool TEXT,
      mode TEXT NOT NULL,
      status TEXT NOT NULL,
      started_at TEXT NOT NULL,
      finished_at TEXT,
      message_count INTEGER NOT NULL DEFAULT 0,
      attachment_count INTEGER NOT NULL DEFAULT 0,
      bytes_uploaded INTEGER NOT NULL DEFAULT 0
    );

    CREATE INDEX IF NOT EXISTS ix_vault_imports_account_started
      ON vault_imports(account_id, started_at DESC);

    CREATE TABLE IF NOT EXISTS conversations (
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

    CREATE TABLE IF NOT EXISTS participants (
      id INTEGER PRIMARY KEY,
      conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
      handle TEXT NOT NULL,
      name_hint TEXT,
      UNIQUE(conversation_id, handle)
    );

    CREATE INDEX IF NOT EXISTS ix_participants_handle ON participants (handle);

    CREATE TABLE IF NOT EXISTS messages (
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
      duplicate_of INTEGER REFERENCES messages(id) ON DELETE SET NULL,
      import_id INTEGER REFERENCES vault_imports(id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS ix_messages_conversation_timestamp
      ON messages (conversation_id, timestamp);
    CREATE INDEX IF NOT EXISTS ix_messages_conversation_source_timestamp
      ON messages (conversation_id, source, timestamp);
    CREATE INDEX IF NOT EXISTS ix_messages_account_id ON messages (account_id);
    CREATE UNIQUE INDEX IF NOT EXISTS ix_messages_account_source_guid
      ON messages (account_id, source, guid)
      WHERE guid IS NOT NULL AND guid != '';
    CREATE INDEX IF NOT EXISTS ix_messages_content_key
      ON messages (content_key)
      WHERE content_key IS NOT NULL AND content_key != '';
    CREATE INDEX IF NOT EXISTS ix_messages_duplicate_of
      ON messages (duplicate_of)
      WHERE duplicate_of IS NOT NULL;
    CREATE INDEX IF NOT EXISTS ix_messages_import_id
      ON messages (import_id)
      WHERE import_id IS NOT NULL;
    CREATE INDEX IF NOT EXISTS ix_conversations_account_id
      ON conversations (account_id);

    CREATE TABLE IF NOT EXISTS attachments (
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

    CREATE INDEX IF NOT EXISTS ix_attachments_sha256 ON attachments (sha256);
    CREATE INDEX IF NOT EXISTS ix_attachments_message_id ON attachments (message_id);

    CREATE TABLE IF NOT EXISTS tapbacks (
      id INTEGER PRIMARY KEY,
      message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
      part_index INTEGER NOT NULL DEFAULT 0,
      kind TEXT NOT NULL,
      emoji TEXT,
      is_from_me INTEGER NOT NULL,
      sender TEXT
    );

    CREATE INDEX IF NOT EXISTS ix_tapbacks_message_id ON tapbacks (message_id);
    CREATE INDEX IF NOT EXISTS ix_messages_source ON messages (source);

    CREATE TABLE IF NOT EXISTS staging_conversations (
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

    CREATE TABLE IF NOT EXISTS staging_participants (
      id INTEGER PRIMARY KEY,
      conversation_id INTEGER NOT NULL REFERENCES staging_conversations(id) ON DELETE CASCADE,
      handle TEXT NOT NULL,
      name_hint TEXT,
      UNIQUE(conversation_id, handle)
    );

    CREATE TABLE IF NOT EXISTS staging_messages (
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
      sort_order INTEGER NOT NULL,
      import_id INTEGER REFERENCES vault_imports(id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS ix_staging_messages_conversation_timestamp
      ON staging_messages (conversation_id, timestamp);
    CREATE INDEX IF NOT EXISTS ix_staging_messages_account_id
      ON staging_messages (account_id);
    CREATE UNIQUE INDEX IF NOT EXISTS ix_staging_messages_account_source_guid
      ON staging_messages (account_id, source, guid)
      WHERE guid IS NOT NULL AND guid != '';

    CREATE TABLE IF NOT EXISTS staging_attachments (
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

    CREATE INDEX IF NOT EXISTS ix_staging_attachments_sha256 ON staging_attachments (sha256);
    CREATE INDEX IF NOT EXISTS ix_staging_attachments_message_id ON staging_attachments (message_id);

    CREATE TABLE IF NOT EXISTS staging_tapbacks (
      id INTEGER PRIMARY KEY,
      message_id INTEGER NOT NULL REFERENCES staging_messages(id) ON DELETE CASCADE,
      part_index INTEGER NOT NULL DEFAULT 0,
      kind TEXT NOT NULL,
      emoji TEXT,
      is_from_me INTEGER NOT NULL,
      sender TEXT
    );

    CREATE INDEX IF NOT EXISTS ix_staging_tapbacks_message_id ON staging_tapbacks (message_id);

    CREATE TABLE IF NOT EXISTS contacts (
      id INTEGER PRIMARY KEY,
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      preferred_name TEXT,
      preferred_handle TEXT
    );

    CREATE INDEX IF NOT EXISTS ix_contacts_account_id ON contacts (account_id);

    CREATE TABLE IF NOT EXISTS contact_handles (
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      handle TEXT NOT NULL,
      contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
      PRIMARY KEY (account_id, handle)
    );

    CREATE INDEX IF NOT EXISTS ix_contact_handles_contact_id
      ON contact_handles (contact_id);

    CREATE TABLE IF NOT EXISTS contact_labels (
      id INTEGER PRIMARY KEY,
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      name TEXT NOT NULL,
      UNIQUE(account_id, name)
    );

    CREATE TABLE IF NOT EXISTS contact_label_members (
      contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
      label_id INTEGER NOT NULL REFERENCES contact_labels(id) ON DELETE CASCADE,
      PRIMARY KEY (contact_id, label_id)
    );

    CREATE TABLE IF NOT EXISTS trashed_handles (
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      handle TEXT NOT NULL,
      trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
      PRIMARY KEY (account_id, handle)
    );

    CREATE TABLE IF NOT EXISTS trashed_conversations (
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      conversation_id INTEGER NOT NULL,
      trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
      PRIMARY KEY (account_id, conversation_id)
    );

    CREATE TABLE IF NOT EXISTS trashed_contacts (
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      contact_id INTEGER NOT NULL,
      trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
      PRIMARY KEY (account_id, contact_id)
    );
  `);

  ensureAccountColumns(db);
  ensureImportIdColumns(db);
  ensureMessagesFts(db);
}

/** Open a writable vault connection with the complete current schema ready. */
export function openWritableVaultDb(): Database.Database {
  const db = new Database(ensureDbParentDir(), { timeout: 15000 });
  try {
    db.pragma("journal_mode = WAL");
    db.pragma("busy_timeout = 15000");
    db.pragma("foreign_keys = ON");
    ensureVaultSchema(db);
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}

/** Marker for the one-time FTS5 backfill of existing messages. */
export const MESSAGES_FTS_BACKFILL_META_KEY = "messages_fts_backfill_v1";
/** Marker that current FTS sync trigger definitions are installed. */
export const MESSAGES_FTS_TRIGGERS_META_KEY = "messages_fts_triggers_v1";

/** Contentless FTS5 index over message body/subject plus attachment text. */
function ensureMessagesFts(db: Database.Database): void {
  if (!tableExists(db, "messages")) return;

  db.exec(`
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
  `);

  db.exec(`
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
  `);
  db.prepare(
    `INSERT OR REPLACE INTO schema_meta (key, value) VALUES (?, '1')`,
  ).run(MESSAGES_FTS_TRIGGERS_META_KEY);

  backfillMessagesFts(db);
}

function backfillMessagesFts(db: Database.Database): void {
  if (!tableExists(db, "schema_meta")) return;
  const already = db
    .prepare(`SELECT COUNT(*) AS n FROM schema_meta WHERE key = ?`)
    .get(MESSAGES_FTS_BACKFILL_META_KEY) as { n: number };
  if (already.n > 0) return;

  db.exec(`
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
  `);
  db.prepare(`INSERT INTO schema_meta (key, value) VALUES (?, '1')`).run(
    MESSAGES_FTS_BACKFILL_META_KEY,
  );
}
