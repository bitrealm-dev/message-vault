---
title: Extract messages
description: Extract messages from a phone backup in the desktop app — pick a source, choose your settings, and get your messages.
---

The **Extract Messages** tab reads a phone backup or an app export and writes your messages as a JSONL (JSON Lines) archive. This is usually the first step: extract first, then convert to another format or import into the vault.

## Before you start

Have these ready:

- The backup file or folder from your phone ([preparation guides](/prepare-your-backups/iphone-ipad/))
- Any password, key, or owner phone numbers required by that backup type
- An empty folder for the output — do not use the same folder as the source

## Run an extraction

1. Open the desktop app and choose **Extract Messages**
2. Pick the **Backup type** — iPhone backup, SMS Backup & Restore, WhatsApp, or another format
3. Fill in the fields shown for that type — source path, output directory, owner identity
4. Choose an **Attachments** mode (Copy is the default for a full archive)
5. Optionally set a date range or enable obfuscation
6. Select **Run**

Extract Messages always writes **JSONL**. Use the **Format** tab afterward if you need CSV, EML, MBOX, JSON, or Android XML.

## Tabs in the desktop app

| Tab | What it does |
|---|---|
| **Extract Messages** | Read a phone backup or app export and write JSONL |
| **Format** | Convert an existing export to a different format |
| **Contacts** | Validate or clean a contacts file |
| **Vault** | Push a JSONL export into the vault |
| **Log** | Show the full output from the last run |

## Find the result

The output directory is your archive. Most formats write one file per conversation. Android XML writes one `smses.xml` for the whole export. When media copying is on, attachments go in an `attachments/` folder next to the conversation files.

## Cancellation

While a run is in progress, use **Cancel** when it appears. Cancellation is cooperative — the desktop app cannot stop the external WhatsApp helper mid-run. Wait for it to finish or stop it manually.
