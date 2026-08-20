---
title: iPhone or iPad
description: Get Messages data from an iPhone or iPad — what you need, where to find it, and what Import expects.
---

The desktop app can read iPhone or iPad messages from a device backup or from a Mac. You need the device or a Mac signed into the same Apple Account.

## What you need

One of these:

- **An unencrypted iPhone backup** made with iTunes (Windows) or Finder (macOS)
- **An encrypted iPhone backup** whose password you know
- **A Mac with Messages** signed into your Apple Account — the app can read `chat.db` directly

## How to get the data

### Make an iPhone backup

Follow Apple's official guide to [back up your iPhone](https://support.apple.com/en-us/108369). An unencrypted backup is fine — messages do not require encryption. If the backup is encrypted, you need the password.

The backup is a folder on your computer. On macOS it is at `~/Library/Application Support/MobileSync/Backup/`. On Windows it is at `%APPDATA%\Apple Computer\MobileSync\Backup\`.

### Copy chat.db from a Mac

If you use Messages on a Mac:

1. Open the Messages app on the Mac — this keeps the database current
2. The database is at `~/Library/Messages/chat.db`
3. Point Import at this file (source **iMessage - macOS**)

## What Import does with it

The desktop app reads SMS, iMessage, and attachments. It can identify participants and resolve contact names from an Apple AddressBook database when that file is available.

## Known limitations

- You need the device or a Mac with Messages signed in. There is no cloud access.
- Android XML cannot store Apple-only fields like message effects and Tapbacks. That matters only if you later [convert](/user/how-to/convert-formats/) to XML.

## Next step

Open the desktop app, sign in to the vault, and go to **Import**. Choose **iPhone - iOS** for a device backup folder, or **iMessage - macOS** for `chat.db`. See [Import from a backup](/user/import-from-a-backup/).
