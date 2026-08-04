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
CREATE INDEX IF NOT EXISTS ix_staging_messages_account_id ON staging_messages (account_id);
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
