---
title: Search
description: Find people or find messages, then step through matches inside a conversation.
---

Search answers two different questions. Pick the one you mean from the tabs in
the search dropdown (the chevron next to the search box):

- **People** — *who do I know?* Filters your contact list by name, number,
  label, first/last contact date, or message counts. It never looks at message
  text, and it can return people with no messages at all.
- **Messages** — *what was said?* Full-text search across message bodies.
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
| `with:name` | Conversation includes this person |
| `from:name` | Message sent by this person |
| `after:2020` / `before:2021-06-01` | Date bounds |
| `is:direct` / `is:group` | Only 1-1 or only group conversations |
| `source:imessage` | Only one import source |
| `has:attachment` | Message has a photo or file |
| `within:label` | Only contacts with this label |
| `search:contacts` | Switch to People search |
| `handle:text` | (People) name or number contains text |
| `first-contact:>=2019` / `last-contact:<2022` | (People) activity dates |
| `group-count:>5` / `message-count:>100` | (People) message counts |
