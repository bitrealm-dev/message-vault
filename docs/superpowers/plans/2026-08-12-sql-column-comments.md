# SQL Column Comments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Document every column in the vault SQLite DDL under `schema/sql/` with a `--` comment so engineers reading the source (or the generated TypeScript copy) know what each field means without leaving the file.

**Architecture:** SQLite has no `COMMENT ON COLUMN` statement. Comments live only in the `.sql` source as `--` lines immediately above each column. The Rust server embeds those files via `include_str!`; `scripts/sync-vault-schema.mjs` copies the same text into `web-next/src/lib/vaultSchema.generated.ts`. A small Node checker fails if any `CREATE TABLE` / `CREATE VIRTUAL TABLE` column lacks a preceding comment. Indexes and FTS trigger bodies are out of scope (they are not table fields).

**Tech Stack:** SQLite DDL in `schema/sql/`, Node checker script, `scripts/sync-vault-schema.mjs`, `cargo test -p message-vault-server`.

## Global Constraints

- Comment style: one `--` line immediately above each column; plain English; include allowed values when the column is an enum-like string.
- Do not change column names, types, defaults, constraints, indexes, or trigger SQL.
- Do not use PostgreSQL-style `COMMENT ON` (SQLite ignores / rejects it).
- Touch only `schema/sql/*.sql` (table DDL), the checker script, and regenerated `web-next/src/lib/vaultSchema.generated.ts`.
- Skip `fts_triggers_create.sql` and `fts_triggers_drop.sql` (no column definitions).
- Keep meanings consistent with `docs/src/content/docs/reference/database.md` and with existing in-file comments (for example `handles.service` vs `messages.service`).
- Staging columns use the same field meanings as the lasting tables they mirror; note “staging” only in the table header comment.
- After editing SQL, always run `node scripts/sync-vault-schema.mjs` so the generated TypeScript copy stays identical.

## File map

| File | Role |
|------|------|
| `scripts/check-sql-column-comments.mjs` | Fails if any table/virtual-table column lacks a preceding `--` comment |
| `schema/sql/contacts.sql` | `contacts`, `handles`, `contact_handles`, labels, trash markers |
| `schema/sql/accounts.sql` | Accounts, tokens, prefs, `schema_meta`, import history |
| `schema/sql/messages.sql` | Conversations, participants, messages, attachments, tapbacks |
| `schema/sql/staging.sql` | Import scratch tables mirroring lasting message tables |
| `schema/sql/fts_virtual.sql` | Contentless FTS5 columns |
| `web-next/src/lib/vaultSchema.generated.ts` | Regenerated DDL strings (do not hand-edit) |

---

### Task 1: Column-comment checker

**Files:**
- Create: `scripts/check-sql-column-comments.mjs`

**Interfaces:**
- Consumes: `schema/sql/{accounts,messages,staging,contacts,fts_virtual}.sql`
- Produces: exit code `0` when every column has a `--` comment on the previous non-empty line; exit code `1` with a `file:line:column` list otherwise

- [ ] **Step 1: Write the checker**

Create `scripts/check-sql-column-comments.mjs`:

