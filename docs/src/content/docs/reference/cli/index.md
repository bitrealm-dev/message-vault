---
title: Command-line tools
description: Choose a Message Vault command for scripts and terminal workflows.
---

The release includes command-line converters for the same source and output families used by the desktop app. Use them for repeatable scripts, automation, or options that are easier to enter in a terminal.

Each source has its own command:

- `imessage-ir-exporter` reads a Mac `chat.db` or iPhone backup.
- `sms-backup-restore-exporter` reads SMS Backup & Restore XML.
- `whatsapp-exporter` runs `wtsexporter` and converts its result.
- `go-sms-pro-exporter`, `imazing-exporter`, `openextract-exporter`, and `sms-backup-plus-exporter` handle the limited rescue formats.
- `message-reexporter` converts an existing Message Vault output directory.

Most converters accept `--format json|jsonl|csv|eml|mbox|xml`, an input path, an output directory, date filters, media settings, and obfuscation settings. Source-specific commands also require identity, passwords, keys, contacts, or platform values where applicable.

Keep input and output separate. Use `--help` on the installed command for its exact options. Detailed generated pages for each tool are provided separately from this landing page.
