---
title: Settings
description: Account settings, Import API token, read-only mode, and appearance.
---

Settings opens at `/settings/account` with two tabs:

## Account

- **Sign-in details** — username, primary email, additional emails
- **Read-only mode** — blocks edits and destructive actions in the web UI;
  CLI and HTTP imports still work
- **Import API token** — copy or regenerate for `vault-push` / Vault tab
  (not your website login)
- **Vault identity** — owner name and phones used when matching “from me”
  messages (reingest after changing these)
- **Danger zone** — delete all messages for the account, or delete the account
  (demo account cannot be deleted)

New accounts start **read-only**. Turn that off and save before editing
contacts or trashing items.

## Appearance

- Message badges
- Theme
- Date/time format

## Demo reset

**Reset demo** is CLI-only (`cargo run --release -- reset-demo`). The web menu
shows instructions only. See [Try the demo](/get-started/try-the-demo/).