```js
#!/usr/bin/env node
/**
 * Fail if any CREATE TABLE / CREATE VIRTUAL TABLE column in schema/sql
 * lacks a `--` comment on the immediately preceding non-empty line.
 *
 * Usage: node scripts/check-sql-column-comments.mjs
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sqlDir = path.join(root, "schema", "sql");
const FILES = [
  "accounts.sql",
  "messages.sql",
  "staging.sql",
  "contacts.sql",
  "fts_virtual.sql",
];

const COLUMN_RE =
  /^\s{4}([a-z_][a-z0-9_]*)\s+(INTEGER|TEXT|REAL|BLOB)\b/i;
const FTS_COLUMN_RE = /^\s{4}([a-z_][a-z0-9_]*),?\s*$/i;
const FTS_OPTION_RE = /^\s{4}(content|tokenize)\s*=/i;
const CONSTRAINT_START =
  /^\s{4}(UNIQUE|PRIMARY\s+KEY|FOREIGN\s+KEY|CHECK|CONSTRAINT)\b/i;

function previousNonEmpty(lines, index) {
  for (let i = index - 1; i >= 0; i--) {
    const t = lines[i].trim();
    if (t.length === 0) continue;
    return { text: t, line: i + 1 };
  }
  return null;
}

function checkFile(file) {
  const full = path.join(sqlDir, file);
  const lines = fs.readFileSync(full, "utf8").split(/\r?\n/);
  const errors = [];
  let inCreate = false;
  let inFts = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    if (/^CREATE\s+TABLE\b/i.test(trimmed)) {
      inCreate = true;
      inFts = false;
      continue;
    }
    if (/^CREATE\s+VIRTUAL\s+TABLE\b/i.test(trimmed)) {
      inCreate = true;
      inFts = /USING\s+fts5\s*\(/i.test(trimmed);
      continue;
    }
    if (inCreate && trimmed === ");") {
      inCreate = false;
      inFts = false;
      continue;
    }
    if (!inCreate) continue;
    if (CONSTRAINT_START.test(line)) continue;
    if (inFts && FTS_OPTION_RE.test(line)) continue;

    let col = null;
    if (inFts) {
      const m = line.match(FTS_COLUMN_RE);
      if (m) col = m[1];
    } else {
      const m = line.match(COLUMN_RE);
      if (m) col = m[1];
    }
    if (!col) continue;

    const prev = previousNonEmpty(lines, i);
    if (!prev || !prev.text.startsWith("--")) {
      errors.push(`${file}:${i + 1}:${col}`);
    }
  }
  return errors;
}

const all = FILES.flatMap(checkFile);
if (all.length === 0) {
  console.log("check-sql-column-comments: OK");
  process.exit(0);
}
console.error("Missing column comments:");
for (const e of all) console.error(`  ${e}`);
process.exit(1);
```

- [ ] **Step 2: Run the checker to verify it fails**

Run: `node scripts/check-sql-column-comments.mjs`

Expected: exit code `1`, many lines like `contacts.sql:2:id`, `messages.sql:2:id`, etc. (only the handful of already-commented columns pass).

- [ ] **Step 3: Commit**

```bash
git add scripts/check-sql-column-comments.mjs
git commit -m "$(cat <<'EOF'
test: add checker for SQL column comments

EOF
)"
```

---

### Task 2: Comment every column in `contacts.sql`

**Files:**
- Modify: `schema/sql/contacts.sql` (full file rewrite of comments only)

**Interfaces:**
- Consumes: Task 1 checker
- Produces: fully commented contacts / handles / trash DDL

- [ ] **Step 1: Replace `schema/sql/contacts.sql` with commented DDL**

Overwrite the file with:

```sql
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
```

- [ ] **Step 2: Run the checker scoped to contacts (expect remaining failures elsewhere)**

Run: `node scripts/check-sql-column-comments.mjs 2>&1 | rg 'contacts\.sql' || true`

Expected: no `contacts.sql:` lines in the error output (other files still fail).

- [ ] **Step 3: Commit**

```bash
git add schema/sql/contacts.sql
git commit -m "$(cat <<'EOF'
docs: comment every column in contacts.sql

EOF
)"
```

---

### Task 3: Comment every column in `accounts.sql`

**Files:**
- Modify: `schema/sql/accounts.sql` (full file rewrite of comments only)

**Interfaces:**
- Consumes: Task 1 checker; `handles` already defined in `contacts.sql` (load order unchanged)
- Produces: fully commented account / token / import DDL

- [ ] **Step 1: Replace `schema/sql/accounts.sql` with commented DDL**

Overwrite the file with:

```sql
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
    hanko_user_id TEXT
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
    -- When this session token was issued (SQLite datetime / ISO string).
    created_at TEXT NOT NULL
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
    -- When this API token was created.
    created_at TEXT NOT NULL,
    -- Unix-seconds string; NULL until first successful Bearer use.
    last_accessed_at TEXT
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
```

- [ ] **Step 2: Confirm accounts.sql is clean in the checker**

Run: `node scripts/check-sql-column-comments.mjs 2>&1 | rg 'accounts\.sql' || true`

Expected: no `accounts.sql:` lines.

- [ ] **Step 3: Commit**

```bash
git add schema/sql/accounts.sql
git commit -m "$(cat <<'EOF'
docs: comment every column in accounts.sql

EOF
)"
```

---

### Task 4: Comment every column in `messages.sql`

**Files:**
- Modify: `schema/sql/messages.sql` (full file rewrite of comments only)

**Interfaces:**
- Consumes: Task 1 checker
- Produces: fully commented conversation / message / attachment / tapback DDL

