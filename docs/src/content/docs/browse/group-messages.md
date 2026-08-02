---
title: Group Messages
description: Four-panel layout for multi-party threads.
---

**Group Messages** (`/group-messages`) uses a four-panel layout:

1. **Nav** — app sidebar
2. **My contact** — your account identity (read-only chrome matching the contact list)
3. **Group chats** — all multi-party threads for the account
4. **Thread** — selected conversation messages

URL query params include `?g=<conversationId>` and optional `?y=<year>` for
year-scoped views.

Participant chips can open contact create/edit overlays when the vault is not
read-only. Soft-delete group chats from the list; restore them from
[Trash](/browse/trash-and-undo/).
