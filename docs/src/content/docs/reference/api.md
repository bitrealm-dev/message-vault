---
title: HTTP import API
description: Endpoints, auth, and defaults for the Rust serve import and export API.
---

`cargo run --release -p message-vault-server -- serve` reads `[server]` in `config/config.toml`
(`bind`). Prefer the desktop app [Import](/import-from-a-backup/) screen or **`vault-push`** CLI for
day-to-day import; prefer [Export](/how-to/export-from-the-vault/) or **`vault-pull`** for download. This page
documents the HTTP surface those tools call. They send [JSONL](/reference/export-structure/)
(and upload attachments by SHA-256) to these endpoints. Read-only export lets
clients pull messages and asset bytes back out with the same Bearer token.

## Endpoints

| Method | Path | Auth | Role |
|--------|------|------|------|
| `GET` | `/health` | None | Liveness |
| `GET` | `/v1/auth/check` | Bearer session token or API token | Validate token |
| `GET` | `/v1/export/messages?q=&limit=&cursor=&account=&source=` | Bearer token | **Read-only** message export |
| `GET` | `/v1/export/messages/count?q=&account=&source=` | Bearer token | **Read-only** match counts (no message bodies) |
| `GET` | `/v1/assets/{sha256}?source=&account=` | Bearer token | **Read-only** asset download |
| `HEAD` | `/v1/assets/{sha256}?source=&account=` | Bearer token | Probe if asset exists |
| `PUT` | `/v1/assets/{sha256}?source=&account=` | Bearer token | Upload asset |
| `POST` | `/v1/assets/{sha256}/uploads…` | Bearer token | Multipart asset upload |
| `POST` | `/v1/imports` | Bearer token | Start import session (`{ source, mode, tool? }`) |
| `POST` | `/v1/imports/{id}/complete` | Bearer token | Finish session with counts |
| `POST` | `/v1/import?source=&account=&mode=&dedupe=&import_id=` | Bearer token | Import JSONL |

Auth is per-account only (no host-wide admin token). For CLI tools, create a
named **API token** under **Settings → Account** in the web UI (shown once
at creation). A GUI sign-in uses a separate **session** Bearer that rotates on
each login; that session token is not what you copy into `vault-push` /
`vault-pull`. An API token authorizes import (write) and message/asset
export (read) only — not profile, settings, or browse. **Export routes never
delete or mutate** vault data; there is no message/asset DELETE API.

`vault-push` starts a session with `POST /v1/imports`, passes `import_id` on each
`POST /v1/import`, then completes the session so Settings → Storage can list
import history. Messages promoted during that session store `messages.import_id`.
If `import_id` is omitted on `POST /v1/import`, the server starts and finishes a
one-shot session for that request so Storage still records the import.


Bulk `POST /v1/import` opens its own SQLite connection for staging/promote so it
does not hold the serve process’s short session mutex across JSONL and asset
work. Same-account imports stay serialized; export and auth open their own
connections and can proceed under WAL while an import runs.

## Export (read-only)

### `GET /v1/export/messages`

Returns a page of messages for the token’s account, with conversation metadata
and attachment records (`sha256`, `mime_type`, `path`, …). Attachment **bytes
are not inline** — fetch them with `GET /v1/assets/{sha256}`.

| Query | Description |
|-------|-------------|
| `q` | Fastmail-style search (see below). Empty = all non-trashed messages |
| `limit` | Page size (default 100, max 500) |
| `cursor` | Opaque cursor from a previous `next_cursor` |
| `account` | Optional username/UUID; must match the token |
| `source` | Optional source id override (also settable via `source:` in `q`) |

Response shape:

```json
{
  "ok": true,
  "query": "has:attachment after:2020-01-01",
  "messages": [ /* … */ ],
  "next_cursor": "2015-03-12T14:05:22-04:00|0|12",
  "truncated": true
}
```