- [ ] **Step 1: Replace `schema/sql/messages.sql` with commented DDL**

Overwrite the file with:

```sql
-- One chat thread per account + chat handle.
CREATE TABLE IF NOT EXISTS conversations (
    -- Surrogate primary key for this conversation.
    id INTEGER PRIMARY KEY,
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Thread identity handle (`handles.id`); peer for 1:1, group chat id for groups.
    chat_handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    -- Thread shape: 'individual' | 'group' (and any other values the importer writes).
    conversation_type TEXT NOT NULL,
    -- Group display title; NULL for 1:1 chats.
    group_title TEXT,
    -- When the source export was produced (if known).
    exported_at TEXT,
    -- Path or name of the source file this thread came from.
    source_file TEXT NOT NULL,
    UNIQUE(account_id, chat_handle_id)
);

CREATE INDEX IF NOT EXISTS ix_conversations_account_id ON conversations (account_id);

-- One handle listed in a conversation (including the owner when present).
CREATE TABLE IF NOT EXISTS participants (
    -- Surrogate primary key for this participant row.
    id INTEGER PRIMARY KEY,
    -- Parent conversation (`conversations.id`).
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    -- Participant identity (`handles.id`).
    handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    -- Resolved address-book contact when known (`contacts.id`).
    contact_id INTEGER REFERENCES contacts(id) ON DELETE SET NULL,
    -- Display name residue from the source for this participant.
    name_alias TEXT,
    UNIQUE(conversation_id, handle_id)
);

CREATE INDEX IF NOT EXISTS ix_participants_handle_id ON participants (handle_id);
CREATE INDEX IF NOT EXISTS ix_participants_contact_id ON participants (contact_id);

-- One message in a conversation.
CREATE TABLE IF NOT EXISTS messages (
    -- Surrogate primary key for this message.
    id INTEGER PRIMARY KEY,
    -- Parent conversation (`conversations.id`).
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    -- Owning vault account (`accounts.id`) denormalized for account-scoped queries.
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Backup/source family that produced this row (for example imessage, whatsapp).
    source TEXT NOT NULL,
    -- Source-native message id when available; used for exact dedupe with source.
    guid TEXT,
    -- Message time as stored by the source (often local / offset-bearing RFC3339).
    timestamp TEXT NOT NULL,
    -- Message time normalized to UTC when available.
    timestamp_utc TEXT,
    -- 1 = sent by the vault owner; 0 = received from someone else.
    is_from_me INTEGER NOT NULL,
    -- Sender identity (`handles.id`); NULL when unknown.
    sender_handle_id INTEGER REFERENCES handles(id) ON DELETE SET NULL,
    -- Per-message transport: sms / imessage / rcs / whatsapp / …
    service TEXT,
    -- Optional subject line (for example MMS subject).
    subject TEXT,
    -- Plain-text body; may be NULL for attachment-only or announcement rows.
    body TEXT,
    -- 1 = system/announcement bubble; 0 = normal user message.
    is_announcement INTEGER NOT NULL DEFAULT 0,
    -- 1 = this message is a threaded reply; 0 = top-level.
    is_reply INTEGER NOT NULL DEFAULT 0,
    -- GUID of the message this reply refers to (when is_reply = 1).
    thread_originator_guid TEXT,
    -- Part index within the originator message for multi-part replies.
    thread_originator_part INTEGER,
    -- Count of replies hanging off this message (denormalized from the source).
    num_replies INTEGER NOT NULL DEFAULT 0,
    -- Stable order within the conversation when timestamps collide.
    sort_order INTEGER NOT NULL,
    -- Hash fingerprint for cross-source duplicate detection (chat/time/body/attachments).
    content_key TEXT,
    -- Points at the kept message when this row is a flagged duplicate.
    duplicate_of INTEGER REFERENCES messages(id) ON DELETE SET NULL,
    -- Import run that inserted this row (`vault_imports.id`).
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

-- File or media part attached to a message.
CREATE TABLE IF NOT EXISTS attachments (
    -- Surrogate primary key for this attachment.
    id INTEGER PRIMARY KEY,
    -- Parent message (`messages.id`).
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    -- Relative path from the export/staging layout when known.
    path TEXT,
    -- Original filename from the source.
    original_name TEXT,
    -- MIME type of the original bytes when known.
    mime_type TEXT,
    -- 1 = sticker; 0 = ordinary attachment.
    is_sticker INTEGER NOT NULL DEFAULT 0,
    -- Optional speech-to-text or OCR text for search.
    transcription TEXT,
    -- SHA-256 hex of the stored original bytes when present.
    sha256 TEXT,
    -- Path under the vault assets store for the original file.
    assets_path TEXT,
    -- Original file size in bytes when known.
    size_bytes INTEGER,
    -- Why bytes are absent (for example not_exported, decrypt_failed); NULL if present.
    missing_reason TEXT,
    -- SHA-256 hex of a converted/compressed derivative used by the browser.
    derived_sha256 TEXT,
    -- Path under the vault assets store for the derivative file.
    derived_assets_path TEXT,
    -- MIME type of the derivative file.
    derived_mime_type TEXT
);

CREATE INDEX IF NOT EXISTS ix_attachments_sha256 ON attachments (sha256);
CREATE INDEX IF NOT EXISTS ix_attachments_message_id ON attachments (message_id);

-- Reaction (tapback) on a message part.
CREATE TABLE IF NOT EXISTS tapbacks (
    -- Surrogate primary key for this reaction.
    id INTEGER PRIMARY KEY,
    -- Parent message (`messages.id`).
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    -- Which part of a multi-part message this reaction targets (0 = first/default).
    part_index INTEGER NOT NULL DEFAULT 0,
    -- Reaction kind from the source (for example loved, liked, emphasized).
    kind TEXT NOT NULL,
    -- Emoji glyph when the reaction is custom/emoji-based.
    emoji TEXT,
    -- 1 = reaction from the vault owner; 0 = from someone else.
    is_from_me INTEGER NOT NULL,
    -- Reactor identity (`handles.id`); NULL when unknown.
    sender_handle_id INTEGER REFERENCES handles(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS ix_tapbacks_message_id ON tapbacks (message_id);
```

