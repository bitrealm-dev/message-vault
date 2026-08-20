---
title: Export from the vault
description: Pull messages from the vault to a folder on your computer.
---

**Export** downloads messages (and attachments when available) from a running vault to a folder on your computer. In the desktop app, Export appears in the sidebar after you sign in. The browser can also trigger a simpler download when you are logged in.

## Before you start

- A vault that is running and an account with messages already imported
- The desktop app signed in, or a browser session at the vault URL (for example `http://localhost:8080`)
- An empty output folder on disk

## Export from the desktop app

1. Sign in to the vault in the desktop app
2. Open **Export** in the sidebar
3. Choose a scope:
   - **Export entire vault** — all conversations you can access
   - **Export current view** — matches what you are browsing
   - **Export selected** — only conversations you selected
4. Choose a save path and format when prompted (JSONL, JSON, or CSV)
5. Start the export and wait for the progress steps to finish

The desktop path uses the same machinery as the `vault-pull` command: messages and attachments land under the folder you chose.

## From the terminal

Use [`vault-pull`](/vault/developer/reference/cli/vault-pull/) with the vault base URL and an API token from [Settings → Account](/vault/user/how-to/settings/).
