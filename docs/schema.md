# Vault database tables

This file describes every lasting table in the Message Vault SQLite database, and how the tables point at each other.

The tables fall into four groups:

1. **Chats and texts** — threads, people in those threads, messages, files, reactions
2. **People and groups** — your address book inside the vault
3. **Staging** — temporary copies used while importing, then copied into the real tables
4. **Trash markers** — small lists that mark something as trashed without deleting the chat data

Chats and people are **not** joined by a shared person ID. They meet when the **same phone number or email text** appears in both places.

```mermaid
erDiagram
    conversations ||--o{ participants : "has"
    conversations ||--o{ messages : "has"
    messages ||--o{ attachments : "has"
    messages ||--o{ tapbacks : "has"
    messages ||--o| messages : "duplicate_of"
    contacts ||--o{ contact_handles : "has"
    contacts ||--o{ contact_label_members : "in"
    contact_labels ||--o{ contact_label_members : "has"
    contact_handles }o--o| participants : "same phone or email text"
```

---

## Chats and texts

### `conversations`

One row = one chat thread.

| Column | Plain meaning |
|--------|----------------|
| `id` | Internal number for this chat |
| `account_id` | Which vault tenant owns this chat (points to `accounts.id`) |
| `chat_identifier` | How the chat is named in the backup (often a phone number for a 1:1 chat, or a group id) |
| `service` | Optional service label from the export |
| `conversation_type` | Kind of chat, such as individual or group |
| `group_title` | Optional group name |
| `exported_at` | Optional time the export was made |
| `source_file` | Which export file this chat came from |

Other tables point here with `conversation_id`.

### `participants`

One row = one phone number or email that belongs in one chat.

| Column | Plain meaning |
|--------|----------------|
| `id` | Internal number for this row |
| `conversation_id` | Which chat this person is in (points to `conversations.id`) |
| `handle` | Their phone number or email |
| `name_hint` | Optional display name that came with the backup (not the same as a vault contact) |

The same handle can appear in more than one chat. Each chat gets its own participant row. Inside one chat, each handle appears only once (`UNIQUE` on conversation + handle).

### `messages`

One row = one text (or similar message) inside a chat.

| Column | Plain meaning |
|--------|----------------|
| `id` | Internal number for this message |
| `conversation_id` | Which chat it belongs to (points to `conversations.id`) |
| `account_id` | Which vault tenant owns this message (denormalized from the conversation; used for per-account GUID uniqueness) |
| `source` | Which archive it came from (for example `sms-backup-plus`) |
| `guid` | Optional id string from that archive (unique together with `account_id` + `source`) |
| `timestamp` / `timestamp_utc` | When it was sent |
| `is_from_me` | Whether you sent it |
| `sender` | Who sent it (phone/email text when known) |
| `subject` / `body` | Subject and text body |
| `content_key` | Hash used when comparing the same SMS across archives |
| `duplicate_of` | If set, points to another `messages.id` that this row is a copy of |

Other message fields store reply and announcement details used by some exporters.

### `attachments`

One row = one file tied to one message (photo, video, and so on).

| Column | Plain meaning |
|--------|----------------|
| `id` | Internal number |
| `message_id` | Which message owns this file (points to `messages.id`) |
| `path` / `assets_path` | Where the file lives on disk |
| `original_name` | Original filename when known |
| `mime_type` | File type |
| `sha256` | Fingerprint of the file bytes |
| `derived_*` | Optional converted copy (for example a resized image) |

### `tapbacks`

One row = one reaction on a message (heart, thumbs up, and so on).

| Column | Plain meaning |
|--------|----------------|
| `id` | Internal number |
| `message_id` | Which message was reacted to (points to `messages.id`) |
| `kind` / `emoji` | What kind of reaction |
| `is_from_me` / `sender` | Who reacted |

---

## People and groups

### `accounts`

One row = one web login account for the Message Vault site.

| Column | Plain meaning |
|--------|----------------|
| `id` | Stable account identifier (UUID). Does not change when username or emails are updated. |
| `username` | Login username (unique, mutable) |
| `read_only` | When set, blocks edits to contacts, groups, and messages |

### `account_emails`

One row = one email address for a web account.

| Column | Plain meaning |
|--------|----------------|
| `account_id` | Points to `accounts.id` |
| `email` | Email address (unique across all accounts, mutable only by editing the row) |
| `is_primary` | Exactly one row per account is primary; the primary email cannot be deleted |

An account must always have one primary email. Additional emails can be added or removed; a non-primary email can be promoted to primary.

On first access, the account row is seeded from legacy `config.toml` `[account]` when present.

### `account_api_tokens`

One row = the import API token for a web account (`vault-push` / `Authorization: Bearer`).