- [ ] **Step 2: Confirm messages.sql is clean in the checker**

Run: `node scripts/check-sql-column-comments.mjs 2>&1 | rg 'messages\.sql' || true`

Expected: no `messages.sql:` lines.

- [ ] **Step 3: Commit**

```bash
git add schema/sql/messages.sql
git commit -m "$(cat <<'EOF'
docs: comment every column in messages.sql

EOF
)"
```

---

### Task 5: Comment every column in `staging.sql`

**Files:**
- Modify: `schema/sql/staging.sql` (full file rewrite of comments only)

**Interfaces:**
- Consumes: Task 1 checker; meanings match Task 4 lasting tables
- Produces: fully commented staging DDL

- [ ] **Step 1: Replace `schema/sql/staging.sql` with commented DDL**

Overwrite the file with:

```sql
-- Import scratch copy of conversations; cleared/promoted per account during import.
CREATE TABLE IF NOT EXISTS staging_conversations (
    -- Surrogate primary key for this staging conversation.
    id INTEGER PRIMARY KEY,
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Thread identity handle id (resolved into handles during staging).
    chat_handle_id INTEGER NOT NULL,
    -- Thread shape: 'individual' | 'group' (and any other values the importer writes).
    conversation_type TEXT NOT NULL,
    -- Group display title; NULL for 1:1 chats.
    group_title TEXT,
    -- When the source export was produced (if known).
    exported_at TEXT,
    -- Path or name of the source file this thread came from.
    source_file TEXT NOT NULL,
    UNIQUE(account_id, chat_handle_id)
);

-- Import scratch copy of participants.
CREATE TABLE IF NOT EXISTS staging_participants (
    -- Surrogate primary key for this staging participant.
    id INTEGER PRIMARY KEY,
    -- Parent staging conversation (`staging_conversations.id`).
    conversation_id INTEGER NOT NULL REFERENCES staging_conversations(id) ON DELETE CASCADE,
    -- Participant identity handle id (resolved during staging).
    handle_id INTEGER NOT NULL,
    -- Optional contact id when already resolved during staging.
    contact_id INTEGER,
    -- Display name residue from the source for this participant.
    name_alias TEXT,
    UNIQUE(conversation_id, handle_id)
);

-- Import scratch copy of messages (no content_key / duplicate_of until promote).
CREATE TABLE IF NOT EXISTS staging_messages (
    -- Surrogate primary key for this staging message.
    id INTEGER PRIMARY KEY,
    -- Parent staging conversation (`staging_conversations.id`).
    conversation_id INTEGER NOT NULL REFERENCES staging_conversations(id) ON DELETE CASCADE,
    -- Owning vault account (`accounts.id`).
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Backup/source family that produced this row.
    source TEXT NOT NULL,
    -- Source-native message id when available.
    guid TEXT,
    -- Message time as stored by the source.
    timestamp TEXT NOT NULL,
    -- Message time normalized to UTC when available.
    timestamp_utc TEXT,
    -- 1 = sent by the vault owner; 0 = received from someone else.
    is_from_me INTEGER NOT NULL,
    -- Sender identity handle id; NULL when unknown.
    sender_handle_id INTEGER,
    -- Per-message transport: sms / imessage / rcs / whatsapp / …
    service TEXT,
    -- Optional subject line.
    subject TEXT,
    -- Plain-text body; may be NULL for attachment-only or announcement rows.
    body TEXT,
    -- 1 = system/announcement bubble; 0 = normal user message.
    is_announcement INTEGER NOT NULL DEFAULT 0,
    -- 1 = this message is a threaded reply; 0 = top-level.
    is_reply INTEGER NOT NULL DEFAULT 0,
    -- GUID of the message this reply refers to (when is_reply = 1).
    thread_originator_guid TEXT,
    -- Part index within the originator message for multi-part replies.
    thread_originator_part INTEGER,
    -- Count of replies hanging off this message (denormalized from the source).
    num_replies INTEGER NOT NULL DEFAULT 0,
    -- Stable order within the conversation when timestamps collide.
    sort_order INTEGER NOT NULL,
    -- Import run that staged this row (`vault_imports.id`).
    import_id INTEGER REFERENCES vault_imports(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS ix_staging_messages_conversation_timestamp
    ON staging_messages (conversation_id, timestamp);
CREATE INDEX IF NOT EXISTS ix_staging_messages_account_id ON staging_messages (account_id);
CREATE UNIQUE INDEX IF NOT EXISTS ix_staging_messages_account_source_guid
    ON staging_messages (account_id, source, guid)
    WHERE guid IS NOT NULL AND guid != '';

-- Import scratch copy of attachments.
CREATE TABLE IF NOT EXISTS staging_attachments (
    -- Surrogate primary key for this staging attachment.
    id INTEGER PRIMARY KEY,
    -- Parent staging message (`staging_messages.id`).
    message_id INTEGER NOT NULL REFERENCES staging_messages(id) ON DELETE CASCADE,
    -- Relative path from the export/staging layout when known.
    path TEXT,
    -- Original filename from the source.
    original_name TEXT,
    -- MIME type of the original bytes when known.
    mime_type TEXT,
    -- 1 = sticker; 0 = ordinary attachment.
    is_sticker INTEGER NOT NULL DEFAULT 0,
    -- Optional speech-to-text or OCR text for search.
    transcription TEXT,
    -- SHA-256 hex of the stored original bytes when present.
    sha256 TEXT,
    -- Path under the vault assets store for the original file.
    assets_path TEXT,
    -- Original file size in bytes when known.
    size_bytes INTEGER,
    -- Why bytes are absent; NULL if present.
    missing_reason TEXT,
    -- SHA-256 hex of a converted/compressed derivative.
    derived_sha256 TEXT,
    -- Path under the vault assets store for the derivative file.
    derived_assets_path TEXT,
    -- MIME type of the derivative file.
    derived_mime_type TEXT
);

CREATE INDEX IF NOT EXISTS ix_staging_attachments_sha256 ON staging_attachments (sha256);
CREATE INDEX IF NOT EXISTS ix_staging_attachments_message_id ON staging_attachments (message_id);

-- Import scratch copy of tapbacks.
CREATE TABLE IF NOT EXISTS staging_tapbacks (
    -- Surrogate primary key for this staging reaction.
    id INTEGER PRIMARY KEY,
    -- Parent staging message (`staging_messages.id`).
    message_id INTEGER NOT NULL REFERENCES staging_messages(id) ON DELETE CASCADE,
    -- Which part of a multi-part message this reaction targets (0 = first/default).
    part_index INTEGER NOT NULL DEFAULT 0,
    -- Reaction kind from the source (for example loved, liked, emphasized).
    kind TEXT NOT NULL,
    -- Emoji glyph when the reaction is custom/emoji-based.
    emoji TEXT,
    -- 1 = reaction from the vault owner; 0 = from someone else.
    is_from_me INTEGER NOT NULL,
    -- Reactor identity handle id; NULL when unknown.
    sender_handle_id INTEGER
);

CREATE INDEX IF NOT EXISTS ix_staging_tapbacks_message_id ON staging_tapbacks (message_id);
```

