---
title: "Pull from Message Vault"
description: "Command-line options for downloading messages from Message Vault into a local JSONL folder."
tableOfContents:
  minHeadingLevel: 2
  maxHeadingLevel: 4
---

## vault-pull

Pull messages from a running Message Vault into a local **JSONL** export folder (`*.jsonl` + `attachments/`).

### Synopsis

```bash title="Synopsis"
vault-pull \
  --url http://127.0.0.1:8080 \
  --key "$VAULT_KEY" \
  --out ./from-vault \
  --query 'has:attachment' \
  --after 2020-01-01 \
  --before 2021-01-01
```

Prefer `VAULT_KEY` / `VAULT_URL` environment variables over putting the key on the command line.

### Description

Uses the same Import API Bearer token as `vault-push`. Export HTTP routes are **read-only** (`GET /v1/export/messages`, `GET /v1/export/messages/count`, `GET /v1/assets/{sha256}`).

Authenticates with the vault, pages matching messages, writes per-conversation JSONL under `--out`, and downloads attachments by SHA-256. A journal under the output directory supports resume on re-runs.

Create a named API token under **Settings → Account** in the vault UI. The vault base URL is the same origin as the web UI (for example `http://127.0.0.1:8080`).

Prefer the desktop app **Export** screen for a GUI. See [Export from the vault](/vault/user/how-to/export-from-the-vault/).

### Options

| Flag | Env | Meaning |
|------|-----|---------|
| `--url` | `VAULT_URL` | Base URL of the vault (UI and API on the same origin) |
| `--username` | | Optional; account is resolved from the vault key |
| `--key` | `VAULT_KEY` | API token (Settings → Account) |
| `--out` | | Output directory for JSONL + `attachments/` |
| `--query` | | Fastmail-style search query (optional) |
| `--after` / `--before` | | Optional date bounds (`YYYY-MM-DD`) |
| `--source` | | Restrict to one vault source id |
| `--skip-attachments` | | Messages only |
| `--page-limit N` | | Page size for `/v1/export/messages` |
| `--auth-only` | | Authenticate and exit |

Use `vault-pull --help` for the full flag list.

### See also

The desktop app **Export** screen, or [Export from the vault](/vault/user/how-to/export-from-the-vault/). [`vault-push`](/vault/developer/reference/cli/vault-push/) imports in the other direction.
