---
title: SMS and MMS on Android
description: Get SMS and MMS data from an Android phone — what you need, where to get it, and what the desktop app can do with it.
---

The recommended path for Android SMS and MMS uses the [SMS Backup &amp; Restore](https://play.google.com/store/apps/details?id=com.riteshsahu.SMSBackupRestore) app (SyncTech) from Google Play. It produces a readable XML file the desktop app can process.

## What you need

- An Android phone with SMS Backup &amp; Restore installed
- At least one XML backup file transferred to your computer
- The phone numbers that belonged to the device — needed so the app can tell sent messages from received ones

A contacts file (VCF or vCard CSV) is optional but helps fill in display names.

## How to get the data

1. Install [SMS Backup &amp; Restore](https://play.google.com/store/apps/details?id=com.riteshsahu.SMSBackupRestore) from Google Play
2. Open the app and create a backup — choose XML format
3. Transfer the `.xml` file to your computer (email, USB, or cloud storage)
4. If the backup is an encrypted ZIP, unlock it before use — the desktop app does not open encrypted archives

You can also provide a folder of multiple XML files. The desktop app combines them into one export.

## What the desktop app does with it

The desktop app reads SMS and MMS from the XML, resolves contacts, and writes messages in your chosen format. MMS attachments — photos, videos, audio — are copied or embedded depending on your settings.

## Known limitations

- Only SMS and MMS are supported. Call logs, drafts, failed, and outbox messages are skipped
- Incorrect owner phone numbers can swap senders or mis-group conversations. Enter every number that belonged to the device
- Writing back to Android XML creates a file SMS Backup &amp; Restore can read, but Apple-only fields from other sources are dropped

## Next step

Transfer the XML file to your computer, open the desktop app, choose **Extract Messages**, and set the source to **SMS Backup &amp; Restore**. Enter the owner phone numbers and choose the output directory.
