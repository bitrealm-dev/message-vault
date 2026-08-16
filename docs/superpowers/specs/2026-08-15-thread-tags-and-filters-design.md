# Thread tags, contact-group filters, and saved searches

**Date:** 2026-08-15
**Status:** Approved for implementation

## Goal

Give the sidebar three jobs that stay distinct:

1. **Contact groups** — named sets of people. Click shows the contact list. Search can include or exclude those people when listing threads.
2. **Saved searches** — named queries stored in the browser (today’s Saved Groups, new heading).
3. **Thread tags** — names stamped on whole conversations. Click shows those threads. Search `tag:Holiday` finds messages in tagged threads.

## Why

Contact groups are for **who**. Thread tags are for **which threads**. A saved search is a **rule** that can match new threads later. Storing three conversation ids as a query is not a tag: adding or removing a thread would mean rewriting the query.

Individual messages are not tagged. Search is how someone finds one text inside a thread.

## Product rules

| Item | Rule |
|------|------|
| Contact group click | Opens the contact list for that group. Does not switch to threads. |
| Thread tag click | Opens the thread list for that tag. |
| Saved search click | Runs the stored query. |
| Tag target | Whole conversation only. New messages in that thread match. A new thread does not, until tagged. |
| Several names | A contact can be in several groups. A thread can have several tags. |
| Selection | Contact list and thread list both have checkboxes. The Groups or Tags menu applies to every checked row. If none are checked, the menu applies to the highlighted row. Mixed state when only some selected rows already have the name. |
| Same menu shape | Checkboxes, create name, clear all. Different heading and search token. |

## Sidebar copy

Top nav stays **Threads**, **Contacts**, **Trash**.

Below that:

- **Contact groups**
- **Saved searches** (replace “Saved Groups”)
- **Thread tags**

## Search tokens

Message / thread list:

| Token | Meaning |
|-------|---------|
| `people:Family` | Threads that involve at least one contact in that contact group. Aliases: `within:`, `label:` (already implemented). |
| `-people:Family` | Hide threads that involve any contact in that group. |
| `tag:Holiday` | Threads that have that thread tag. |
| `-tag:Holiday` | Hide threads that have that tag. |

`group:` on the **contact** list still means contact-group membership (`group:Family`, `group:none`).

`group:none` on **message** search still means “one result row per matching message.” `is:group` still means multi-person chats. Do not reuse `group:` for thread tags.

Names with spaces use quotes: `tag:"Work Friends"`, `people:"Work Friends"`.

## Data

New SQLite tables (parallel to `contact_groups` / `contact_group_members`):

- `conversation_tags` — `id`, `account_id`, `name` (unique per account)
- `conversation_tag_members` — `conversation_id`, `tag_id`

Guest clone copies tags and memberships after conversations are copied.

Saved searches stay in `localStorage`. No server table for them.

## HTTP

Mirror contact-group routes:

- `GET/POST/PATCH/DELETE /v1/thread-tags`
- `GET /v1/thread-tags/members?name=`
- `POST /v1/conversations/tags` — `{ ids, name, enable }`

JSON field name is `tags` (list of strings), not `labels` or `groups`.

Reserved tag names include `threads`, `tags`, `tag`, `trash`, `contacts`, and the same collision list used for contact groups (`group`, `groups`, …).

## GUI files

- Rename Saved Groups heading and aria text to Saved searches.
- Rename the contact-group sidebar heading to Contact groups.
- Contact list: checkboxes + Groups menu over the checked set.
- Thread list: checkboxes + Tags menu.
- New Thread tags sidebar (create, rename, delete, click → `tag:<name>` on the thread list).
- Routes `/tag/:slug` and `/no-tag` optional; equivalent to setting the thread-list query to `tag:Name` or `-` empty tag set. Prefer `/tag/:slug` so the sidebar can show the active tag without parsing `q`.

## Errors

- Empty name, reserved name, duplicate name (case-insensitive): 400 or 409, same as contact groups.
- Unknown conversation id when tagging: 404.
- Guest accounts may tag; they already may edit.

## Testing

- Rust: create / rename / delete tag; membership add/remove; `list_conversations` with `tag:`, `-tag:`, `people:`, `-people:`.
- Web: slug helpers and reserved names for tags (same pattern as `contactGroups.test.ts`).
- Schema contract lists the new tables.

## Out of scope

- Tags on individual messages.
- Storing hand-picked thread ids as a saved search.
- Server-side saved searches.
- Renaming the demo-seed TOML `[labels]` section (that is vCard category rates; `[groups]` already means chat groups).
