---
title: Command-line tools
description: Choose a Message Vault command for scripts and terminal workflows.
---

These pages are Developer docs. The User Guide [Import](/vault/user/import-from-a-backup/) chapter does not require these commands.

Build the workspace from this repository to get command-line converters and vault tools that match the desktop app. Use them for repeatable scripts, automation, or options that are easier to enter in a terminal.

How those commands fit together is on [Message Transfer](/vault/developer/message-transfer/). The tree and binaries list is on [Vault Design](/vault/developer/vault-design/).

## Supported

- [`imessage-ir-exporter`](/vault/developer/reference/cli/imessage-ir-exporter/) reads a Mac `chat.db` or iPhone backup.
- [`sms-backup-restore-exporter`](/vault/developer/reference/cli/sms-backup-restore-exporter/) reads SMS Backup & Restore XML.
- [`whatsapp-exporter`](/vault/developer/reference/cli/whatsapp-exporter/) runs `wtsexporter` and converts its result.

## Rescue / experimental

Limited formats. Prefer a supported backup when one can still be made.

- [`go-sms-pro-exporter`](/vault/developer/reference/cli/go-sms-pro-exporter/)
- [`imazing-exporter`](/vault/developer/reference/cli/imazing-exporter/)
- [`openextract-exporter`](/vault/developer/reference/cli/openextract-exporter/)
- [`sms-backup-plus-exporter`](/vault/developer/reference/cli/sms-backup-plus-exporter/)

## Vault JSONL

- [`message-reexporter`](/vault/developer/reference/cli/message-reexporter/) converts an existing Message Vault output directory.
- [`vault-push`](/vault/developer/reference/cli/vault-push/) imports a JSONL export folder into a running Message Vault. See [Extract to files](/vault/user/how-to/extract-to-files/).
- [`vault-pull`](/vault/developer/reference/cli/vault-pull/) downloads messages from a running vault into a local JSONL folder. See [Export from the vault](/vault/user/how-to/export-from-the-vault/).

Most converters accept `--format json|jsonl|csv|eml|mbox|xml`, an input path, an output directory, date filters, media settings, and obfuscation settings. Source-specific commands also require identity, passwords, keys, contacts, or platform values where applicable.

Keep input and output separate. Use `--help` on the installed command for its exact options. Open a command in the sidebar for its flags.
