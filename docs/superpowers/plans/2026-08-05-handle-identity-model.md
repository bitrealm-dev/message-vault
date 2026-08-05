# Handle Identity Model — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the phone-number-centric contact model with a first-class `handles` table that supports any identifier type (phone, email, Discord username, etc.) across any messaging service.

**Architecture:** A new `handles` table becomes the canonical source for all identifiers. Every column that currently stores a raw handle string (11 columns across 8 tables) references `handles.id` instead. Contacts become pure "person" records linked to handles through `contact_handles`. The client-side `ContactsBook`, `NameMapping`, and `OwnerPhoneSet` are rewritten to be handle-type-aware. `IrService` gains Discord/Signal/Telegram/Slack variants. Backfill/stub-contact logic is removed — unassigned handles stay NULL in `participants.contact_id`.

**Tech Stack:** Rust (rusqlite 0.40, message-ir, phone crate), Next.js 16 (better-sqlite3), SQLite DDL

## Global Constraints

- No backward compatibility — old JSONL without `handle_type` is rejected
- Phones normalize to E.164 via the `phone` crate; emails lowercase; usernames as-is
- `contacts.preferred_name` is UI label only, never a match key
- Contacts must have a name — no stub contacts for unknown handles
- Owner handles stay separate from contacts — owner is never a contact
- `handle_type` values: `'phone'`, `'email'`, `'username'`, `'other'`
- New `IrService` variants: `Discord`, `Signal`, `Telegram`, `Slack`; `Unknown` for future

---

### Task 1: `handles` table DDL + related schema changes (server)

**Files:**
- Rewrite: `schema/sql/contacts.sql`
- Modify: `schema/sql/messages.sql`
- Modify: `schema/sql/staging.sql`
- Modify: `schema/sql/accounts.sql`
- Delete: `schema/sql/fts_backfill.sql` (references old `messages` columns)
- Modify: `schema/sql/fts_virtual.sql`
- Modify: `schema/sql/fts_triggers_create.sql`
- Modify: `schema/sql/fts_triggers_drop.sql`

**Interfaces:**
- Produces: `handles` table `(id, account_id, raw, normalized, handle_type, service)`, `UNIQUE(account_id, normalized, handle_type)`
- Produces: Updated `contacts` (no `preferred_handle`), `contact_handles` (FK to handles, `name_hint`), `participants` (FK to handles + `contact_id` FK), `messages` (`sender_handle_id`), `conversations` (`chat_handle_id`), `tapbacks` (`sender_handle_id`), `account_handles` (replaces `account_phones`), `trashed_handles` (`handle_id` FK)
- Produces: Updated staging tables mirroring production changes

- [ ] **Step 1: Rewrite `schema/sql/contacts.sql`**

Write the new contacts DDL:

```sql
CREATE TABLE IF NOT EXISTS contacts (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    preferred_name TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_contacts_account_id ON contacts (account_id);

CREATE TABLE IF NOT EXISTS handles (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    raw TEXT NOT NULL,
    normalized TEXT NOT NULL,
    handle_type TEXT NOT NULL,
    service TEXT,
    UNIQUE(account_id, normalized, handle_type)
);

CREATE INDEX IF NOT EXISTS ix_handles_account_id ON handles (account_id);
CREATE INDEX IF NOT EXISTS ix_handles_normalized ON handles (account_id, normalized);

CREATE TABLE IF NOT EXISTS contact_handles (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    name_hint TEXT,
    PRIMARY KEY (account_id, handle_id)
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
    handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (account_id, handle_id)
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
```

- [ ] **Step 2: Rewrite `schema/sql/messages.sql`**

Replace all raw handle TEXT columns with handle_id FKs:

```sql
CREATE TABLE IF NOT EXISTS conversations (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    chat_handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    service TEXT,
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
    name_hint TEXT,
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
```

- [ ] **Step 3: Rewrite `schema/sql/staging.sql`**

Mirror production changes for staging tables (no FKs — staging is temporary):

```sql
CREATE TABLE IF NOT EXISTS staging_conversations (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    chat_handle_id INTEGER NOT NULL,
    service TEXT,
    conversation_type TEXT NOT NULL,
    group_title TEXT,
    exported_at TEXT,
    source_file TEXT NOT NULL,
    UNIQUE(account_id, chat_handle_id)
);

CREATE TABLE IF NOT EXISTS staging_participants (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES staging_conversations(id) ON DELETE CASCADE,
    handle_id INTEGER NOT NULL,
    contact_id INTEGER,
    name_hint TEXT,
    UNIQUE(conversation_id, handle_id)
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
    sender_handle_id INTEGER,
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
    sender_handle_id INTEGER
);

CREATE INDEX IF NOT EXISTS ix_staging_tapbacks_message_id ON staging_tapbacks (message_id);
```

- [ ] **Step 4: Update `schema/sql/accounts.sql`**

Replace `account_phones` with `account_handles`:

```sql
-- In accounts.sql, replace:
--   CREATE TABLE IF NOT EXISTS account_phones (...)
-- With:
CREATE TABLE IF NOT EXISTS account_handles (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    PRIMARY KEY (account_id, handle_id)
);
```

The full updated accounts.sql keeps everything else unchanged: `accounts`, `account_emails`, `account_api_tokens`, `account_prefs`, `schema_meta`, `vault_imports` tables stay as-is.

- [ ] **Step 5: Update FTS SQL files**

`fts_virtual.sql` — keep `messages_fts` virtual table (it indexes `body`, `subject`, `attachment_text` — no handle columns). Unchanged.

`fts_triggers_create.sql` — no change (triggers reference `messages.id`, `attachments` columns — none changed).

`fts_triggers_drop.sql` — no change.

