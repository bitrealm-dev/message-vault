---
title: iPhone or iPad
description: Get Messages data from an iPhone or iPad — what you need, where to find it, and what the desktop app can do with it.
---

The desktop app can read your iPhone or iPad messages from a device backup or directly from a Mac. You need access to the device or a Mac that is signed into the same Apple Account.

## What you need

One of these:

- **An unencrypted iPhone backup** made with iTunes (Windows) or Finder (macOS)
- **An encrypted iPhone backup** whose password you know
- **A Mac with Messages** signed into your Apple Account — the desktop app can read `chat.db` directly

## How to get the data

### Make an iPhone backup

Follow Apple's official guide to [back up your iPhone](https://support.apple.com/en-us/108369). An unencrypted backup is fine — messages do not require encryption. If the backup is encrypted, you need the password.

The backup is a folder on your computer. On macOS it is at `~/Library/Application Support/MobileSync/Backup/`. On Windows it is at `%APPDATA%\Apple Computer\MobileSync\Backup\`.

### Copy chat.db from a Mac

If you use Messages on a Mac, the desktop app can read the database directly:

1. Open the Messages app on your Mac — this keeps the database current
2. The database is at `~/Library/Messages/chat.db`
3. Point the desktop app at this file

## What the desktop app does with it

The desktop app reads SMS, iMessage, and attachments. It can identify participants, resolve contact names from an Apple AddressBook database, and write the messages in your chosen output format.

## Known limitations

- You need the device or a Mac with Messages signed in. There is no cloud access.
- Android XML output drops Apple-only fields like message effects and Tapbacks. Use JSON when you want to preserve iMessage details.

## Next step

With the backup or database ready, open the desktop app and go to **Extract Messages**. Choose **iPhone backup** as the source and point it at the backup folder or `chat.db`.
