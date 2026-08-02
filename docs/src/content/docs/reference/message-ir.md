---
title: message-ir ingest
description: The vault imports message-ir JSONL v3 from Message Exporters.
---

The vault accepts **message-ir JSONL** (schema version **3**) only. Shared Rust
types live in the exporters
[`message-ir`](https://github.com/bitrealm-dev/message-exporters/tree/main/crates/message/ir)
crate; this repo depends on that package over git.

## Happy path

**Phone backup → message-ir JSONL → vault import / ingest / `POST /v1/import` → SQLite**

There is no separate “vault JSONL” wire. Optional nested fields (especially
`imessage`) are omitted when unused — for example an SMS row has
`"imessage": null`.

Phone backup → message-ir conversion:
[Message Exporters](https://bitrealm-dev.github.io/message-exporters/)
([install / `lib/` + `cli/` layout](https://bitrealm-dev.github.io/message-exporters/get-started/install/)).

## File shape

One `*.jsonl` per conversation (plus media under `attachments/`):

1. **Line 1** — conversation header (`schema_version`, `export`, `conversation`)
2. **Following lines** — one `IrMessage` each (`timestamp_unix_ms`, `direction`,
   `service`, `text`, `attachments`, optional `imessage`, …)

`service` is the channel (`sms`, `imessage`, `rcs`, …). Rich Apple fields live
under the optional `imessage` object (tapbacks, replies, parts, edits, …).

Attachment records may include `digest_sha256` so remote clients can
`PUT /v1/assets/{sha256}` first, then import without multipart file parts.

## Clients

- Same-machine: CLI `ingest` / `import` against a local IR staging folder
- Remote: Message Exporters Vault tab / `vault-push` should POST **message-ir**
  JSONL (upload assets by SHA-256, then import). Older clients that still project
  to a former vault-only NDJSON shape need updating.
