import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

import Database from "better-sqlite3";

import { assetsDirName, dataDir } from "./paths";

function tableExists(db: Database.Database, name: string): boolean {
  const row = db
    .prepare(
      `SELECT COUNT(*) AS n FROM sqlite_master WHERE type = 'table' AND name = ?`,
    )
    .get(name) as { n: number };
  return row.n > 0;
}

function tableHasColumn(db: Database.Database, table: string, column: string): boolean {
  const row = db
    .prepare(
      `SELECT COUNT(*) AS n FROM pragma_table_info(?) WHERE name = ?`,
    )
    .get(table, column) as { n: number };
  return row.n > 0;
}

/** Fresh-start: drop legacy single-tenant vault tables lacking account_id. */
function wipeLegacyVaultTables(db: Database.Database): void {
  db.exec(`
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
  `);
}

export function ensureVaultSchema(db: Database.Database): void {
  if (
    tableExists(db, "conversations") &&
    !tableHasColumn(db, "conversations", "account_id")
  ) {
    wipeLegacyVaultTables(db);
  }

  db.exec(`PRAGMA foreign_keys = ON;`);

  db.exec(`
    CREATE TABLE IF NOT EXISTS accounts (
      id TEXT PRIMARY KEY,
      username TEXT NOT NULL UNIQUE COLLATE NOCASE,
      read_only INTEGER NOT NULL DEFAULT 0,
      password_hash TEXT,
      first_name TEXT NOT NULL DEFAULT '',
      last_name TEXT NOT NULL DEFAULT '',
      preferred_name TEXT
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
      duplicate_of INTEGER REFERENCES messages(id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS ix_messages_conversation_timestamp
      ON messages (conversation_id, timestamp);
    CREATE INDEX IF NOT EXISTS ix_messages_conversation_source_timestamp
      ON messages (conversation_id, source, timestamp);
    CREATE INDEX IF NOT EXISTS ix_messages_content_key
      ON messages (content_key)
      WHERE content_key IS NOT NULL AND content_key != '';
    CREATE INDEX IF NOT EXISTS ix_messages_duplicate_of
      ON messages (duplicate_of)
      WHERE duplicate_of IS NOT NULL;
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
      sort_order INTEGER NOT NULL
    );

    CREATE INDEX IF NOT EXISTS ix_staging_messages_conversation_timestamp
      ON staging_messages (conversation_id, timestamp);

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
      first_name TEXT,
      last_name TEXT,
      preferred_name TEXT,
      exclude INTEGER NOT NULL DEFAULT 0,
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

  migrateLegacyAccountsEmailColumn(db);
  migrateVaultOwnersIntoAccounts(db);
  migrateContactGroupsToLabels(db);
  migrateContactStatusesToLabels(db);
  migrateContactsPreferredName(db);
  migrateMessagesAccountGuid(db);
  migrateStagingAccountGuid(db);
  migrateAccountsDefaultReadOnly(db);
  migrateAccountsPasswordHash(db);
  migrateAccountApiTokensToHash(db);
  migrateParticipantsHandleIndex(db);
  migrateAttachmentSizeBytes(db);
  backfillAttachmentSizeBytes(db);
  ensureMessagesFts(db);
}

function migrateAccountsPasswordHash(db: Database.Database): void {
  if (!tableExists(db, "accounts")) return;
  if (tableHasColumn(db, "accounts", "password_hash")) return;
  db.exec(`ALTER TABLE accounts ADD COLUMN password_hash TEXT`);
}

/** Add `size_bytes` column on attachments tables. */
function migrateAttachmentSizeBytes(db: Database.Database): void {
  if (tableExists(db, "attachments") && !tableHasColumn(db, "attachments", "size_bytes")) {
    db.exec(`ALTER TABLE attachments ADD COLUMN size_bytes INTEGER`);
  }
  if (
    tableExists(db, "staging_attachments") &&
    !tableHasColumn(db, "staging_attachments", "size_bytes")
  ) {
    db.exec(`ALTER TABLE staging_attachments ADD COLUMN size_bytes INTEGER`);
  }
}

const ATTACHMENT_SIZE_BACKFILL_META_KEY = "attachment_size_bytes_backfill_v1";

/** One-time fill of `size_bytes` from on-disk asset blobs. */
function backfillAttachmentSizeBytes(db: Database.Database): void {
  if (
    !tableExists(db, "attachments") ||
    !tableHasColumn(db, "attachments", "size_bytes")
  ) {
    return;
  }
  db.exec(`
    CREATE TABLE IF NOT EXISTS schema_meta (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );
  `);
  const already = db
    .prepare(`SELECT COUNT(*) AS n FROM schema_meta WHERE key = ?`)
    .get(ATTACHMENT_SIZE_BACKFILL_META_KEY) as { n: number };
  if (already.n > 0) return;

  const rows = db
    .prepare(
      `SELECT a.id AS id, a.assets_path AS assets_path,
              m.source AS source, c.account_id AS account_id
       FROM attachments a
       JOIN messages m ON m.id = a.message_id
       JOIN conversations c ON c.id = m.conversation_id
       WHERE a.size_bytes IS NULL
         AND a.assets_path IS NOT NULL
         AND trim(a.assets_path) != ''`,
    )
    .all() as Array<{
    id: number;
    assets_path: string;
    source: string;
    account_id: string;
  }>;

  const update = db.prepare(
    `UPDATE attachments SET size_bytes = ? WHERE id = ?`,
  );
  const assetsName = assetsDirName();
  const root = dataDir();
  const fill = db.transaction(() => {
    for (const row of rows) {
      const file = path.join(
        root,
        row.account_id,
        row.source,
        assetsName,
        row.assets_path,
      );
      try {
        const st = fs.statSync(file);
        if (st.isFile()) update.run(st.size, row.id);
      } catch {
        // Missing blob — leave NULL.
      }
    }
    db.prepare(`INSERT INTO schema_meta (key, value) VALUES (?, '1')`).run(
      ATTACHMENT_SIZE_BACKFILL_META_KEY,
    );
  });
  fill();
}

function hashApiTokenPlaintext(token: string): string {
  return crypto.createHash("sha256").update(token, "utf8").digest("hex");
}

/** Migrate plaintext `token` → `token_hash` (SHA-256 hex). */
function migrateAccountApiTokensToHash(db: Database.Database): void {
  if (!tableExists(db, "account_api_tokens")) return;
  const hasToken = tableHasColumn(db, "account_api_tokens", "token");
  const hasHash = tableHasColumn(db, "account_api_tokens", "token_hash");
  if (hasHash && !hasToken) return;

  if (hasToken) {
    db.exec(`PRAGMA foreign_keys = OFF;`);
    db.exec(`
      CREATE TABLE account_api_tokens_new (
        account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
        token_hash TEXT NOT NULL UNIQUE,
        created_at TEXT NOT NULL
      );
    `);
    const rows = db
      .prepare(`SELECT account_id, token, created_at FROM account_api_tokens`)
      .all() as Array<{ account_id: string; token: string; created_at: string }>;
    const insert = db.prepare(
      `INSERT INTO account_api_tokens_new (account_id, token_hash, created_at)
       VALUES (?, ?, ?)`,
    );
    for (const row of rows) {
      insert.run(row.account_id, hashApiTokenPlaintext(row.token), row.created_at);
    }
    db.exec(`
      DROP TABLE account_api_tokens;
      ALTER TABLE account_api_tokens_new RENAME TO account_api_tokens;
      PRAGMA foreign_keys = ON;
    `);
    return;
  }

  if (!hasHash) {
    db.exec(`
      DROP TABLE IF EXISTS account_api_tokens;
      CREATE TABLE account_api_tokens (
        account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
        token_hash TEXT NOT NULL UNIQUE,
        created_at TEXT NOT NULL
      );
    `);
  }
}

function migrateParticipantsHandleIndex(db: Database.Database): void {
  if (!tableExists(db, "participants")) return;
  db.exec(
    `CREATE INDEX IF NOT EXISTS ix_participants_handle ON participants (handle);`,
  );
}

/** Marker for the one-time FTS5 backfill of existing messages. */
export const MESSAGES_FTS_BACKFILL_META_KEY = "messages_fts_backfill_v1";

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

/** Marker for the one-time migration that locks existing accounts by default. */
export const ACCOUNTS_DEFAULT_READ_ONLY_META_KEY =
  "accounts_default_read_only_v1";
export const CONTACT_STATUS_LABELS_META_KEY = "contact_status_labels_v1";

/**
 * One-time conversion of the legacy exclude flag into ordinary labels.
 * Future code no longer gives either label special behavior.
 */
function migrateContactStatusesToLabels(db: Database.Database): void {
  if (
    !tableExists(db, "contacts") ||
    !tableExists(db, "contact_labels") ||
    !tableExists(db, "contact_label_members") ||
    !tableExists(db, "schema_meta")
  ) {
    return;
  }
  const already = db
    .prepare(`SELECT COUNT(*) AS n FROM schema_meta WHERE key = ?`)
    .get(CONTACT_STATUS_LABELS_META_KEY) as { n: number };
  if (already.n > 0) return;

  db.transaction(() => {
    db.exec(`
      INSERT INTO contact_labels (account_id, name)
      SELECT DISTINCT c.account_id, 'Active'
      FROM contacts c
      WHERE NOT EXISTS (
        SELECT 1 FROM contact_labels cl
        WHERE cl.account_id = c.account_id AND cl.name = 'Active' COLLATE NOCASE
      );
      INSERT INTO contact_labels (account_id, name)
      SELECT DISTINCT c.account_id, 'Inactive'
      FROM contacts c
      WHERE NOT EXISTS (
        SELECT 1 FROM contact_labels cl
        WHERE cl.account_id = c.account_id AND cl.name = 'Inactive' COLLATE NOCASE
      );

      INSERT OR IGNORE INTO contact_label_members (contact_id, label_id)
      SELECT c.id, cl.id
      FROM contacts c
      JOIN contact_labels cl
        ON cl.account_id = c.account_id
       AND cl.name = CASE WHEN c.exclude != 0 THEN 'Inactive' ELSE 'Active' END
           COLLATE NOCASE;

      UPDATE contacts SET exclude = 0 WHERE exclude != 0;
    `);
    db.prepare(`INSERT INTO schema_meta (key, value) VALUES (?, '1')`).run(
      CONTACT_STATUS_LABELS_META_KEY,
    );
  })();
}

/** One-time: lock every existing account. Later unlocks are preserved. */
function migrateAccountsDefaultReadOnly(db: Database.Database): void {
  if (!tableExists(db, "accounts") || !tableExists(db, "schema_meta")) {
    return;
  }
  const already = db
    .prepare(`SELECT COUNT(*) AS n FROM schema_meta WHERE key = ?`)
    .get(ACCOUNTS_DEFAULT_READ_ONLY_META_KEY) as { n: number };
  if (already.n > 0) return;
  db.prepare(`UPDATE accounts SET read_only = 1`).run();
  db.prepare(`INSERT INTO schema_meta (key, value) VALUES (?, '1')`).run(
    ACCOUNTS_DEFAULT_READ_ONLY_META_KEY,
  );
}

/** Denormalize account_id onto messages; scope GUID uniqueness per account. */
function migrateMessagesAccountGuid(db: Database.Database): void {
  if (!tableExists(db, "messages")) return;

  if (!tableHasColumn(db, "messages", "account_id")) {
    db.exec(
      `ALTER TABLE messages ADD COLUMN account_id TEXT REFERENCES accounts(id);`,
    );
    db.exec(`
      UPDATE messages
      SET account_id = (
        SELECT c.account_id FROM conversations c
        WHERE c.id = messages.conversation_id
      )
      WHERE account_id IS NULL;
    `);
    const orphans = db
      .prepare(
        `SELECT COUNT(*) AS n FROM messages WHERE account_id IS NULL OR account_id = ''`,
      )
      .get() as { n: number };
    if (orphans.n > 0) {
      throw new Error(
        `messages.account_id migration found ${orphans.n} orphan message(s)`,
      );
    }
  }

  // Fresh CREATE TABLE IF NOT EXISTS may still leave a legacy global GUID index.
  db.exec(`
    DROP INDEX IF EXISTS ix_messages_source_guid;
    CREATE INDEX IF NOT EXISTS ix_messages_account_id ON messages (account_id);
    CREATE UNIQUE INDEX IF NOT EXISTS ix_messages_account_source_guid
      ON messages (account_id, source, guid)
      WHERE guid IS NOT NULL AND guid != '';
  `);
}

/**
 * Older staging_messages lacked account_id. Staging is ephemeral — rebuild when
 * the schema is stale so CREATE INDEX on account_id cannot fail at startup.
 */
function migrateStagingAccountGuid(db: Database.Database): void {
  if (!tableExists(db, "staging_messages")) {
    ensureStagingIndexes(db);
    return;
  }
  if (
    !tableHasColumn(db, "staging_messages", "account_id") ||
    indexExists(db, "ix_staging_messages_source_guid")
  ) {
    db.exec(`
      PRAGMA foreign_keys = ON;
      DROP TABLE IF EXISTS staging_tapbacks;
      DROP TABLE IF EXISTS staging_attachments;
      DROP TABLE IF EXISTS staging_messages;
      DROP TABLE IF EXISTS staging_participants;
      DROP TABLE IF EXISTS staging_conversations;
    `);
    db.exec(`
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
        sort_order INTEGER NOT NULL
      );
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
      CREATE TABLE IF NOT EXISTS staging_tapbacks (
        id INTEGER PRIMARY KEY,
        message_id INTEGER NOT NULL REFERENCES staging_messages(id) ON DELETE CASCADE,
        part_index INTEGER NOT NULL DEFAULT 0,
        kind TEXT NOT NULL,
        emoji TEXT,
        is_from_me INTEGER NOT NULL,
        sender TEXT
      );
    `);
  }
  ensureStagingIndexes(db);
}

function ensureStagingIndexes(db: Database.Database): void {
  if (!tableExists(db, "staging_messages")) return;
  db.exec(`
    CREATE INDEX IF NOT EXISTS ix_staging_messages_conversation_timestamp
      ON staging_messages (conversation_id, timestamp);
    DROP INDEX IF EXISTS ix_staging_messages_source_guid;
    CREATE INDEX IF NOT EXISTS ix_staging_messages_account_id
      ON staging_messages (account_id);
    CREATE UNIQUE INDEX IF NOT EXISTS ix_staging_messages_account_source_guid
      ON staging_messages (account_id, source, guid)
      WHERE guid IS NOT NULL AND guid != '';
    CREATE INDEX IF NOT EXISTS ix_staging_attachments_sha256 ON staging_attachments (sha256);
    CREATE INDEX IF NOT EXISTS ix_staging_attachments_message_id ON staging_attachments (message_id);
    CREATE INDEX IF NOT EXISTS ix_staging_tapbacks_message_id ON staging_tapbacks (message_id);
  `);
}

function indexExists(db: Database.Database, name: string): boolean {
  const row = db
    .prepare(
      `SELECT COUNT(*) AS n FROM sqlite_master WHERE type = 'index' AND name = ?`,
    )
    .get(name) as { n: number };
  return row.n > 0;
}

/** Rename legacy contact_groups* tables to contact_labels*. */
function migrateContactGroupsToLabels(db: Database.Database): void {
  if (!tableExists(db, "contact_groups") || tableExists(db, "contact_labels")) {
    return;
  }

  db.exec(`PRAGMA foreign_keys = OFF;`);
  db.exec(`
    ALTER TABLE contact_groups RENAME TO contact_labels;
    ALTER TABLE contact_group_members RENAME TO contact_label_members;
    ALTER TABLE contact_label_members RENAME COLUMN group_id TO label_id;
  `);
  db.exec(`PRAGMA foreign_keys = ON;`);
}

/** Rebuild accounts without legacy email column; emails live in account_emails. */
function migrateLegacyAccountsEmailColumn(db: Database.Database): void {
  if (!tableExists(db, "accounts") || !tableHasColumn(db, "accounts", "email")) {
    return;
  }

  db.exec(`PRAGMA foreign_keys = OFF;`);

  const rows = db
    .prepare(
      `SELECT id, email FROM accounts WHERE email IS NOT NULL AND trim(email) != ''`,
    )
    .all() as Array<{ id: string; email: string }>;

  const insert = db.prepare(
    `INSERT OR IGNORE INTO account_emails (account_id, email, is_primary)
     VALUES (?, ?, 1)`,
  );
  for (const row of rows) {
    insert.run(row.id, row.email.trim());
  }

  db.exec(`
    CREATE TABLE accounts_new (
      id TEXT PRIMARY KEY,
      username TEXT NOT NULL UNIQUE COLLATE NOCASE,
      read_only INTEGER NOT NULL DEFAULT 0,
      password_hash TEXT,
      first_name TEXT NOT NULL DEFAULT '',
      last_name TEXT NOT NULL DEFAULT '',
      preferred_name TEXT
    );
    INSERT INTO accounts_new (
      id, username, read_only, password_hash, first_name, last_name, preferred_name
    )
      SELECT id, username, read_only, NULL, '', '', NULL FROM accounts;
    DROP TABLE accounts;
    ALTER TABLE accounts_new RENAME TO accounts;
  `);

  db.exec(`PRAGMA foreign_keys = ON;`);
}

export const VAULT_OWNERS_INTO_ACCOUNTS_META_KEY = "vault_owners_into_accounts_v1";

/** Fold vault_owners* into accounts + account_phones; drop legacy owner tables. */
function migrateVaultOwnersIntoAccounts(db: Database.Database): void {
  if (!tableExists(db, "accounts")) return;

  if (!tableHasColumn(db, "accounts", "first_name")) {
    db.exec(`ALTER TABLE accounts ADD COLUMN first_name TEXT NOT NULL DEFAULT ''`);
  }
  if (!tableHasColumn(db, "accounts", "last_name")) {
    db.exec(`ALTER TABLE accounts ADD COLUMN last_name TEXT NOT NULL DEFAULT ''`);
  }
  if (!tableHasColumn(db, "accounts", "preferred_name")) {
    db.exec(`ALTER TABLE accounts ADD COLUMN preferred_name TEXT`);
  }

  db.exec(`
    CREATE TABLE IF NOT EXISTS account_phones (
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      phone TEXT NOT NULL,
      PRIMARY KEY (account_id, phone)
    );
  `);

  db.exec(`
    CREATE TABLE IF NOT EXISTS schema_meta (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );
  `);
  const already = db
    .prepare(`SELECT COUNT(*) AS n FROM schema_meta WHERE key = ?`)
    .get(VAULT_OWNERS_INTO_ACCOUNTS_META_KEY) as { n: number };
  if (already.n > 0) return;

  if (tableExists(db, "vault_owners")) {
    if (!tableHasColumn(db, "vault_owners", "first_name")) {
      db.exec(`
        ALTER TABLE vault_owners ADD COLUMN first_name TEXT NOT NULL DEFAULT '';
        ALTER TABLE vault_owners ADD COLUMN last_name TEXT NOT NULL DEFAULT '';
        UPDATE vault_owners
        SET first_name = trim(display_name)
        WHERE first_name = '' OR first_name IS NULL;
      `);
    }

    db.exec(`
      UPDATE accounts
      SET
        first_name = coalesce(
          (SELECT NULLIF(trim(vo.first_name), '') FROM vault_owners vo WHERE vo.account_id = accounts.id),
          first_name
        ),
        last_name = coalesce(
          (SELECT NULLIF(trim(vo.last_name), '') FROM vault_owners vo WHERE vo.account_id = accounts.id),
          last_name
        ),
        preferred_name = coalesce(
          (
            SELECT NULLIF(trim(vo.display_name), '')
            FROM vault_owners vo
            WHERE vo.account_id = accounts.id
          ),
          NULLIF(trim(
            trim(coalesce(
              (SELECT NULLIF(trim(vo.first_name), '') FROM vault_owners vo WHERE vo.account_id = accounts.id),
              ''
            )) || ' ' || trim(coalesce(
              (SELECT NULLIF(trim(vo.last_name), '') FROM vault_owners vo WHERE vo.account_id = accounts.id),
              ''
            ))
          ), ''),
          preferred_name
        )
      WHERE EXISTS (SELECT 1 FROM vault_owners vo WHERE vo.account_id = accounts.id);
    `);

    if (tableExists(db, "vault_owner_phones")) {
      db.exec(`
        INSERT OR IGNORE INTO account_phones (account_id, phone)
        SELECT account_id, phone FROM vault_owner_phones;
      `);
    }
    if (tableExists(db, "vault_owner_emails")) {
      db.exec(`
        INSERT OR IGNORE INTO account_emails (account_id, email, is_primary)
        SELECT account_id, email, 0 FROM vault_owner_emails;
      `);
    }

    db.exec(`
      DROP TABLE IF EXISTS vault_owner_emails;
      DROP TABLE IF EXISTS vault_owner_phones;
      DROP TABLE IF EXISTS vault_owners;
    `);
  }

  db.prepare(`INSERT INTO schema_meta (key, value) VALUES (?, '1')`).run(
    VAULT_OWNERS_INTO_ACCOUNTS_META_KEY,
  );
}

export const CONTACTS_PREFERRED_NAME_META_KEY = "contacts_preferred_name_v1";

/** Add `preferred_name` and backfill from first + last once. */
function migrateContactsPreferredName(db: Database.Database): void {
  if (!tableExists(db, "contacts")) return;
  if (!tableHasColumn(db, "contacts", "preferred_name")) {
    db.exec(`ALTER TABLE contacts ADD COLUMN preferred_name TEXT`);
  }

  db.exec(`
    CREATE TABLE IF NOT EXISTS schema_meta (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );
  `);
  const already = db
    .prepare(`SELECT COUNT(*) AS n FROM schema_meta WHERE key = ?`)
    .get(CONTACTS_PREFERRED_NAME_META_KEY) as { n: number };
  if (already.n > 0) return;

  db.exec(`
    UPDATE contacts
    SET preferred_name = NULLIF(trim(
      trim(coalesce(first_name, '')) || ' ' || trim(coalesce(last_name, ''))
    ), '')
    WHERE preferred_name IS NULL OR trim(preferred_name) = '';
  `);
  db.prepare(`INSERT INTO schema_meta (key, value) VALUES (?, '1')`).run(
    CONTACTS_PREFERRED_NAME_META_KEY,
  );
}
