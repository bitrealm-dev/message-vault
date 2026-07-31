---
title: Vault JSONL
description: Wire schemas for conversation JSONL that Message Vault imports.
---

Shared Rust types live in
[`crates/message-json`](https://github.com/bitrealm-dev/message-vault-rs/tree/main/crates/message-json).
Docs and code may say **JSONL** or **NDJSON**; both mean one JSON object per
line.

## Schema boundary

Message Exporters and Message Vault use **adjacent but distinct** wire formats.
That split is intentional.

| Layer | Owned by | What it is |
|-------|----------|------------|
| **message-ir** | [Message Exporters](https://bitrealm-dev.github.io/message-exporters/) (`message-ir` crate) | Common export folder shape (IR schema v3): one `*.jsonl` per conversation plus `attachments/` |
| **Vault JSONL** (“vault NDJSON”) | This repo (`message_json::vault`) | Vault ingest wire: `"schema": "vault"`, `schema_version` 1 |
| **Projection** | Exporters `vault-push` | Projects message-ir → vault JSONL, then calls `POST /v1/import` |

Current happy path: **backup → message-ir JSONL → vault-push → vault JSONL → SQLite**.

Vault-NDJSON is **not** message-ir. Same conversation content after a deliberate
projection; different envelope, versioning, and field shape. The vault binary
does not depend on the exporters IR crate.

Phone backup → message-ir conversion:
[Message Exporters](https://bitrealm-dev.github.io/message-exporters/)
([install / `lib/` + `cli/` layout](https://bitrealm-dev.github.io/message-exporters/get-started/install/)).

## Wire schemas the vault accepts

Conversation headers carry a `"schema"` discriminator and `schema_version`:

| Wire schema | Who writes it | Discriminator |
|-------------|---------------|---------------|
| **Vault JSONL** | Message Exporters `vault-push`; `POST /v1/import` | `"schema": "vault"`, `schema_version` 1 |
| **iMessage JSONL** | Legacy iOS exporter wire | `"schema": "imessage"`, `schema_version` 4 |
| **SMS JSONL** | SMS Backup+ exporter | `"schema": "sms"`, `schema_version` 2 |

`vault` is the standard message shape for every source. It holds every field
the vault understands (text, attachments, tapbacks, replies, announcements, …).
Sources leave unused fields empty or omit them. `service` is the channel
(`SMS`, `iMessage`, …), not the wire schema name.

Attachment records may include `sha256` so remote clients can
`PUT /v1/assets/{sha256}` first, then import without multipart file parts.

Conversation headers use `"conversation_type": "individual" | "group"`.

Vault import auto-detects which schema a file uses from the conversation header
and maps everything into vault records for SQLite. Legacy `imessage` / `sms`
wires are still accepted; new exporters push **vault** after projecting from
message-ir.

An exporters **export folder** is message-ir JSONL (one `*.jsonl` per
conversation, plus media under `attachments/`). What arrives at the import API
after `vault-push` is vault JSONL.
