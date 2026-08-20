---
title: "Push to Message Vault"
description: "Command-line options for importing a JSONL export folder into Message Vault."
tableOfContents:
  minHeadingLevel: 2
  maxHeadingLevel: 4
---

## vault-push

Push a Message Vault **JSONL** export folder into a running Message Vault (`message-vault-server serve`).

### Synopsis

```bash
vault-push --url URL --key TOKEN --input DIR [options]
```

Prefer `VAULT_KEY` / `VAULT_URL` environment variables over putting the key on the command line.

### Description

Reads per-conversation `.jsonl` files (message-ir schema v3) under `--input`, uploads each unique attachment by SHA-256 (`PUT /v1/assets/{sha256}`), then combines conversations into bounded JSONL batches (`POST /v1/import` with `Content-Type: application/jsonl`). Requests reuse HTTP connections and are flushed at the configured message count or an **8 MiB** body target (well under Cloudflare’s ~100 MB proxied upload limit). Attachments larger than **~90 MiB** cannot be chunked and need a direct tunnel to the vault import port. Attachment uploads run concurrently. Import requests stay sequential because the server reserves one temporary import area for each account.

Progress and a durable journal (`.vault-import-state.jsonl`) live under the input directory so re-runs can resume. Secrets are never written to the journal or report.

### Options

| Flag | Env | Meaning |
|------|-----|---------|
| `--url` | `VAULT_URL` | Base URL of `message-vault-server serve` (e.g. `http://127.0.0.1:8080` — UI and API share this origin) |
| `--key` | `VAULT_KEY` | App password from Settings → Account (Vault key) |
| `--username` | | Optional; account is resolved from the vault key |
| `--input` | | JSONL export directory |
| `--mode append\|replace` | | Default `append` (resume-safe) |
| `--continue-on-error` | | Keep going after a failed conversation (default true) |
| `--force` | | Force reprocessing: ignore the local journal and send again (append + server dedupe; does not wipe the vault). See [Import from a backup](/user/import-from-a-backup/) and [Extract to files](/user/how-to/extract-to-files/). |
| `--skip-attachments` | | Import messages without uploading attachments |
| `--max-retries N` | | Transient HTTP retries (default 3) |
| `--batch-size N` | | Target messages per import request across conversations (default 1000; requests also flush near 8 MiB, under Cloudflare’s ~100 MB limit) |
| `--asset-upload-workers N` | | Simultaneous attachment uploads (default 8). Use `1` to disable parallel uploads. Message imports always remain sequential. Conversations also prepare (hash/upload) a few files ahead on background workers. |
| `--verify-digests` | | Hash every attachment and fail on mismatch with export `digest_sha256` (default off: trust present export digests; hash only when missing). |
| `--auth-only` | | Authenticate and exit |
| `--report` / `--log` / `--journal` | | Override artifact paths |

### See also

The desktop app **Import** screen, or [Import from a backup](/user/import-from-a-backup/) and [Extract to files](/user/how-to/extract-to-files/).
