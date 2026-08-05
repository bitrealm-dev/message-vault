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

CREATE TABLE IF NOT EXISTS account_handles (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    PRIMARY KEY (account_id, handle_id)
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

CREATE UNIQUE INDEX IF NOT EXISTS ix_accounts_hanko_user_id
    ON accounts(hanko_user_id)
    WHERE hanko_user_id IS NOT NULL AND hanko_user_id != '';
