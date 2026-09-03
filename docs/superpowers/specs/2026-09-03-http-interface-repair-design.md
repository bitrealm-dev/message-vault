# HTTP interface repair: read routes, one convention, trash, names

Date: 2026-09-03. Status: agreed with the maintainer after an architecture
review of `crates/vault/server/` and `web/src/`. Decisions are recorded in
ADR-0005 (one shape for every route) and ADR-0006 (an import names the
Contact). Vocabulary is CONTEXT.md's: Conversation, Contact, Handle, Trash,
Unknown, Import Run, Export.

## What is wrong today

Verified at commit `14c7c419` against a throwaway vault:

1. The interface has no route for reading one Conversation. The web reads
   messages through `GET /v1/export/messages?q=in:#id` and finds a
   conversation by paging `GET /v1/conversations` until the id appears
   (`web/src/lib/fetchConversationById.ts`). A trashed conversation cannot be
   read at all: the Messages list always appends `NOT trashed`
   (`search/emit.rs:58`), and the Trash screen navigates into that dead end.
2. Nothing on the interface writes the trash. Every `INSERT` into
   `trashed_conversations`, `trashed_handles`, `trashed_contacts` is in a
   test module. `trashed_handles` is read by nothing a person can reach.
3. Every route file chose its own paging, envelope, list key, id type, and
   strictness (ADR-0005 lists them). `source=` means a directory slug on
   import and assets and a three-word set on export.
4. An import names nobody. Every imported Contact is "(unknown)"; the
   backup's name lives in `participants.name_alias` and a first-wins,
   read-only copy in `contact_handles.name_alias`. The conversation list and
   the message pane resolve names with different joins and precedence
   (`conversations_api.rs:354`, `export_api.rs:571`), so one person shows two
   names.
5. Every user-caused import failure is a bare 500. `run_import_path` returns
   `anyhow::Error`, which `server.rs:350` maps to "internal server error".
   `POST /v1/import` requires `source` in the query deserializer, so a missing
   value is an Axum plain-text 400 rather than the JSON error the code at
   `import/mod.rs:1443` was written to produce. `contacts_api.rs:1224` does the
   mirror thing: every error, sqlx included, becomes a 400.
