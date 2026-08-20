---
title: Search
description: Find conversations or contacts, then open matches in the thread view.
---

Use the search box in the app header. Open the advanced search form (chevron next to the box) when you want structured fields instead of typing operators by hand.

Advanced search has two modes:

- **Messages** — *what was said?* Full-text search across message bodies (operators for from/to/with, attachments, dates, and sources). Results are usually one row per conversation (match count + snippet).
- **Contacts** — *who do I know?* Filters the contact list by name, phone, and related fields. It does not search message text.

Typing plain words in the search box runs a **Messages** search.

## Opening a message result

Clicking a message result opens the conversation so you can read the matching thread. Use in-conversation find when you need to step through highlights inside a long chat.

Save a query under **Saved searches** in the sidebar — [Saved searches](/how-to/saved-searches/).

## Query operators

The search box accepts operators (the advanced form composes many of these for you):

| Operator | Meaning |
|----------|---------|
| `"exact phrase"` | Match the phrase |
| `-word` / `NOT word` | Exclude messages containing the word |
| `word*` | Prefix match (e.g. `avoc*` matches avocado) |
| `OR` / `AND` / `(…)` | Boolean free-text matching (AND is default between words) |
| `from:me` / `from:name` | Sent by you, or by this sender |
| `to:me` / `to:name` | Received by you, or sent by you to this person |
| `with:name` | Conversation involves this person (any role) |
| `first:text` / `last:text` | Contact first / last name (Contacts mode, or Messages with-person) |
| `is:nofirst` / `is:nolast` | Empty first / last name |
| `phone:text` | Phone or email handle |
| `subject:text` | Subject contains text |
| `text:terms` | Body only (bare terms also search subject and attachment names) |
| `after:2020` / `before:2021-06-01` | Date bounds (local calendar; also `7d` / `1w` / `1m` / `1y`) |
| `is:direct` / `is:group` | Only 1-1 or only group conversations |
| `source:imessage` | Only one import source |
| `has:attachment` / `has:noattachment` | With or without attachments |
| `filename:text` | Attachment filename contains text |
| `filetype:image` | Attachment category (`image`, `video`, `audio`, `document`, `contact`, `other`; `pdf` → document) |
| `larger:1M` / `smaller:500k` | Attachment size bounds (`K` / `M` / `G`, or raw bytes) |
| `group:none` | One result row per matching message (default is per conversation) |
| `context:2` | Include N surrounding messages when showing/opening a hit |
| `sort:date-asc` / `sort:relevance` | Oldest first, or FTS best-match (default newest first) |
| `in:title` | Restrict to a conversation by title or handle |
| `people:Family` | Threads that involve at least one contact in that contact group. Aliases: `within:`, `label:` |
| `-people:Family` | Hide threads that involve that contact group |
| `tag:Holiday` | Threads that have this thread tag. Quote names with spaces: `tag:"Work Friends"` |
| `-tag:Holiday` | Hide threads that have this thread tag |
| `tag:none` | Threads with no thread tags |
| `within:Family` | Same as `people:` (older spelling) |
| `search:contacts` | Switch to Contacts search |
| `handle:text` | (Contacts) combined name or number contains text |
| `first-contact:>=2019` / `last-contact:<2022` | (Contacts) first / last message dates |
| `group-count:>5` / `message-count:>100` | (Contacts) group / direct message counts |
