CREATE TABLE IF NOT EXISTS conversations (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    chat_handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    conversation_type TEXT NOT NULL,
    group_title TEXT,
    exported_at TEXT,
    source_file TEXT NOT NULL,
    UNIQUE(account_id, chat_handle_id)
);

CREATE INDEX IF NOT EXISTS ix_conversations_account_id ON conversations (account_id);

CREATE TABLE IF NOT EXISTS participants (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    contact_id INTEGER REFERENCES contacts(id) ON DELETE SET NULL,
    name_alias TEXT,
    UNIQUE(conversation_id, handle_id)
);

CREATE INDEX IF NOT EXISTS ix_participants_handle_id ON participants (handle_id);
CREATE INDEX IF NOT EXISTS ix_participants_contact_id ON participants (contact_id);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    guid TEXT,
    timestamp TEXT NOT NULL,
    timestamp_utc TEXT,
    is_from_me INTEGER NOT NULL,
    sender_handle_id INTEGER REFERENCES handles(id) ON DELETE SET NULL,
    -- Per-message transport: sms / imessage / rcs / whatsapp / …
    service TEXT,
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
CREATE INDEX IF NOT EXISTS ix_messages_source ON messages (source);

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
    sender_handle_id INTEGER REFERENCES handles(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS ix_tapbacks_message_id ON tapbacks (message_id);
