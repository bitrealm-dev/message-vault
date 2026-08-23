---
title: Troubleshooting
description: Fix common problems with the desktop app and reaching the vault in the browser.
---

## Desktop app

### The app will not start

**Windows SmartScreen or "unrecognized app" warning.** Click **More info** and then **Run anyway**. The app is not signed with a code-signing certificate, so Windows flags it on first launch. You only need to allow it once.

**macOS Gatekeeper or "cannot be opened" warning.** Go to **System Settings → Privacy & Security** and click **Open Anyway** next to the message about the app. Alternatively, right-click the app in Finder and choose **Open**.

### Import or Extract fails

**Wrong platform.** If the app guesses the wrong platform for an iPhone backup (iOS vs macOS), set **iPhone - iOS** or **iMessage - macOS** explicitly. For WhatsApp, choose **WhatsApp - iOS** or **WhatsApp - Android**.

**Encrypted backup password is wrong.** The app cannot read an encrypted iPhone backup without the correct password.

**Wrong WhatsApp decryption key.** The key must be the full 64-character hex string, or a key file path. Re-export the key if the value is uncertain.

**wtsexporter not found.** Install `wtsexporter` with the commands on [Install the desktop app](/vault/user/get-started/install-the-desktop-app/). Confirm it is on `PATH`, then retry.

```bash title="Install wtsexporter"
pipx install "whatsapp-chat-exporter[android_backup,crypt15]"
```

**Cancellation does not stop immediately.** The app cannot stop the external `wtsexporter` process mid-run. Wait for it to finish or kill the process manually.

### Media problems

**ffmpeg or ffprobe not found.** **Convert** and **Compress** need FFmpeg on `PATH`. Install it with the commands on [Install the desktop app](/vault/user/get-started/install-the-desktop-app/).

**"Input and output must differ" (Format).** Choose a new empty output folder.

**Some messages are missing from a rescue import.** Limited formats cannot preserve everything. See [Rescue imports](/vault/user/how-to/rescue-imports/).

## Reaching the vault

### Cannot reach the vault from the browser or desktop app

The website and API share **port 8080**. Use `http://localhost:8080`. Confirm the container is running (`docker ps`) and that nothing else has taken 8080.

### SQLITE_CANTOPEN on `/app/data/vault.db`

The `data/` directory inside the volume may be owned by `root` while the container runs as a non-root user. On Linux:

```bash title="Fix volume ownership"
docker run --rm -v message-vault-data:/data alpine chown -R 1000:1000 /data
```

Then restart the container. This is a one-time fix when you first create a named volume on Linux. Windows and macOS Docker Desktop volumes are not affected.

### Port already in use

```bash title="Find what is using port 8080"
# Linux / macOS
lsof -i :8080

# Windows
netstat -ano | findstr :8080
```

Stop the other process. From a clone, `./scripts/run-vault-dev.sh` and a Compose stack both want port 8080. See [Docker](/vault/developer/docker/) if two Compose files are fighting over that port.

## Command-line import errors

Schema version, `vault-push`, and HTTP status codes: [HTTP API](/vault/developer/reference/api/) and [`vault-push`](/vault/developer/reference/cli/vault-push/).

## Getting help

Open an issue on [GitHub](https://github.com/bitrealm-io/message-vault/issues). Include the operating system, Docker vs from-source, the backup source, and the error text. Do not include passwords, API tokens, phone numbers, or message content.
