---
title: WhatsApp on iPhone
description: Get WhatsApp data from an iPhone backup for Import.
---

WhatsApp data on iPhone lives inside the device backup you make with iTunes or Finder.

## What you need

- An iPhone backup (unencrypted or encrypted) that includes WhatsApp data
- The desktop app, with `wtsexporter` installed — see [Install the desktop app](/vault/user/get-started/install-the-desktop-app/)

## How to get the data

Follow Apple's official guide to [back up your iPhone](https://support.apple.com/en-us/108369). WhatsApp data is included in the backup — no special settings are required.

WhatsApp-on-iPhone Import points at the Finder/iTunes backup folder. It does not ask for the Apple backup password. Encrypted device backups are a `wtsexporter` limitation, not a field on this form.

## What Import does with it

The desktop app runs `wtsexporter` to extract WhatsApp messages from the backup, then imports the result. Conversation names use a `__whatsapp` suffix so they stay separate from Apple Messages threads.

## Known limitations

- The desktop app cannot cancel `wtsexporter` mid-run — wait for it to finish or stop it manually
- WhatsApp import needs `wtsexporter` on `PATH`

## Next step

Open **Import**, set the source to **WhatsApp**, set **Platform** to **iPhone**, and point it at the backup folder. See [Import from a backup](/vault/user/import-from-a-backup/).
