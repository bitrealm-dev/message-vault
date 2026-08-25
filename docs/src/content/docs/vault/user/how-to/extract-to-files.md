---
title: Extract to files
description: Write a JSONL (JSON Lines) folder from a backup without importing into the vault.
---

[Import](/vault/user/import-from-a-backup/) in the desktop app reads a phone backup and stores it in the vault in one run. Use this page when the goal is files on disk instead: scripts, [format conversion](/vault/user/how-to/convert-formats/), or a later CLI push.

There is no Extract action on the login screen. Write JSONL with the per-source exporter commands.

## Before you start

- The backup file or folder ([Prepare a backup](/vault/user/prepare-a-backup/))
- Any password, key, or owner phone numbers required by that backup type
- An empty folder for the output — do not use the same folder as the source
- A workspace build so the exporter commands are on your PATH ([Command-line tools](/vault/developer/reference/cli/))

## Write JSONL from a backup

Pick the exporter that matches the backup:

- [`imessage-ir-exporter`](/vault/developer/reference/cli/imessage-ir-exporter/) — Mac `chat.db` or iPhone backup
- [`sms-backup-restore-exporter`](/vault/developer/reference/cli/sms-backup-restore-exporter/) — SMS Backup & Restore XML
- [`whatsapp-exporter`](/vault/developer/reference/cli/whatsapp-exporter/) — WhatsApp via `wtsexporter`

Rescue formats (GO SMS Pro, iMazing, OpenExtract, SMS Backup+) have their own commands on the same CLI index.

Pass `--format jsonl`, the backup path, and a separate output directory. Use `--help` on the installed command for passwords, keys, owner phones, dates, and media flags.

Most outputs are one file per conversation. Folder layout: [Export structure](/vault/developer/reference/export-structure/).

To convert that folder to CSV, EML, MBOX, JSON, or Android XML, see [Convert formats](/vault/user/how-to/convert-formats/).

To push an existing JSONL folder into the vault, use [`vault-push`](/vault/developer/reference/cli/vault-push/) with an API token from [Settings → Account](/vault/user/how-to/settings/).
