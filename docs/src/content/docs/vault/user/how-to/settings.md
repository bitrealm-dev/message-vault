---
title: Settings
description: Account, profile, storage, and appearance settings.
---

Open **Settings** in the sidebar. Settings has four tabs:

## Account

- **Username** — read-only account id used for sign-in
- **Password** — change password when local auth is enabled
- **API tokens** — named Bearer secrets for command-line `vault-push` / `vault-pull`. When creating one, choose **import**, **export**, or **both**. Each secret is shown once at creation; revoke when finished. Signing in to the website uses a separate session token that changes on each login and does not revoke these tokens. Desktop **Import** does not need an API token. CLI flags: [Command-line tools](/vault/developer/reference/cli/)
- **Danger zone** — delete all messages for the account, or delete the account (the demo account cannot be deleted)

## Profile

- **Display name** — name shown for messages you sent
- **Handles** — phone numbers or emails used to recognize you in imports

## Storage

- **Usage** — attachment storage for this account
- **Largest attachments** — top attachments by file size
- Import history may also appear here when available

## Appearance

- Theme (light / dark / system)
- Related display preferences for the website

## Demo reset

Resetting sample data is a server command: [`reset-demo`](/vault/developer/reference/server-cli/#reset-demo). For a Docker volume, see [Docker](/vault/developer/docker/).