| Column | Plain meaning |
|--------|----------------|
| `account_id` | Points to `accounts.id` (one token per account) |
| `token` | Plaintext token shown in Settings (v1; regenerate replaces it) |
| `created_at` | When the current token was issued |

Created on signup (and backfilled on Settings load). Distinct from the host-wide `[server] api_token` admin secret in `config.toml`.

### `vault_owners`

Per-account message / vault owner profile (whose backups this tenant holds).

| Column | Plain meaning |
|--------|----------------|
| `account_id` | Points to `accounts.id` |
| `display_name` | Display name for “you” in exports and the UI |

### `vault_owner_phones` / `vault_owner_emails`

Phone numbers and emails for the vault owner, scoped by `account_id`.

### `contacts`

One row = one person in the vault address book.

| Column | Plain meaning |
|--------|----------------|
| `id` | Internal number for this person |
| `account_id` | Which vault tenant owns this contact |
| `first_name` / `last_name` | Their name |
| `exclude` | Flag used to mark someone as excluded in the UI |
| `preferred_handle` | Optional main phone or email to show |

### `contact_handles`

One row = one phone number or email that belongs to one contact.

| Column | Plain meaning |
|--------|----------------|
| `account_id` | Which vault tenant owns this handle |
| `handle` | The phone or email |
| `contact_id` | Which contact owns it (points to `contacts.id`) |

One contact can have many handles. Each handle points to only one contact per account (`PRIMARY KEY (account_id, handle)`).

### `contact_labels`

One row = one named contact label (for example Family).

| Column | Plain meaning |
|--------|----------------|
| `id` | Internal number |
| `account_id` | Which vault tenant owns this label |
| `name` | Label name (unique per account) |

### `contact_label_members`

One row = “this contact has this label.”

| Column | Plain meaning |
|--------|----------------|
| `contact_id` | Points to `contacts.id` |
| `label_id` | Points to `contact_labels.id` |

Together those two columns are the primary key, so a contact appears at most once under a given label.

---

## How chats meet people

There is no column like `contact_id` on `conversations` or `participants`.

The link is the **handle text**:

- A 1:1 chat’s `conversations.chat_identifier` is often a phone or email.
- `participants.handle` is also a phone or email.
- `contact_handles.handle` is the address-book side of the same string.

If the strings match, the UI can treat that chat as belonging to that contact. If there is no matching `contact_handles` row, the chat can show up as unassigned.

`participants.name_hint` is only a label from the backup. It does not create a row in `contacts`.

```mermaid
flowchart LR
  chat["conversations / participants"] -->|"same phone or email string"| book["contact_handles"]
  book --> person["contacts"]
```

---

## Staging tables

During import, data is first written into mirror tables, then moved into the real tables above.

| Staging table | Matches |
|---------------|---------|
| `staging_conversations` | `conversations` (includes `account_id`) |
| `staging_participants` | `participants` |
| `staging_messages` | `messages` (includes `account_id`; without cross-source duplicate fields). Import clears staging **per account**, not globally. |
| `staging_attachments` | `attachments` |
| `staging_tapbacks` | `tapbacks` |

Relationships inside staging are the same shape: conversations → participants and messages → attachments and tapbacks. After a successful promote, the lasting data lives in the non-staging tables.

---

## Trash markers

These tables do not store the chat or contact themselves. They mark an id or handle as trashed. The underlying conversation or contact rows can still exist.

### `trashed_handles`

| Column | Plain meaning |
|--------|----------------|
| `account_id` | Which vault tenant trashed this handle |
| `handle` | Phone or email marked trashed |
| `trashed_at` | When it was marked |

### `trashed_conversations`

| Column | Plain meaning |
|--------|----------------|
| `account_id` | Which vault tenant trashed this chat |
| `conversation_id` | Chat id marked trashed |
| `trashed_at` | When it was marked |

### `trashed_contacts`

| Column | Plain meaning |
|--------|----------------|
| `account_id` | Which vault tenant trashed this contact |
| `contact_id` | Contact id marked trashed |
| `trashed_at` | When it was marked |

---

## Quick map

| You want… | Look in… |
|-----------|----------|
| A chat thread | `conversations` |
| Who is listed in a chat | `participants` |
| The texts | `messages` |
| Photos and files | `attachments` |
| Reactions | `tapbacks` |
| A person you named | `contacts` |
| Web login account | `accounts` |
| Their phone numbers / emails | `contact_handles` |
| Contact labels | `contact_labels` + `contact_label_members` |
| Soft-deleted items | `trashed_*` |
| Import scratch space | `staging_*` |

Legacy single-tenant databases without `account_id` on `conversations` are wiped on first access (fresh start). See `ensure_vault_schema` in [`src/schema.rs`](../src/schema.rs).

Table definitions in code: [`src/schema.rs`](../src/schema.rs).

Related: [how message dedupe works](dedupe.md).
