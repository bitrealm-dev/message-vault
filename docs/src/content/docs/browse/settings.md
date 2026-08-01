---
title: Settings
description: Account, access, and appearance settings.
---

Settings opens at `/settings/account` with three tabs:

## Account

- **Your identity** — username and sign-in method, plus the name shown for
  messages you sent and phone numbers used to recognize you
- **Danger zone** — delete all messages for the account, or delete the account
  (demo account cannot be deleted)

## Access

- **View-only mode** — blocks edits and deletions while browsing. Settings and
  imports remain available
- **Message import** — generate an API token for importing messages. It is
  shown once; delete and generate it again if needed

New accounts start with view-only mode off. The demo account starts with it on.

## Appearance

- Message badges
- Theme
- Date/time format

## Demo reset

**Reset demo** is CLI-only (`cargo run --release -- reset-demo`). The web menu
shows instructions only. See [Try the demo](/get-started/try-the-demo/).
