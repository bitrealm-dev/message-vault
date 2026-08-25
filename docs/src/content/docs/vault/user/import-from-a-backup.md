---
title: Import from a backup
description: Use the desktop app Import screen to read a phone backup and store it in the vault.
---

**Import** is in the desktop app sidebar after you sign in. It is not shown in the browser-only UI. Pick a backup source, point at the file or folder, and start the run. The app extracts from that backup and pushes into the vault in one flow.

JSONL (JSON Lines) folders on disk are a command-line task: [Extract to files](/vault/user/how-to/extract-to-files/).

## Before you start

- A vault that is running — [Try the vault](/vault/user/get-started/try-the-vault/)
- The desktop app signed in as **your** account (not `demo`), server URL such as `http://localhost:8080`
- A prepared backup — [Prepare a backup](/vault/user/prepare-a-backup/)

## Run Import

1. Sign in to the vault in the desktop app
2. Open **Import** in the sidebar
3. Choose a **source** that matches the backup:

   | Source in the app | Typical files |
   |---|---|
   | **iPhone - iOS** | iTunes/Finder backup folder |
   | **iMessage - macOS** | `chat.db` |
   | **WhatsApp - iOS** | iPhone backup that includes WhatsApp |
   | **WhatsApp - Android** | `msgstore.db` or `msgstore.db.crypt*` plus key |
   | **SMS Backup & Restore** | SyncTech XML |

   Rescue sources (GO SMS Pro, iMazing, OpenExtract, SMS Backup+) are documented under [rescue imports](/vault/user/how-to/rescue-imports/).

4. Fill in paths, passwords, keys, or owner phone numbers for that source
5. Optionally set how contact names should be filled from vault contacts ([Contacts and labels](/vault/user/how-to/contacts-and-labels/))
6. Start the run and watch the on-screen progress and log

## Resume and force reprocessing

Import writes a journal file (`.vault-import-state.jsonl`) next to the work it does. On a later run with the same vault and folder, the journal skips work that already finished.

Leave **force reprocessing** off when continuing an interrupted upload.

Turn force reprocessing on when a previous run left messages without attachments, you fixed missing files, or the local journal is wrong. The vault still deduplicates on its end — messages and attachments already stored are skipped rather than duplicated. Force reprocessing does not wipe the database.

## After the run

Use the on-screen log for successes, failures, and the end summary. Then open **Conversations** — [Browse your messages](/vault/user/browse-your-messages/).

API tokens under **Settings → Account** are for command-line tools, not for this screen. Desktop Import uses the signed-in session. See [Command-line tools](/vault/developer/reference/cli/) if you need `vault-push`.
