---
title: WhatsApp on iPhone
description: Get WhatsApp data from an iPhone — what you need, where to find it, and what the desktop app can do with it.
---

The desktop app reads WhatsApp messages from an iPhone backup. WhatsApp data on iPhone is stored inside the device backup you make with iTunes or Finder.

## What you need

- An iPhone backup (unencrypted or encrypted) that includes WhatsApp data
- The desktop app with `wtsexporter` — it ships in the release archive, no separate install needed

## How to get the data

Follow Apple's official guide to [back up your iPhone](https://support.apple.com/en-us/108369). WhatsApp data is included in the backup — no special settings are required.

If the backup is encrypted, you need the password. The desktop app does not store it.

## What the desktop app does with it

The desktop app runs `wtsexporter` to extract the WhatsApp messages from the backup, then converts the result into your chosen output format. Conversation filenames use a `__whatsapp` suffix so they stay separate from your Apple Messages conversations.

## Known limitations

- The desktop app cannot cancel `wtsexporter` mid-extraction — wait for it to finish or stop it manually
- WhatsApp export needs the `wtsexporter` helper. It is included in the app archive, next to the desktop app binary

## Next step

With the backup ready, open the desktop app, choose **Extract Messages**, set the source to **WhatsApp**, and set the platform to **iOS**. Enter the backup path and choose the output format.
