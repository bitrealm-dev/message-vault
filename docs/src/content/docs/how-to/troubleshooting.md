---
title: Troubleshooting
description: Fix common problems with the desktop app and reaching the vault in the browser.
---

## Desktop app

### The app will not start

**Windows SmartScreen or "unrecognized app" warning.** Click **More info** and then **Run anyway**. The app is not signed with a code-signing certificate, so Windows flags it on first launch. You only need to allow it once.

**macOS Gatekeeper or "cannot be opened" warning.** Go to **System Settings → Privacy & Security** and click **Open Anyway** next to the message about the app. Alternatively, right-click the app in Finder and choose **Open**.

**The archive was not extracted.** Running the app from inside the downloaded `.zip` or `.tgz` will fail. Extract the entire archive to a permanent folder and keep every file together — the `lib/` and `cli/` folders must stay next to the app.

**Helper programs moved or deleted.** The app looks for `ffmpeg` / `ffprobe` under `lib/` and `wtsexporter` under `cli/`, next to the app binary. Extract the archive fresh and keep the layout intact.

### Import or Extract fails

**Wrong platform.** If the app guesses the wrong platform for an iPhone backup (iOS vs macOS), set **iPhone - iOS** or **iMessage - macOS** explicitly. For WhatsApp, choose **WhatsApp - iOS** or **WhatsApp - Android**.

**Encrypted backup password is wrong.** The app cannot read an encrypted iPhone backup without the correct password.

**Wrong WhatsApp decryption key.** The key must be the full 64-character hex string, or a key file path. Re-export the key if the value is uncertain.

**wtsexporter not found.** The WhatsApp path needs the helper in `cli/wtsexporter` next to the app binary. Building from source: install with pip and see [Run from source](/developer/run-from-source/).

```bash
pip install 'whatsapp-chat-exporter[android_backup,crypt15]'
```

**Cancellation does not stop immediately.** The app cannot stop the external `wtsexporter` process mid-run. Wait for it to finish or kill the process manually.

### Media problems

**ffmpeg or ffprobe not found.** **Convert** and **Compress** need FFmpeg under `lib/` next to the binary, or on `PATH` when building from source.

**"Input and output must differ" (Format).** Choose a new empty output folder.

**Some messages are missing from a rescue import.** Limited formats cannot preserve everything. See [Rescue imports](/how-to/rescue-imports/).

## Reaching the vault

### Cannot reach the vault from the browser or desktop app

The website and API share **port 8080**. Use `http://localhost:8080`. Confirm the container is running (`docker ps`) and that nothing else has taken 8080.

### SQLITE_CANTOPEN on `/app/data/vault.db`

The `data/` directory inside the volume may be owned by `root` while the container runs as a non-root user. On Linux:

```bash
docker run --rm -v message-vault-data:/data alpine chown -R 1000:1000 /data
```

Then restart the container. This is a one-time fix when you first create a named volume on Linux. Windows and macOS Docker Desktop volumes are not affected.

### Port already in use

```bash
# Linux / macOS
lsof -i :8080

# Windows
netstat -ano | findstr :8080
```

Stop the other process, or see [Operator Docker](/developer/docker-compose/) if two Compose stacks are fighting over the same ports.

## Command-line import errors

Schema version, `vault-push`, and HTTP status codes: [HTTP API](/reference/api/) and [`vault-push`](/reference/cli/vault-push/).

## Getting help

Open an issue on [GitHub](https://github.com/bitrealm-dev/message-vault/issues). Include the operating system, Docker vs from-source, the backup source, and the error text. Do not include passwords, API tokens, phone numbers, or message content.
