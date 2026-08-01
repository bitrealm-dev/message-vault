---
title: HTTP import API
description: Endpoints, auth, and defaults for the Rust serve import API.
---

`cargo run --release -- serve` reads `[server]` in `config/config.toml`
(`bind`). Prefer Message Exporters **`vault-push`** / the **`message-exporter`**
Vault tab for day-to-day use; this page documents the HTTP surface those tools
call. They project message-ir export folders into
[vault JSONL](/reference/vault-jsonl/) before posting here.

## Endpoints

| Method | Path | Auth |
|--------|------|------|
| `GET` | `/health` | None |
| `GET` | `/v1/auth/check` | Bearer Import API token |
| `PUT` | `/v1/assets/{sha256}?source=&account=` | Bearer token |
| `POST` | `/v1/import?source=&account=&mode=&dedupe=` | Bearer token |

Auth is per-account only (no host-wide admin token). Tokens come from
**Settings → Access** in the web UI.

## Import body formats

- `Content-Type: application/jsonl` — body only; attachments pre-uploaded by
  SHA256
- `Content-Type: multipart/form-data` — field `jsonl` plus `file` parts
  (relative paths such as `attachments/photo.jpg`)

Request body limit: 512 MiB.

## Defaults (different from CLI)

| Query | HTTP default | CLI `ingest` / `import` default |
|-------|--------------|----------------------------------|
| `mode` | `append` | `replace` |
| `dedupe` | `false` | `ingest` runs dedupe unless `--skip-dedupe` |
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
