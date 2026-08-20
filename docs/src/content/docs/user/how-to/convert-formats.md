---
title: Convert formats
description: Change an export folder from one format to another without re-reading the original backup.
---

**Format** reads a Message Vault export folder and writes it in a different format. Open Format from the desktop app **login screen** without signing into a vault (offline Format action).

Happy-path [Import](/user/import-from-a-backup/) does not require this step.

## What Format reads

The input folder must contain exports in one of these formats:

- JSON (`.json` files per conversation)
- JSONL (JSON Lines — `.jsonl` files per conversation)
- CSV (`.csv` files per conversation)
- MBOX (`.mbox` files per conversation)
- EML (one folder per conversation, `.eml` files inside)
- Android XML (one `smses.xml`)

Create an empty output folder. Input and output cannot be the same.

## What Format writes

| Format | Shape | Media |
|---|---|---|
| **CSV** | One `.csv` per conversation | `attachments/` folder. Columns: [CSV columns](/developer/reference/csv-columns/) |
| **JSON** | One indented `.json` per conversation | `attachments/` folder |
| **JSONL** | One `.jsonl` per conversation | `attachments/` folder |
| **EML** | One folder per conversation, one `.eml` per message | Embedded |
| **MBOX** | One `.mbox` per conversation | Embedded |
| **Android XML** | One `smses.xml` | Embedded. Apple-only fields are dropped |

## Run a conversion

1. Open the desktop app and choose **Format** from the login screen
2. Select the **Input directory**
3. Choose the **Output format**
4. Choose a different **Output directory**
5. Pick the attachment mode — **Copy** keeps media when present. Details: [Media and privacy](/user/how-to/media-and-privacy/)
6. Optionally enable obfuscation
7. Start the run and watch the on-screen log

The detector identifies the input format automatically. It ignores `attachments/` folders and old metadata files.

## Limitations

- Conversion can only carry media that is still present in the source folder
- Android XML cannot store Apple-only fields — use JSON or JSONL when preserving iMessage detail matters

Command line: [`message-reexporter`](/developer/reference/cli/message-reexporter/).