Delete `fts_backfill.sql` — it references no handle columns, but with the schema changing, the backfill script will need regeneration anyway after import is wired up.

- [ ] **Step 6: Verify `schema_meta` references**

The `schema_meta` table is defined in both `accounts.sql` and `fts_virtual.sql`. Remove the duplicate `CREATE TABLE IF NOT EXISTS schema_meta` from `fts_virtual.sql` — keep only the one in `accounts.sql`.

- [ ] **Step 7: Commit**

```bash
git add schema/sql/
git commit -m "feat(schema): add handles table, rewrite contacts/messages for typed identifiers

Introduce canonical handles(id, account_id, raw, normalized, handle_type, service).
Replace raw handle TEXT columns with handle_id FKs across conversations,
participants, messages, tapbacks. Add participants.contact_id FK. Rename
account_phones to account_handles. Remove contacts.preferred_handle; add
contact_handles.name_hint for per-source names.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Update server schema.rs DDL loading + ensure functions

**Files:**
- Modify: `src/db/schema.rs`

**Interfaces:**
- Consumes: New DDL files from Task 1
- Produces: Updated `CONTACTS_TABLES_DDL`, `MESSAGE_TABLES_DDL`, `STAGING_TABLES_DDL` constants; updated `ensure_vault_schema`, `ensure_contacts_schema`, `ensure_accounts_schema`

- [ ] **Step 1: Update `include_str!` paths and constant names**

Read `src/db/schema.rs`. The file uses `include_str!` to embed each `.sql` file. The current constants are:

```rust
const ACCOUNTS_DDL: &str = include_str!("../../schema/sql/accounts.sql");
const MESSAGE_TABLES_DDL: &str = include_str!("../../schema/sql/messages.sql");
const STAGING_TABLES_DDL: &str = include_str!("../../schema/sql/staging.sql");
const CONTACTS_DDL: &str = include_str!("../../schema/sql/contacts.sql");
```

Rename `CONTACTS_DDL` to `CONTACTS_TABLES_DDL` for naming consistency. The `include_str!` paths stay valid since the file structure hasn't changed — only the SQL content changed.

- [ ] **Step 2: Update `ensure_accounts_schema`**

The function that creates account-related tables. Search for where `account_phones` DDL is applied — it's embedded in `ACCOUNTS_DDL` from `accounts.sql`. Since Task 1 already rewrote `accounts.sql` with `account_handles` instead of `account_phones`, the existing `ensure_accounts_schema` function just works — no logic change needed. The `CREATE TABLE IF NOT EXISTS` in the DDL handles the rename.

- [ ] **Step 3: Update `ensure_contacts_schema`**

Change the function name from `ensure_contacts_schema` to keep it as-is. The function just applies `CONTACTS_TABLES_DDL` — the new SQL from Task 1 includes `handles` table creation alongside `contacts`/`contact_handles`/`contact_labels`/trash tables.

Verify the function signature and call sites don't need changes beyond the constant rename.

- [ ] **Step 4: Update `ensure_vault_schema`**

This is the master function that calls all the individual ensure functions. It applies:
- `ACCOUNTS_DDL`
- `MESSAGE_TABLES_DDL`
- `STAGING_TABLES_DDL`
- `CONTACTS_TABLES_DDL`

Update to use the renamed constant. No logic changes — the new DDL handles everything via `CREATE TABLE IF NOT EXISTS`.

- [ ] **Step 5: Remove `ensure_messages_fts` references to old columns**

Check that `ensure_messages_fts` (which installs FTS triggers) doesn't reference any columns that were renamed. The FTS triggers reference `messages.id`, `messages.body`, `messages.subject`, and `attachments.original_name`/`attachments.transcription` — none of these changed. No update needed.

- [ ] **Step 6: Update the schema contract test**

In the `#[cfg(test)]` section, there's likely a test that compares the applied schema against `fixtures/schema/current-schema.json`. This test will fail because the schema changed. Update the test comment noting that `current-schema.json` will be regenerated in a later task (after `sync-vault-schema.mjs` is run).

- [ ] **Step 7: Verify all `ensure_*` functions apply the handles table before dependent tables**

The `handles` table must be created before `contact_handles`, `participants`, `messages`, `conversations`, etc. (FK targets must exist). Since `handles` is defined in `contacts.sql` and `CONTACTS_TABLES_DDL` includes the `handles` table, and `ensure_vault_schema` applies `CONTACTS_TABLES_DDL` before `MESSAGE_TABLES_DDL`, ordering is correct. Verify this ordering in the code.

- [ ] **Step 8: Build to verify**

```bash
cargo build -p message-vault-rs
```

- [ ] **Step 9: Commit**

```bash
git add src/db/schema.rs
git commit -m "refactor(schema): update DDL constants for handles table, rename CONTACTS_DDL"
```

---

### Task 3: Add `HandleType` enum and `IrService` variants to `message-ir`

**Files:**
- Modify: `crates/message/ir/src/lib.rs`

**Interfaces:**
- Produces: `HandleType` enum `{ Phone, Email, Username, Other }` with `as_str()` / `parse()`
- Produces: `IrService` gains `Discord`, `Signal`, `Telegram`, `Slack`
- Produces: `IrParticipant` gains `handle_type: Option<HandleType>`

- [ ] **Step 1: Add `HandleType` enum**

After the `IrConversationType` block in `lib.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandleType {
    Phone,
    Email,
    Username,
    Other,
}

impl HandleType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Phone => "phone",
            Self::Email => "email",
            Self::Username => "username",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "phone" => Self::Phone,
            "email" => Self::Email,
            "username" => Self::Username,
            _ => Self::Other,
        }
    }
}
```

