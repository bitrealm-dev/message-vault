---
title: Vault JSONL
description: Wire schemas for conversation JSONL that Message Vault imports.
---

Message Exporters produce JSONL records; the vault binary imports them. Shared
Rust types live in
[`crates/message-json`](https://github.com/bitrealm-dev/message-vault-rs/tree/main/crates/message-json).

## Wire schemas

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
and maps everything into vault records for SQLite.

An **export folder** is one `*.jsonl` file per conversation, plus media
(typically under `attachments/`).

Phone backup → JSONL conversion:
[Message Exporters](https://bitrealm-dev.github.io/message-exporters/).
