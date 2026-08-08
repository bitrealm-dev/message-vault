---
title: Import into the vault
description: Push a JSONL export into the vault — resume safely after interruptions, or force a full re-send.
---

After you have a JSONL export folder (with an `attachments/` directory when media was copied), you can import it into a running vault from the desktop app.

## Before you start

- A vault that is running — [quick start](/introduction/quick-start/) or [Docker install](/set-up-the-server/docker-install/)
- A vault import token from **Settings → Access** in the vault web interface
- A JSONL export folder made by the desktop app

## Normal import (resume-safe)

The import writes a journal file (`.vault-import-state.jsonl`) next to your export. On a later run with the same vault and folder, the journal lets the app skip what it already finished:

- Conversations already marked complete
- Messages already recorded by file and ID
- Attachments already recorded by their content hash

Leave **Force reprocessing** off when continuing an interrupted upload or re-running after a partial success.

## Force reprocessing

**Force reprocessing** tells the desktop app to ignore the local journal and send everything again. The vault server deduplicates on its end — messages and attachments that are already stored are skipped rather than duplicated.

Turn it on when:

- A previous run left messages without their attachments
- You fixed missing attachment files and want another pass
- The local journal is wrong or you intentionally want a full re-send

Force reprocessing is "send again" in append mode. It does not wipe the database.

## After the run

Check the **Log** tab for progress, failures, and the end summary — conversations succeeded, failed, or skipped, assets uploaded or skipped, and total time.
