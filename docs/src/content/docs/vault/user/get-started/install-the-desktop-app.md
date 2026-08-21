---
title: Install the desktop app
description: Download the desktop app from GitHub Releases, or build it from source.
---

The desktop app reads phone backups and imports them into the vault. Browsing can stay in the website; Import and Export need this app.

## Download

Open the [latest release on GitHub](https://github.com/bitrealm-io/message-vault/releases) and download the archive for your operating system.

### Linux

1. Download the `.tgz` archive.
2. Extract it into a folder you plan to keep (`tar -xzf message-vault-*.tgz`).
3. Run the binary from that folder.

### Windows

1. Download the `.zip` archive.
2. Extract it to a folder you plan to keep — do not run the app from inside the archive.
3. Open the extracted folder and run the `.exe`.
4. If SmartScreen shows a warning, choose the option to run it once — the app is not code-signed yet.

### macOS

1. Download the `.zip` archive for Apple Silicon (M-series and later).
2. Extract it to a folder you plan to keep.
3. Run the app from that folder.
4. If Gatekeeper blocks the app, allow it once in the security prompt — it is not code-signed yet.

## What is in the archive

Keep all the files together. The app looks for helpers next to itself:

- **The desktop app** — the program you run
- **`ffmpeg` and `ffprobe`** — convert and compress media
- **`wtsexporter`** — extracts WhatsApp messages
- **License notices**

If you move the app without its helpers, media conversion and WhatsApp extraction will not work.

## Build from source

Compiling the app and the vault from a git checkout: [Run from source](/vault/developer/run-from-source/). Linux system libraries and WSL notes are on [Contributing](/vault/developer/contributing/).

## Next

Sign in with **http://127.0.0.1:8080** and your vault username and password, then [Import from a backup](/vault/user/import-from-a-backup/). The desktop app uses the IPv4 address because `localhost` can resolve to IPv6, which a local Docker vault does not listen on.
