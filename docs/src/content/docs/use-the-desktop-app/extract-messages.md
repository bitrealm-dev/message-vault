---
title: Extract messages
description: Extract messages from a phone backup in the desktop app — pick a source, choose your settings, and get your messages.
---

**Extract Messages** reads a phone backup or an app export and writes your messages as a JSONL (JSON Lines) archive. This is usually the first step: extract first, then convert to another format or import into the vault.

You can open Extract from the desktop app **login screen** without signing into a vault (use the offline Extract action). You can also extract as part of the signed-in **Import** flow.

## Before you start

Have these ready:

- The backup file or folder from your phone ([preparation guides](/prepare-your-backups/iphone-ipad/))
- Any password, key, or owner phone numbers required by that backup type
- An empty folder for the output — do not use the same folder as the source

## Run an extraction (offline)

1. Open the desktop app
2. On the login screen, choose **Extract** (you do not need a vault URL yet)
3. Pick the **Source** — iPhone / iMessage, SMS Backup & Restore, WhatsApp, or another format
4. Choose the backup path and an **output directory**
5. Start the run and watch the on-screen log for progress

Extract always writes **JSONL**. Use **Format** afterward if you need CSV, EML, MBOX, JSON, or Android XML.

## Extract as part of Import

After you sign in to a vault, open **Import** in the sidebar. That flow can extract from a backup and push into the vault in one guided run. See [Import into the vault](/use-the-desktop-app/import-into-vault/).

## What else the desktop app does

| Action | Where | What it does |
|---|---|---|
| **Extract** | Login (offline) or Import | Read a phone backup and write JSONL |
| **Format** | Login (offline) | Convert an existing export to another format |
| **Import** | Sidebar (signed in, desktop only) | Push JSONL into the vault (optionally extract first) |
| **Export** | Sidebar (signed in, desktop only) | Pull messages from the vault to disk |

Browsing conversations, contacts, search, and settings uses the same interface in the desktop app and in the browser at your vault URL.

## Find the result

The output directory is your archive. Most formats write one file per conversation. Android XML writes one `smses.xml` for the whole export. When media copying is on, attachments go in an `attachments/` folder next to the conversation files.

## Cancellation

While a run is in progress, use **Cancel** when it appears. Cancellation is cooperative — the desktop app cannot stop the external WhatsApp helper mid-run. Wait for it to finish or stop it manually.
