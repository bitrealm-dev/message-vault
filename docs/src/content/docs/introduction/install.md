---
title: Install the desktop app
description: Download the desktop app from GitHub Releases and run it on Linux, Windows, or macOS.
---

The desktop app extracts messages from backups, converts them between formats, and imports them into the vault. It runs on your computer and works alongside the vault — or on its own, just for extracting and converting.

## Download

Open the [latest release on GitHub](https://github.com/bitrealm-dev/message-vault/releases) and download the archive for your operating system.

## Linux

1. Download the `.tgz` archive for Linux.
2. Extract it into a folder you plan to keep (`tar -xzf message-vault-*.tgz`).
3. Run the binary from that folder.

## Windows

1. Download the `.zip` archive for Windows.
2. Extract it to a folder you plan to keep — do not run the app from inside the archive.
3. Open the extracted folder and run the `.exe`.
4. If SmartScreen shows a warning, choose the option to run it once — the app is not code-signed yet.

## macOS

1. Download the `.zip` archive for Apple Silicon (M-series and later).
2. Extract it to a folder you plan to keep.
3. Run the app from that folder.
4. If Gatekeeper blocks the app, allow it once in the security prompt — it is not code-signed yet.

## What is in the archive

Keep all the files together — the app looks for its helper programs next to itself:

- **The desktop app** — the program you run
- **`ffmpeg` and `ffprobe`** — convert and compress media attached to messages
- **`wtsexporter`** — extracts WhatsApp messages
- **License notices** — project and third-party licenses

If you move the app without its helpers, media conversion and WhatsApp extraction will not work.

## System requirements

The desktop app needs a few common system libraries to start. For the exact list per operating system, see [CONTRIBUTING.md](https://github.com/bitrealm-dev/message-vault/blob/main/CONTRIBUTING.md) in the Message Vault repository.

## Next steps

- [What is Message Vault?](/introduction/what-is-message-vault/)
- [Quick start](/introduction/quick-start/) — run the demo vault and connect this app to it
