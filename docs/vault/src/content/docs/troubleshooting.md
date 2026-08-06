---
title: Troubleshooting
description: Fix common problems with Message Vault.
---

## SQLITE_CANTOPEN on `/app/data/vault.db`

**Symptom**: the Docker container fails to start with `SQLITE_CANTOPEN`.

**Fix**: the `data/` directory inside the volume is owned by `root` but the container runs as a non-root user. Change ownership:

```bash
docker run --rm -v message-vault-data:/data alpine chown -R 1000:1000 /data
```

Then restart the container. This is a one-time fix when you first create a named volume on Linux. Windows and macOS Docker Desktop volumes are not affected.

## ffmpeg missing

**Symptom**: media conversion fails with "ffmpeg not found" or converted assets do not appear.

**Fix**: the published Docker image includes FFmpeg. If you are running natively, install it from your package manager:

```bash
# Debian / Ubuntu
sudo apt install ffmpeg

# macOS
brew install ffmpeg

# Windows
winget install Gyan.FFmpeg
```

Verify with `ffmpeg -version`. After installing, run `cargo run --release -- process-assets --force` to convert any assets that were skipped.

## Port conflicts

**Symptom**: "address already in use" on port 3000 or 8080.

**Fix**: something else is already using one of the ports. Check what is listening:

```bash
# Linux / macOS
lsof -i :3000
lsof -i :8080

# Windows
netstat -ano | findstr :3000
```

If another instance of the vault is already running, stop it first. If another application is using the port, change the bind address in `config/config.toml` (for port 8080) and the Next.js dev server port (for port 3000).

Running both `compose-dev.yml` and `compose-release.yml` at the same time will conflict — they share the default ports and the volume name.

## Import failures

### "schema_version mismatch"

**Symptom**: `POST /v1/import` returns an error about an unexpected `schema_version`.

**Fix**: the import API expects message-ir schema version 3. If the export was made with an older version of the desktop app, re-export it with the current version. See the [message-ir reference](/reference/message-ir/).

### "Input directory has no .jsonl files"

**Symptom**: the CLI `import` command or `vault-push` reports no `.jsonl` files found.

**Fix**: the input directory must contain `.jsonl` files at the top level or one level deep (per-conversation subdirectories). Verify the path and check that the files are not inside a subdirectory the tool is not scanning. If you used the Exporters app, the output folder is the correct input path.

### Account resolution

**Symptom**: `--account` value is not recognized.

**Fix**: use the exact username (not display name) shown under **Settings → Access** in the web UI, or the account UUID. Both work.

### Token issues

**Symptom**: `401 Unauthorized` from the import API.

**Fix**: API tokens are shown once when created under **Settings → Access → Vault Import**. If you lost the token, generate a new one. The old token stops working when a new one is created (rotation). Tokens are stored as SHA-256 hashes — the server never stores the plain text.

### Import validation errors

**Symptom**: `400 Bad Request` from `POST /v1/import`.

Common causes:
- **Missing `source` query parameter.** The API requires `?source=...` on every import request (or `source` in the session-start body for multipart).
- **Part number less than 1.** Multipart part indices are 1-based.
- **Part too large.** Each part must not exceed the `part_size` advertised by the server.
- **Source ID format.** Source names must be lowercase alphanumeric with hyphens or underscores, up to 64 characters, and not start with `-` or `_`.

### Asset already_present

**Symptom**: attachment uploads report `already_present: true` and no bytes are transferred.

**This is normal.** The server deduplicates assets by SHA-256 content hash. If the same file was already uploaded for this source (or during a previous import), the upload is skipped. It does not mean the attachment is missing — the server already has it.

## Web UI issues

### Next.js dev vs production

**Symptom**: the web UI behaves differently after `npm run build` vs `npm run dev`.

**Fix**: `npm run dev` starts a development server with hot reload. `npm run build` produces a production build for `npm start`. The published Docker image uses the production build. For development, use the dev server.

### Browser cache after upgrade

**Symptom**: the web UI looks wrong or errors after an update.

**Fix**: hard-refresh the browser (`Ctrl+Shift+R` or `Cmd+Shift+R`). The Next.js production build fingerprints assets, but the service worker or browser cache can hold stale copies.

### Hanko configuration

**Symptom**: `VAULT_AUTH=hanko` mode does not work.

**Fix**: Hanko mode requires `NEXT_PUBLIC_HANKO_API_URL` baked at Next.js build time. Rebuild the web UI after setting this variable. The Hanko project URL must be reachable from the browser (not just from the server). This mode is intended for the hosted Bitrealm service — use `VAULT_AUTH=local` for self-hosting.

## Getting help

If the issue is not covered here, open an issue on GitHub:

- [Message Vault issues](https://github.com/bitrealm-dev/message-vault-rs/issues)
- [Message Exporters issues](https://bitrealm-dev/message-vault-io/issues)

Include:
- Your setup (Docker / Compose / native, operating system)
- The command or action that failed
- The full error message or log output
- Your `config/config.toml` with any secrets removed

Do not include passwords, API tokens, phone numbers, or message content in a public issue.