- [ ] **Step 2: Add new `IrService` variants**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IrService {
    Sms,
    #[serde(rename = "imessage")]
    IMessage,
    Whatsapp,
    Rcs,
    Discord,
    Signal,
    Telegram,
    Slack,
    Unknown,
}

impl IrService {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sms => "sms",
            Self::IMessage => "imessage",
            Self::Whatsapp => "whatsapp",
            Self::Rcs => "rcs",
            Self::Discord => "discord",
            Self::Signal => "signal",
            Self::Telegram => "telegram",
            Self::Slack => "slack",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "sms" => Self::Sms,
            "imessage" => Self::IMessage,
            "whatsapp" => Self::WhatsApp,
            "rcs" => Self::Rcs,
            "discord" => Self::Discord,
            "signal" => Self::Signal,
            "telegram" => Self::Telegram,
            "slack" => Self::Slack,
            _ => Self::Unknown,
        }
    }
}
```

- [ ] **Step 3: Add `handle_type` to `IrParticipant`**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrParticipant {
    pub handle: String,
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle_type: Option<HandleType>,
}
```

- [ ] **Step 4: Build message-ir to verify**

```bash
cargo build -p message-ir
```

- [ ] **Step 5: Commit**

---

### Task 4: Rewrite `ContactsBook` to be handle-generic

**Files:**
- Modify: `crates/message/contacts/src/book.rs`
- Modify: `crates/message/contacts/src/lib.rs` (re-exports may need update)

**Interfaces:**
- Consumes: `HandleType` from `message-ir`
- Produces: `ContactsBook` with `by_name: HashMap<String, (String, HandleType)>`, `by_handle: HashMap<(String, HandleType), String>`
- Produces: Updated `lookup_phone_by_name` → `lookup_handle_by_name`, `lookup_name_by_phone` → `lookup_name_by_handle`

- [ ] **Step 1: Update struct definition**

```rust
use message_ir::HandleType;

pub struct ContactsBook {
    /// Normalized name key → (normalized handle, handle type).
    by_name: HashMap<String, (String, HandleType)>,
    /// (normalized handle, handle type) → display name.
    by_handle: HashMap<(String, HandleType), String>,
}
```

- [ ] **Step 2: Update `insert_entry`**

```rust
fn insert_entry(&mut self, display: &str, phones: &[String]) {
    let display = collapse_inner_whitespace(display);
    if display.is_empty() || phones.is_empty() {
        return;
    }
    let key = normalize_name_key(&display);
    // All entries from VCF/vCard CSV are phone type
    let handle_type = HandleType::Phone;
    for phone in phones {
        let Some(digits) = sanitize_number(phone) else {
            continue;
        };
        let normalized = phone::to_e164(&digits);
        if !key.is_empty() {
            self.by_name
                .entry(key.clone())
                .or_insert_with(|| (normalized.clone(), handle_type));
        }
        self.by_handle
            .entry((normalized.clone(), handle_type))
            .or_insert_with(|| display.clone());
    }
}
```

- [ ] **Step 3: Rename lookup methods**

```rust
/// Look up (normalized handle, type) for a display / export name.
pub fn lookup_handle_by_name(&self, name: &str) -> Option<(String, HandleType)> {
    let key = normalize_name_key(name);
    if key.is_empty() {
        return None;
    }
    self.by_name.get(&key).cloned()
}

/// Look up display name for a (normalized handle, type).
pub fn lookup_name_by_handle(&self, normalized: &str, handle_type: HandleType) -> Option<&str> {
    self.by_handle.get(&(normalized.to_string(), handle_type)).map(String::as_str)
}
```

- [ ] **Step 4: Remove `lookup_e164_by_name`**

The E.164 method was phone-specific. Replace with `lookup_handle_by_name` which returns the normalized handle + type — callers handle E.164 formatting themselves.

- [ ] **Step 5: Update `enrich_display_name`**

```rust
pub fn enrich_display_name(&self, handle: &str, handle_type: HandleType, name: &str) -> Option<String> {
    if !is_blank_or_unknown_name(name) {
        return None;
    }
    // Normalize handle based on type before lookup
    let normalized = normalize_handle(handle, handle_type);
    self.lookup_name_by_handle(&normalized, handle_type).map(str::to_string)
}
```

Add `normalize_handle` helper:

```rust
fn normalize_handle(raw: &str, handle_type: HandleType) -> String {
    match handle_type {
        HandleType::Phone => {
            sanitize_number(raw)
                .map(|d| phone::to_e164(&d))
                .unwrap_or_else(|| raw.to_string())
        }
        HandleType::Email => raw.trim().to_lowercase(),
        HandleType::Username | HandleType::Other => raw.trim().to_string(),
    }
}
```

- [ ] **Step 6: Update `load_vcf` and `load_vcard_csv`**

Both methods currently call `book.insert_entry(&display, &phones)`. This still works — all entries from VCF/vCard CSV are phone type. No changes needed to the loaders except that `insert_entry` now handles the new types.

- [ ] **Step 7: Update tests**

Update existing tests to use the new method names and `HandleType::Phone`:

```rust
#[test]
fn loads_vcard_csv_both_directions() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_file(
        &dir,
        "contacts.csv",
        "First Name,Last Name,Mobile Phone,Home Phone\n\
Sam,Example,15555550122,\n\
Pat,Contact,+15555550133,+15555550144\n",
    );
    let book = ContactsBook::load_vcard_csv(&path).unwrap();
    assert_eq!(
        book.lookup_handle_by_name("Sam Example"),
        Some(("+15555550122".to_string(), HandleType::Phone))
    );
    assert_eq!(
        book.lookup_name_by_handle("+15555550122", HandleType::Phone),
        Some("Sam Example")
    );
    assert_eq!(
        book.lookup_name_by_handle("+15555550133", HandleType::Phone),
        Some("Pat Contact")
    );
    assert_eq!(
        book.lookup_name_by_handle("+15555550144", HandleType::Phone),
        Some("Pat Contact")
    );
}
```

