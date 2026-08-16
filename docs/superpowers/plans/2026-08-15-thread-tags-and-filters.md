# Thread Tags and Contact-Group Filters Implementation Plan

> **For agentic workers:** Implement task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add thread tags on whole conversations, checkbox multi-assign for contacts and threads, rename Saved Groups to Saved searches, and filter threads with `tag:` / `people:`.

**Architecture:** Mirror `contact_groups` for tags (`conversation_tags` tables + `/v1/thread-tags`). Thread list filters live in `conversations_api::list_conversations`. Message export already honors `within:`/`label:`; add `people:` as an alias and `-people:` / `tag:` there too.

**Tech Stack:** Rust (`message-vault-server`), SQLite, React 19 + TypeScript in `web/`.

## Global Constraints

- GUI words: Contact groups, Saved searches, Thread tags.
- Tags apply to whole conversations only.
- Do not reuse `group:` for thread tags.
- Restart the vault after server changes (`docker compose restart vault`).

---

## File map

| File | Role |
|------|------|
| `schema/sql/messages.sql` | `conversation_tags` / `conversation_tag_members` |
| `tests/fixtures/schema/current-schema.json` | Schema contract |
| `crates/vault/server/src/thread_tags_api.rs` | CRUD + membership |
| `crates/vault/server/src/server.rs` | HTTP routes |
| `crates/vault/server/src/conversations_api.rs` | `tag:` / `people:` on thread list |
| `crates/vault/server/src/search_query.rs` | `people:` / `tag:` / minus forms |
| `crates/vault/server/src/export_api.rs` | Message-search SQL for those tokens |
| `crates/vault/server/src/guest_clone.rs` | Copy tags onto guest accounts |
| `web/src/lib/threadTags.ts` | Client + reserved names |
| `web/src/components/TagsMenu.tsx` | Assign menu (shared shape with GroupsMenu) |
| `web/src/components/ThreadTagsNav.tsx` | Sidebar list |
| `web/src/components/LeftPanel.tsx` | Headings + Thread tags nav |
| `web/src/screens/ContactList.tsx` | Checkboxes + multi-assign |
| `web/src/screens/ConversationList.tsx` | Checkboxes + Tags menu |
| `web/src/components/InfiniteOffsetList.tsx` | Optional checkbox column |
| `web/src/components/AppLayout.tsx` | `/tag/:slug` + pass tag filter |
| `docs/src/content/docs/how-to/search.md` | Tokens |
| `docs/src/content/docs/how-to/saved-searches.md` | Heading |

---

### Task 1: Schema and thread-tag API

- [ ] Add tables to `schema/sql/messages.sql` and `current-schema.json`.
- [ ] Add `thread_tags_api.rs` (copy the contact-groups pattern: list, create, rename, delete, members, set membership).
- [ ] Register routes on `/v1/thread-tags` and `/v1/conversations/tags`.
- [ ] Copy tags in `guest_clone.rs`.
- [ ] `cargo test -p message-vault-server thread_tags`

### Task 2: Thread-list and message-search tokens

- [ ] Parse `tag:`, `-tag:`, `people:`, `-people:` (quoted names) in `conversations_api`.
- [ ] SQL: EXISTS on `conversation_tag_members`; people uses contact-group member ids + `involves_contact` / same join as `within:`.
- [ ] Alias `people:` to `within` in `search_query.rs`; add `tag` / exclude fields; apply in `export_api.rs`.
- [ ] Tests for list filters.
- [ ] `cargo test -p message-vault-server list_conversations`

### Task 3: Web — names, checkboxes, menus

- [ ] Rename Saved Groups → Saved searches; Groups nav heading → Contact groups.
- [ ] `threadTags.ts` + tests; `useThreadTags`; `TagsMenu`; `ThreadTagsNav`.
- [ ] Checkboxes on contact list; Groups menu uses the checked set (or the highlighted row).
- [ ] Checkboxes + Tags menu on conversation list.
- [ ] Routes `/tag/:slug` and `/no-tag`.
- [ ] Docs: search tokens and saved-searches heading.
- [ ] `cd web && npm test -- --run src/lib/threadTags.test.ts && npx tsc --noEmit`

## Done when

- Sidebar shows Contact groups, Saved searches, Thread tags.
- Checking several contacts or threads and using the menu updates membership.
- `tag:Holiday` and `people:Family` filter the thread list.
- Vault restart applies the new tables.
