---
title: Trash and undo
description: Soft-delete contacts and group chats, restore from Trash, and undo/redo.
---

## Trash

**Trash** (`/trash`) holds soft-deleted contacts, unassigned handles, and group
chats. Underlying conversation and contact rows can still exist; trash markers
hide them from the main lists until you restore or permanently delete.

## Undo / redo

These actions are undoable from the list actions menu:

- Soft-delete contacts
- Soft-delete group chats
- Create label
- Delete label

After an undoable action, a snackbar appears at the bottom of the screen for
**15 seconds** with an **Undo** control. Choosing Undo or Redo from the actions
menu does **not** show that snackbar. Pressing Escape does not dismiss it; use
the snackbar’s dismiss control.

History depth is limited (20 entries).
