---
title: Why manual backups?
description: Why you provide backups yourself — a platform limitation, not a Message Vault one.
---

To get your messages into Message Vault, you first make a backup of your phone and point the desktop app at it. This step is manual, and there is a reason for it: the companies that make your phone and messaging apps do not offer an alternative.

## Apple, Google, and WhatsApp do not open message access

Apple, Google, and WhatsApp do not provide API access to your message data. There is no supported way for any program to log in and download your message history — no official API, and no sync service you can plug into. This applies to every messaging platform, not just one.

## Your messages already live in local files

Instead, your messages are stored in local databases and backups on the device itself:

- An iPhone keeps its messages inside the device backup you make on your computer.
- An Android phone keeps them in a local database on the phone — and in the SMS Backup & Restore XML file you can export.
- WhatsApp messages live in the app's local database on your phone.

The desktop app reads those local files directly. No login, no sync, no third-party server in between — just your files and your computer.

## A platform limitation, not a Message Vault limitation

Every tool that works with message history operates this way, because the platform owners do not open access to the data. We would love a one-click path too, but the constraint comes from Apple, Google, and WhatsApp — not from Message Vault.

If cloud access ever becomes available, the app will adapt. For now, the manual steps are the only path — and they take a few minutes.

## Where to start

The next section walks you through preparing each backup:

- [iPhone or iPad](/prepare-your-backups/iphone-ipad/)
- [WhatsApp on iPhone](/prepare-your-backups/iphone-whatsapp/)
- [SMS and messages on Android](/prepare-your-backups/android-sms/)
- [WhatsApp on Android](/prepare-your-backups/android-whatsapp/)
