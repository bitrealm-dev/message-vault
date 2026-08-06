---
title: Navigation and sources
description: Sidebar sections, Message Sources filter, and the Combined view.
---

## Sidebar

| Label | Route | Meaning |
|-------|-------|---------|
| **Home** | `/` | Dashboard stats |
| **All** | `/all` | Every non-trashed contact |
| **Group Messages** | `/group-messages` | Multi-party threads |
| **Trash** | `/trash` | Soft-deleted contacts and group chats |
| **Settings** | `/settings/account` | Account, Access, appearance |

Additional contact views: **No Messages** (`/no-messages`), **No label**
(`/no-label`), and per-label pages under `/label/[slug]`. Labels appear in the
sidebar when present. Legacy `/contacts` and `/excluded` redirect to **All**.

Contact pages use a multi-panel layout (list → threads → messages / details).

## Message Sources

The **Message Sources** control lists sources discovered from imported data
(`data/<account_id>/<source_id>/` and the database). They are **not**
configured in `config.toml`.

- A **single source** shows every message from that archive, including
  soft-hidden duplicates.
- **Combined** merges person threads and hides soft-deduped copies
  (`duplicate_of`).

See [import modes and dedupe](/import/modes-and-dedupe/).
