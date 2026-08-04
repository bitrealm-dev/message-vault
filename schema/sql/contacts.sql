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
