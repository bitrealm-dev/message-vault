---
title: WhatsApp on Android
description: Get WhatsApp data from an Android phone — what you need, where to find it, and what the desktop app can do with it.
---

Android WhatsApp data lives in a local database on the phone. The desktop app needs that database — or an encrypted copy of it — along with the matching key.

## What you need

One of these:

- **An encrypted WhatsApp backup** (`msgstore.db.crypt*`) plus the matching key file or hexadecimal key
- **An already-extracted message database** (`msgstore.db`)
- Optionally, the `wa.db` contacts database for name resolution
- Optionally, the WhatsApp media folder for photos and videos

Use files from the same backup. A mismatched key will not decrypt.

The desktop app ships with `wtsexporter` — no separate install.

## How to get the data

There is no single path because WhatsApp and Android versions vary. In general:

1. Copy the WhatsApp database from the phone. If the phone is not rooted, use a file manager that can access `/data/data/com.whatsapp/databases/` or create a WhatsApp backup that includes the database
2. If you have an encrypted crypt file, get the matching key — WhatsApp stores it in `/data/data/com.whatsapp/files/key` on the device
3. Transfer the files to your computer

See WhatsApp's official [backup documentation](https://faq.whatsapp.com/) for the current backup steps for your version.

## What the desktop app does with it

The desktop app runs `wtsexporter` to read the WhatsApp database or encrypted backup, then converts the result into your chosen output format. Conversation filenames use a `__whatsapp` suffix.

The app can also read WhatsApp Business databases — enable that in advanced options.

## Known limitations

- Getting the files from the phone varies by device and Android version. On modern phones without root access, the direct database path may be restricted
- The desktop app cannot cancel `wtsexporter` mid-extraction — wait for it to finish or stop it manually
- A key value that the app recognizes as a passphrase is never saved to disk

## Next step

With the database and key ready, open the desktop app, choose **Extract Messages**, set the source to **WhatsApp**, and the platform to **Android**. Fill in the backup path, key, and output directory.
