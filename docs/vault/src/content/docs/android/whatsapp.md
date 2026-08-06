---
title: Export WhatsApp from Android
description: Export an Android WhatsApp database or encrypted backup through wtsexporter.
---

Android WhatsApp export requires files that `wtsexporter` can read. How you obtain them depends on the device and backup type; Message Vault does not extract them directly from the phone.

## Prepare the input

Keep the release folder together (`cli/wtsexporter` ships with the archive), then gather the applicable files:

- an encrypted WhatsApp backup such as `msgstore.db.crypt15`, plus the matching key file or crypt15 hexadecimal key; or
- an already extracted message database;
- optionally, a contacts database such as `wa.db`;
- optionally, the WhatsApp media folder.

Use the files from the same backup. A key that does not match the encrypted database cannot decrypt it.

## Export in the desktop app

1. Open **Export** and choose **WhatsApp**.
2. Choose the output format.
3. Set **Platform** to **Android**.
4. Fill the **Backup** and **Key** fields when using an encrypted backup.
5. Show advanced options to set an explicit message database, contacts database, media folder, or **WhatsApp Business**.
6. Choose a new empty output directory and an attachment mode.
7. Select **Run exporter**, then read **Log**.

The key value may be a key-file path or crypt15 hexadecimal material. The GUI does not save it to `export.ini`.

## If it fails

- Confirm that `wtsexporter` can be found under `cli/` next to the app, through `MESSAGE_VAULT_IO_BIN` or `PATH`, or through `WTSEXPORTER`.
- Confirm that the backup and key belong together.
- Confirm that an explicitly selected database or media folder exists.
- Wait for extraction to finish or fail. The app cannot cancel the external helper mid-run.

Conversation filenames use `__whatsapp` so they remain separate from SMS and other services.

## Use the command line

See the [`whatsapp-exporter` reference](/reference/cli/whatsapp-exporter/) for crypt keys, explicit database paths, WhatsApp Business mode, helper discovery, and all available flags.
