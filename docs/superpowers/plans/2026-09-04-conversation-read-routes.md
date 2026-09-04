# Conversation Read Routes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reading a conversation gets its own two routes, so Export goes back to
being the download path only and no screen reads through it.

**Architecture:** The message-row loading inside `export_messages` becomes a
shared loader both routes call. `GET /v1/conversations/{id}` answers the same
`ConversationSummary` the list returns, and `GET /v1/conversations/{id}/messages`
answers a page of them. On the web, `useConversationMessages` is rewritten on
TanStack Query, `fetchConversationById` — which scanned list pages until it found
one conversation — is deleted, and the message screen stops building `in:#id`
search queries.

**Tech Stack:** Rust (Axum, sqlx over SQLite and Postgres, utoipa), TypeScript
(React 19, TanStack Query, Vitest, Biome).

**Spec:** `docs/superpowers/specs/2026-09-03-http-interface-repair-design.md`,
section "Conversation read routes". This is pull request 4 of the eight in
`docs/superpowers/plans/2026-09-03-http-interface-repair-roadmap.md`.

## Global Constraints

- **Export is the download button, never the path a screen reads by.** When this
  pull request is done, `grep -rn 'export/messages\|exportMessages\|countExportMessages' web/src --include=*.ts --include=*.tsx | grep -v '\.test\.'`
  lists only `web/src/lib/vaultApi.ts` and the Export screen.
- **ADR-0005.** Every route answers in the one shape. After **every** server
  change regenerate both artifacts and verify:
  ```bash
  cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
  (cd web && npm run gen:api)
  ./scripts/check-generated-api-types.sh
  ```
- **ADR-0002.** One way to fetch data in `web/`: TanStack Query over the route
  functions in `web/src/lib/vaultApi.ts`, with keys built in `vaultKeys.ts` and
  nowhere else. No `useState` + `AbortController` request bookkeeping of its
  own — TanStack owns "newest request wins".
- **ADR-0006.** One query names a participant:
  `crates/vault/server/src/db/participant_names.rs`. Both new routes use it. Do
  not add a second naming rule.
- **No migration, and data preservation is not a goal.** No schema change is
  needed here; if one becomes necessary, bump `SCHEMA_VERSION` and let vaults
  rebuild. Never write a migration or a compatibility shim.
- **Route shape:** `Page<T>` is `{items, total, limit, offset}` and already
  exists in `crates/vault/server/src/paging.rs`.
- **Verification.** `./scripts/check-pr.sh` passes on the head commit.

## Decisions this plan makes

