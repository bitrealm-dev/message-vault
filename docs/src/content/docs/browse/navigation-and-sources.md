---
title: Navigation and sources
description: Sidebar sections, conversations list, and message sources.
---

After you sign in (browser or desktop app), the left sidebar is the main way to move around the vault.

## Sidebar

| Label | Meaning |
|-------|---------|
| **Conversations** | Message threads — one-to-one and group chats |
| **Contacts** | People and handles discovered from imports |
| **Trash** | Soft-deleted items when the Trash view is available (see [Trash](/browse/trash-and-undo/)) |
| **Import** | Push backups into the vault (desktop app only) |
| **Export** | Pull messages to disk (desktop app only) |
| **Settings** | Profile, storage, and appearance |
| **Sign out** | End the session |

**Saved groups** under the sidebar store search queries you reuse. Create one with **+ New**, then click a group name to run that search again.

There is no separate Home dashboard route and no Next.js-style paths such as `/all` or `/settings/account`. Navigation is in-app view state.

## Conversations

Open **Conversations** to browse threads. Select a conversation to read messages and view attachments. Use search (header search box) to filter by text or operators — see [Search](/browse/search/).

## Message sources

Imports are tagged with a **source** (for example iMessage or WhatsApp). Search and advanced filters can limit results with `source:…`. Sources come from data you imported, not from entries you edit in `config.toml`.

- Searching without a source filter looks across everything you can access
- `source:imessage` (or another source id) limits matches to that import line

See [import into the vault](/use-the-desktop-app/import-into-vault/) for how pushes land in the database.
