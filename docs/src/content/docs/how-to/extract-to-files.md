---
title: Extract to files
description: Write a JSONL (JSON Lines) folder from a backup without importing into the vault.
---

**Extract** on the desktop app **login screen** reads a phone backup and writes JSONL (JSON Lines) plus an `attachments/` folder when media copying is on. You do not need a vault URL.

[Import](/import-from-a-backup/) already extracts and pushes in one run. Use this page when you want files on disk — scripts, [Format](/how-to/convert-formats/), or a later CLI push.

## Before you start

- The backup file or folder ([Prepare a backup](/prepare-a-backup/))
- Any password, key, or owner phone numbers required by that backup type
- An empty folder for the output — do not use the same folder as the source

## Run Extract

1. Open the desktop app
2. On the login screen, choose **Extract**
3. Pick the **Source** (same labels as Import: **iPhone - iOS**, **iMessage - macOS**, WhatsApp, SMS Backup & Restore, or a rescue format)
4. Choose the backup path and an **output directory**
5. Start the run and watch the on-screen log

Extract always writes **JSONL**. Use **Format** afterward for CSV, EML, MBOX, JSON, or Android XML.

Most outputs are one file per conversation. Android XML (after Format) is one `smses.xml`. Folder layout: [Export structure](/reference/export-structure/).

## Cancellation

Use **Cancel** when it appears. Cancellation is cooperative. The desktop app cannot stop the external WhatsApp helper mid-run. Wait for it to finish or stop it manually.

Command line: the per-source exporters under [Command-line tools](/reference/cli/). To push an existing JSONL folder into the vault, use [`vault-push`](/reference/cli/vault-push/) with an API token from [Settings → Account](/how-to/settings/).
