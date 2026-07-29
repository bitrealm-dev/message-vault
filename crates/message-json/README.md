# message-json

Shared **JSONL interchange** schemas for message archives.

Message Exporters produce these records; the vault binary imports them.

## Wire schemas

Conversation headers carry a `"schema"` discriminator and `schema_version`:

| Wire schema | Rust module | Who writes it | Discriminator |
|-------------|-------------|---------------|---------------|
| **Vault JSONL** | [`message_json::vault`](src/vault.rs) | Message Exporters `vault-push`; `POST /v1/import` body | `"schema": "vault"`, `schema_version` 1 |
| **iMessage JSONL** | [`message_json::imessage`](src/imessage.rs) | `imessage-exporter-json` (legacy wire) | `"schema": "imessage"`, `schema_version` 4 (headers without `schema` still default to imessage) |
| **SMS JSONL** | [`message_json::sms`](src/sms.rs) | SMS Backup+ exporter | `"schema": "sms"`, `schema_version` 2 |

`vault` is the one standard message shape for every source. It holds every field the vault understands (text, attachments, tapbacks, replies, announcements, …). Sources leave unused fields empty or omit them. `service` is the channel (`SMS`, `iMessage`, …), not the wire schema name. Attachment records may include `sha256` so remote clients can `PUT /v1/assets/{sha256}` first, then import without multipart file parts.

Conversation headers use `"conversation_type": "individual" | "group"` (not `"type"`).

Vault import auto-detects which schema a file uses from the conversation header and maps everything into vault records for SQLite.

**Breaking:** older JSONL with `"type"` / `schema_version` 1 or 3, and DBs with a `conv_type` column, are not read. Re-export and re-ingest.

## Modules

| Module | Role |
|--------|------|
| [`vault`](src/vault.rs) | One standard schema for all sources (`schema_version` **1**); rich fields omitted when unused |
| [`imessage`](src/imessage.rs) | Legacy iOS exporter wire shape (`schema_version` **4**) |
| [`sms`](src/sms.rs) | Legacy SMS Backup+ JSONL |

## Usage

Workspace members depend on this crate via the root `Cargo.toml` workspace. From another crate in the same repo:

```toml
message_json = { path = "../message-json" }
```

```rust
use message_json::vault::{
    AttachmentRecord, ConversationRecord, ExportRecord, MessageRecord, ParticipantRecord,
};

let header = ConversationRecord::header(
    "+15551212",
    "individual",
    None,
    vec![ParticipantRecord {
        handle: "+15551212".into(),
        name_hint: None,
    }],
    "SMS",
    "2024-01-01T00:00:00Z",
);
serde_json::to_writer(stdout, &ExportRecord::Conversation(header))?;
```

## Exporters

Phone backup → JSONL conversion lives in
[message-exporters](https://github.com/bitrealm-dev/message-exporters).
Historical CSV→vault notes: [`docs/CSV_INGEST.md`](docs/CSV_INGEST.md).

## Out of scope

Phone normalization, emoji decoding, SMIL/MMS parsing, and archive-format readers stay in the exporter crates.
