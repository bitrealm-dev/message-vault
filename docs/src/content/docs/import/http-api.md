---
title: HTTP import API
description: Endpoints, auth, and defaults for the Rust serve import API.
---

`cargo run --release -- serve` reads `[server]` in `config/config.toml`
(`bind`). Prefer Message Exporters **`vault-push`** / the **`message-exporter`**
Vault tab for day-to-day use; this page documents the HTTP surface those tools
call. They send [message-ir JSONL](/reference/message-ir/) (and upload
attachments by SHA-256) to these endpoints.

## Endpoints

| Method | Path | Auth |
|--------|------|------|
| `GET` | `/health` | None |
| `GET` | `/v1/auth/check` | Bearer Import API token |
| `POST` | `/v1/imports` | Bearer token — start an import session (`{ source, mode, tool? }`) |
| `POST` | `/v1/imports/{id}/complete` | Bearer token — finish session with counts |
| `PUT` | `/v1/assets/{sha256}?source=&account=` | Bearer token |
| `POST` | `/v1/import?source=&account=&mode=&dedupe=&import_id=` | Bearer token |

Auth is per-account only (no host-wide admin token). Tokens come from
**Settings → Access** in the web UI.

`vault-push` starts a session with `POST /v1/imports`, passes `import_id` on each
`POST /v1/import`, then completes the session so Settings → Storage can list
import history. Messages promoted during that session store `messages.import_id`.

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
```

Health check: <http://127.0.0.1:8080/health>
