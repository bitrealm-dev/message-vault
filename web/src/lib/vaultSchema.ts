import Database from "better-sqlite3";

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
      read_only INTEGER NOT NULL DEFAULT 0
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
  migrateVaultOwnerNameColumns(db);
  migrateContactGroupsToLabels(db);
  migrateMessagesAccountGuid(db);
  migrateStagingAccountGuid(db);
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
      read_only INTEGER NOT NULL DEFAULT 0
    );
    INSERT INTO accounts_new (id, username, read_only)
      SELECT id, username, read_only FROM accounts;
    DROP TABLE accounts;
    ALTER TABLE accounts_new RENAME TO accounts;
  `);

  db.exec(`PRAGMA foreign_keys = ON;`);
}

function migrateVaultOwnerNameColumns(db: Database.Database): void {
  if (!tableExists(db, "vault_owners")) return;
  if (tableHasColumn(db, "vault_owners", "first_name")) return;

  db.exec(`
    ALTER TABLE vault_owners ADD COLUMN first_name TEXT NOT NULL DEFAULT '';
    ALTER TABLE vault_owners ADD COLUMN last_name TEXT NOT NULL DEFAULT '';
    UPDATE vault_owners
    SET first_name = trim(display_name)
    WHERE first_name = '' OR first_name IS NULL;
  `);
}
