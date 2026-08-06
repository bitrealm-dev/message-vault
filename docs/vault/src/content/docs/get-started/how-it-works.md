---
title: How it works
description: Understand the pipeline that turns a phone backup into files you can keep and use.
---

Every export follows the same three-step pipeline, no matter which phone or backup format you start with:

```
phone backup → intermediate structure → your chosen format
```

## The intermediate step (message-ir)

Before the tool writes your final files, every message and conversation is first turned into a shared per-conversation structure. The project calls this **message-ir** internally — it is a plain-text JSON Lines file (`.jsonl`) with one message per line, plus an `attachments/` folder next to it.

This intermediate step is what makes the tool work across so many combinations: one exporter parses an Apple backup, another parses SMS Backup & Restore XML, and they both produce the same structure. From there, every output format reads the same structure back.

You can think of message-ir as the tool's internal "workspace" format. You do not need to understand it to use the app, but it has two practical benefits:

- **Re-format without re-extracting.** The desktop app's **Format** tab reads an existing JSONL export and writes any other format. You do not need the original phone backup again.
- **Import into Message Vault.** The [Message Vault](https://bitrealm-dev.github.io/message-vault-rs/) server reads message-ir JSONL. An export from the desktop app can be uploaded directly.

The JSONL files are plain text — you can open them in any text editor. For the full technical definition, see the [export structure](/understand-output/export-structure/) page and the vault's [message-ir reference](https://bitrealm-dev.github.io/message-vault-rs/reference/message-ir/).

## The pipeline in detail

1. **Parse.** The source exporter reads your backup file or folder — an iPhone backup, an SMS Backup & Restore XML file, a WhatsApp database, or one of the other supported formats. It extracts every message, attachment reference, and participant identity.

2. **Resolve.** Owner identity (your phone number or Apple ID), contact names, and group participants are filled in. Attachment files may be copied, converted, or compressed at this stage depending on your settings.

3. **Write.** The exporter packages every conversation in your chosen format. The default is JSONL (one file per conversation, one line per message). You can also choose CSV, EML, MBOX, JSON, or Android XML — either at extraction time or later with the Format tab.

## A short glossary

| Term | What it means |
|------|---------------|
| **JSONL** | JSON Lines — one JSON object per line of text. Each conversation is one `.jsonl` file. Plain text, easy for machines and humans to read. |
| **JSON** | Pretty-printed JSON. One `.json` file per conversation. Same data as JSONL, formatted with indentation. |
| **CSV** | Comma-Separated Values — a spreadsheet format. One `.csv` file per conversation. |
| **EML** | Electronic Mail — the standard file format for a single email. Each message becomes one `.eml` file. |
| **MBOX** | Mailbox — a single file containing many emails. One `.mbox` file per conversation. |
| **XML** | The SyncTech SMS Backup & Restore format (`smses.xml`). One file for the entire export. Used by Android backup/restore tools. |
| **VCF** | vCard File — the standard format for contact cards. Used to import address books. |
| **E.164** | The international phone number format (e.g. `+15555550100`). The tool normalizes all phone numbers to this format. |
| **PDU** | Protocol Data Unit — a binary MMS message format used by some older Android backup tools. |
