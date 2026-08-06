---
title: Export Apple Messages
description: Export iMessage and SMS from a Mac Messages database or an iPhone backup.
---

Message Vault can read a Mac Messages `chat.db` or an iPhone backup and write JSON, JSONL, CSV, EML, MBOX, or Android-compatible XML.

## Prepare the input

Use one of these inputs:

- a Mac `chat.db`; or
- the root folder of an unencrypted iPhone backup; or
- the root folder of an encrypted iPhone backup whose password you know.

You can also prepare an Apple AddressBook database if you want names resolved when using a Mac Messages database.

## Export in the desktop app

1. Open **Export**.
2. Set **Backup type** to **iPhone backup**.
3. Choose the output format.
4. Set **Database / iOS backup path** to `chat.db` or the iPhone backup folder.
5. Choose a new empty output directory.
6. Leave **Platform** on **Auto**, or select **macOS** or **iOS** when automatic detection is wrong.
7. Enter the backup password when the iPhone backup is encrypted.
8. Choose how to handle attachments. **Copy** is the usual choice for an archive.
9. If needed, show advanced options and set an attachment root, one conversation identifier, or an Apple contacts path.
10. Select **Run exporter**, then check **Log**.

## Check the result

Most formats create one artifact per conversation. Android XML creates one `smses.xml`. JSON, JSONL, and CSV place copied media in `attachments/`; EML, MBOX, and XML embed it.

If the export fails, check that the path points to a real Messages database or iPhone backup. For an encrypted backup, check the password. **Convert** and **Convert & compress** also require `ffmpeg` and `ffprobe`.

Android XML drops Apple-only fields. Use JSON when preserving iMessage details for later conversion matters.

## Use the command line

See the [`imessage-ir-exporter` reference](/reference/cli/imessage-ir-exporter/) for platform selection, encrypted-backup passwords, conversation filters, attachment handling, and all available flags.