- [ ] **Step 8: Build and test**

```bash
cargo test -p contacts
```

- [ ] **Step 9: Commit**

---

### Task 5: Rewrite `NameMapping` to map names → `(normalized_handle, HandleType)`

**Files:**
- Modify: `crates/message/contacts/src/mapping.rs`

**Interfaces:**
- Consumes: `HandleType` from `message-ir`
- Produces: `NameMapping` with `incorrect_to_handle: HashMap<String, (String, HandleType)>`
- Produces: `handle_for_incorrect_name` replacing `phone_for_incorrect_name`

- [ ] **Step 1: Update struct**

```rust
use message_ir::HandleType;

pub struct NameMapping {
    /// Normalized incorrect name → (normalized handle, handle type).
    incorrect_to_handle: HashMap<String, (String, HandleType)>,
}
```

- [ ] **Step 2: Update `load` to accept `Handle,HandleType,Incorrect Name` CSV**

```rust
pub fn load(path: &Path) -> Result<Self> {
    let file = File::open(path).with_context(|| format!("open name mapping {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let header = lines.next().transpose()?.unwrap_or_default();
    let header_parts = crate::book::split_csv_line(&header);
    let header_l: Vec<String> = header_parts
        .iter()
        .map(|h| h.trim().to_ascii_lowercase().replace('_', " "))
        .collect();

    let handle_idx = header_l.iter().position(|h| h == "handle" || h == "phone");
    let type_idx = header_l.iter().position(|h| h == "handle type" || h == "handletype");
    let incorrect_idx = header_l
        .iter()
        .position(|h| h == "incorrect name" || h == "incorrectname" || h == "incorrect");

    let (Some(handle_idx), Some(incorrect_idx)) = (handle_idx, incorrect_idx) else {
        anyhow::bail!(
            "name mapping CSV {} missing required header Handle,Incorrect Name",
            path.display()
        );
    };

    let mut mapping = Self::empty();
    for (idx, line) in lines.enumerate() {
        let line = line.with_context(|| format!("read name mapping line {}", idx + 2))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts = crate::book::split_csv_line(line);
        let handle_raw = parts.get(handle_idx).map(|s| s.trim()).unwrap_or("");
        let incorrect = parts
            .get(incorrect_idx)
            .map(|s| collapse_inner_whitespace(s.trim()))
            .unwrap_or_default();
        if handle_raw.is_empty() || incorrect.is_empty() {
            continue;
        }

        // Infer handle type from column or default to Phone
        let handle_type = type_idx
            .and_then(|i| parts.get(i))
            .map(|s| HandleType::parse(s.trim()))
            .unwrap_or(HandleType::Phone);

        let normalized = match handle_type {
            HandleType::Phone => {
                let Some(digits) = sanitize_number(handle_raw) else {
                    continue;
                };
                phone::to_e164(&digits)
            }
            HandleType::Email => handle_raw.trim().to_lowercase(),
            HandleType::Username | HandleType::Other => handle_raw.trim().to_string(),
        };

        let key = normalize_name_key(&incorrect);
        if key.is_empty() {
            continue;
        }
        mapping
            .incorrect_to_handle
            .entry(key)
            .or_insert((normalized, handle_type));
    }
    Ok(mapping)
}
```

- [ ] **Step 3: Update lookup method**

```rust
/// If `eml_name` is an incorrect export name, return (normalized handle, type).
pub fn handle_for_incorrect_name(&self, eml_name: &str) -> Option<&(String, HandleType)> {
    let key = normalize_name_key(eml_name);
    if key.is_empty() {
        return None;
    }
    self.incorrect_to_handle.get(&key)
}
```

- [ ] **Step 4: Update tests**

```rust
#[test]
fn loads_handle_incorrect_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("map.csv");
    let mut f = std::fs::File::create(&path).unwrap();
    write!(
        f,
        "Handle,HandleType,Incorrect Name\n\
+15555550144,phone,Jordan Alias (SKIP)\n\
user@example.com,email,Casey Email\n"
    )
    .unwrap();
    let mapping = NameMapping::load(&path).unwrap();
    assert_eq!(
        mapping.handle_for_incorrect_name("Jordan Alias (SKIP)"),
        Some(&("+15555550144".to_string(), HandleType::Phone))
    );
    assert_eq!(
        mapping.handle_for_incorrect_name("casey email"),
        Some(&("user@example.com".to_string(), HandleType::Email))
    );
}

#[test]
fn defaults_to_phone_type_when_column_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("map.csv");
    let mut f = std::fs::File::create(&path).unwrap();
    write!(
        f,
        "Phone,Incorrect Name\n\
+15555550144,Jordan Alias (SKIP)\n"
    )
    .unwrap();
    let mapping = NameMapping::load(&path).unwrap();
    assert_eq!(
        mapping.handle_for_incorrect_name("Jordan Alias (SKIP)"),
        Some(&("+15555550144".to_string(), HandleType::Phone))
    );
}
```

- [ ] **Step 5: Build and test**

```bash
cargo test -p contacts
```

- [ ] **Step 6: Commit**

---

### Task 6: Rewrite `OwnerPhoneSet` → `OwnerHandleSet`

**Files:**
- Modify: `crates/message/phone/src/lib.rs`

**Interfaces:**
- Consumes: `HandleType` from `message-ir`
- Produces: `OwnerHandleSet` with `is_owner(handle: &str, handle_type: HandleType) -> bool`

- [ ] **Step 1: Add `OwnerHandleSet` struct and remove `OwnerPhoneSet`**

