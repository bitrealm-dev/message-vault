# Handle identity model — design spec

## Problem

The contact model was built on one assumption: a contact is a phone number with a display name. This breaks as the system moves beyond SMS/iMessage/WhatsApp to services like Discord, Signal, and Telegram where identifiers are usernames or opaque IDs. It also breaks when one person uses multiple phone numbers over time, or uses both a phone number and a Discord handle.

Specific issues:

- `contact_handles.handle` is untyped TEXT — phones, emails, and future Discord IDs are indistinguishable
- `ContactsBook` (client) maps name↔phone digits only — non-phone handles cannot be stored or looked up
- `NameMapping` maps export names to phone digits only
- `OwnerPhoneSet` identifies the account holder by phone digits only
- `IrService` enum has no Discord, Signal, Telegram, or Slack variants
- `participants` has no FK to contacts — name resolution is fuzzy string matching at read time
- No conceptual "person" that owns multiple handles across services
- Backfill creates stub contacts for unknown handles, producing nameless contacts the user never asked for

## Design

### Core principle

Handles are a first-class entity. A handle is any identifier a messaging service uses to address someone: a phone number, an email address, a Discord username, a Telegram ID. Every place in the system that currently stores a raw handle string references `handles.id` instead.

Contacts are pure "person" records. They own no handles directly — they link to handles through `contact_handles`. A contact's name is just a name, never a match key.

### `handles` table (new, canonical source)

```sql
CREATE TABLE handles (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    raw TEXT NOT NULL,              -- as encountered in the export
    normalized TEXT NOT NULL,       -- comparison key (E.164 / lowercase / as-is)
    handle_type TEXT NOT NULL,      -- 'phone', 'email', 'username', 'other'
    service TEXT,                   -- 'sms', 'imessage', 'whatsapp', 'discord', etc.
    UNIQUE(account_id, normalized, handle_type)
);
```

**`normalized`** is computed from `raw` based on `handle_type`:

- `phone`: E.164 via the `phone` crate (e.g. `+15555550100`)
- `email`: lowercase (e.g. `user@example.com`)
- `username`: as-is, case-sensitive (e.g. `User#1234`)
- `other`: as-is, exact match

**`handle_type`** determines how the system matches, displays, and normalizes the handle.

**`service`** records which messaging service this handle belongs to. The same phone number can appear for SMS and WhatsApp as separate `handles` rows (same normalized, different service), since it's the same person on different platforms.

### Contacts — simplified

```sql
CREATE TABLE contacts (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    preferred_name TEXT NOT NULL    -- UI label only, never a match key
);

CREATE TABLE contact_handles (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    is_preferred INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account_id, handle_id)
);

CREATE TABLE contact_labels (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    UNIQUE(account_id, name)
);

CREATE TABLE contact_label_members (
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    label_id INTEGER NOT NULL REFERENCES contact_labels(id) ON DELETE CASCADE,
    PRIMARY KEY (contact_id, label_id)
);
```

`preferred_handle` moves from `contacts` to `contact_handles.is_preferred`. `preferred_name` is enforced NOT NULL at the application layer — every contact must have a name. No more stub contacts for unknown handles.

### Dependent tables — FKs to handles

| Table | Old column | New column |
|---|---|---|
| `participants` | `handle TEXT` | `handle_id INTEGER REFERENCES handles(id)` |
| `participants` | _(none)_ | `contact_id INTEGER REFERENCES contacts(id)` — nullable |
| `messages` | `sender TEXT` | `sender_handle_id INTEGER REFERENCES handles(id)` |
| `conversations` | `chat_identifier TEXT` | `chat_handle_id INTEGER REFERENCES handles(id)` |
| `tapbacks` | `sender TEXT` | `sender_handle_id INTEGER REFERENCES handles(id)` |
| `account_phones` | `phone TEXT` | _(table renamed to `account_handles`)_ `handle_id INTEGER REFERENCES handles(id)` |
| `trashed_handles` | `handle TEXT` | `handle_id INTEGER REFERENCES handles(id)` |
| `staging_conversations` | `chat_identifier TEXT` | `chat_handle_id INTEGER` (no FK, staging is temporary) |
| `staging_participants` | `handle TEXT` | `handle_id INTEGER` (no FK, staging is temporary) |
| `staging_messages` | `sender TEXT` | `sender_handle_id INTEGER` (no FK, staging is temporary) |

### Participant-to-contact wiring

`participants.contact_id` is populated during import when the handle resolves to a known contact via `contact_handles`. It stays NULL for unassigned handles. The web UI already queries for handles without contacts — no stub contact needed.

When a user merges two contacts, the merge operation updates `contact_handles`, `participants.contact_id`, and `contacts.preferred_name` in a transaction. No hunting through messages.

### Owner identity

```sql
CREATE TABLE account_handles (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    handle_id INTEGER NOT NULL REFERENCES handles(id) ON DELETE CASCADE,
    PRIMARY KEY (account_id, handle_id)
);
```

Owner handles are separate from contacts — an owner is never a contact. The owner handle set prevents self-handles from appearing in contact lists and unassigned views.

### No more backfill or stub contacts

`ensure_unknown_contacts` is removed. `fill_empty_contact_names_from_participants` is removed. A handle without a contact is simply a handle with `participants.contact_id IS NULL`. The unassigned view lists them for manual action. Every contact that exists was deliberately created and has a name.

### No backward compatibility

Old JSONL exports that lack `handle_type` fields are rejected at import. No dual-read code, no fallback paths, no gradual column renames. This is a clean break.

