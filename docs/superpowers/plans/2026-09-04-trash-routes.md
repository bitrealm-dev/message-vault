# Trash Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A person can put a conversation or a contact in the trash and take it
back out, which today they cannot do at all.

**Architecture:** Four routes — trash and restore, for a conversation and for a
contact — over a `db/trash` module that owns the two marker tables and the
account purge. `trashed_handles` is deleted: a handle is not a thing anyone
trashes, and the column only ever made the Conversations list disagree with
itself. The web gets a mutation pair per kind, a "Move to trash" action on the
conversation header and the contact drawer, and "Restore" on the Trash screen.

**Tech Stack:** Rust (Axum, sqlx over SQLite and Postgres, utoipa), TypeScript
(React 19, TanStack Query, Vitest, Biome).

**Spec:** `docs/superpowers/specs/2026-09-03-http-interface-repair-design.md`,
section "Trash". Pull request 5 of the eight in
`docs/superpowers/plans/2026-09-03-http-interface-repair-roadmap.md`.

## What is actually true today

**Nothing can be trashed.** Every `INSERT INTO trashed_conversations`,
`trashed_contacts` and `trashed_handles` in the tree is in test code — verified
with `grep -rn 'INSERT INTO trashed_' crates/vault/server/src`. There is no
route, no CLI path, and no web action that writes any of them.

So the whole trash feature is read-only and permanently empty in production:
the search language answers `trashed:yes`, the Conversations and Contacts lists
filter trashed rows out by default, the Trash screen counts what is in there and
reports "Trash is empty", and it always will. This pull request is what makes
the feature real, and that is worth saying plainly in its description.

## Global Constraints

- **ADR-0005.** Every route answers in the one shape. After **every** server
  change regenerate both artifacts and verify:
  ```bash
  cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
  (cd web && npm run gen:api)
  ./scripts/check-generated-api-types.sh
  ```
- **ADR-0002.** One way to fetch data in `web/`: TanStack Query over
  `web/src/lib/vaultApi.ts`, keys built in `vaultKeys.ts` and nowhere else. A
  mutation invalidates by prefix; it does not hand-maintain a cache.
- **A schema change means bumping `SCHEMA_VERSION`** in
  `crates/vault/server/src/db/schema.rs`. Every vault is rebuilt empty and
  re-imported. That is intended — no migration, no shim, no deprecation path.
- **Every column in `schema/sql/*.sql` carries a comment**
  (`scripts/check-sql-column-comments.mjs`).
- **Trash is a property of the Conversation.** Trashing a message and trashing
  a conversation are the same operation; the conversation is always the unit.
- **Verification.** `./scripts/check-pr.sh` passes on the head commit.

## Decisions this plan makes

**"Stops appending `NOT trashed` unconditionally" means gating, not removing.**
The spec's sentence about the Messages list kind is about the word
*unconditionally*. `search/emit.rs` gates the filter on `!uses("trashed")` for
Contacts and Conversations, but appends it for Messages no matter what — so
`trashed:yes` is unanswerable on the Messages list, and Export cannot be asked
for a trashed conversation's messages even explicitly. Messages gets the same
gate the other two have. The default is unchanged: a query that does not mention
`trashed:` still excludes trashed conversations, so no download quietly gains
content.

**`trashed_handles` goes.** A handle is an address, not something a person puts
in the bin, and the table exists only because the Conversations list once
trashed by chat handle. Keeping it means two ways to hide one conversation, and
the conversation list and the contact drawer already apply them with different
SQL (`NOT_TRASHED_CONVERSATION` in `search/emit.rs`, `NOT_TRASHED_CHAT_HANDLE_SQL`
in `contacts_api.rs`). Dropping it is a schema bump, which is free here.

## File Structure

**Created**

- `crates/vault/server/src/db/trash.rs` — the two marker tables, the four
  operations, and the account purge that `db/account_profile.rs` does today.
- `web/src/lib/trash.ts` — the two mutation pairs.

**Modified — server**