```rust
use message_ir::HandleType;

/// All configured owner handles (normalized, typed).
#[derive(Debug, Clone)]
pub struct OwnerHandleSet {
    handles: HashSet<(String, HandleType)>,
}

impl OwnerHandleSet {
    pub fn new(handles: &[(String, HandleType)]) -> Result<Self> {
        if handles.is_empty() {
            bail!("owner handle required: pass --owner-phone or --owner-handle");
        }
        let mut set = HashSet::new();
        for (raw, handle_type) in handles {
            let normalized = match handle_type {
                HandleType::Phone => {
                    let d = sanitize_number(raw)
                        .with_context(|| format!("owner phone has no usable digits: {raw}"))?;
                    to_e164(&d)
                }
                HandleType::Email => raw.trim().to_lowercase(),
                HandleType::Username | HandleType::Other => raw.trim().to_string(),
            };
            set.insert((normalized, *handle_type));
        }
        Ok(Self { handles: set })
    }

    pub fn is_owner(&self, raw: &str, handle_type: HandleType) -> bool {
        let normalized = match handle_type {
            HandleType::Phone => {
                let Some(d) = sanitize_number(raw) else {
                    return false;
                };
                to_e164(&d)
            }
            HandleType::Email => raw.trim().to_lowercase(),
            HandleType::Username | HandleType::Other => raw.trim().to_string(),
        };
        self.handles.contains(&(normalized, handle_type))
    }
}
```

- [ ] **Step 2: Keep `OwnerPhoneSet` as a deprecated wrapper**

For exporters that haven't been updated yet, keep a compatibility constructor:

```rust
impl OwnerHandleSet {
    /// Convenience for exporters that only know about phone numbers.
    pub fn from_phones(phones: &[String]) -> Result<Self> {
        let handles: Vec<(String, HandleType)> = phones
            .iter()
            .map(|p| (p.clone(), HandleType::Phone))
            .collect();
        Self::new(&handles)
    }
}
```

- [ ] **Step 3: Add `HandleType` dependency to `phone` crate's Cargo.toml**

```toml
message-ir = { git = "https://github.com/bitrealm-dev/message-exporters", package = "message-ir" }
```

Wait — `phone` is a shared crate pulled by the server from `bitrealm-dev/message-exporters`. But in the client repo it's at `crates/message/phone/`. Let me check which one this task modifies.

Actually, `phone` is in the client repo at `crates/message/phone/`. The server pulls it via git. So this task modifies the client repo.

Add to `crates/message/phone/Cargo.toml`:
```toml
message-ir = { path = "../ir" }
```

- [ ] **Step 4: Build and test**

```bash
cargo test -p phone
```

- [ ] **Step 5: Commit**

---

### Task 7: Update `message-ir-format` for handle_type serialization

**Files:**
- Modify: `crates/message/ir-format/src/write.rs` (CSV headers, format sink)
- Modify: `crates/message/ir-format/src/read_csv.rs` (CSV parsing)
- Modify: `crates/message/ir-format/src/read_mail.rs` (EML parsing)

**Interfaces:**
- Consumes: `HandleType` from `message-ir`, updated `IrParticipant`
- Produces: CSV columns include `handle_type`; EML parser sets handle_type on participants

- [ ] **Step 1: Update CSV write**

In `write.rs`, the CSV header constant and participant serialization:
- Add `"handle_type"` to CSV headers
- Serialize `participant.handle_type` in the participants JSON cell

- [ ] **Step 2: Update CSV read**

In `read_csv.rs`, parse `handle_type` from the participants JSON cell or the dedicated column. Default to `HandleType::Other` when absent.

- [ ] **Step 3: Update EML read**

In `read_mail.rs`, set `handle_type` on parsed `IrParticipant` values:
- Handle contains `@` → `HandleType::Email`
- Handle matches phone pattern (digits, +, etc.) → `HandleType::Phone`
- Otherwise → `HandleType::Other`

- [ ] **Step 4: Build and test**

```bash
cargo build -p message-ir-format
cargo test -p message-ir-format
```

- [ ] **Step 5: Commit**

---

### Task 8: Update `models.rs` — parse handle_type from JSONL, map to import records

**Files:**
- Modify: `src/models.rs`

**Interfaces:**
- Consumes: Updated `message-ir` with `HandleType`, new `IrService` variants, `IrParticipant.handle_type`
- Produces: `ParticipantRecord` gains `handle_type: Option<HandleType>`; `ConversationRecord` gains `chat_handle_type`

- [ ] **Step 1: Update `ParticipantRecord`**

```rust
#[derive(Debug, Clone)]
pub struct ParticipantRecord {
    pub handle: String,
    pub name_hint: Option<String>,
    pub handle_type: Option<HandleType>,
}
```

- [ ] **Step 2: Update `conversation_from_ir`**

```rust
fn conversation_from_ir(header: &ConversationHeader) -> ConversationRecord {
    // ... existing code ...
    participants: header
        .conversation
        .participants
        .iter()
        .map(|p| ParticipantRecord {
            handle: p.handle.clone(),
            name_hint: p.display_name.clone(),
            handle_type: p.handle_type,
        })
        .collect(),
    // ...
}
```

- [ ] **Step 3: Update `message_from_ir`**

Set `sender_handle_type` on messages by inferring from the sender handle + service context. For incoming messages where `is_from_me` is false:
- If `sender_handle` contains `@` → `HandleType::Email`
- If service is SMS/iMessage/WhatsApp/RCS and handle looks like phone → `HandleType::Phone`
- Otherwise → `HandleType::Other`

- [ ] **Step 4: Update `service_label` for new services**