- [ ] **Step 2: Confirm staging.sql is clean in the checker**

Run: `node scripts/check-sql-column-comments.mjs 2>&1 | rg 'staging\.sql' || true`

Expected: no `staging.sql:` lines.

- [ ] **Step 3: Commit**

```bash
git add schema/sql/staging.sql
git commit -m "$(cat <<'EOF'
docs: comment every column in staging.sql

EOF
)"
```

---

### Task 6: Comment every column in `fts_virtual.sql`

**Files:**
- Modify: `schema/sql/fts_virtual.sql`

**Interfaces:**
- Consumes: Task 1 checker (FTS column branch)
- Produces: fully commented FTS5 virtual table

- [ ] **Step 1: Replace `schema/sql/fts_virtual.sql` with commented DDL**

Overwrite the file with:

```sql
-- Contentless FTS5 index over message body/subject plus attachment text.
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    -- Indexed message body text (synced from messages.body).
    body,
    -- Indexed message subject text (synced from messages.subject).
    subject,
    -- Indexed attachment filenames + transcriptions for the message.
    attachment_text,
    content='',
    tokenize='unicode61 remove_diacritics 2'
);
```

- [ ] **Step 2: Run the full checker — expect PASS**

Run: `node scripts/check-sql-column-comments.mjs`

