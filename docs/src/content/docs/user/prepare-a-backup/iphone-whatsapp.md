---
title: WhatsApp on iPhone
description: Get WhatsApp data from an iPhone backup for Import.
---

WhatsApp data on iPhone lives inside the device backup you make with iTunes or Finder.

## What you need

- An iPhone backup (unencrypted or encrypted) that includes WhatsApp data
- The desktop app with `wtsexporter` — it ships in the release archive

## How to get the data

Follow Apple's official guide to [back up your iPhone](https://support.apple.com/en-us/108369). WhatsApp data is included in the backup — no special settings are required.

If the backup is encrypted, you need the password. The desktop app does not store it.

## What Import does with it

The desktop app runs `wtsexporter` to extract WhatsApp messages from the backup, then imports the result. Conversation names use a `__whatsapp` suffix so they stay separate from Apple Messages threads.

## Known limitations

- The desktop app cannot cancel `wtsexporter` mid-run — wait for it to finish or stop it manually
- WhatsApp import needs the `wtsexporter` helper next to the app binary

## Next step

Open **Import**, set the source to **WhatsApp - iOS**, and point it at the backup. See [Import from a backup](/user/import-from-a-backup/).