```rust
fn service_label(service: IrService) -> String {
    match service {
        IrService::Sms => "SMS".into(),
        IrService::IMessage => "iMessage".into(),
        IrService::Whatsapp => "WhatsApp".into(),
        IrService::Rcs => "RCS".into(),
        IrService::Discord => "Discord".into(),
        IrService::Signal => "Signal".into(),
        IrService::Telegram => "Telegram".into(),
        IrService::Slack => "Slack".into(),
        IrService::Unknown => "Unknown".into(),
    }
}
```

- [ ] **Step 5: Build**

```bash
cargo build -p message-vault-rs
```

- [ ] **Step 6: Commit**

---

### Task 9: Update `src/db/contacts.rs` — handle resolution through `handles` table

**Files:**
- Modify: `src/db/contacts.rs`

**Interfaces:**
- Consumes: `handles` table, `HandleType` from `message-ir`
- Produces: Updated `load_contacts_if_needed` → resolves handles through `handles` table
- Produces: Removed `ensure_unknown_contacts` and `fill_empty_contact_names_from_participants`

- [ ] **Step 1: Remove `ensure_unknown_contacts` function** (entire function, lines 500-650)

- [ ] **Step 2: Remove `fill_empty_contact_names_from_participants`** (entire function, lines 655-723)

- [ ] **Step 3: Remove helper functions only used by the above**

`best_name_hint_for_handle` (line 725), `useful_name_hint` (line 753), `looks_like_phone` (line 770) — remove.

- [ ] **Step 4: Update `load_contacts_if_needed`**

The function loads VCF/vCard CSV into the contacts/contact_handles tables. With the new schema, it must also insert into `handles` before `contact_handles`:

In `insert_contact_drafts`:
```rust
fn insert_contact_drafts(
    conn: &mut Connection,
    account_id: &str,
    drafts: Vec<ContactDraft>,
) -> Result<ContactLoadStats> {
    let mut stats = ContactLoadStats::default();
    let drafts = merge_duplicate_phone_drafts(drafts);
    let tx = conn.transaction()?;

    for draft in drafts {
        // Insert contact
        let preferred_name = draft.preferred_name.as_deref().unwrap_or("Unknown");
        tx.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, ?2)",
            params![account_id, preferred_name],
        )?;
        let contact_id = tx.last_insert_rowid();
        stats.contacts += 1;

        for phone in &draft.phones {
            // Ensure handle exists
            let normalized = phone::to_e164(&sanitize_number(phone).unwrap_or_default());
            tx.execute(
                "INSERT OR IGNORE INTO handles (account_id, raw, normalized, handle_type)
                 VALUES (?1, ?2, ?3, 'phone')",
                params![account_id, phone, normalized],
            )?;
            let handle_id: i64 = tx.query_row(
                "SELECT id FROM handles WHERE account_id = ?1 AND normalized = ?2 AND handle_type = 'phone'",
                params![account_id, normalized],
                |row| row.get(0),
            )?;

            // Link contact to handle
            tx.execute(
                "INSERT OR IGNORE INTO contact_handles (account_id, handle_id, contact_id)
                 VALUES (?1, ?2, ?3)",
                params![account_id, handle_id, contact_id],
            )?;
            stats.phones += 1;
        }

        // Labels unchanged
        for label_name in &draft.labels {
            let label_id = ensure_label(&tx, account_id, label_name)?;
            tx.execute(
                "INSERT OR IGNORE INTO contact_label_members (contact_id, label_id) VALUES (?1, ?2)",
                params![contact_id, label_id],
            )?;
            stats.labels += 1;
        }
    }

    tx.commit()?;
    Ok(stats)
}
```

- [ ] **Step 5: Update `ContactDraft`**

`ContactDraft` currently has `phones: Vec<String>`. Keep it — VCF/vCard CSV only produce phone handles. The `load_from_vcf` and `load_from_vcard_csv` methods stay phone-specific.

- [ ] **Step 6: Update `is_email_handle` usage**

`is_email_handle` was used by `snapshot_email_handles` / `restore_email_handles`. With the handles table, emails are just `handle_type = 'email'` rows. Update `restore_email_handles` to query by `handle_type = 'email'` and match by phone-set join through contact_handles.

- [ ] **Step 7: Remove `handle_match_key`**

This was used for owner matching. Replace with `handles.normalized` comparison.

- [ ] **Step 8: Build**

```bash
cargo build -p message-vault-rs
```

- [ ] **Step 9: Commit**

---

### Task 10: Update `src/import.rs` — resolve handles during staging import

**Files:**
- Modify: `src/import.rs`

**Interfaces:**
- Consumes: Updated `ParticipantRecord` with `handle_type`, `handles` table
- Produces: Import inserts into `handles`, uses `handle_id` in staging tables

This is the largest and most complex task. `import.rs` is ~1700 lines.

- [ ] **Step 1: Add handle resolution helper**

```rust
/// Resolve or create a handle row. Returns the handle id.
fn resolve_handle(
    conn: &Connection,
    account_id: &str,
    raw: &str,
    handle_type: HandleType,
    service: Option<&str>,
) -> Result<i64> {
    let normalized = normalize_handle(raw, handle_type);
    conn.execute(
        "INSERT OR IGNORE INTO handles (account_id, raw, normalized, handle_type, service)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![account_id, raw, normalized, handle_type.as_str(), service],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM handles WHERE account_id = ?1 AND normalized = ?2 AND handle_type = ?3",
        params![account_id, normalized, handle_type.as_str()],
        |row| row.get(0),
    )?;
    Ok(id)
}
```

- [ ] **Step 2: Update staging conversation insert**

Where the code inserts into `staging_conversations`:
- Resolve `chat_identifier` through `resolve_handle` → get `chat_handle_id`
- Insert `chat_handle_id` instead of raw `chat_identifier`
- Set `handle_type` from message context (Phone for SMS, Username for Discord, etc.)

- [ ] **Step 3: Update staging participant insert**

