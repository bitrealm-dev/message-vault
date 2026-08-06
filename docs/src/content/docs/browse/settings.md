---
title: Settings
description: Account, access, storage, and appearance settings.
---

Settings opens at `/settings/account` with four tabs:

## Account

- **Your identity** — user ID and sign-in method, plus the name shown for
  messages you sent and phone numbers used to recognize you
- **Danger zone** — delete all messages for the account, or delete the account
  (demo account cannot be deleted)

## Access

- **View-only mode** — blocks edits and deletions while browsing. Settings and
  imports remain available
- **Message import** — generate an API token for importing messages. It is
  shown once; delete and generate it again if needed

New accounts start with view-only mode off. The demo account starts with it on.

## Storage

- **Usage** — total attachment bytes for this account
- **Import history** — date, import type (source), message count, and
  attachment count for each recorded vault push or CLI import
- **Largest attachments** — top attachments by file size

## Appearance

- Message badges
- Theme
- Date/time format

## Demo reset

**Reset demo** is CLI-only (`cargo run --release -- reset-demo`). For the demo
account, the web menu shows instructions only. See
[Try the demo](/get-started/try-the-demo/).
