---
title: Convert formats
description: Change an export folder from one format to another without re-reading the original backup.
---

[`message-reexporter`](/vault/developer/reference/cli/message-reexporter/) reads a Message Vault export folder and writes it in a different format. There is no Format action on the login screen.

Happy-path [Import](/vault/user/import-from-a-backup/) does not require this step.

## What it reads

The input folder must contain exports in one of these formats:

- JSON (`.json` files per conversation)
- JSONL (JSON Lines — `.jsonl` files per conversation)
- CSV (`.csv` files per conversation)
- MBOX (`.mbox` files per conversation)
- EML (one folder per conversation, `.eml` files inside)
- Android XML (one `smses.xml`)

Create an empty output folder. Input and output cannot be the same.

## What it writes

| Format | Shape | Media |
|---|---|---|
| **CSV** | One `.csv` per conversation | `attachments/` folder. Columns: [CSV columns](/vault/developer/reference/csv-columns/) |
| **JSON** | One indented `.json` per conversation | `attachments/` folder |
| **JSONL** | One `.jsonl` per conversation | `attachments/` folder |
| **EML** | One folder per conversation, one `.eml` per message | Embedded |
| **MBOX** | One `.mbox` per conversation | Embedded |
| **Android XML** | One `smses.xml` | Embedded. Apple-only fields are dropped |

## Run a conversion

1. Build the workspace so `message-reexporter` is available
2. Point `--input` at the existing export folder
3. Point `--output` at a different empty folder
4. Set `--format` to `json`, `jsonl`, `csv`, `eml`, `mbox`, or `xml`
5. Optional: `--media-mode` (`clone` keeps media when present; details: [Media and privacy](/vault/user/how-to/media-and-privacy/)) and `--obfuscate`

The detector identifies the input format automatically. It ignores `attachments/` folders and old metadata files. Full flags: [`message-reexporter`](/vault/developer/reference/cli/message-reexporter/).

## Limitations

- Conversion can only carry media that is still present in the source folder
- Android XML cannot store Apple-only fields — use JSON or JSONL when preserving iMessage detail matters
