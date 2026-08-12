-- Address-book person for one vault account.
CREATE TABLE IF NOT EXISTS contacts (
    -- Surrogate primary key for this contact row.
    id INTEGER PRIMARY KEY,
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Display name shown in the UI (address-book preferred name only).
    preferred_name TEXT NOT NULL,
    -- Address-book shape last changed (not message activity).
    last_modified TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS ix_contacts_account_id ON contacts (account_id);

-- One platform identity (phone, email, username, or other) per account.
CREATE TABLE IF NOT EXISTS handles (
    -- Surrogate primary key for this handle row.
    id INTEGER PRIMARY KEY,
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Identity string exactly as the backup/source wrote it.
    raw TEXT NOT NULL,
    -- Dedup key: E.164 when unambiguous for phones; otherwise cleaned digits/text.
    normalized TEXT NOT NULL,
    -- Human-readable reason the number needs review; NULL when normalization is trusted.
    normalized_note TEXT,
    -- Shape of the identity: 'phone' | 'email' | 'username' | 'other'.
    handle_type TEXT NOT NULL,
    -- Platform identity: 'phone' | 'whatsapp' (not per-message SMS/iMessage/RCS).
    service TEXT NOT NULL,
    UNIQUE(account_id, normalized, handle_type, service)
);

CREATE INDEX IF NOT EXISTS ix_handles_account_id ON handles (account_id);
CREATE INDEX IF NOT EXISTS ix_handles_normalized ON handles (account_id, normalized);

-- Links one handle to at most one contact within an account.
CREATE TABLE IF NOT EXISTS contact_handles (
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Linked identity (`handles.id`).
    handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    -- Address-book person that owns this handle (`contacts.id`).
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    -- Name the source gave for this handle (may differ from preferred_name).
    name_alias TEXT,
    PRIMARY KEY (account_id, handle_id)
);

CREATE INDEX IF NOT EXISTS ix_contact_handles_contact_id
    ON contact_handles (contact_id);

-- Named label a user can attach to contacts.
CREATE TABLE IF NOT EXISTS contact_labels (
    -- Surrogate primary key for this label.
    id INTEGER PRIMARY KEY,
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Label text unique per account.
    name TEXT NOT NULL,
    UNIQUE(account_id, name)
);

-- Membership of a contact in a label.
CREATE TABLE IF NOT EXISTS contact_label_members (
    -- Contact in the label (`contacts.id`).
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    -- Label that includes the contact (`contact_labels.id`).
    label_id INTEGER NOT NULL REFERENCES contact_labels(id) ON DELETE CASCADE,
    PRIMARY KEY (contact_id, label_id)
);

-- Soft-delete marker for a handle; underlying handle row stays.
CREATE TABLE IF NOT EXISTS trashed_handles (
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Handle marked trash (`handles.id`).
    handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    -- When the handle entered trash (SQLite datetime string).
    trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (account_id, handle_id)
);

-- Soft-delete marker for a conversation; chat rows stay until purge.
CREATE TABLE IF NOT EXISTS trashed_conversations (
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Conversation marked trash (`conversations.id`, no FK so chat can remain).
    conversation_id INTEGER NOT NULL,
    -- When the conversation entered trash (SQLite datetime string).
    trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (account_id, conversation_id)
);

-- Soft-delete marker for a contact; contact row stays until purge.
CREATE TABLE IF NOT EXISTS trashed_contacts (
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Contact marked trash (`contacts.id`, no FK so contact can remain).
    contact_id INTEGER NOT NULL,
    -- When the contact entered trash (SQLite datetime string).
    trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (account_id, contact_id)
);
