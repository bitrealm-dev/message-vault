---
title: Output formats
description: Compare CSV, JSON, JSONL, EML, MBOX, and Android XML — pick the format that works for what you want to do next.
---

Choose the format the next program needs. JSONL is the extraction default because it is compact and easy to process one record at a time.

## CSV

One `.csv` file per conversation. Use it for spreadsheets, filtering, scripts, and systems that accept rows and columns. Copied media goes in `attachments/` — rows contain relative paths. See the [CSV columns reference](/reference/csv-columns/).

## JSON

One indented `.json` file per conversation. Use it as the default archive format when you may convert formats later or want a structured document for each chat. Copied media goes in `attachments/`.

## JSONL (JSON Lines)

One `.jsonl` file per conversation — a conversation header line followed by one message per line. Use JSON Lines when a script should stream or filter records without loading a whole conversation. [JSON Lines](https://jsonlines.org/) calls it "a convenient format for storing structured data that may be processed one record at a time." Copied media goes in `attachments/`.

The **Extract Messages** tab always writes JSONL. Use the **Format** tab to convert to other formats afterward.

## EML

One folder per conversation, one `.eml` file per message. Use it when a mail program imports individual email files. Media is embedded as MIME attachments — no `attachments/` folder.

## MBOX

One `.mbox` mailbox per conversation. Use it when a mail program prefers one file instead of individual EML files. Media is embedded as MIME attachments — no `attachments/` folder.

## Android XML

One `smses.xml` in the SyncTech SMS Backup & Restore format. Use it when an Android workflow needs the format that app reads. Media is embedded in the XML. Apple-only fields are dropped because the format cannot represent them.
