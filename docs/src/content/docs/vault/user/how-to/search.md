---
title: Search
description: Find conversations or contacts, then open matches in the thread view.
---

Use the search box in the app header. Open the advanced search form (chevron next to the box) when you want structured fields instead of typing operators by hand.

Advanced search has two modes:

- **Messages** — *what was said?* Searches message text; the words below narrow it. Results are usually one row per conversation (match count + snippet).
- **Contacts** — *who do I know?* Filters the contact list by name, phone, and related fields. It does not search message text.

Typing plain words in the search box runs a **Messages** search.

## Opening a message result

Clicking a message result opens the conversation so you can read the matching thread. Use in-conversation find when you need to step through highlights inside a long chat.

Save a query under **Saved searches** in the sidebar — [Saved searches](/vault/user/how-to/saved-searches/).

## The search language

One language works on all three lists: **Contacts**, **Conversations**, and **Messages**. Plain words search the row's own text: a contact's name and handles, a conversation's title and who is in it, a message's body, subject, and attachment names. Everything else is a word, a colon, and a value.

### How values work

- Put quotes around a value with a space or a colon: `group:"Book Club"`. Two quotes in a row are one quote.
- Case never matters.
- `#12` means the thing with that id: `group:#7`, `with:#42`, `in:#19`.
- `none` and `any` work on every word that names a thing or holds text: `tag:none`, `attachment:any`, `name:none`.
- Dates name a span: `2024` is the year, `2024-05` the month, `2024-05-01` the day, `7d` or `2w` or `3m` or `1y` the last that long, plus `today` and `yesterday`. Add `>=` for from its start, `<` for before its start, `>` for after its end, `<=` for up to its end, or `a..b` for a range: `date:2019`, `date:>=2019`, `date:<1m`, `date:2019..2021`.
- Sizes are `500k`, `1M`, `2G`, or plain bytes. Counts are plain numbers. Both take the same `>`, `>=`, `<`, `<=`, and `a..b`.
- A comma inside a value means either: `service:imessage,sms`. Repeating a word means both: `tag:Work tag:Urgent`.
- `-` in front of anything means not: `-tag:Work`, `-avocado`. `or` and parentheses work as you would expect: `(toast or guacamole) avocado`.
- `avoc*` matches words starting with avoc; `"exact phrase"` matches the phrase.

A word the list does not have is refused with a message that says so, and offers the nearest word when there is one.

### The words

C, V, and M mark which lists accept the word: Contacts, Conversations, Messages.

| Word | Means | Values | Lists |
|---|---|---|---|
| `body:` | message body only | text, `none`, `any` | V M |
| `subject:` | subject line only | text, `none`, `any` | V M |
| `name:` | a person's name: this contact, or someone in the conversation | text, `none`, `any` | C V M |
| `title:` | the conversation's title | text, `none`, `any` | V M |
| `handle:` | a phone number, email, or username | text, `none`, `any` | C V M |
| `with:` | this person is in the conversation | name, handle, `#id` | V M |
| `from:` | this person sent it | `me`, name, handle, `#id` | M |
| `to:` | it was sent to this person | `me`, name, handle, `#id` | M |
| `in:` | this one conversation | title, handle, `#id` | M |
| `group:` | in this Contact Group: the contact, or someone in the conversation | name, `#id`, `none`, `unknown` | C V M |
| `tag:` | the conversation carries this Message Tag | name, `#id`, `none` | C V M |
| `kind:` | direct or group conversation | `direct`, `group` | C V M |
| `service:` | how the message travelled | `imessage`, `sms`, `mms`, `rcs`, `whatsapp` | C V M |
| `source:` | which backup it was imported from | `imessage`, `whatsapp`, `sms` | V M |
| `import:` | brought in by this Import Run | `#id`, `last` | V M |
| `date:` | when a message was sent; on Contacts and Conversations, has a message then | date | C V M |
| `first-message:` | the date of the earliest message | date | C V M |
| `last-message:` | the date of the latest message | date | C V M |
| `attachment:` | what is attached | `image`, `video`, `audio`, `document`, `pdf`, `contact`, `other`, `any`, `none` | V M |
| `filename:` | an attachment's file name | text, `pre*` | V M |
| `size:` | an attachment's size | `>1M`, `<500k`, `100k..2M` | V M |
| `messages:` | how many messages | `>100`, `0`, `1..10` | C V |
| `conversations:` | how many conversations | count | C |
| `groups:` | how many Contact Groups | count | C |
| `participants:` | how many people in the conversation | count | V M |
| `attachments:` | how many attachments on the message | count | M |
| `trashed:` | in the trash | `yes`, `no`, `any` | C V |

### Examples

- `from:me to:"Jane Doe" (avocado or "guacamole night")` on Messages.
- `last-message:<2022` on Contacts: everyone you have not heard from since 2022.
- `group:Family date:2019..2021 attachment:image size:>1M` on Messages.
- `participants:>2 -tag:Archive` on Conversations.
- `group:none messages:>0` on Contacts: people with messages who are in no Contact Group.

Sorting, the Contacts switch, and how results are grouped are controls on the screen, not words in the search, so a Saved Search means what you want and never how the screen looked.