6. `SCHEMA_VERSION` is 4 (`crates/libs/ir/src/lib.rs:29`, since #286).
   CLAUDE.md, AGENTS.md, and `docs/src/content/docs/vault/developer/message-transfer.md`
   say 3. The demo staging files are untracked output and regenerate at 4; they
   are not a defect.
7. The web builds search strings in five places with three quoting rules
   (`nameCollection.ts:100`, `AppLayout.tsx:33`, `buildAdvancedQuery.ts:87,101`,
   `useConversationMessages.ts:28,110`). Tests fake the route functions, so a
   string the server refuses passes every test; `vaultApi.test.ts:118`
   asserts `in:#A`, which the server reads as text. `vault-pull`'s
   `compose_query` emits `after:`/`before:`, words that no longer exist, and
   has no caller. `docs/.../developer/reference/api.md:44` lists eight deleted
   words.
8. The message screen keeps its own request state (`useConversationMessages.ts`),
   catches every error into `setMessages([])`, has no empty state
   (`MessageThread.tsx:36`), renders `message.reactions` (never sent) and
   ignores `tapbacks` (always sent). `web/tsconfig.json` excludes `*.test.ts`
   from type checking, so test fixtures drift from the generated types.
9. The message-reading route has zero route-level tests; eleven `setup()`
   functions in three idioms mean most server tests enter below the
   interface.
10. `contact_groups_api.rs` and `message_tags_api.rs` are the same 149 lines
    modulo a spec function, both pass-throughs over `named_set_api.rs`, itself
    a thin layer over `named_membership.rs`.

## The design

### Interface convention (ADR-0005)

- A thing: `GET /v1/<collection>/{id}` returns the thing as itself.
- A list: `?offset=&limit=` in, `{items, total, limit, offset}` out. `limit`
  above the route's cap and `offset` above `MAX_LIST_OFFSET` are 400s, not
  clamps. One `Page<T>` type on the server (`page_limits.rs` grows into the
  paging module) and one `Page<T>` in the generated web types.
- A failure: `{error: "<sentence>"}` with the status. Axum rejections
  (`Query`, `Path`, `Json`) are mapped into the same body by a custom
  rejection handler in `server.rs`.
- No `ok` field anywhere. `ok: true` on auth and import responses goes with
  the rest.
- Every id is an integer. `ConversationSummary.id: String` becomes `i64`.
- `saved_searches_api.rs` drops `rename_all = "camelCase"`.
- `MAX_LIST_LIMIT` is one constant with one meaning; the contacts summaries
  body cap gets its own name.
- `source=` on `GET /v1/export/messages` and `/count` is removed. The
  `source:` search word covers it. `source=` on import and assets keeps
  meaning the directory slug and is validated in one place.
- Read routes take `FullAccess` (signed-in session). Token scopes unchanged.

### Conversation read routes

```
GET /v1/conversations/{id}
  → ConversationSummary (same shape as a list row, integer id, trash included)
  404 when the id is not this account's

GET /v1/conversations/{id}/messages?offset=&limit=&year=
  → Page<Message>, ascending by timestamp then sort_order
  year= narrows to one calendar year in the vault's stored offset
  404 for an unknown conversation; trashed conversations are readable
```

`Message` is today's `ExportMessage` with `conversation` and `tapbacks`
kept, `sender` resolved through the participant naming module. The Export
route keeps returning the same `Message` type so one type serves both; the
export-specific fields (`query`, `truncated`) live on Export's page only if
still needed after the cursor goes.

Implementation: a `conversations` module owning `get_conversation`,
`list_conversation_messages`, and the SQL both share with the list route.
`export_api.rs` keeps the search-driven page and count and calls the same
message-row loader.

The web:
- `vaultApi.ts` gains `getConversation(id)` and
  `listConversationMessages(id, {offset, limit, year})`; `exportMessages`
  and `countExportMessages` stay for the Export screen only.
- `vaultKeys.ts` gains `conversations.detail(id)` and
  `conversations.messages(id, params)`.
- `useConversationMessages.ts` is rewritten on `useVaultQuery`: no
  `useState`/`AbortController` of its own; TanStack owns "newest request
  wins". It exposes `data`, `error`, `isLoading`.
- `MessageRoute.tsx` reads the conversation through `conversations.detail`
  and uses `location.state` only as `placeholderData`, never as the source
  of truth. `fetchConversationById.ts` is deleted.
- `MessageThread.tsx` gets an empty state ("No messages in this
  conversation") and an error state that shows the server's sentence.
- `types.ts` loses the nine phantom fields; `ImessageBubble` and
  `DiscordBubble` render `tapbacks`.
- `tsconfig.json` type-checks `*.test.ts` too.
- Route tests: `MessageRoute`, `MessageView`, `MessageThread` each get a
  test; `useConversationMessages.test.tsx` uses a numeric id.

### Trash

```
POST /v1/conversations/{id}/trash     → 204
POST /v1/conversations/{id}/restore   → 204
POST /v1/contacts/{id}/trash          → 204
POST /v1/contacts/{id}/restore        → 204
```

Idempotent: trashing a trashed thing is 204. 404 for an id not in this
account. A `trash` module in `db/` owns the two tables and the account purge
that `account_profile.rs` does today. `trashed_handles` is dropped from the
schema and from every query that reads it (`contacts_api.rs:217-231`, the
`trashed:` emitter). The Messages list kind stops appending `NOT trashed`
unconditionally: trash is a property of the Conversation, applied by the
Conversations and Contacts lists, and the read route ignores it.

The web: `vaultApi.ts` gains four functions; a `useTrashConversation` /
`useRestoreConversation` mutation pair (and the contact pair) in a
`trash.ts` feature module invalidating `conversations.*`, `contacts.*`, and
`trash.*` keys; a "Move to trash" action on the conversation header and
contact drawer; "Restore" on the Trash screen. The Trash screen's select
opens the conversation through the read route.

### Names (ADR-0006)

Import:
- `ensure_contact_for_handle` creates the Contact with the backup's name when
  it creates one, `origin = import`. An existing nameless Contact whose
  `origin` is `import` gets the name; a Contact with any name is untouched.
- `seed_contact_handle_alias` and `contact_handles.name_alias` are deleted.
- `ContactNameMode` and the `contact_name_mode` parameter are deleted from
  the server, `vault-push`, and `src-tauri/src/commands/push.rs`.
- `participants.name_alias` keeps the backup's name for that conversation.

Reading: one `participant_names` module in `db/` with a single query:

```
name = COALESCE(NULLIF(trim(c.preferred_name), ''),
                NULLIF(trim(p.name_alias), ''),
                h.raw)
join: participants p LEFT JOIN contact_handles ch ON ch.handle_id = p.handle_id
     LEFT JOIN contacts c ON c.id = ch.contact_id
```

`p.contact_id` is not consulted for naming. `ConversationParticipant` becomes
`{name, handle, service, contact_id}`; `ExportParticipant` becomes the same
type. The contact drawer's read-only "Alias" column goes; the drawer shows
the Contact's name and the handle.

Address-book load (`contacts/address-book`) keeps #286's rule: it replaces
rows it owns and overwrites an `origin = import` name with the book's name.

### Import failures

`run_import_path` and its callees return `ImportError`, an enum:

| variant | status | sentence |
| --- | --- | --- |
| `SchemaVersion {found, expected}` | 400 | "This file is schema version 3; the vault reads version 4." |
| `Parse {file, line, cause}` | 400 | "Could not read line 12 of p123.jsonl: …" |
| anything else (disk, database, a bug) | 500 | "internal server error" (cause on stderr) |

A missing attachment is not a failure: the import counts it in
`assets_missing` and succeeds, as today. An account that does not match the
token is already a 403 from `resolve_import_account`. A missing or bad
`source` stays the handler's own 400 sentence.

`ImportQuery.source` gets `#[serde(default)]` so the missing case reaches the
handler instead of the query deserializer. `validate_source_id` is called once. `contact_mutate_handler` maps
`sqlx` errors to 500 and validation to 400. The startup banner prints the
real contract. CLAUDE.md, AGENTS.md, and the transfer doc say 4;
`test_support.rs:7` stops naming `accounts_api.rs`;
`conversations_api.rs:135` says `in:#<id>`.

### Query text on the web

One module `web/src/lib/searchQuery.ts` exports builders (`forGroup(name)`,
`forTag(name)`, `forHandle(handle)`, `forContact(id)`, `withKind(q, kind)`,
`year(q, year)`, `advanced(input)`) and one `quote(value)` that quotes on
whitespace, parentheses, or a quote, and escapes an embedded quote. Every
screen calls it; no template literal with `<word>:` survives outside it.

A test in that module writes every builder's output for a fixed set of
inputs to `tests/fixtures/search/web-queries.txt` (one per line, sorted) and
fails if the committed file differs. `crates/vault/server/src/search/tests.rs`
reads the same file and asserts every line parses on the list named in a
leading `contacts|conversations|messages\t` column. `vault-pull`'s
`compose_query` is deleted; `api.md` is rewritten from the field registry (the
docs test from #311 already covers the user page; the reference page joins
it).

### Tests and fixtures

`crates/vault/server/src/test_support.rs` becomes the one fixture:
`test_vault()` (schema, account, optional seeded conversations from a small
builder) and `test_vault_http()` (the router on top). The eleven `setup()`
functions are replaced by it. New and rewritten routes are tested through
HTTP; existing function-level tests move up where the function is not
otherwise public.

### Named sets

`contact_groups_api.rs` and `message_tags_api.rs` fold into `named_set_api.rs`
as one generic registration with the two specs. If utoipa's path macro
cannot take a generic, a small macro instantiates the handlers; the two files
still go.

## Out of scope

- Searching messages across conversations: #313.
- Permanent delete and Empty Trash: #314.
- The Contacts list's own conventions beyond paging and shape.

## Delivery

Eight pull requests, in this order, each leaving main working:

1. Import failures typed; `source` contract fixed; three docs say 4; stale
   comments fixed.
2. Interface convention: paging, shapes, errors, integer ids, `source=` off
   Export, camelCase off Saved Searches; web types regenerated; `vault-pull`
   on offset paging; `compose_query` deleted.
3. Names: import names the Contact; one loader; `contact_handles.name_alias`
   and `contact_name_mode` deleted; drawer updated.
4. Conversation read routes; message screen on TanStack Query with error and
   empty states; `fetchConversationById` deleted; tapbacks rendered; phantom
   fields deleted; tests type-checked.
5. Trash module, four routes, `trashed_handles` dropped, web actions.
6. Query text module, shared example file, `api.md` rewritten.
7. One fixture, route-level tests for Export and the read routes.
8. Named-set route files folded.
