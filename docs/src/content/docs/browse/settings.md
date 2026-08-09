---
title: Settings
description: Profile, storage, and appearance settings.
---

Open **Settings** in the sidebar. Settings has three tabs:

## Profile

- **Your identity** — username, display name for messages you sent, and handles (phone numbers or emails) used to recognize you
- **Import API token** — generate a Bearer token for `vault-push` / `vault-pull` and other API clients. It is shown once; delete and generate again if needed
- **Password** — change password when local auth is enabled
- **Danger zone** — delete all messages for the account, or delete the account (the demo account cannot be deleted)

The desktop app signs in with username and password against the vault URL. The Import API token is mainly for CLI and automation.

## Storage

- **Usage** — attachment storage for this account
- **Largest attachments** — top attachments by file size
- Import history may also appear here or under a related import-history view when available

## Appearance

- Theme (light / dark / system)
- Related display preferences for the web UI

## Demo reset

**Reset demo** is CLI-only (`cargo run --release -p message-vault-server -- reset-demo`). For the demo account, reset from Docker as described in [Try the demo](/set-up-the-server/try-the-demo/).
