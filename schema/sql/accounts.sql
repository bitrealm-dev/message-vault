-- Vault login account (web UI + API owner).
CREATE TABLE IF NOT EXISTS accounts (
    -- Stable account id (opaque string primary key).
    id TEXT PRIMARY KEY,
    -- Login user id; unique case-insensitively.
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    -- 1 = demo/read-only account that must not mutate data; 0 = normal.
    read_only INTEGER NOT NULL DEFAULT 0,
    -- Password verifier hash; NULL when password auth is unused.
    password_hash TEXT,
    -- Display name for “you” in the UI.
    preferred_name TEXT,
    -- Optional Hanko identity provider user id.
    hanko_user_id TEXT,
    -- 'ready' | 'assigned' for hosted guest copies; NULL for every other account.
    guest_status TEXT
);

-- Email addresses attached to an account (not used for login).
CREATE TABLE IF NOT EXISTS account_emails (
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Email address; unique case-insensitively across the vault.
    email TEXT NOT NULL UNIQUE COLLATE NOCASE,
    -- 1 = primary email for this account; at most one per account via partial index.
    is_primary INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account_id, email)
);

CREATE UNIQUE INDEX IF NOT EXISTS ix_account_emails_one_primary
    ON account_emails(account_id)
    WHERE is_primary = 1;

-- Handles that mean “me” when matching message participants.
CREATE TABLE IF NOT EXISTS account_handles (
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Self identity (`handles.id`).
    handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    PRIMARY KEY (account_id, handle_id)
);

-- GUI session Bearer (one per account; rotates on login). Prefix: mv-user-
CREATE TABLE IF NOT EXISTS account_session_tokens (
    -- Owning vault account (`accounts.id`); also the primary key (one session).
    account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    -- Hash of the session Bearer secret (never store the raw token).
    token_hash TEXT NOT NULL UNIQUE,
    -- Unix-seconds string for when this session token was issued.
    created_at TEXT NOT NULL,
    -- Unix-seconds string; session rejected after this time.
    expires_at TEXT NOT NULL DEFAULT '0'
);

-- Named CLI API tokens (many per account). Prefix: mv-api-
-- scopes: 'import' | 'export' | 'both'
CREATE TABLE IF NOT EXISTS account_api_tokens (
    -- Opaque token id (primary key).
    id TEXT PRIMARY KEY,
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- User-visible label in Settings.
    label TEXT NOT NULL,
    -- Hash of the API Bearer secret (never store the raw token).
    token_hash TEXT NOT NULL UNIQUE,
    -- Allowed operations: 'import' | 'export' | 'both'.
    scopes TEXT NOT NULL DEFAULT 'both',
    -- Masked form for Settings (e.g. mv-api-Sd..mE). Not enough to recover the secret.
    token_hint TEXT NOT NULL DEFAULT 'mv-api-..',
    -- Unix-seconds string for when this API token was created.
    created_at TEXT NOT NULL,
    -- Unix-seconds string; NULL until first successful Bearer use.
    last_accessed_at TEXT,
    -- Unix-seconds string; NULL means no expiry.
    expires_at TEXT,
    -- Soft-disable without deleting the row.
    disabled INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS ix_account_api_tokens_account
    ON account_api_tokens(account_id);

-- Per-account key/value preferences for the UI and server.
CREATE TABLE IF NOT EXISTS account_prefs (
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Preference name (for example theme or feature flags).
    key TEXT NOT NULL,
    -- Preference value stored as text.
    value TEXT NOT NULL,
    PRIMARY KEY (account_id, key)
);

-- Process-wide schema markers (for example FTS trigger install flag).
CREATE TABLE IF NOT EXISTS schema_meta (
    -- Marker name (for example messages_fts_triggers_v1).
    key TEXT PRIMARY KEY,
    -- Marker value (usually '1' when installed).
    value TEXT NOT NULL
);

-- One row per import run into the vault.
CREATE TABLE IF NOT EXISTS vault_imports (
    -- Surrogate primary key for this import run.
    id INTEGER PRIMARY KEY,
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Backup/source family (for example imessage, whatsapp, sms-backup-restore).
    source TEXT NOT NULL,
    -- Client/tool name that performed the import (optional).
    tool TEXT,
    -- Import mode string recorded by the importer.
    mode TEXT NOT NULL,
    -- Run status (for example running, completed, failed, cancelled).
    status TEXT NOT NULL,
    -- When the import started.
    started_at TEXT NOT NULL,
    -- When the import finished; NULL while still running.
    finished_at TEXT,
    -- Messages accepted during this run.
    message_count INTEGER NOT NULL DEFAULT 0,
    -- Attachments accepted during this run.
    attachment_count INTEGER NOT NULL DEFAULT 0,
    -- Bytes uploaded for assets during this run.
    bytes_uploaded INTEGER NOT NULL DEFAULT 0,
    -- Wall-clock duration of the whole run in milliseconds.
    duration_ms INTEGER,
    -- Time spent parsing input in milliseconds.
    parse_ms INTEGER,
    -- Time spent converting/media work in milliseconds.
    convert_ms INTEGER,
    -- Time spent uploading in milliseconds.
    upload_ms INTEGER,
    -- JSON blob with a human-readable run summary for Import History.
    summary_json TEXT
);

CREATE INDEX IF NOT EXISTS ix_vault_imports_account_started
    ON vault_imports(account_id, started_at DESC);

-- Per-item warning or error recorded during an import run.
CREATE TABLE IF NOT EXISTS vault_import_issues (
    -- Surrogate primary key for this issue row.
    id INTEGER PRIMARY KEY,
    -- Parent import run (`vault_imports.id`).
    import_id INTEGER NOT NULL REFERENCES vault_imports(id) ON DELETE CASCADE,
    -- Issue class (for example warning or error).
    kind TEXT NOT NULL,
    -- Pipeline step where the issue happened.
    step TEXT NOT NULL,
    -- Item identifier (path, guid, or similar).
    item TEXT NOT NULL,
    -- Human-readable explanation.
    reason TEXT NOT NULL,
    -- When the issue was recorded.
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_vault_import_issues_import
    ON vault_import_issues(import_id);

CREATE UNIQUE INDEX IF NOT EXISTS ix_accounts_hanko_user_id
    ON accounts(hanko_user_id)
    WHERE hanko_user_id IS NOT NULL AND hanko_user_id != '';