---

## Client-side changes (message-vault-io)

### `HandleType` enum (new, in `message-ir`)

```rust
pub enum HandleType {
    Phone,     // E.164 normalized
    Email,     // lowercase normalized
    Username,  // exact, case-sensitive
    Other,     // untyped, exact match
}
```

Serialized into JSONL. Default serialization is `"other"` so old-style handle fields don't need the key.

### `IrService` — new variants

```rust
pub enum IrService {
    Sms,
    IMessage,
    Whatsapp,
    Rcs,
    Discord,      // new
    Signal,       // new
    Telegram,     // new
    Slack,        // new
    Unknown,
}
```

`Unknown` catches future services without schema changes.

### `IrParticipant` — gains `handle_type`

```rust
pub struct IrParticipant {
    pub handle: String,
    pub display_name: Option<String>,
    pub handle_type: Option<HandleType>,   // new
}
```

Exporters set this when they know the handle type (phone for SMS, email for iMessage, username for Discord).

### `ContactsBook` — handle-generic

```rust
pub struct ContactsBook {
    by_name: HashMap<String, (String, HandleType)>,    // name → (normalized, type)
    by_handle: HashMap<(String, HandleType), String>,  // (normalized, type) → display name
}
```

Loading a VCF or vCard CSV populates both directions. Phone numbers get E.164 normalization through the `phone` crate. Emails get lowercased. The handle type is part of the lookup key so a Discord username and a phone number cannot collide.

### `NameMapping` — name → handle (generic)

```rust
pub struct NameMapping {
    incorrect_to_handle: HashMap<String, (String, HandleType)>,
}
```

CSV format: `Handle,HandleType,Incorrect Name`. When `HandleType` column is absent, defaults to `phone` for backward compatibility with existing mapping files already on disk.

### `OwnerHandleSet` (replaces `OwnerPhoneSet`)

```rust
pub struct OwnerHandleSet {
    handles: HashSet<(String, HandleType)>,  // (normalized, type)
}
```

Used during export to determine which messages are from the owner. Imported from config or CLI args as a list of handles with types.

---

## Migration plan

No backward compatibility. Breaking change across both repos.

### Phase 1: Schema foundations (message-vault-rs)

1. Write new DDL in `schema/sql/` — `handles` table, updated `contacts`, updated dependent tables
2. Update `src/db/schema.rs` — new DDL strings, `ensure_vault_schema` applies them
3. Drop old tables/columns (no coexistence — clean cut)

### Phase 2: Client types (message-vault-io)

4. Add `HandleType` enum to `message-ir`
5. Add new `IrService` variants
6. Add `handle_type` field to `IrParticipant`
7. Rewrite `ContactsBook` — generic handles, keyed by `(normalized, HandleType)`
8. Rewrite `NameMapping` — name → `(normalized, HandleType)`
9. Rewrite `OwnerPhoneSet` → `OwnerHandleSet`
10. Update all exporters to emit `handle_type` in participants and messages

### Phase 3: Server import/export (message-vault-rs)

11. Update `src/models.rs` — parse `handle_type` from JSONL, resolve handles through `handles` table
12. Update `src/import.rs` — insert into `handles`, use `handle_id` in dependent tables
13. Remove `ensure_unknown_contacts` and `fill_empty_contact_names_from_participants` from `src/db/contacts.rs`
14. Update `src/export_api.rs` — join through handles, emit `handle_type`
15. Update `src/db/contacts.rs` — `load_contacts_if_needed` resolves through handles
16. Update `src/dedupe.rs` — content keys use `handles.normalized`
17. Update `src/search_query.rs` — handle-based operators use `handles.normalized`
18. Update `src/process_assets.rs`, `src/reset_demo.rs`, `crates/demo-seed`

### Phase 4: Web UI (message-vault-rs/web)

19. Regenerate `vaultSchema.generated.ts` via `sync-vault-schema.mjs`
20. Update all better-sqlite3 read/write modules to join through handles
21. Update contact CRUD (`contactsWrite.ts`, `contactsRead.ts`) — handle type, no phone-only gates
22. Update unassigned view — `participants.contact_id IS NULL`, no backfill
23. Update merge — transaction updates `contact_handles` + `participants.contact_id`
24. Update owner identity — `account_handles` replaces `account_phones`
25. Update search — typed handle matching

### Phase 5: Contracts and tests

26. Run `sync-vault-schema.mjs --check` — verify generated TS matches SQL
27. Run `regen-search-goldens.mjs` — update search parse fixtures if grammar changed
28. Update all Rust tests — schema tests, import tests, export API tests, dedupe tests
29. Update all web tests — contact CRUD, merge, unassigned, search
30. Run full smoke suite: `smoke-import-api.sh`, `smoke-vault-push.sh`, `smoke-export-api.sh`

---

## What is NOT in scope

- **Contact FTS index** — search already has `SearchMode::Contacts` in the grammar; building the index is future work
- **Automatic cross-service identity merging** — the schema supports it (one contact, many handles of different types/services), but detecting that "Discord User#1234" and "+15555550100" are the same person is manual or exporter-driven, not automatic
- **Server-side contact HTTP API** — contact CRUD remains web-side (better-sqlite3 against the vault DB). The axum server handles import/export/assets only
- **Per-handle display name overrides** — `contacts.preferred_name` is one name per contact. If a person is "Sam" on SMS but "Samuel" on Discord, the Discord handle's `raw` value carries that context; the UI can show the contact name with the handle for disambiguation