Messages are ordered ascending by `(timestamp, sort_order, id)`. Pass
`next_cursor` as `cursor` for the next page. `search:contacts` is rejected with
`400` (export is message-oriented).

### `GET /v1/export/messages/count`

Same auth and filters as `/v1/export/messages` (`q`, `account`, `source`), but
returns aggregate counts only — no paging and no message payloads. Clients use
this for a cheap preview before a full export.

| Query | Description |
|-------|-------------|
| `q` | Fastmail-style search (same as export). Empty = all non-trashed messages |
| `account` | Optional username/UUID; must match the token |
| `source` | Optional source id override |

Response shape:

```json
{
  "ok": true,
  "query": "has:attachment after:2020-01-01",
  "messages": 85476,
  "attachments": 3169,
  "total_bytes": 48234412
}
```

`attachments` is the number of unique non-empty attachment digests among
matching messages. `total_bytes` sums known `size_bytes` for those digests
(unknown sizes are omitted from the sum).

### Search operators (`q`)

Export uses a **metadata** search subset (sender, participants, contact
`preferred_name`, attachment names/MIME, dates, source, group/direct, labels).
It does **not** run the web UI’s full-text `messages_fts` path.

- Free text terms and `"quoted phrases"` (AND); `-term` / `-"phrase"` to exclude
- `from:`, `with:` / `to:`, `subject:`, `has:attachment`
- `after:YYYY-MM-DD`, `before:YYYY-MM-DD` (year-only `YYYY` → `YYYY-01-01`)
- `source:`, `is:group`, `is:direct` (individual)
- `within:` / `label:` (contacts on a label)
- Trash is always excluded; legacy `in:trash` is ignored

### `GET /v1/assets/{sha256}`

Downloads a previously stored blob for `source` + account. Same auth and query
params as `HEAD`/`PUT`. Returns the raw body with `Content-Type` when known.

## Import body formats

- `Content-Type: application/jsonl` — body only; attachments pre-uploaded by
  SHA256
- `Content-Type: multipart/form-data` — field `jsonl` plus `file` parts
  (relative paths such as `attachments/photo.jpg`)

Request body limit: 512 MiB.

## Defaults (different from CLI)

| Query | HTTP default | CLI `import` default |
|-------|--------------|----------------------|
| `mode` | `append` | `replace` |
| `dedupe` | `false` | runs dedupe unless `--skip-dedupe` |
| `source` | Required query param | From each conversation’s IR `export.source` (or `--source`) |
| `account` | Optional when Bearer token identifies the tenant | Required `--account` |

## Verify a token

```bash
curl -sS "http://127.0.0.1:8080/v1/auth/check" \
  -H "Authorization: Bearer <import-api-token-from-settings>"
```

## Smoke tests

```bash
./scripts/test/smoke-import-api.sh
./scripts/test/smoke-vault-push.sh
./scripts/test/smoke-export-api.sh
```

Health check: <http://127.0.0.1:8080/health>

## Multipart asset upload

Large attachments (over ~90 MiB) are uploaded in parts. The server advertises a chunk size (`part_size`) and the client splits the declared byte count into that many parts.

### `POST /v1/assets/{sha256}/uploads?source=&account=`

Start a multipart upload session. Request body:

```json
{ "mime": "image/jpeg", "bytes": 268435456 }
```

| Field | Required | Description |
|-------|:--------:|-------------|
| `mime` | no | MIME type recorded with the asset |
| `bytes` | yes | Declared total size in bytes. Must not exceed `asset_max_bytes` (default 512 MiB). |

Response when the asset is not yet stored:

```json
{ "ok": true, "upload_id": "…", "part_size": 67108864 }
```

`part_size` is the per-part byte limit (default 64 MiB). The client calculates the part count from `ceil(bytes / part_size)`.

Response when the asset already exists (SHA-256 match):

```json
{ "ok": true, "sha256": "…", "assets_path": "…", "already_present": true }
```

No upload is needed — the asset is already stored.

