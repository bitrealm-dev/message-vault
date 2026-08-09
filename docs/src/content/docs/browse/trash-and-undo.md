---
title: Trash
description: Soft-deleted items in the sidebar Trash view.
---

The sidebar includes a **Trash** view for soft-deleted conversations and related items.

## Current status

The unified web UI shows **Trash** in the navigation. Soft-delete **restore** and **empty trash** API routes are not part of the current vault server surface (`/v1/*`). Treat Trash as a placeholder navigation target until those endpoints ship — do not rely on restore, permanent delete, or undo snackbars described in older guides.

For day-to-day browsing, use **Conversations** and **Contacts**. To remove all message content for an account, use **Settings → Profile** danger-zone actions (delete messages), which are wired to the live account APIs.

## Related

- [Navigation](/browse/navigation-and-sources/)
- [Settings](/browse/settings/)
