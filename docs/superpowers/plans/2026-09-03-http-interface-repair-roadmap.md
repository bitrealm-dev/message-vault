# HTTP Interface Repair: Roadmap

The binding design is `docs/superpowers/specs/2026-09-03-http-interface-repair-design.md`.
It ships as eight pull requests, one at a time, in this order, each leaving
main working. This file is the one place that says which pull request is
next, what it still has to deliver, and what earlier reviews handed to it.

The spec was written before PR 1 and PR 2 changed the code. Where a section
below says "already done" or "changed since the spec", this file wins over
the spec's wording; for everything else, the spec wins.

## Status

| # | Pull request | Spec section | State |
| --- | --- | --- | --- |
| 1 | Import failures typed; `source` contract; schema docs say 4 | Import failures | merged, #316 |
| 2 | One shape for every route (ADR-0005) | Interface convention | merged, #317 |
| 3 | An import names the Contact (ADR-0006) | Names | **next** |
| 4 | Conversation read routes; message screen on TanStack Query | Conversation read routes | queued |
| 5 | Trash module and four routes | Trash | queued |
| 6 | One query builder on the web; shared example file | Query text on the web | queued |
| 7 | One test fixture; route-level tests | Tests and fixtures | queued |
| 8 | Named-set route files folded | Named sets | queued |

Plans so far: `2026-09-03-import-failures-and-schema-docs.md` (PR 1),
`2026-09-03-route-convention.md` (PR 2).

## How a pull request is delivered

1. Branch from main in a worktree.
2. Read this file's section for the pull request, then the spec section and
   the ADR it names, then run the section's inventory commands so the plan
   is written against the code as it is, not as the spec remembers it.
3. Write the plan with superpowers:writing-plans to
   `docs/superpowers/plans/<date>-<name>.md`. Its Global Constraints copy
   the spec's exact values plus the standing rules: ADR-0002 (one way to
   fetch data in `web/`), ADR-0005 (every route answers in the one shape;
   regenerate `docs/src/assets/openapi.json` and `web/src/lib/vaultApi.types.ts`
   after every server change), and "export is the download button, never
   the path a screen reads by".
4. Execute with superpowers:subagent-driven-development: a review after
   every task, a whole-branch review at the end, one fix wave.
5. `./scripts/check-pr.sh` passes on the head commit. Push, open the pull
   request against main, wait for CI, squash-merge with a conventional
   commit whose body says what changed and why in plain English.
6. Update the Status table: the merged row gets its number, the next row
   gets **next**, and the carried-over list below loses what shipped.

Done when step 6 is committed on main.

## PR 3: Names (ADR-0006)

Spec section "Names (ADR-0006)"; `docs/adr/0006-an-import-names-the-contact.md`.

Nothing in this section has shipped. Inventory before planning:

```
grep -rln 'name_alias\|contact_name_mode\|ContactNameMode' crates/vault/server/src crates/libs/vault-push/src src-tauri/src
grep -rn 'seed_contact_handle_alias\|ensure_contact_for_handle' crates/vault/server/src
```

Done when: the first grep returns only the participant loader's use of
`participants.name_alias`; one `db/participant_names` module owns the
naming query and every route that names a participant calls it;
`ConversationParticipant` and `ExportParticipant` are one
`{name, handle, service, contact_id}` type; the contact drawer shows the
Contact's name and the handle with no Alias column; the address-book rule
from #286 still holds and is tested; `check-pr.sh` passes.

Carried over: none.

## PR 4: Conversation read routes

Spec section "Conversation read routes".

Changed since the spec:

- Conversation ids are already numbers end to end in the web (PR 2).
- `useConversationMessages.ts` already reads a page's `items` and `total`
  and makes no count call, but it still reads through
  `GET /v1/export/messages` with an `in:#id` query. That is the defect this
  pull request removes: reading a conversation gets its own route, and
  Export goes back to being the download path only.
- Export answers `Page<ExportMessage>`; the spec's `Message` type is
  `ExportMessage` renamed and shared by both routes.

Inventory before planning:

```
grep -rn 'export/messages\|exportMessages\|countExportMessages' web/src --include=*.ts --include=*.tsx | grep -v '\.test\.'
grep -n 'reactions\|tapbacks' web/src/lib/types.ts
```

