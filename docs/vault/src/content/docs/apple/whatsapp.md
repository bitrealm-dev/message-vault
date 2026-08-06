---
title: Export WhatsApp from Apple
description: Export WhatsApp chats from an iPhone backup with the wtsexporter helper.
---

WhatsApp export uses `wtsexporter` to read an iPhone backup. Message Vault then converts the extracted JSON into the output format you choose.

## Prepare the input

1. Install Message Vault and keep the release folder together (`cli/wtsexporter` ships with the archive).
2. Locate the iPhone backup that contains WhatsApp data.
3. Create a new empty output folder.

## Export in the desktop app

1. Open **Export**.
2. Set **Backup type** to **WhatsApp**.
3. Choose the output format.
4. Set **Platform** to **iOS**.
5. Set **Backup** to the iPhone backup path. This field is required for iOS.
6. Choose the output directory and attachment mode.
7. If needed, set dates or enable obfuscation.
8. Select **Run exporter** and read the **Log**.

Extraction happens first. Conversion starts after `wtsexporter` succeeds. Conversation filenames use a `__whatsapp` suffix so they remain separate from Apple Messages conversations.

## If it fails

- Confirm that `wtsexporter` or `wtsexporter.exe` is under `cli/` next to the app, available through `MESSAGE_VAULT_IO_BIN` or `PATH`, or named by `WTSEXPORTER`.
- Confirm that the backup path is set and points to the expected iPhone backup.
- Wait for extraction to finish or fail. The desktop app cannot cancel `wtsexporter` in the middle of its run.

The output may contain `wtsexporter_result.json` in addition to the selected conversation files and media.

## Use the command line

See the [`whatsapp-exporter` reference](/reference/cli/whatsapp-exporter/) for iOS backup arguments, helper discovery, date filters, media settings, and all available flags.
