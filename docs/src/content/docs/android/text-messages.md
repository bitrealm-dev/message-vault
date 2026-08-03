---
title: Export Android text messages
description: Convert SMS Backup & Restore XML into JSON, CSV, mail archives, or Android XML.
---

The recommended Android SMS and MMS path starts with a readable SyncTech SMS Backup & Restore XML file.

## Prepare the backup

1. Create an XML backup in SMS Backup & Restore.
2. Copy the `.xml` file to the computer. You may also use a folder containing multiple `.xml` backups; they are combined into one export.
3. If the app produced an encrypted ZIP, unlock and unzip it first. Message Vault does not open encrypted ZIP backups.
4. Write down every phone number that belonged to the backed-up device.
5. Optionally prepare a contacts VCF or vCard CSV.

Owner phone numbers are required. They identify the owner in MMS groups and help determine chat membership and senders. A wrong number can reverse or mis-group messages.

## Export in the desktop app

1. Open **Export**.
2. Set **Backup type** to **SMS Backup & Restore**.
3. Choose the output format.
4. Set **Input** to one XML file or a directory of XML files.
5. Choose a new empty output directory.
6. Enter the owner's phone number or numbers.
7. Optionally choose a contacts file.
8. Choose the attachment mode, then set dates or obfuscation if needed.
9. Select **Run exporter** and check **Log**.

## What is included

SMS and MMS are supported. Embedded MMS media can be copied or embedded in the chosen output. Call logs are ignored. Draft, failed, queued, and outbox messages are skipped.

Missing contacts do not stop the export; unresolved names remain blank. Invalid dates, unknown message types, empty participants, and unreadable attachment data can cause individual records to be skipped and counted in the log.

Writing Android XML creates one `smses.xml` that SMS Backup & Restore can read. Apple-only fields from other source formats cannot be represented in this XML format.

## Use the command line

See the [`sms-backup-restore-exporter` reference](/reference/cli/sms-backup-restore-exporter/) for processing multiple backups, repeating owner phone numbers, selecting contacts, and all available flags.
