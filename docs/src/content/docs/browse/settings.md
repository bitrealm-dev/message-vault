---
title: Settings
description: Account, profile, storage, and appearance settings.
---

Open **Settings** in the sidebar. Settings has four tabs:

## Account

- **Username** — read-only account id used for sign-in
- **Password** — change password when local auth is enabled
- **API tokens** — create named Bearer secrets for `vault-push` / `vault-pull`. When creating one, choose **import**, **export**, or **both**. Each secret is shown once at creation; revoke when finished. Signing in to the GUI uses a separate session token that changes on each login and does not revoke these tokens
- **Danger zone** — delete all messages for the account, or delete the account (the demo account cannot be deleted)

The desktop app and web UI sign in with username and password. Use API tokens for CLI and automation.

## Profile

- **Display name** — name shown for messages you sent
- **Handles** — phone numbers or emails used to recognize you in imports

## Storage

- **Usage** — attachment storage for this account
- **Largest attachments** — top attachments by file size
- Import history may also appear here when available

## Appearance

- Theme (light / dark / system)
- Related display preferences for the web UI

## Demo reset

**Reset demo** is CLI-only (`cargo run --release -p message-vault-server -- reset-demo`). For the demo account, reset from Docker as described in [Try the demo](/set-up-the-server/try-the-demo/).