- `crates/vault/server/src/db/mod.rs` — register the module.
- `crates/vault/server/src/conversations_api.rs` — two handlers; drop
  `NOT_TRASHED_CHAT_HANDLE_SQL` uses.
- `crates/vault/server/src/contacts_api.rs` — two handlers; drop
  `NOT_TRASHED_CHAT_HANDLE_SQL`.
- `crates/vault/server/src/openapi.rs` — register the four routes.
- `crates/vault/server/src/server.rs` — banner lines.
- `crates/vault/server/src/search/emit.rs` — `NOT_TRASHED_CONVERSATION` loses
  its `trashed_handles` clause; the Messages arm gains the `uses("trashed")`
  gate.
- `crates/vault/server/src/db/account_profile.rs` — the purge moves out.
- `crates/vault/server/src/db/schema.rs` — `SCHEMA_VERSION` bumped.
- `schema/sql/contacts.sql` — `trashed_handles` removed.

**Modified — web**

- `web/src/lib/vaultApi.ts` — four route functions.
- `web/src/screens/message/ConversationHeader.tsx` — "Move to trash".
- `web/src/components/ContactDrawer.tsx` — "Move to trash".
- `web/src/screens/TrashScreen.tsx` — "Restore".

---

### Task 1: A `db/trash` module owning both tables

**Files:**
- Create: `crates/vault/server/src/db/trash.rs`
- Modify: `crates/vault/server/src/db/mod.rs`, `crates/vault/server/src/db/account_profile.rs`

**Interfaces:**
- Produces: `crate::db::trash::{trash_conversation, restore_conversation, trash_contact, restore_contact}`, each
  `async fn(conn: &mut AnyConnection, account_id: &str, id: i64) -> Result<bool, sqlx::Error>`
  returning `false` when the id is not this account's, and
  `pub async fn purge_account(conn, account_id) -> Result<(), sqlx::Error>`.
  Task 2 calls the four; Task 1 itself moves the purge.

Returning `bool` rather than an error type is deliberate: "not this account's"
is the only failure that is not a database fault, and the handler turns it into
404. Trashing something already trashed returns `true` — idempotent, per the
spec.

- [ ] **Step 1: Write the failing tests**

In the new module's `mod tests`, against a real pool:

- trashing a conversation this account owns returns `true` and the row appears
  in `trashed_conversations`
- trashing it again returns `true` and there is still exactly one row
- restoring removes the row and returns `true`
- restoring something not trashed returns `true` and changes nothing
- an id belonging to another account returns `false` for all four, and writes
  nothing to either table
- the same five for a contact
- `purge_account` removes that account's rows from both tables and leaves
  another account's alone

- [ ] **Step 2: Run and watch them fail**

`cargo test -p message-vault-server db::trash`

- [ ] **Step 3: Write the module**

Each operation checks ownership and acts in one statement where it can. A
conversation is this account's when a row in `conversations` has that id and
`account_id`; a contact likewise in `contacts`. Do the ownership check
explicitly rather than relying on an insert failing, so restoring something that
was never trashed can still tell "not yours" from "not trashed".

Move `purge_account`'s trash deletes out of `db/account_profile.rs` and call
this module from wherever that purge runs. Read the surrounding function first:
if it deletes other tables too, only the trash tables move.

- [ ] **Step 4: Run the tests, then the suite**

