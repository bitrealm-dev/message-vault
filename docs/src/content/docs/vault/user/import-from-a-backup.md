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
   | **iMessage** → **Platform:** **iPhone backup** | Finder/iTunes backup folder (device UUID directory), not a `.db` file inside it |
   | **iMessage** → **Platform:** **Mac Messages** | `chat.db` |
   | **WhatsApp - iOS** | iPhone backup that includes WhatsApp |
   | **WhatsApp - Android** | `msgstore.db` or `msgstore.db.crypt*` plus key |
   | **SMS Backup & Restore** | SyncTech XML |

   Rescue sources (GO SMS Pro, iMazing, OpenExtract, SMS Backup+) are documented under [rescue imports](/vault/user/how-to/rescue-imports/).

4. Fill in paths, passwords, keys, or owner phone numbers for that source. A red asterisk marks a field that has no default and must be filled. **(Optional)** marks an empty field you can leave blank. Dropdowns that already have a value (Platform, Attachments, Contacts) have no extra mark.
5. Optionally set how contact names should be filled from vault contacts ([Contacts and labels](/vault/user/how-to/contacts-and-labels/))
6. Start the run and watch the on-screen progress and log

### iMessage fields

After you pick **iMessage**, **Platform** chooses Mac Messages or iPhone backup.

**iPhone backup**

- **iPhone Backup Directory** (required) — the device UUID folder from Finder or iTunes, not `sms.db` inside it. See [iPhone or iPad](/vault/user/prepare-a-backup/iphone-ipad/).
- **Encryption password** — required (red asterisk) when the backup is encrypted. **(Optional)** when it is not. Fill it in only for an encrypted backup.

**Mac Messages**

- **Messages database** (required) — path to `chat.db`.
- **Attachment folder (Optional)** — leave empty when `Attachments` and `StickerCache` sit next to `chat.db` (the usual Mac layout under `~/Library/Messages`). Set this only when those folders live somewhere else, for example after copying `chat.db` on its own.
- **Apple Contacts file (Optional)** — leave empty to use the local AddressBook on a live Mac. Point at `AddressBook-v22.abcddb` or `AddressBook.sqlitedb` only if that file is not in the usual Contacts location. People do not normally move that file.

**Attachments** and **Contacts** apply to both platforms. Attachments is Copy / Convert / Compress / Skip. Contacts fills names from vault contacts after import; that is separate from the Apple Contacts file above.

## Resume and force reprocessing

Import writes a journal file (`.vault-import-state.jsonl`) next to the work it does. On a later run with the same vault and folder, the journal skips work that already finished.

Leave **force reprocessing** off when continuing an interrupted upload.

Turn force reprocessing on when a previous run left messages without attachments, you fixed missing files, or the local journal is wrong. The vault still deduplicates on its end — messages and attachments already stored are skipped rather than duplicated. Force reprocessing does not wipe the database.

## After the run

Use the on-screen log for successes, failures, and the end summary. Then open **Conversations** — [Browse your messages](/vault/user/browse-your-messages/).

API tokens under **Settings → Account** are for command-line tools, not for this screen. Desktop Import uses the signed-in session. See [Command-line tools](/vault/developer/reference/cli/) if you need `vault-push`.
