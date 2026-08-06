---
title: Rescue an SMS Backup+ export
description: Convert offline SMS Backup+ EML files and understand direction and attachment limits.
---

The SMS Backup+ importer targets version 1.5.11 EML layouts. It reads files already saved on disk. It does not sign in to Gmail, connect to IMAP, or download messages.

## Required input

- One `.eml` file or one directory of EML files in the desktop app.
- A separate output directory.
- Owner phone numbers and owner email addresses needed to identify sent messages.

Contacts are optional but recommended. Use a VCF or vCard CSV. An optional name-mapping CSV has the columns `Phone,Incorrect Name`.

The command-line tool can accept more than one input root and can read defaults from `config/owner.toml`. The desktop app requires exactly one input path.

The importer recognizes both one-message-per-file exports and archive emails that contain multiple messages. Exact EML headers and archive-body rules are maintained in the crate’s technical format reference.

## Run the import

In **Export**, choose **SMS Backup+ (experimental)**. Select the EML file or directory, enter owner phone and email identities, choose optional contacts or name mapping, then select the output format and directory.

## Known limitations

- Owner email is used for sent detection when `X-smssync-type` is absent. Missing identity can make direction uncertain or stop the import.
- Contacts are needed to resolve many name-to-phone relationships. Unresolved conversations can be written to `unknown`.
- Archive attachments are paired to messages by order, so attachment-to-message matching is best effort.
- Duplicate archive and individual-message copies are detected using conversation, timestamp rounded to the second, direction, and text. When they match, the individual-message copy is preferred and attachments are combined by file content.
- Source email layouts vary. Fields that SMS Backup+ did not write cannot be recovered.

## Use the command line

See the [`sms-backup-plus-exporter` reference](/reference/cli/sms-backup-plus-exporter/) for repeatable input paths, owner identities, configuration defaults, and all available flags.