Where the code inserts into `staging_participants`:
- Resolve each participant's handle through `resolve_handle`
- Look up `contact_id` from `contact_handles` by `handle_id`
- Insert `handle_id`, `contact_id`, `name_hint`

- [ ] **Step 4: Update staging message insert**

Where the code inserts into `staging_messages`:
- If `sender` is present, resolve through `resolve_handle` → `sender_handle_id`
- If `is_from_me`, `sender_handle_id` is NULL (unchanged)

- [ ] **Step 5: Update staging tapback insert**

Where the code inserts into `staging_tapbacks`:
- If `sender` is present, resolve → `sender_handle_id`

- [ ] **Step 6: Update staging-to-production promote**

When promoting from staging to production tables, the handle_id values are already in staging — they carry through to the INSERT INTO production SELECT FROM staging.

- [ ] **Step 7: Remove backfill calls**

Remove calls to `ensure_unknown_contacts` and `fill_empty_contact_names_from_participants` — these functions were removed in Task 9.

- [ ] **Step 8: Update `conversation_key` logic for dedupe**

The `chat_identity_for_content_key` in `dedupe.rs` uses `chat_identifier`. Update to use `handles.normalized` for the key.

- [ ] **Step 9: Build**

```bash
cargo build -p message-vault-rs
```

- [ ] **Step 10: Commit**

---

### Task 11: Update `src/export_api.rs` — join through handles

**Files:**
- Modify: `src/export_api.rs`

**Interfaces:**
- Consumes: Handle IDs in production tables
- Produces: Export queries emit `handle`, `handle_type`, `name_hint` from joins

- [ ] **Step 1: Update `ExportParticipant`**

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ExportParticipant {
    pub handle: String,
    pub name_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle_type: Option<String>,
}
```

- [ ] **Step 2: Update `load_participants`**

The SQL that loads participants for export:

```sql
SELECT p.handle_id, p.name_hint, p.contact_id,
       h.raw AS handle, h.handle_type, h.normalized,
       c.preferred_name
FROM participants p
JOIN handles h ON h.id = p.handle_id
LEFT JOIN contacts c ON c.id = p.contact_id
WHERE p.conversation_id = ?1
ORDER BY p.id
```

Map result rows to `ExportParticipant { handle: h.raw, name_hint, handle_type: h.handle_type }`.

- [ ] **Step 3: Update conversation export**

The conversation export query `chat_identifier` → join through `handles`:

```sql
SELECT c.id, h.raw AS chat_identifier, c.service, c.conversation_type, ...
FROM conversations c
JOIN handles h ON h.id = c.chat_handle_id
WHERE ...
```

- [ ] **Step 4: Update message export**

Message sender: join `messages.sender_handle_id → handles.raw`.

- [ ] **Step 5: Update `involves_contacts_sql` and related search helpers**

These functions reference `contact_handles.handle` — update to join through `handles`:

```sql
-- Before
SELECT 1 FROM contact_handles ch
WHERE ch.handle = c.chat_identifier ...

