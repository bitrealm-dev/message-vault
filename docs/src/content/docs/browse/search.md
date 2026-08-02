---
title: Search
description: Find contacts or find messages, then step through matches inside a conversation.
---

Search answers two different questions. Pick the one you mean from the tabs in
the search dropdown (the chevron next to the search box):

- **Contacts** — *who do I know?* Filters your contact list by a combined
  handle (name or number), or expand Handle for first/last name (including
  empty), phone, label, first/last message date, or direct/group message
  counts. It never looks at message text, and it can return contacts with no
  messages at all.
- **Messages** — *what was said?* Full-text search across message bodies
  (Fastmail-style operators for from/to/with, attachments, and dates).
  Results show one row per **conversation**, with a count of matching messages
  and a snippet of the best match — not one row per message.

Typing plain words in the search box runs a **Messages** search.

## Opening a message result

Clicking a message result opens the conversation, scrolls to the best match,
and opens the **find bar** prefilled with your search words. The find bar
shows how many messages match (for example, `3 of 12`) and the arrows —
or <kbd>Enter</kbd> / <kbd>Shift</kbd>+<kbd>Enter</kbd> — step through every
match. Matches are highlighted in the conversation.

## Find in a conversation

You can also search inside any open conversation directly: click the search
icon in the conversation header or press <kbd>Ctrl</kbd>+<kbd>F</kbd>
(<kbd>Cmd</kbd>+<kbd>F</kbd> on Mac). <kbd>Esc</kbd> closes the find bar.

## Query operators

The search box also accepts operators, composed for you by the advanced form:

| Operator | Meaning |
|----------|---------|
| `"exact phrase"` | Match the phrase |
| `-word` | Exclude messages containing the word |
| `from:me` / `from:name` | Sent by you, or by this sender |
| `to:me` / `to:name` | Received by you, or sent by you to this person |
| `with:name` | Conversation involves this person (any role) |
| `first:text` / `last:text` | Contact first / last name (Contacts, or Messages with-person) |
| `is:nofirst` / `is:nolast` | Empty first / last name (Contacts, or Messages with-person) |
| `phone:text` | Phone or email handle (Contacts, or Messages with-person) |
| `subject:text` | Subject contains text |
| `text:terms` | Body only (bare terms also search subject and attachment names) |
| `after:2020` / `before:2021-06-01` | Date bounds (local calendar; also `7d` / `1w` / `1m` / `1y`) |
| `is:direct` / `is:group` | Only 1-1 or only group conversations |
| `source:imessage` | Only one import source |
| `has:attachment` / `has:noattachment` | With or without attachments |
| `filename:text` | Attachment filename contains text |
| `filetype:image` | Attachment category (`image`, `video`, `audio`, `document`, `contact`, `other`; `pdf` → document) |
| `in:title` | Restrict to a conversation by title or handle |
| `within:label` | Only contacts with this label |
| `search:contacts` | Switch to Contacts search |
| `handle:text` | (Contacts) combined name or number contains text |
| `first-contact:>=2019` / `last-contact:<2022` | (Contacts) first / last message dates |
| `group-count:>5` / `message-count:>100` | (Contacts) group / direct message counts |