Done when: `GET /v1/conversations/{id}` and `GET /v1/conversations/{id}/messages`
are in `openapi.json` with HTTP-level tests including 404 and `year=`;
the first grep lists only the Export screen; `fetchConversationById.ts`
is gone; `MessageThread` has an empty state and an error state that shows
the server's sentence; tapbacks render; the nine phantom fields are out of
`types.ts`; `tsconfig.json` type-checks `*.test.ts*`; `check-pr.sh` passes.

Carried over from the PR 2 review: every Export page now runs `COUNT(*)`
before its row query, so the read route should take `total` from one
count per request and never per page of a year walk.

## PR 5: Trash

Spec section "Trash".

Inventory before planning:

```
grep -rln trashed_handles crates/vault/server
grep -rn 'NOT trashed\|trashed:' crates/vault/server/src
```

Done when: the four routes answer 204 (idempotent) or 404 with HTTP-level
tests; `trashed_handles` is out of the schema and the first grep returns
nothing; trash is a property of the Conversation applied by the
Conversations and Contacts lists and ignored by the read route; the web has
the two mutation pairs in `trash.ts`, "Move to trash" on the conversation
header and contact drawer, and "Restore" on the Trash screen;
`check-pr.sh` passes.

Carried over: none. Permanent delete and Empty Trash stay in #314.

## PR 6: Query text on the web

Spec section "Query text on the web".

Changed since the spec: `vault-pull`'s `compose_query` is already deleted
(PR 2). The `api.md` section "One shape for every route" written in PR 2
stays when the page is rewritten from the field registry.

Inventory before planning (broad on purpose; comments and prose match too):

```
grep -rln 'in:#\|group:\|tag:\|contact:\|handle:\|kind:\|date:' web/src --include=*.ts --include=*.tsx | grep -v '\.test\.'
```

Done when: every `<word>:` term the web sends is built in
`web/src/lib/searchQuery.ts`; the builder test writes
`tests/fixtures/search/web-queries.txt` and fails on drift;
`crates/vault/server/src/search/tests.rs` parses every line of that file on
its named list; `api.md` is generated from the registry and the docs test
covers it; `check-pr.sh` passes.

Carried over from the PR 2 review: `useSavedSearchWrite` in
`web/src/lib/savedSearches.ts` returns `UseMutationResult<unknown, …>` for
all three writes; give it a result type parameter so create and update
stay typed as `SavedSearch`.

## PR 7: Tests and fixtures

Spec section "Tests and fixtures".

Changed since the spec: `test_vault()` exists in
`crates/vault/server/src/test_support.rs`; `test_vault_http()` does not, and
`get_raw`, `post_raw`, and `delete_raw` each bind, spawn, and abort their
own server.

Inventory before planning:

```
grep -rn 'fn setup(' crates/vault/server/src
grep -c 'TcpListener::bind' crates/vault/server/src/test_support.rs
```

Done when: the first grep returns nothing; one `serve(state)` helper backs
every raw and JSON HTTP helper; Export and both read routes have
route-level tests through it; a contacts test covers `offset` above
`MAX_LIST_OFFSET` as the conversations test does; `check-pr.sh` passes.

Carried over from the PR 2 review, all in server code this pull request
already reworks:

- `import/mod.rs` still takes Axum's `Multipart` directly; map its
  rejections to `{error}` like the three extractors in `extract.rs`.
- `read_body_limited`, `discard_body`, and `stream_body_to_file` in
  `server.rs` answer 400 for their own oversize checks; 413 is the status
  that carries that meaning.
- The fast `Content-Length` 413 from `RequestBodyLimitLayer` carries no
  CORS headers because the limit layer sits outside the CORS layer; decide
  whether to reorder or document.
- Issue #273: `scripts/test/smoke-vault-push.sh` does not exercise
  `vault-push` and nothing runs it. Fold it into the route-level tests or
  close the issue with the reason.

## PR 8: Named sets

Spec section "Named sets".

Inventory before planning:

```
ls crates/vault/server/src | grep -E 'contact_groups_api|message_tags_api|named_set_api'
```

Done when: `contact_groups_api.rs` and `message_tags_api.rs` are gone, the
routes and their OpenAPI operations are unchanged
(`git diff main -- docs/src/assets/openapi.json` shows nothing), and
issue #281 is closed by the pull request; `check-pr.sh` passes.

## Out of scope

Searching messages across conversations: #313. Permanent delete and Empty
Trash: #314. The Contacts list's own conventions beyond paging and shape.
