# vault-push

Push a Message Vault **JSONL** export folder into a running Message Vault (`message-vault-rs serve`).

## Synopsis

```bash
vault-push --url URL --key TOKEN --input DIR [options]
```

Prefer `VAULT_KEY` / `VAULT_URL` environment variables over putting the key on the command line.

## Description

Reads per-conversation `.jsonl` files (message-ir schema v3) under `--input`, uploads each unique attachment by SHA-256 (`PUT /v1/assets/{sha256}`), then combines conversations into bounded message-ir JSONL batches (`POST /v1/import` with `Content-Type: application/jsonl`). Requests reuse HTTP connections and are flushed at the configured message count or an **8 MiB** body target (well under Cloudflare’s ~100 MB proxied upload limit). Attachments larger than **~90 MiB** cannot be chunked and need a direct tunnel to the vault import port. Attachment uploads run concurrently. Import requests stay sequential because the server reserves one temporary import area for each account.

Progress and a durable journal (`.vault-import-state.jsonl`) live under the input directory so re-runs can resume. Secrets are never written to the journal or report.

## Options

| Flag | Env | Meaning |
|------|-----|---------|
| `--url` | `VAULT_URL` | Base URL of `message-vault-rs serve` (e.g. `http://127.0.0.1:8080` or `https://app.bitrealm.dev`), **not** the Next.js UI on `:3000` |
| `--key` | `VAULT_KEY` | Per-account Import API token (Vault key) |
| `--username` | | Optional; account is resolved from the vault key |
| `--input` | | JSONL export directory |
| `--mode append\|replace` | | Default `append` (resume-safe) |
| `--continue-on-error` | | Keep going after a failed conversation (default true) |
| `--force` | | Ignore journal; re-upload and re-import |
| `--skip-attachments` | | Import messages without uploading attachments |
| `--max-retries N` | | Transient HTTP retries (default 3) |
| `--batch-size N` | | Target messages per import request across conversations (default 1000; requests also flush near 8 MiB, under Cloudflare’s ~100 MB limit) |
| `--asset-upload-workers N` | | Simultaneous attachment uploads (default 4). Use `1` to disable parallel uploads. Message imports always remain sequential. |
| `--auth-only` | | Authenticate and exit |
| `--report` / `--log` / `--journal` | | Override artifact paths |

## See also

Message Vault GUI → **Vault** tab.
