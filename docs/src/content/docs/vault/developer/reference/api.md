---
title: HTTP API
description: Tokens, import sessions, search syntax, and JSONL upload for people writing tools against the vault.
---

Route schemas, status codes, and JSON fields live in the generated [HTTP API reference](/vault/developer/rustdoc/http/). Crate types and functions live in [Rust crate docs](/vault/developer/rustdoc/). This page is the prose those tools need that is not a JSON schema.

`message-vault-server serve` reads `[server]` in `config/config.toml` (`bind`). Day-to-day import still uses the desktop [Import](/vault/user/import-from-a-backup/) screen or **`vault-push`**. Download uses [Export](/vault/user/how-to/export-from-the-vault/) or **`vault-pull`**. Those tools call the HTTP API with [JSONL](/vault/developer/reference/export-structure/) and attachment bytes keyed by SHA-256.

## Tokens

Auth is per-account. There is no host-wide admin token.

Create a named **API token** under **Settings → Account** (shown once). Copy that value into `vault-push` / `vault-pull`. A website login uses a **session** Bearer that rotates on each login. Do not paste a session token into CLI tools.

Send either token as:

```http title="Bearer header"
Authorization: Bearer <token>
```

An API token may import (write) and export messages and assets (read). It may not change profile, settings, or browse-only website routes. Export routes never delete vault data.

Turn on a local explorer with `[server] openapi_ui = true`, then open `/docs` on that vault. The explorer is off by default. “Try it” still sends this header.

## Import session

`vault-push` starts a session with `POST /v1/imports`, passes `import_id` on each `POST /v1/import`, then `POST /v1/imports/{id}/complete` so Settings → Storage can list history. Messages promoted in that session store `messages.import_id`.

If `import_id` is omitted on `POST /v1/import`, the server starts and finishes a one-shot session so Storage still records the import.

Bulk `POST /v1/import` opens its own SQLite connection so it does not hold the serve process’s short session mutex across JSONL and asset work. Same-account imports stay serialized. Export and auth open their own connections and can proceed under WAL while an import runs.

## Import body

- `Content-Type: application/jsonl` or `application/x-ndjson` — body only; attachments already uploaded by SHA-256
- `Content-Type: multipart/form-data` — field `jsonl` plus `file` parts (relative paths such as `attachments/photo.jpg`)

Request body limit matches `[server] asset_max_bytes` (default 512 MiB).

HTTP `mode` defaults to `append` (CLI `import` defaults to `replace`). HTTP `dedupe` defaults to false (CLI runs dedupe unless `--skip-dedupe`). HTTP `source` is a required query parameter. `account` is optional when the Bearer token already identifies the tenant.

## Search operators (`q`)

Export uses a **metadata** search subset (sender, participants, contact `preferred_name`, attachment names/MIME, dates, source, group/direct, labels). It does **not** run the website full-text `messages_fts` path.

- Free text terms and `"quoted phrases"` (AND); `-term` / `-"phrase"` to exclude
- `from:`, `with:` / `to:`, `subject:`, `has:attachment`
- `after:YYYY-MM-DD`, `before:YYYY-MM-DD` (year-only `YYYY` → `YYYY-01-01`)
- `source:`, `is:group`, `is:direct` (individual)
- `people:` / `within:` / `label:` (threads that involve a contact group)
- `-people:` (hide those threads)
- `tag:` / `-tag:` (thread tags; `tag:none` for untagged threads)
- Trash is always excluded; legacy `in:trash` is ignored
- `search:contacts` on message export returns `400`

## Verify a token

```bash title="Verify a token"
curl -sS "http://127.0.0.1:8080/v1/auth/check" \
  -H "Authorization: Bearer <import-api-token-from-settings>"
```

## Smoke tests

```bash title="Smoke tests"
./scripts/test/smoke-import-api.sh
./scripts/test/smoke-vault-push.sh
./scripts/test/smoke-export-api.sh
```

Health check: <http://127.0.0.1:8080/health>