### `PUT /v1/assets/{sha256}/uploads/{upload_id}/parts/{part}`

Upload one part. `part` is 1-indexed and must be at least 1. The body is the raw part bytes. Each part must not exceed the `part_size` advertised at session start.

Response:

```json
{ "ok": true, "part": 1, "bytes": 67108864 }
```

Parts can be uploaded in any order. The server stores each part in a temporary staging directory keyed by `upload_id`.

### `POST /v1/assets/{sha256}/uploads/{upload_id}/complete`

Assemble all uploaded parts, verify the SHA-256 of the combined bytes, and store the asset. This endpoint handles a race with a concurrent single-PUT safely — if another upload finishes first, the multipart result is dropped and the already-stored digest is returned.

Response:

```json
{ "ok": true, "sha256": "…", "assets_path": "…", "already_present": false }
```

### `DELETE /v1/assets/{sha256}/uploads/{upload_id}`

Abort an in-progress multipart upload. Staged parts are removed. Idempotent — calling it on an already-completed or already-aborted session returns `{ "ok": true }`.

## Export message fields

Messages returned by `GET /v1/export/messages` have this shape:

| Field | Type | Description |
|-------|------|-------------|
| `guid` | string | Unique message identifier |
| `timestamp` | string | ISO-8601 timestamp with timezone offset |
| `timestamp_utc` | string | UTC ISO-8601 timestamp |
| `timestamp_unix_ms` | number | Unix epoch in milliseconds |
| `direction` | string | `incoming` or `outgoing` |
| `service` | string | `sms`, `imessage`, `whatsapp`, `rcs`, or `unknown` |
| `sender` | string | Sender phone number or handle |
| `sender_display` | string | Resolved sender display name |
| `is_from_me` | boolean | True for outgoing messages |
| `subject` | string or null | MMS/email subject line |
| `text` | string | Message body text |
| `is_announcement` | boolean | True for group announcement/name-change messages |
| `announcement` | string or null | The announcement text |
| `is_reply` | boolean | True if this message is a reply to another |
| `thread_originator_guid` | string or null | GUID of the message being replied to |
| `thread_originator_part` | number or null | Part index of the replied-to content |
| `num_replies` | number | Count of replies to this message |
| `attachments` | array | List of attachment objects (see below) |
| `tapbacks` | array | List of tapback/reaction objects (see below) |

Each attachment:

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | Original relative path (e.g. `attachments/photo.jpg`) |
| `original_name` | string | Original filename on the device |
| `mime_type` | string or null | Detected MIME type |
| `sha256` | string | SHA-256 hex digest of the file contents |
| `is_sticker` | boolean | True if the attachment is a sticker |
| `transcription` | string or null | Audio message transcription text |

Each tapback (reaction):

| Field | Type | Description |
|-------|------|-------------|
| `part_index` | number | Which attachment or message part the reaction applies to |
| `kind` | string | Tapback type (varies by platform) |
| `emoji` | string or null | Emoji character for the reaction |
| `is_from_me` | boolean | True if the reaction is from the account owner |
| `sender` | string | Handle of the person who reacted |

## Configuration reference

`[server]` in `config/config.toml` controls the import/export API:

| Key | Default | Description |
|-----|---------|-------------|
| `bind` | `127.0.0.1:8080` | Address the HTTP API listens on |
| `asset_max_bytes` | `536870912` (512 MiB) | Maximum size of a single attachment (single PUT body or multipart total). Must be greater than 0. |
| `asset_part_size` | `67108864` (64 MiB) | Multipart chunk size advertised to clients. Must not exceed `asset_max_bytes`. Keep under ~100 MiB for Cloudflare-proxied setups. |

The environment variable `VAULT_ASSET_PART_SIZE` can override the advertised part size at runtime (it is clamped to `1..=asset_part_size`). Set it before starting the server. This is primarily a test and operations knob — end users should use `config.toml` instead.