**`handle` and `service` become optional on `Participant`.** Inherited from PR 3
(issue #320, item 1) and settled by Matt: `load_for_conversations` inner-joins
`handles`, so a participant the source named without recording an address never
appears in a conversation. This plan switches to a LEFT JOIN and makes both
fields nullable, so a caller can tell "no address" from "empty address". A
required string standing in for something that does not exist is the kind of
shape PR 3 spent its diff deleting.

**A year still loads in full, and each request still costs one `COUNT(*)`.** The
PR 2 review noted that every Export page runs a count before its row query, and
warned against paying that per page of a year walk. Selecting a year loads the
whole year today so that find-in-conversation searches across it, and this plan
keeps that. The route returns `total` from one count per request, which is the
contract; a 2000-message year costs four counts across four requests. Making the
year a single unbounded response would be the alternative, and it trades a
bounded request for an unbounded one. If the count cost shows up in practice,
the fix is to skip the count when `offset > 0` and carry the first page's total,
which is a later change, not this one. Recorded with its cost and four
ways out in issue #323.

## File Structure

**Created**

- `crates/vault/server/src/db/conversation_messages.rs` — the shared message-row
  loader: the `SELECT`, the `RawRow` mapping, and the attachment and tapback
  joins that both Export and the new read route need.

**Modified — server**

- `crates/vault/server/src/db/mod.rs` — register the module.
- `crates/vault/server/src/export_api.rs` — `ExportMessage` is renamed `Message`
  and moves to the shared module with `ExportConversation`, `ExportAttachment`
  and `ExportTapback`; `export_messages` keeps the search filter, the count and
  the page and calls the shared loader.
- `crates/vault/server/src/conversations_api.rs` — two new handlers and their
  utoipa annotations.
- `crates/vault/server/src/server.rs` — the two routes, and the startup banner.
- `crates/vault/server/src/db/participant_names.rs` — LEFT JOIN; `handle` and
  `service` become `Option<String>`.

**Modified — web**

- `web/src/lib/vaultApi.ts` — `getConversation`, `listConversationMessages`.
- `web/src/lib/vaultKeys.ts` — `conversations.detail`, `conversations.messages`.
- `web/src/screens/message/useConversationMessages.ts` — rewritten on
  `useVaultQuery`.
- `web/src/components/MessageRoute.tsx` — reads through `conversations.detail`;
  `location.state` becomes `placeholderData` only.
- `web/src/screens/message/MessageThread.tsx` — empty state and error state.
- `web/src/lib/types.ts` — the nine phantom fields go.
- `web/src/components/messages/ImessageBubble.tsx` and `DiscordBubble.tsx` —
  render `tapbacks`.
- `web/src/components/ConversationRow.tsx`, `chatBubbleShared.tsx`,
  `MessageView.tsx` — handle a nullable `handle`/`service`.
- `web/tsconfig.json` — type-check `*.test.ts*`.

**Deleted — web**

- `web/src/lib/fetchConversationById.ts` and `fetchConversationById.test.ts`.

---

### Task 1: One message-row loader, shared

**Files:**
- Create: `crates/vault/server/src/db/conversation_messages.rs`
- Modify: `crates/vault/server/src/db/mod.rs`, `crates/vault/server/src/export_api.rs`

**Interfaces:**
- Produces: `crate::db::conversation_messages::{Message, MessageConversation, Attachment, Tapback}` and
  `pub async fn load_messages(conn: &mut AnyConnection, where_sql: &str, params: &[SqlParam], limit: u32, offset: u32) -> Result<Vec<Message>, ApiError>`.
  Tasks 3 and 4 call `load_messages`; Task 2 does not.

`ExportMessage` is renamed `Message` because it is no longer Export's — both
routes return it. Its `conversation`, `attachments` and `tapbacks` fields keep
their shapes and their types are renamed to match (`ExportConversation` →
`MessageConversation`, `ExportAttachment` → `Attachment`, `ExportTapback` →
`Tapback`). This is a wire-shape rename: the OpenAPI schema names change and the
web's type aliases follow in Task 5.

- [ ] **Step 1: Move the types and the loader**

Move `ExportMessage`, `ExportConversation`, `ExportAttachment` and
`ExportTapback` out of `export_api.rs` into the new module under their new
names, keeping every field, every `#[serde(...)]` attribute and every doc
comment exactly as they are. Move `RawRow`, `load_attachments` and
`load_tapbacks` with them.

Write `load_messages` as the body of `export_messages` from the `let mut sql =`
line through the `.collect()` that builds the messages — everything except the
filter compilation, the count, and the `Page` construction, which stay in
`export_api.rs`. It takes the already-compiled `where_sql` and `params` so the
search filter stays Export's business and the conversation filter can be
Task 3's.

Register the module in `crates/vault/server/src/db/mod.rs` in alphabetical
order.

- [ ] **Step 2: Make `export_messages` call it**

`export_messages` keeps `message_filter`, `count_matching_messages` and the
`Page` construction, and replaces the moved body with one `load_messages` call.
Re-export the renamed types from `export_api.rs` if any caller outside the crate
needs them; check `crates/vault/server/src/lib.rs` for what it exports.

- [ ] **Step 3: Build and fix every caller the compiler names**

```bash
cargo build --workspace
```
Expected: errors naming every use of the four renamed types, including in
`crates/libs/vault-pull/` — that crate mirrors this wire shape and its structs
are named after it. Rename there too. Fix until clean.

- [ ] **Step 4: Run the tests**

```bash
cargo test -p message-vault-server
cargo test -p vault-pull
```
Expected: PASS. Export's behaviour is unchanged — this task only moves code.

- [ ] **Step 5: Regenerate and verify**

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
(cd web && npm run gen:api)
./scripts/check-generated-api-types.sh
```
Then update the type aliases in `web/src/lib/types.ts` to the new schema names
so the web still type-checks: `Message`, `MessageConversation`,
`MessageAttachment` and `MessageTapback` all point at the renamed schemas.

- [ ] **Step 6: Commit**

```bash
git add crates/ docs/src/assets/openapi.json web/src/lib/
git commit -m "refactor(api): one message-row loader for Export and the read route"
```

---

### Task 2: `GET /v1/conversations/{id}`

**Files:**
- Modify: `crates/vault/server/src/conversations_api.rs`, `crates/vault/server/src/server.rs`

**Interfaces:**
- Consumes: the existing `ConversationSummary` and its list query.
- Produces: `GET /v1/conversations/{id}` answering `ConversationSummary`, 404 when
  the id is not this account's. Task 6 calls it from the web.

The route answers the same shape as a list row so a caller that already has one
does not have to convert. A trashed conversation is readable — trash is a
property the list applies, not a gate on reading.

- [ ] **Step 1: Write the failing HTTP tests**

In `conversations_api.rs`'s `mod tests`, add tests through the crate's HTTP
helpers (follow whatever `get_raw`/JSON helper the file's existing route tests
use — do not invent a second way):

- a conversation this account owns returns 200 and a body whose `id` matches and
  whose `participants` carry names
- an id this account does not own returns 404
- an id belonging to another account returns 404, not 403 and not that account's
  conversation
- a trashed conversation returns 200

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -p message-vault-server conversation_detail
```
Expected: FAIL — no such route.

- [ ] **Step 3: Write the handler**

Add `conversation_detail_handler` beside `conversations_list_handler`, with the
same `#[utoipa::path]` shape the neighbouring handlers use: `get`, path
`/v1/conversations/{id}`, tag `Conversations`, a 200 with `ConversationSummary`
and a 404 with the error shape. Reuse the list's row query with a
`WHERE c.id = $1 AND c.account_id = $2` in place of the search filter, so one
SQL shape serves both and the participants come from
`crate::db::participant_names::load_for_conversations`. Return `None` → 404 with
the sentence the other 404s in this file use.

Wire the route in `server.rs` next to the conversation list, and add its line to
the startup banner.

- [ ] **Step 4: Run the tests**

```bash
cargo test -p message-vault-server conversation_detail
```
Expected: PASS.

- [ ] **Step 5: Regenerate, verify, commit**

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
(cd web && npm run gen:api)
./scripts/check-generated-api-types.sh
git add crates/ docs/src/assets/openapi.json web/src/lib/vaultApi.types.ts
git commit -m "feat(api): read one conversation by id"
```

---

### Task 3: `GET /v1/conversations/{id}/messages`

**Files:**
- Modify: `crates/vault/server/src/conversations_api.rs`, `crates/vault/server/src/server.rs`

**Interfaces:**
- Consumes: `crate::db::conversation_messages::load_messages` from Task 1.
- Produces: `GET /v1/conversations/{id}/messages?offset=&limit=&year=` answering
  `Page<Message>`, ascending by timestamp then `sort_order`. Task 5 calls it.

`year=` narrows to one calendar year in the vault's stored offset — the same
year the `date:YYYY` search term matches, so read the existing implementation of
that term in `crates/vault/server/src/search/` and use the same boundary
expressions rather than writing new ones. A mismatch here shows up as messages
missing from the last day of a year.

- [ ] **Step 1: Write the failing HTTP tests**

- a conversation's messages come back ascending by timestamp then `sort_order`
- `limit` and `offset` page, and `total` is the conversation's whole count, not
  the page's length
- `year=` narrows to that calendar year, and its `total` is the year's count
- a message on 31 December at 23:59 local is in that year and not the next
- an unknown id returns 404
- another account's conversation returns 404
- a trashed conversation's messages are readable
- a bad `limit` is refused the way the other paged routes refuse one

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -p message-vault-server conversation_messages
```
Expected: FAIL — no such route.

- [ ] **Step 3: Write the handler**

Build the `WHERE` as `m.conversation_id = $1` plus the account scope plus the
same not-trashed and not-duplicate clauses Export applies, plus the year bounds
when `year` is given, then call `load_messages`. Take `total` from one
`COUNT(*)` over the same `WHERE` before the row query — one count per request.

Return 404 before running the message query when the conversation does not
belong to the account, so an unknown id cannot be distinguished from another
account's by timing or by an empty page.

Wire the route and the banner line.

- [ ] **Step 4: Run the tests, regenerate, commit**

```bash
cargo test -p message-vault-server
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
(cd web && npm run gen:api)
./scripts/check-generated-api-types.sh
git add crates/ docs/src/assets/openapi.json web/src/lib/vaultApi.types.ts
git commit -m "feat(api): read a conversation's messages without the search language"
```

---

### Task 4: A participant with no address appears in the conversation

**Files:**
- Modify: `crates/vault/server/src/db/participant_names.rs`

**Interfaces:**
- Produces: `Participant.handle` and `Participant.service` become
  `Option<String>`. Task 7 handles the web fallout.

Settled by Matt, carried from PR 3 (issue #320, item 1).
`resolve_name_only_participant` creates a participant with `handle_id IS NULL`
whenever a source names someone without recording an address, and the inner
`JOIN handles` drops that person from every conversation they belong to. They
appear in Contacts and in search but never in the thread.

- [ ] **Step 1: Write the failing test**

In `participant_names.rs`'s `mod tests`, seed a participant with
`handle_id IS NULL`, a `name_alias`, and a `contact_id`, then assert
`load_for_conversations` returns them with `name` from the naming rule,
`handle: None` and `service: None`. Add a second test that a conversation with
one addressed and one address-less participant returns both, in participant-id
order.

- [ ] **Step 2: Run and watch it fail**

```bash
cargo test -p message-vault-server participant_names
```
Expected: FAIL — the row is absent.

- [ ] **Step 3: Change the join and the type**

`JOIN handles h ON h.id = p.handle_id` becomes `LEFT JOIN`. `handle` and
`service` become `Option<String>` on the struct, with doc comments saying they
are absent when the source named the person without recording any address.

The `COALESCE` for `name` currently ends at `h.raw`, which is now nullable — so
for an address-less participant the whole expression can be NULL. Keep `name` a
non-optional `String` by falling back to `''` at the end of the COALESCE and
letting the naming rule's second clause (`p.name_alias`) carry these people,
which it always does: a participant with neither an address nor a name is never
created (`resolve_name_only_participant` returns early). Assert that in the
test rather than trusting it.

Apply the same change to `load_for_chat_handle` in the same module if its join
has the same shape.

- [ ] **Step 4: Run, regenerate, commit**

```bash
cargo test -p message-vault-server
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
(cd web && npm run gen:api)
git add crates/ docs/src/assets/openapi.json web/src/lib/vaultApi.types.ts
git commit -m "feat(api): a participant with no address appears in their conversation"
```

The web build breaks here; Task 7 owns it.

---

### Task 5: The message screen reads its own route

**Files:**
- Modify: `web/src/lib/vaultApi.ts`, `web/src/lib/vaultKeys.ts`,
  `web/src/screens/message/useConversationMessages.ts`

**Interfaces:**
- Consumes: Task 3's route.
- Produces: `getConversation(id)`, `listConversationMessages(id, params)`,
  `keys.conversations.detail(id)`, `keys.conversations.messages(id, params)`.
  Task 6 uses `detail`.

- [ ] **Step 1: Add the route functions and the keys**

In `vaultApi.ts`, add `getConversation` and `listConversationMessages` beside the
existing conversation functions, using the same request helper and the generated
types. Leave `exportMessages` and `countExportMessages` alone — the Export screen
still needs them.

In `vaultKeys.ts`, under the `conversations` namespace:

```ts
    details: ["conversations", "detail"] as const,
    detail: (id: number) => ["conversations", "detail", String(id)] as const,
    messages: (id: number, p: { offset: number; limit: number; year: number | null }) =>
      ["conversations", "messages", String(id), p.offset, p.limit, String(p.year)] as const,
```

- [ ] **Step 2: Rewrite `useConversationMessages` on TanStack**

Delete the `useState`/`useRef`/`AbortController` bookkeeping and the
`isAbortError` helper — TanStack owns newest-request-wins. Keep
`conversationYears`, `displaySourceLabel` and `buildFooterLabel` exactly as they
are; they are pure and tested.

The hook keeps `offset`, `activeYear`, `findTerm` and `activeMatch` as local
state — they are view state, not server state — and reads messages with
`useVaultQuery` on `keys.conversations.messages(...)`. A year still loads in
full: keep `fetchAllMessagesForQuery`'s walk, but inside the query function, so
one query entry owns the whole year. Expose `data`, `error` and `isLoading`
alongside the existing names the screen already uses, so Task 6 and Task 7 can
show an error state.

- [ ] **Step 3: Update the hook's tests**

`useConversationMessages.test.tsx` uses a numeric id already (PR 2). Point its
mocks at `listConversationMessages` and wrap the hook in the test QueryClient
provider the repo already has (`web/src/test/vaultProviders.tsx`). Do not weaken
an assertion to fit — if a test's meaning changes, say so.

- [ ] **Step 4: Verify and commit**

```bash
cd web && npx vitest run src/screens/message
git add web/src
git commit -m "refactor(web): the message screen reads the conversation route"
```

---

### Task 6: `MessageRoute` stops scanning list pages

**Files:**
- Modify: `web/src/components/MessageRoute.tsx`
- Delete: `web/src/lib/fetchConversationById.ts`, `web/src/lib/fetchConversationById.test.ts`

**Interfaces:**
- Consumes: `getConversation` and `keys.conversations.detail` from Task 5.

`fetchConversationById` loads list pages 100 at a time until it finds one
conversation — for a vault with 3000 conversations, opening the thirtieth page's
thread costs thirty list requests. Task 2's route replaces it with one.

- [ ] **Step 1: Replace the effect with a query**

Delete the `useState`/`useEffect`/`AbortController` block and read the
conversation with `useVaultQuery` on `keys.conversations.detail(conversationId)`,
`enabled` only when `conversationId !== null`. Pass the router's
`location.state` conversation as `placeholderData` rather than as the source of
truth, so a stale row from a previous list page cannot outlive a refetch.

Keep the three rendered states — loading, error, and "select a conversation" —
and take the error's text from the server's sentence rather than `String(e)`.

- [ ] **Step 2: Delete the scanner**

```bash
cd web && git rm src/lib/fetchConversationById.ts src/lib/fetchConversationById.test.ts
```

- [ ] **Step 3: Add a route test**

`MessageRoute` has no test today. Add one covering: a conversation id in the URL
renders the thread; an id the server 404s on renders the not-found state with a
link back; no id renders the empty pane.

- [ ] **Step 4: Verify and commit**

```bash
cd web && npx vitest run src/components/MessageRoute.test.tsx
git add web/src
git commit -m "refactor(web): open a conversation with one request instead of a page scan"
```

---

### Task 7: The thread says what happened, and the types stop lying

**Files:**
- Modify: `web/src/screens/message/MessageThread.tsx`, `web/src/lib/types.ts`,
  `web/src/components/messages/ImessageBubble.tsx`, `DiscordBubble.tsx`,
  `web/src/components/ConversationRow.tsx`,
  `web/src/components/messages/chatBubbleShared.tsx`,
  `web/src/screens/MessageView.tsx`, `web/tsconfig.json`

**Interfaces:**
- Consumes: Task 4's nullable `handle`/`service`, Task 5's `error`.

- [ ] **Step 1: Type-check the tests, and see what breaks**

In `web/tsconfig.json`, delete the `"exclude": ["src/**/*.test.ts"]` line so
tests are type-checked too.

```bash
cd web && npx tsc --noEmit
```
Record the full error list. It will include Task 4's nullable fields and
whatever the previously-unchecked test files were getting away with.

- [ ] **Step 2: Delete the nine phantom fields**

In `types.ts`, `Message` is `Schema["Message"]` intersected with nine optional
fields the vault has never sent — `reactions`, `reply_to_message`, `embeds`,
`edit_history`, `deleted_indicator`, `effect`, `role_color`, `is_story_reply`,
`forwarded`. Delete the intersection and every branch that renders them, along
with the `Reaction`, `MessageRef`, `Embed` and `EditEntry` interfaces if nothing
else uses them. The comment above them says they were kept so that removing the
branches stayed a separate reviewable change; this is that change.

- [ ] **Step 3: Render tapbacks**

`ImessageBubble` and `DiscordBubble` rendered `reactions`, which never arrived.
The vault does send `tapbacks` on every message. Render those instead, using the
existing `MessageTapback` type. A tapback carries who sent it, so group by emoji
and show a count, the way the deleted `Reaction` branch did.

- [ ] **Step 4: Handle a participant with no address**

`ConversationRow` uses `p.handle` as a React key — with `handle` now nullable,
switch to a key that is always present. `chatBubbleShared`'s `senderName`
matches `p.handle === m.sender`; an address-less participant matches nothing,
which is correct, but check the fallback path still returns a name. `MessageView`
renders participant chips; show the name alone when there is no handle.

- [ ] **Step 5: Empty and error states**

`MessageThread` renders nothing distinguishable when a conversation has no
messages. Add "No messages in this conversation", and an error state that shows
the server's sentence from Task 5's `error` rather than a generic string. Add a
test for each.

- [ ] **Step 6: Verify and commit**

```bash
cd web && npm run lint && npm test && npm run build
git add web/
git commit -m "refactor(web): the thread renders what the vault actually sends"
```

---

### Task 8: The pull request

- [ ] **Step 1: Full check**

```bash
./scripts/check-pr.sh
```
Expected: `All pre-PR checks passed.`

- [ ] **Step 2: Confirm the roadmap's Done-when list**

```bash
grep -rn 'export/messages\|exportMessages\|countExportMessages' web/src --include=*.ts --include=*.tsx | grep -v '\.test\.'
ls web/src/lib/fetchConversationById.ts 2>&1
grep -n 'reactions\|tapbacks' web/src/lib/types.ts
```
Expected: the first lists only `vaultApi.ts` and the Export screen; the second
says no such file; the third shows `tapbacks` and no `reactions`.

- [ ] **Step 3: Open the pull request against main, wait for CI, squash-merge.**

- [ ] **Step 4: Update the roadmap's Status table on a branch** — row 4 merged
with its number, row 5 **next**, this plan added to "Plans so far", and anything
this pull request could not finish carried to the row that inherits it.

## Self-Review

**Spec coverage.** Both routes with 404 and `year=` → Tasks 2 and 3; the shared
`Message` type and loader → Task 1; `vaultApi` and `vaultKeys` additions → Task
5; `useConversationMessages` on TanStack with no `AbortController` → Task 5;
`MessageRoute` on `conversations.detail` with `location.state` as
`placeholderData` and `fetchConversationById` deleted → Task 6; `MessageThread`
empty and error states, the nine phantom fields, tapbacks rendering, and
`tsconfig` type-checking tests → Task 7; the PR 3 inheritance → Task 4.

**Not covered, and why.** The spec says the export-specific fields `query` and
`truncated` live on Export's page "if still needed after the cursor goes" — no
cursor is being removed in this pull request, so those fields stay untouched.
The spec also lists route tests for `MessageView`; Task 7 adds tests for
`MessageThread` and Task 6 for `MessageRoute`, and `MessageView` already has
coverage through the drawer tests — if the reviewer finds it does not, that is a
fair finding.

**Risk to watch.** Task 1 is a rename across a wire shape that `vault-pull`
mirrors. PR 3 shipped a silent data loss in exactly that crate because its copy
of the Export shape used `#[serde(default)]`; Task 1 Step 3 exists to catch the
same class of thing, and its reviewer should check `vault-pull` explicitly.
