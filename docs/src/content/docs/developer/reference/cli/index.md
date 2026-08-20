---
title: Command-line tools
description: Choose a Message Vault command for scripts and terminal workflows.
---

These pages are Developer docs. The User Guide [Import](/user/import-from-a-backup/) chapter does not require these commands.

Build the workspace from this repository to get command-line converters and vault tools that match the desktop app. Use them for repeatable scripts, automation, or options that are easier to enter in a terminal.

Each source has its own command:

- `imessage-ir-exporter` reads a Mac `chat.db` or iPhone backup.
- `sms-backup-restore-exporter` reads SMS Backup & Restore XML.
- `whatsapp-exporter` runs `wtsexporter` and converts its result.
- `go-sms-pro-exporter`, `imazing-exporter`, `openextract-exporter`, and `sms-backup-plus-exporter` handle the limited rescue formats.
- `message-reexporter` converts an existing Message Vault output directory.
- `vault-push` imports a JSONL export folder into a running Message Vault. See [Extract to files](/user/how-to/extract-to-files/).
- `vault-pull` downloads messages from a running vault into a local JSONL folder. See [Export from the vault](/user/how-to/export-from-the-vault/).

Most converters accept `--format json|jsonl|csv|eml|mbox|xml`, an input path, an output directory, date filters, media settings, and obfuscation settings. Source-specific commands also require identity, passwords, keys, contacts, or platform values where applicable.

Keep input and output separate. Use `--help` on the installed command for its exact options. Open a command in the sidebar for its flags.
