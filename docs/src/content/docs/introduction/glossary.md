---
title: Glossary
description: Plain-language definitions of the formats and terms you will meet in this guide.
---

Short, plain-language definitions of the formats and terms you will meet while using Message Vault.

## Formats

**JSONL (JSON Lines)** — One JSON object per line of text, used for data that is processed record-by-record. [JSON Lines](https://jsonlines.org/) calls it "a convenient format for storing structured data that may be processed one record at a time." Each conversation in an export is one `.jsonl` file.

**JSON** — Pretty-printed JSON with indentation. The same data as JSONL, formatted to be easier for a human to read. One `.json` file per conversation.

**CSV** — Comma-separated values, the spreadsheet format. One `.csv` file per conversation, ready to open in your spreadsheet program.

**EML** — The standard file format for a single email. Each message becomes one `.eml` file.

**MBOX** — A single file that holds many emails. One `.mbox` file per conversation.

**XML** — The SyncTech SMS Backup & Restore format (`smses.xml`), used by Android backup and restore tools.

**VCF** — vCard, the standard format for contact cards. Used to bring address books in and keep them clean.

**E.164** — The international phone number format, for example `+15555550100`. Message Vault normalizes phone numbers to this format.

## Software

**Docker** — Runs the vault server in a container, so you do not install it directly on your machine.

**SQLite** — The database that stores your messages in the vault. It is a single file on your computer, and nothing leaves it.

## Where to go next

- [What is Message Vault?](/introduction/what-is-message-vault/)
- [Why manual backups?](/introduction/why-manual-backups/)