`cargo test -p message-vault-server`

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/db/
git commit -m "feat(vault-server): a trash module owning the marker tables"
```

---

### Task 2: The four routes

**Files:**
- Modify: `crates/vault/server/src/conversations_api.rs`, `crates/vault/server/src/contacts_api.rs`,
  `crates/vault/server/src/openapi.rs`, `crates/vault/server/src/server.rs`

**Interfaces:**
- Consumes: Task 1's four functions.
- Produces: `POST /v1/conversations/{id}/trash`, `POST /v1/conversations/{id}/restore`,
  `POST /v1/contacts/{id}/trash`, `POST /v1/contacts/{id}/restore`, each 204 or 404.
  Task 5 calls them.

Routes are registered in `openapi.rs`'s `api_openapi()`, not `server.rs` —
`server.rs` carries only the startup banner lines. Follow how the conversation
read routes did it.

- [ ] **Step 1: Write the failing HTTP tests**

Through the crate's existing HTTP helpers, for each of the four:

- 204 for the account's own id, and the effect is visible — a trashed
  conversation drops out of the conversations list, a restored one comes back
- 204 again on a repeat, with no second row
- 404 for an id that does not exist
- **404 for an id belonging to another account, seeded as a real second account
  with a real conversation or contact** — not merely a missing id. A 403 would
  confirm the id exists.
- the write requires the same auth every other mutating route requires

- [ ] **Step 2: Run and watch them fail**

`cargo test -p message-vault-server trash`

- [ ] **Step 3: Write the handlers**

Four thin handlers over Task 1's functions: `false` → 404 with the sentence the
neighbouring 404s use, `true` → 204. Match the `#[utoipa::path]` shape of the
handlers beside them — tag, security, path parameter, documented responses —
and add the four banner lines.

- [ ] **Step 4: Verify, regenerate, commit**

```bash
cargo test -p message-vault-server
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
(cd web && npm run gen:api)
./scripts/check-generated-api-types.sh
git add crates/ docs/src/assets/openapi.json web/src/lib/vaultApi.types.ts
git commit -m "feat(api): put a conversation or a contact in the trash, and take it back"
```

---

### Task 3: `trashed_handles` is deleted

**Files:**
- Modify: `schema/sql/contacts.sql`, `crates/vault/server/src/db/schema.rs`,
  `crates/vault/server/src/search/emit.rs`, `crates/vault/server/src/contacts_api.rs`,
  `crates/vault/server/src/conversations_api.rs`, `crates/vault/server/src/db/trash.rs`

**Interfaces:**
- Consumes: Task 1's module, whose purge loses one table.

A handle is an address, not something a person bins. The table exists only
because the Conversations list once trashed by chat handle, and keeping it means
two ways to hide one conversation — applied with different SQL in different
files, which is how they come to disagree.

- [ ] **Step 1: Remove the table and bump the schema**

Delete the `trashed_handles` block from `schema/sql/contacts.sql` and set
`SCHEMA_VERSION` to the next number.

- [ ] **Step 2: Run the tests to see what breaks**

`cargo test -p message-vault-server 2>&1 | tail -40`

Expect failures wherever the column is read. Record the list.

- [ ] **Step 3: Remove every reader**

- `search/emit.rs`: `NOT_TRASHED_CONVERSATION` loses its second `NOT EXISTS`,
  keeping only the `trashed_conversations` clause.
- `contacts_api.rs`: `NOT_TRASHED_CHAT_HANDLE_SQL` and every use.
- `conversations_api.rs`: any use, and the test at roughly line 873 that seeds
  `trashed_handles` — that test asserts `trashed:yes` includes handle-trashed
  threads, which is behaviour being removed, so the test goes with it rather
  than being rewritten to assert something else.
- `db/trash.rs`: the purge drops that table.

- [ ] **Step 4: Verify**

```bash
grep -rn trashed_handles crates/ schema/ web/src docs/src
cargo test -p message-vault-server
node scripts/check-sql-column-comments.mjs
```
Expected: the grep returns nothing; tests pass.

- [ ] **Step 5: Commit**

```bash
git add schema/ crates/
git commit -m "refactor(vault-server): delete trashed_handles; a handle is not something you bin"
```

---

### Task 4: The Messages list stops forcing `NOT trashed`

**Files:**
- Modify: `crates/vault/server/src/search/emit.rs`

`search/emit.rs`'s `ListKind::Contacts` and `ListKind::Conversations` arms both
gate their trash filter on `!uses("trashed")`, so a person can ask for trashed
rows explicitly. The `ListKind::Messages` arm appends it unconditionally, so
`trashed:yes` is unanswerable there and Export cannot be asked for a trashed
conversation's messages even when asked directly.

