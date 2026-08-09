---
title: Import into the vault
description: Push a JSONL export into the vault — resume safely after interruptions, or force a full re-send.
---

After you have a JSONL export folder (with an `attachments/` directory when media was copied), you can import it into a running vault from the desktop app. Import is available in the desktop app sidebar after you sign in — it is not shown in the browser-only UI.

## Before you start

- A vault that is running — [quick start](/introduction/quick-start/) or [Docker install](/set-up-the-server/docker-install/)
- The desktop app signed in to that vault (server URL such as `http://localhost:8080`, plus your username and password)
- A JSONL export folder, or a phone backup you will extract during Import

For CLI-only import (`vault-push`), create an app password under **Settings → Account** in the vault UI.

## Use Import in the desktop app

1. Sign in to the vault in the desktop app
2. Open **Import** in the sidebar
3. Choose a backup source (or point at an existing JSONL export when the flow allows)
4. Fill in the paths and options for that source
5. Optionally set how contact names should be filled from vault contacts
6. Start the run and watch the on-screen progress and log

## Normal import (resume-safe)

The import writes a journal file (`.vault-import-state.jsonl`) next to your export. On a later run with the same vault and folder, the journal lets the app skip what it already finished:

- Conversations already marked complete
- Messages already recorded by file and ID
- Attachments already recorded by their content hash

Leave force reprocessing off when continuing an interrupted upload or re-running after a partial success.

## Force reprocessing

Force reprocessing tells the desktop app to ignore the local journal and send everything again. The vault server deduplicates on its end — messages and attachments that are already stored are skipped rather than duplicated.

Turn it on when:

- A previous run left messages without their attachments
- You fixed missing attachment files and want another pass
- The local journal is wrong or you intentionally want a full re-send

Force reprocessing is "send again" in append mode. It does not wipe the database.

## After the run

Use the on-screen progress log for successes, failures, and the end summary — conversations succeeded, failed, or skipped, assets uploaded or skipped, and total time. Then open **Conversations** to browse what landed in the vault.

## Related

- [Extract messages](/use-the-desktop-app/extract-messages/)
- [Export from the vault](/use-the-desktop-app/export-from-vault/)
- [Import API reference](/reference/api/)