Expected: `check-sql-column-comments: OK` and exit code `0`.

- [ ] **Step 3: Commit**

```bash
git add schema/sql/fts_virtual.sql
git commit -m "$(cat <<'EOF'
docs: comment every column in fts_virtual.sql

EOF
)"
```

---

### Task 7: Regenerate generated schema and verify apply still works

**Files:**
- Regenerate: `web-next/src/lib/vaultSchema.generated.ts` (via sync script)
- Possibly unchanged: `tests/fixtures/schema/current-schema.json` (structure-only; comments must not alter it)

**Interfaces:**
- Consumes: commented SQL from Tasks 2–6
- Produces: generated TS strings that include the new `--` comments; schema apply tests still green

- [ ] **Step 1: Sync generated TypeScript**

Run:

```bash
node scripts/sync-vault-schema.mjs
```

Expected: script reports it wrote `web-next/src/lib/vaultSchema.generated.ts` (and does **not** change the structural fixture, or only rewrites it identically).

- [ ] **Step 2: Confirm sync check is clean**

Run:

```bash
node scripts/sync-vault-schema.mjs --check
node scripts/check-sql-column-comments.mjs
```

Expected: both exit `0`.

- [ ] **Step 3: Apply schema via Rust tests**

Run:

```bash
cargo test -p message-vault-server --lib schema::
```

If that filter matches nothing useful, run:

```bash
cargo test -p message-vault-server current_schema -- --nocapture
```

Expected: PASS. Comments must not change table/index/trigger names in `tests/fixtures/schema/current-schema.json`.

- [ ] **Step 4: Spot-check that comments survived into the generated file**

Run:

```bash
rg -n "Per-message transport|Platform identity|Address-book shape last changed" web-next/src/lib/vaultSchema.generated.ts
```

Expected: matches inside the embedded DDL template strings.

- [ ] **Step 5: Commit**

```bash
git add web-next/src/lib/vaultSchema.generated.ts tests/fixtures/schema/current-schema.json
git commit -m "$(cat <<'EOF'
chore: regenerate vault schema after SQL column comments

EOF
)"
```

If `git status` shows `current-schema.json` unchanged, omit it from `git add`.

---

## Self-review

**Spec coverage (user request: comment all fields in SQL files):**
- All `CREATE TABLE` columns in `contacts.sql`, `accounts.sql`, `messages.sql`, `staging.sql` → Tasks 2–5
- All FTS5 indexed columns in `fts_virtual.sql` → Task 6
- Enforcement that none were skipped → Task 1 + Task 6 Step 2
- Downstream consumers stay in sync → Task 7
- Explicitly out of scope: index definitions, FTS trigger bodies (`fts_triggers_*.sql`) — not fields

**Placeholder scan:** Plan includes full DDL text per file; no TBD/TODO steps.

**Type consistency:** Column names and types match the current committed SQL; only `--` comments and table header comments are added.