-- After
SELECT 1 FROM contact_handles ch
JOIN handles h ON h.id = ch.handle_id
WHERE h.normalized = (SELECT h2.normalized FROM handles h2 WHERE h2.id = c.chat_handle_id) ...
```

- [ ] **Step 6: Build**

```bash
cargo build -p message-vault-rs
```

- [ ] **Step 7: Commit**

---

### Task 12: Update remaining server source files

**Files:**
- Modify: `src/dedupe.rs`, `src/search_query.rs`, `src/db/account_profile.rs`, `src/db/vault_imports.rs`, `src/process_assets.rs`, `src/reset_demo.rs`, `src/asset_uploads.rs`, `src/assets.rs`

- [ ] **Step 1: `src/dedupe.rs`** — `chat_identity_for_content_key` uses `chat_identifier`; update to use `handles.normalized` join
- [ ] **Step 2: `src/search_query.rs`** — handle/phone/from/to operators reference handle TEXT columns; update to use `handles.normalized` / `handles.raw`
- [ ] **Step 3: `src/db/account_profile.rs`** — `AccountProfile.phones` becomes `AccountProfile.handle_ids: Vec<i64>`; load from `account_handles JOIN handles`
- [ ] **Step 4: `src/db/vault_imports.rs`** — any handle references (likely none; check `TopAttachment` query)
- [ ] **Step 5: `src/process_assets.rs`** — any handle references (likely none; media-only)
- [ ] **Step 6: `src/reset_demo.rs`** — `DemoOwner.phones` → `DemoOwner.handle_specs: Vec<(String, HandleType)>`; insert into `account_handles` via handles
- [ ] **Step 7: `crates/demo-seed/`** — update demo data generation: `Contact.phones` → handles, participant/message inserts use handle_id
- [ ] **Step 8: Update `export_api.rs` to remove stub contact references** — the export API should not create stub contacts or call backfill functions
- [ ] **Step 9: Build and fix compilation errors across all files**

```bash
cargo build -p message-vault-rs 2>&1 | head -100
```

Iterate until it compiles.

- [ ] **Step 10: Commit**

---

### Task 13: Regenerate schema contract files + update Rust schema tests

**Files:**
- Regenerate: `fixtures/schema/current-schema.json`
- Regenerate: `web/src/lib/vaultSchema.generated.ts`
- Modify: `src/db/schema.rs` (tests)

- [ ] **Step 1: Run schema sync**

```bash
node scripts/sync-vault-schema.mjs
```

This regenerates `web/src/lib/vaultSchema.generated.ts` and `fixtures/schema/current-schema.json`.

- [ ] **Step 2: Update the Rust schema contract test**

The test at the bottom of `src/db/schema.rs` compares the applied schema against `fixtures/schema/current-schema.json`. The table/column/index counts will be different. Update the test to match the new schema (or remove exact counts and check for table existence).

- [ ] **Step 3: Run schema tests**

```bash
cargo test -p message-vault-rs -- schema
```

- [ ] **Step 4: Commit**

---

### Task 14: Update web UI — schema and read layer

**Files:**
- Modify: `web/src/lib/vaultSchema.ts` (ensure functions)
- Modify: `web/src/lib/dbCore.ts` (displayName, sortFields, combinedDedupeSql)
- Modify: `web/src/lib/contactsRead.ts` (contact queries)
- Modify: `web/src/lib/contactsWrite.ts` (contact CRUD)
- Modify: `web/src/lib/unassignedRead.ts` (unassigned handles)
- Modify: `web/src/lib/owner.ts` (owner matching)
- Modify: `web/src/lib/handleKind.ts` (handle type detection)
- Modify: `web/src/lib/search.ts` (search queries)
- Modify: `web/src/lib/types.ts` (TypeScript types)

- [ ] **Step 1: Update `vaultSchema.ts`** — ensure functions to create handles table alongside contacts; update `ensureMessagesFts` if needed
- [ ] **Step 2: Update `dbCore.ts`** — `displayName` function; `combinedDedupeSql` to use handle_id joins
- [ ] **Step 3: Update `contactsRead.ts`** — all queries join through handles; `listContacts`, `getContact`, `loadContactThreadsPage` include handle_type
- [ ] **Step 4: Update `contactsWrite.ts`** — `createContact` accepts `{name, handles: [{raw, handle_type}]}`; remove phone-only gate; `mergeContacts` updates `participants.contact_id`
- [ ] **Step 5: Update `unassignedRead.ts`** — query `participants LEFT JOIN contact_handles` where `contact_id IS NULL`; no backfill
- [ ] **Step 6: Update `owner.ts`** — `ownerHandleMatcher` queries `account_handles JOIN handles`; handle-type-aware matching
- [ ] **Step 7: Update `handleKind.ts`** — `isEmailHandle` stays; add `HandleType` enum; add `normalizeHandle(raw, type)` function
- [ ] **Step 8: Update `search.ts`** — search queries join through handles
- [ ] **Step 9: Update `types.ts`** — `ContactListItem`, `ContactDetail`, `GroupParticipant` types include handle_type
- [ ] **Step 10: Build web**

```bash
cd web && npm run build
```

- [ ] **Step 11: Commit**

---

### Task 15: Update web UI — API routes and components

**Files:**
- Modify: `web/src/app/api/contacts/` (all routes)
- Modify: `web/src/app/api/contacts/merge/route.ts`
- Modify: `web/src/app/api/unassigned/` (all routes)
- Modify: Contact components: `ContactDetailsCard.tsx`, `ContactFormOverlay.tsx`, `BrowseContactList.tsx`, etc.

- [ ] **Step 1: Update API routes** — parameter types accept handle_type; queries updated for handles join
- [ ] **Step 2: Update `merge/route.ts`** — transaction includes `UPDATE participants SET contact_id = ?`
- [ ] **Step 3: Update contact form** — handle type dropdown when adding handles
- [ ] **Step 4: Update contact detail** — show handles grouped by type/service
- [ ] **Step 5: Update unassigned view** — show handle type badges
- [ ] **Step 6: Build and lint**

```bash
cd web && npm run lint && npm run build
```

- [ ] **Step 7: Commit**

---

### Task 16: Update exporters to emit handle_type

**Files:**
- Modify: Each exporter in `crates/exporters/*/src/emit.rs` (or equivalent)

- [ ] **Step 1: SMS Backup & Restore exporter** — set `handle_type: Some(HandleType::Phone)` on participants and messages
- [ ] **Step 2: iMessage exporter** — phone handles → `Phone`; email handles → `Email`
- [ ] **Step 3: WhatsApp exporter** — `Phone`
- [ ] **Step 4: All other exporters** (GO SMS Pro, iMazing, OpenExtract, SMS Backup+) — `Phone`
- [ ] **Step 5: Build all exporters**

```bash
cargo build --workspace
```

- [ ] **Step 6: Commit**

---

### Task 17: Final integration — build both repos, run tests, smoke test

- [ ] **Step 1: Build message-vault-io workspace**

```bash
cargo build --workspace --release
```

- [ ] **Step 2: Run all Rust tests (message-vault-io)**

```bash
cargo test --workspace
```

- [ ] **Step 3: Build message-vault-rs**

```bash
cargo build --workspace --release
```

- [ ] **Step 4: Run all Rust tests (message-vault-rs)**

```bash
cargo test --workspace
```

- [ ] **Step 5: Run web tests**

```bash
cd web && npm test
```

- [ ] **Step 6: Run schema sync check**

```bash
node scripts/sync-vault-schema.mjs --check
```

- [ ] **Step 7: Run smoke tests**

```bash
./scripts/smoke-import-api.sh
./scripts/smoke-vault-push.sh
./scripts/smoke-export-api.sh
```

- [ ] **Step 8: Run search golden tests**

```bash
node scripts/regen-search-goldens.mjs
git diff --exit-code fixtures/search/parse-cases.json
```

- [ ] **Step 9: Fix any failures, iterate**

- [ ] **Step 10: Final commit**

---

### Task 18: Update exporter matrix docs

**Files:**
- Modify: `docs/maintainers/exporter-matrix.md` (message-vault-io)
- Modify: `docs/src/content/docs/reference/database.md` (message-vault-rs) — if it documents the schema

- [ ] **Step 1: Update exporter matrix** — add Discord/Signal/Telegram/Slack rows (unsupported, future)
- [ ] **Step 2: Update database reference docs** — document handles table, updated contact model
- [ ] **Step 3: Commit**