**The default does not change.** A query that does not mention `trashed:` still
excludes trashed conversations, so nothing a person downloads today quietly
gains content tomorrow.

- [ ] **Step 1: Write the failing tests**

In `crates/vault/server/src/search/tests.rs`, on the Messages list:

- a query with no `trashed:` term excludes a trashed conversation's messages —
  this pins the unchanged default and must pass before and after
- `trashed:yes` returns only the trashed conversation's messages
- `trashed:any` returns both

- [ ] **Step 2: Run and watch the second and third fail**

`cargo test -p message-vault-server search::tests`

- [ ] **Step 3: Add the gate**

Wrap the Messages arm's trash `EXISTS` in `if !uses("trashed")`, matching the
two arms above it. Check `fields.rs` registers `trashed` for the Messages list
too — if it does not, register it, or the new tests fail on an unknown field
rather than on the filter.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p message-vault-server
git add crates/vault/server/src/search/
git commit -m "fix(search): the Messages list answers trashed: like the other two"
```

---

### Task 5: The web can trash and restore

**Files:**
- Modify: `web/src/lib/vaultApi.ts`, `web/src/lib/vaultKeys.ts`
- Create: `web/src/lib/trash.ts`

**Interfaces:**
- Consumes: Task 2's routes.
- Produces: `useTrashConversation`, `useRestoreConversation`, `useTrashContact`,
  `useRestoreContact`. Task 6 uses them.

Follow `web/src/lib/messageTags.ts` and `web/src/lib/contactGroups.ts` — those
are this app's existing feature modules wrapping a mutation with its
invalidation, and a new one should be indistinguishable in shape.

**Invalidate the right prefixes.** Trashing a conversation changes the
conversations list, the trash count, and the contacts list (a contact's
conversation counts move). Use the narrowest prefix that is actually stale, and
say in your report which you chose for each mutation and why.

> **Amended after the fact.** This paragraph originally claimed that
> `keys.conversations.messagesAll` and `keys.conversations.details` — two prefix
> handles pull request 4 added with no caller — were needed here. They were not,
> and commit `7f30cb5a` deleted both. Trashing a conversation changes neither
> its detail response nor its messages: `GET /v1/conversations/{id}` answers the
> same summary whether or not the conversation is trashed, and the marker never
> touches a message row. What the conversation pair actually invalidates is
> `conversations.lists`, `trash.all` and `contacts.details`; the contact pair
> invalidates `contacts.lists` and the one `contacts.detail(id)` it names. See
> the comments in `web/src/lib/trash.ts` for the reasoning per prefix.

- [ ] **Step 1: Add the four route functions and any missing key**

- [ ] **Step 2: Write `trash.ts` with the two pairs**

- [ ] **Step 3: Test the invalidation, not just the call**

A test that asserts the mutation function ran proves nothing. Assert that after
a successful trash, the queries that should be stale are, and the ones that
should not be are not — the existing `nameCollection.test.tsx` and
`contactGroups` tests show the shape this repo uses for that.

- [ ] **Step 4: Verify and commit**

```bash
cd web && npm run lint && npx vitest run src/lib
git add web/src/lib
git commit -m "feat(web): trash and restore mutations"
```

---

### Task 6: The actions a person can see

**Files:**
- Modify: `web/src/screens/message/ConversationHeader.tsx`,
  `web/src/components/ContactDrawer.tsx`, `web/src/screens/TrashScreen.tsx`

**Interfaces:**
- Consumes: Task 5's four hooks.

- [ ] **Step 1: "Move to trash" on the conversation header**

Follow the header's existing action affordances rather than inventing one. After
a successful trash the person is looking at a conversation that is no longer in
the list they came from — navigate back to the conversations list rather than
leaving them on a thread that has quietly left it.

- [ ] **Step 2: "Move to trash" on the contact drawer**

Same, in the drawer's existing action area. After trashing, close the drawer.

- [ ] **Step 3: "Restore" on the Trash screen**

The Trash screen currently reports only a count. Restoring needs a target, so a
selected trashed conversation is the unit — the screen's own copy already says
"Select one on the left to view it". Put Restore where the person is looking
after selecting, and make the empty state still make sense when nothing is
selected.

**Product copy states what the product does; it does not warn or hedge.** Trash
is reversible and permanent delete is a separate thing that does not exist yet
(#314), so no copy here should suggest anything is about to be destroyed.

> **Amended after the fact: this task specified an action with no inverse.**
> Step 2 gave the contact drawer "Move to trash" while Step 3 defined the Trash
> screen's unit as a selected trashed *conversation*, so nothing restored a
> contact. `useRestoreContact` had no caller, and the drawer could not be the
> way back either — `GET /v1/contacts/{id}` is trash-gated, so opening a trashed
> contact 404s and the row found by searching `trashed:yes` on the Contacts list
> led nowhere.
>
> The whole-branch review caught it and the fix wave closed it: the Trash screen
> now has two sections. **Conversations** keeps the left column, the `tsel`
> selection and the Restore panel unchanged. **Contacts** is a list in the pane
> itself, one row per trashed contact with its own Restore, because a row is the
> only surface a trash-gated detail route leaves available. Both sections read
> the header search term. When neither holds anything the screen says "Trash is
> empty." and shows no headings.
>
> The trashed contacts are a `useVaultQuery` over the contact list route with
> `trashed:yes`, keyed as `keys.contacts.trashed(q)`. That key sits under the
> `contacts.lists` prefix so `useRestoreContact`'s invalidation reaches it, but
> it is deliberately not `keys.contacts.list(q)`: the contact list screen holds
> that entry as paged `InfiniteData` and this one holds a single page, and two
> shapes must not share a key.
>
> **The lesson for later plans in this series:** a task that adds a way to put
> something into a state owes the same task a way out of it. Check the pair, not
> the action.

- [ ] **Step 4: Tests**

Each of the three: the action is present, invoking it calls the right mutation,
and the follow-through happens — navigation, drawer close, or the row leaving
the trash list.

- [ ] **Step 5: Verify and commit**

```bash
cd web && npm run lint && npm test && npm run build
git add web/src
git commit -m "feat(web): move to trash, and restore"
```

---

### Task 7: The pull request

- [ ] **Step 1:** `./scripts/check-pr.sh` — expect `All pre-PR checks passed.`

- [ ] **Step 2: Confirm the roadmap's Done-when**

```bash
grep -rln trashed_handles crates/vault/server
grep -rn 'INSERT INTO trashed_' crates/vault/server/src | grep -v tests
```
Expected: the first returns nothing; the second now shows the trash module,
where before this pull request it showed only test code.

- [ ] **Step 3:** Open the pull request, wait for CI, squash-merge.

- [ ] **Step 4:** Update the roadmap's Status table on a branch — row 5 merged
with its number, row 6 **next**, this plan added, and anything carried forward
written into the row that inherits it.

## Self-Review

**Spec coverage.** Four routes with 204/404 and HTTP tests → Task 2; the `db/`
trash module owning both tables and the account purge → Task 1; `trashed_handles`
dropped from the schema and every query → Task 3; the Messages list kind → Task
4; the web's mutation pairs, header and drawer actions, and Trash-screen restore
→ Tasks 5 and 6.

**One thing the spec leaves open.** It says the Trash screen's select "opens the
conversation through the read route" — that already works, because the Trash
screen lists through the shared conversation list and PR 4 put the message
screen on the read route. No task is needed; noted so it is not re-litigated.

**Risk to watch.** Task 3 removes a filter from queries several screens share.
The failure mode is a trashed conversation reappearing somewhere, or a
non-trashed one vanishing, in a list nobody wrote a test for. Task 3's reviewer
should check every caller of the two constants, not only that the suite is green.
