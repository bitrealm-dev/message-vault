---
title: Settings
description: Account, Access (read-only and Vault Import), and appearance.
---

Settings opens at `/settings/account` with three tabs:

## Account

- **Vault identity** — username and no-password flag (read-only; set at
  account creation); editable first/last name and phones used when matching
  “from me” messages (reingest after changing name/phones)
- **Danger zone** — delete all messages for the account, or delete the account
  (demo account cannot be deleted)

## Access

- **Read-only mode** — blocks edits and destructive actions while browsing
  the vault (contacts, messages, trash). Settings stay editable; CLI and HTTP
  imports still work
- **Vault Import** — generate an API token for `vault-push` / Vault tab (shown
  once when created; delete and generate again if needed). Not your website
  login

New accounts start **read-only**. Turn that off under Access and save before
editing contacts or trashing items in the browse UI.

## Appearance

- Message badges
- Theme
- Date/time format

## Demo reset

**Reset demo** is CLI-only (`cargo run --release -- reset-demo`). The web menu
shows instructions only. See [Try the demo](/get-started/try-the-demo/).
