---
title: Trash
description: Move a conversation or a contact to Trash, and take it back out again.
---

**Trash** in the sidebar holds conversations and contacts you have set aside. Nothing is deleted: a trashed item keeps all of its messages, handles and group membership, and it comes back exactly as it was.

## Move something to Trash

Open a conversation and click **Move to trash** in its header. The conversation leaves the inbox, along with its messages, and no longer counts towards the message and conversation totals shown for the people in it.

For a contact, open the contact and click **Move to trash** in the drawer. The contact leaves the Contacts list. Their conversations stay where they are — trashing a person is not the same as trashing what you talked about.

## Take it back

Click **Trash** in the sidebar. Trashed conversations are listed in the left column; click one and the pane shows it with a **Restore** button, next to a link to read the conversation before you decide. Trashed contacts are listed under **Contacts** in the pane itself, each row with its own **Restore**.

Restoring puts the item back where it came from. The row leaves Trash as soon as it does.

The search box narrows both lists at once while you are in Trash, so `ada` finds the trashed conversations and contacts that match it.

## Find trashed items from anywhere

Trash is a marker on the item, not a place its data was moved to, so search can ask about it directly. The `trashed:` operator works on Contacts, Conversations and Messages:

- `trashed:yes` — only trashed items
- `trashed:no` — only items that are not trashed, which is what every search does by default
- `trashed:any` — both

Searching `trashed:yes` on Contacts shows trashed contacts in the Contacts list, but they cannot be opened from there — restore them from the Trash view.

See [Search](/vault/user/how-to/search/) for the rest of the operators.

## What is not built yet

There is no permanent delete and no **Empty Trash** — see [issue 314](https://github.com/bitrealm-io/message-vault/issues/314). To remove all message content for an account, use the danger-zone actions under **Settings → Account**.

See [Settings](/vault/user/how-to/settings/).
