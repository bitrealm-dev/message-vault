---
title: Troubleshooting
description: Fix common problems with the Message Vault desktop app and vault server.
---

## Desktop app

### The app will not start

**Windows SmartScreen or "unrecognized app" warning.** Click **More info** and then **Run anyway**. The app is not signed with a code-signing certificate, so Windows flags it on first launch. You only need to allow it once.

**macOS Gatekeeper or "cannot be opened" warning.** Go to **System Settings → Privacy & Security** and click **Open Anyway** next to the message about the app. Alternatively, right-click the app in Finder and choose **Open**.

**The archive was not extracted.** Running the app from inside the downloaded `.zip` or `.tgz` will fail. Extract the entire archive to a permanent folder and keep every file together — the `lib/` and `cli/` folders must stay next to the app.

**Helper programs moved or deleted.** The app looks for `ffmpeg` / `ffprobe` under `lib/` and `wtsexporter` under `cli/`, next to the app binary. If you moved those folders, the app cannot find them. Extract the archive fresh and keep the layout intact.

### Extraction fails

**Wrong platform auto-detection.** If the app guesses the wrong platform for an iPhone backup (iOS vs macOS), use the **Platform** dropdown to set it explicitly. The same applies to WhatsApp: choose Android or iOS in the form.

**Encrypted backup password is wrong.** Double-check the backup password. The app cannot extract from an encrypted iPhone backup without the correct password.

**Wrong WhatsApp decryption key.** The key must be the full 64-character hex string. If your backup uses a key file instead, pass the file path. Re-export the key from your WhatsApp backup tool if the value is uncertain.

**wtsexporter not found.** The WhatsApp path needs a Python helper. It should be in `cli/wtsexporter` next to the app binary. If you are building from source, install it with pip:

```bash
pip install 'whatsapp-chat-exporter[android_backup,crypt15]'
```

Then set `WTSEXPORTER` to the full path or add it to your `PATH`.

**Cancellation does not stop immediately.** The app uses cooperative cancellation. It cannot stop the external `wtsexporter` process mid-run during WhatsApp extraction. Wait for it to finish or kill the process manually.

### Media problems

**ffmpeg or ffprobe not found.** The **Convert** and **Compress** attachment modes need FFmpeg. The app looks for `lib/ffmpeg` and `lib/ffprobe` next to the binary. If you unzipped the archive and kept the folders together, they are already there. If you are building from source, install ffmpeg from your package manager and make sure it is on `PATH`.

**Conversion produces no output or low-quality results.** Check the compress options in the advanced section. The defaults (1080p, 30 fps, 20 MB minimum) are conservative. Raise or lower them for your needs. The on-screen log shows which files were converted and which were skipped.

### Output problems

**"Input and output must differ" error (Format).** When converting between formats, the output directory must be different from the input directory. Choose a new empty folder.

**Conversation names look unexpected.** Files named `group_...` or ending with `__whatsapp` are normal. The tool uses these stem suffixes to distinguish group chats and WhatsApp conversations from other message types.

**Obfuscation preview looks wrong.** If you enabled obfuscation and the results do not look as expected, check the seed value. An empty seed generates a random one at run time — each run produces different pseudonyms. Set an explicit 8-character hex seed for reproducible results.

**Some messages are missing from a rescue import.** The limited rescue formats (GO SMS Pro, iMazing, OpenExtract, SMS Backup+) cannot preserve everything the original backup contained. Each of those guides has a "Known limitations" section. If the source format did not store the data, the exporter cannot recover it.

## Vault server

### SQLITE_CANTOPEN on `/app/data/vault.db`

**Symptom**: the Docker container fails to start with `SQLITE_CANTOPEN`.

**Fix**: the `data/` directory inside the volume is owned by `root` but the container runs as a non-root user. Change ownership:

```bash
docker run --rm -v message-vault-data:/data alpine chown -R 1000:1000 /data
```

Then restart the container. This is a one-time fix when you first create a named volume on Linux. Windows and macOS Docker Desktop volumes are not affected.

### ffmpeg missing (native install)

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

### Cannot reach the vault from the desktop app

**Symptom**: login fails with “Could not reach server.”

**Fix**: the vault UI and API share **port 8080**. Use `http://localhost:8080` (or your host’s LAN URL with port 8080). There is no separate web UI on port 3000. Confirm the container is running (`docker ps`) and that nothing else has taken 8080.

### Port conflicts

**Symptom**: "address already in use" on port 8080.

**Fix**: something else is already using the port. Check what is listening:

```bash
# Linux / macOS
lsof -i :8080

# Windows
netstat -ano | findstr :8080
```

If another instance of the vault is already running, stop it first. If another application is using the port, change the bind address in `config/config.toml`.

Running both `compose-dev.yml` and `compose-release.yml` at the same time will conflict — they share the default ports and the volume name.

### Import failures

#### "schema_version mismatch"

**Symptom**: `POST /v1/import` returns an error about an unexpected `schema_version`.

**Fix**: the import API expects JSONL schema version 3. If the export was made with an older version of the desktop app, re-export it with the current version. See the [export structure reference](/reference/export-structure/).

#### "Input directory has no .jsonl files"

**Symptom**: the CLI `import` command or `vault-push` reports no `.jsonl` files found.

**Fix**: the input directory must contain `.jsonl` files at the top level or one level deep (per-conversation subdirectories). Verify the path and check that the files are not inside a subdirectory the tool is not scanning. If you used the desktop app **Extract** or **Format** flow, the output folder is the correct input path.

#### Account resolution

**Symptom**: `--account` value is not recognized.

**Fix**: use the exact username (not display name) shown under **Settings → Account** in the web UI, or the account UUID. Both work.

#### Token issues

**Symptom**: `401 Unauthorized` from the import API.

**Fix**: create a named API token under **Settings → Account**. The plaintext is shown once. If you lost it, revoke the old one and create a new token. Signing in to the GUI rotates a separate session token and does not change your API tokens. Secrets are stored as SHA-256 hashes — the server never stores the plain text.

#### Import validation errors

**Symptom**: `400 Bad Request` from `POST /v1/import`.

Common causes:
- **Missing `source` query parameter.** The API requires `?source=...` on every import request (or `source` in the session-start body for multipart).
- **Part number less than 1.** Multipart part indices are 1-based.
- **Part too large.** Each part must not exceed the `part_size` advertised by the server.
- **Source ID format.** Source names must be lowercase alphanumeric with hyphens or underscores, up to 64 characters, and not start with `-` or `_`.

#### Asset already_present

**Symptom**: attachment uploads report `already_present: true` and no bytes are transferred.

**This is normal.** The server deduplicates assets by SHA-256 content hash. If the same file was already uploaded for this source (or during a previous import), the upload is skipped. It does not mean the attachment is missing — the server already has it.

### Web UI issues

**Browser cache after upgrade.** If the web UI looks wrong or shows errors after an update, hard-refresh the browser (`Ctrl+Shift+R` or `Cmd+Shift+R`).

**Hanko configuration.** If `VAULT_AUTH=hanko` mode does not work, check that `HANKO_API_URL` is set and reachable from the browser. This mode is intended for the hosted Bitrealm service — use `VAULT_AUTH=local` for self-hosting.

## Getting help

If the issue is not covered here, open an issue on GitHub:

- [Message Vault issues](https://github.com/bitrealm-dev/message-vault/issues)

Include:
- Your setup (Docker / Compose / native, operating system)
- The backup source (if using the desktop app)
- The full error message or log output
- Your `config/config.toml` with any secrets removed

Do not include passwords, API tokens, phone numbers, or message content in a public issue.
