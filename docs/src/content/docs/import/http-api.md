---
title: HTTP import API
description: Endpoints, auth, and defaults for the Rust serve import and export API.
---

`cargo run --release -- serve` reads `[server]` in `config/config.toml`
(`bind`). Prefer Message Exporters **`vault-push`** / the **`message-exporter`**
Vault tab for day-to-day use; this page documents the HTTP surface those tools
call. They send [message-ir JSONL](/reference/message-ir/) (and upload
attachments by SHA-256) to these endpoints. Read-only export lets clients pull
messages and asset bytes back out with the same Bearer token.

## Endpoints

| Method | Path | Auth | Role |
|--------|------|------|------|
| `GET` | `/health` | None | Liveness |
| `GET` | `/v1/auth/check` | Bearer Import API token | Validate token |
| `GET` | `/v1/export/messages?q=&limit=&cursor=&account=&source=` | Bearer token | **Read-only** message export |
| `GET` | `/v1/assets/{sha256}?source=&account=` | Bearer token | **Read-only** asset download |
| `HEAD` | `/v1/assets/{sha256}?source=&account=` | Bearer token | Probe if asset exists |
| `PUT` | `/v1/assets/{sha256}?source=&account=` | Bearer token | Upload asset |
| `POST` | `/v1/assets/{sha256}/uploads…` | Bearer token | Multipart asset upload |
| `POST` | `/v1/imports` | Bearer token | Start import session (`{ source, mode, tool? }`) |
| `POST` | `/v1/imports/{id}/complete` | Bearer token | Finish session with counts |
| `POST` | `/v1/import?source=&account=&mode=&dedupe=&import_id=` | Bearer token | Import JSONL |

Auth is per-account only (no host-wide admin token). Tokens come from
**Settings → Access** in the web UI. The same Bearer token authorizes import
(write) and export (read). **Export routes never delete or mutate** vault data;
there is no message/asset DELETE API.

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
./scripts/smoke-import-api.sh
./scripts/smoke-vault-push.sh
./scripts/smoke-export-api.sh
```

Health check: <http://127.0.0.1:8080/health>
