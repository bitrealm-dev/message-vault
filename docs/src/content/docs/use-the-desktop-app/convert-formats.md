---
title: Convert between formats
description: Change an export from one format to another — JSONL to CSV, JSON to EML, and every other combination.
---

The **Format** tab reads a Message Vault export folder and writes it in a different format. Use it after extracting messages — or whenever you need the same conversations in a new format without re-extracting from the original backup.

## What Format reads

The input folder must contain exports in one of these formats:

- JSON (`.json` files per conversation)
- JSONL (`.jsonl` files per conversation)
- CSV (`.csv` files per conversation)
- MBOX (`.mbox` files per conversation)
- EML (one folder per conversation, `.eml` files inside)
- Android XML (one `smses.xml`)

Create an empty output folder. Input and output cannot be the same.

## Run a conversion

1. Open the desktop app and choose **Format**
2. Select the **Input directory** — the folder with the existing export
3. Choose the **Output format** — CSV, JSON, JSONL, EML, MBOX, or Android XML
4. Choose a different **Output directory**
5. Pick the attachment mode — **Copy** keeps media when present
6. Optionally enable obfuscation
7. Select **Run**

The detector identifies the input format automatically. It ignores `attachments/` folders and old metadata files.

## Format limitations

- Android XML cannot store Apple-only fields — use JSON when preserving iMessage detail matters
- Conversion can only carry media that is still present in the source folder
- EML, MBOX, and Android XML embed media; JSON, JSONL, and CSV keep it in an `attachments/` folder

## From the terminal

You can also convert from the command line. See the [`message-reexporter` reference](/reference/cli/message-reexporter/) for flags and options.
