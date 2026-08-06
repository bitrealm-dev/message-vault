---
title: Rescue an OpenExtract export
description: Convert OpenExtract conversation CSV files without claiming unavailable groups or media.
---

The OpenExtract importer targets OpenExtract 0.5.1 conversation CSV files. The source format is thin, so use this path only when no fuller backup is available.

## Required input

Provide either:

- one `all_conversations.csv` or `conversation_*.csv`; or
- a directory containing those conversation CSV files.

A contacts VCF from the OpenExtract export is strongly recommended. You can instead use another contacts VCF or a vCard CSV. Only one contacts input can be used.

## Run the import

In **Export**, choose **OpenExtract (experimental)**. Select the CSV file or directory, choose the optional contacts file, set the output format and directory, then run the exporter.

## Known limitations

- `*_attachments.csv` sidecar files are ignored.
- Binary media is not extracted.
- Group conversations are not represented.
- `Sender` can be a phone number, a display name, or `me`.
- Without contacts, phones and names may not resolve. The exporter warns but continues.
- Name-only conversations are written with name-based filenames and may be unreliable for later tools that require phone identifiers.

Date filtering and obfuscation are available. Attachment conversion settings do not restore media that OpenExtract did not provide.

## Use the command line

See the [`openextract-exporter` reference](/reference/cli/openextract-exporter/) for contacts options, date filters, obfuscation, and all available flags.
