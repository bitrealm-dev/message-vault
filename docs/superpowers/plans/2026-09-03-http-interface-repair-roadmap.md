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
| 3 | An import names the Contact (ADR-0006) | Names | merged, #319 |
| 4 | Conversation read routes; message screen on TanStack Query | Conversation read routes | merged, #325 |
| 5 | Trash module and four routes | Trash | merged, #329 |
| 6 | One query builder on the web; shared example file | Query text on the web | **next** |
| 7 | One test fixture; route-level tests | Tests and fixtures | queued |
| 8 | Named-set route files folded | Named sets | queued |

Plans so far: `2026-09-03-import-failures-and-schema-docs.md` (PR 1),
`2026-09-03-route-convention.md` (PR 2),
`2026-09-03-an-import-names-the-contact.md` (PR 3),
`2026-09-04-conversation-read-routes.md` (PR 4),
`2026-09-04-trash-routes.md` (PR 5).

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

## PR 3: Names (ADR-0006) — merged, #319

Shipped in `61ce1621`. One `db/participant_names` module owns the naming query
and every route that names a participant calls it; `ConversationParticipant` and
`ExportParticipant` are one `Participant {name, handle, service, contact_id}`;
`contact_handles.name_alias` is gone (schema 8) along with `contact_name_mode`,
the web's own naming rule, its browser-storage preference, the Appearance
toggle, and the contact drawer's Alias column. An address-book load adopts the
Contact an import made rather than standing a nameless duplicate beside it.

Four defects the reviews caught and the pull request fixed: `vault-push`'s
sibling `vault-pull` kept its own copy of the Export shape with
`#[serde(default)]` on the removed field, so participants pulled vault-to-vault
would have arrived nameless with no error; an address book overwrote a name the
person typed, because nothing set `origin = 'user'` on rename; `search/emit.rs`
kept a second naming rule joining through `participants.contact_id`, a column
written once at import and never updated; and a participant-less conversation
named itself by its raw handle.

Left behind, with the reasoning: issue #320.

## PR 4: Conversation read routes — merged, #325

Shipped in `d9c57ab8`. `GET /v1/conversations/{id}` and
`GET /v1/conversations/{id}/messages` both exist, sharing one message-row loader
with Export so the two cannot drift on ordering, duplicate filtering or
participant naming; `year=` reuses the same span computation and column
`date:YYYY` uses. `useConversationMessages` runs on TanStack Query with no
`AbortController` of its own, `MessageRoute` reads one conversation in one
request with `location.state` as `placeholderData`, `fetchConversationById` is
gone, the nine phantom fields are out of `types.ts`, tapbacks render, and
`tsconfig.json` type-checks the tests.

Five defects the reviews caught and the pull request fixed: a vault-to-vault
pull would have failed outright once the server started sending `handle: null`,
because `vault-pull`'s hand-written mirror declared that field `String` — the
third defect of that class in that crate, now covered by a test that parses a
real JSON page instead of building the structs in Rust; issue #324, in the same
file, where `vault-pull` read a `service` the server has never sent; an importer
writing participant rows with no address, no name and no contact and counting
them in its participant total; a Contact rename that could never reach an
address-less participant; and `exportMessages`/`countExportMessages`, which
turned out to have no callers at all, so this pull request's own Done-when was
being met by accident.

Left behind, with the reasoning: issue #326. The year-load decision it inherited
stays open as issue #323.

## PR 5: Trash — merged, #329

Shipped in `d3235a8e`. Worth recording what the inventory found: until this pull
request **nothing in the product could put anything in the trash**. Every
`INSERT INTO trashed_*` in the tree was test code, so the search language
answered `trashed:yes`, the lists filtered, the Trash screen counted, and none of
it could ever have anything to count.

Four idempotent routes over a `db/trash` module owning the marker tables and the
account purge, answering 204 or 404 with no way to tell another account's id from
one that never existed. `trashed_handles` deleted (schema 9). The Messages list
answers `trashed:` like the other two, with the default unchanged so no export
gained content silently. On the web, a `trash.ts` feature module, "Move to trash"
on the conversation header and the contact drawer, and a Trash screen with a
section for each kind.

Five defects the reviews caught: trashing a contact was a one-way door, because
the plan specified an action with no inverse and a trashed contact cannot even be
opened; the account purge never deleted `trashed_contacts`; registering
`trashed:` on Messages needed an alias bridge or the SQL referenced an unbound
alias; two cache-key prefixes were alive only on a prediction that this pull
request would need them; and three published pages still described the pre-trash
product, one telling people not to rely on restore.

Left behind, with the reasoning: issue #328.

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

Carried over from PR 5 (issue #328): nothing checks the operator table in
`docs/src/content/docs/vault/user/how-to/search.mdx` against the field registry
in `crates/vault/server/src/search/fields.rs`. It drifted on the first change
that touched it, with CI green throughout. This pull request rewrites `api.md`
from the registry and already adds a docs test; extending that test to cover the
user page's table would close the same gap in the same pass.

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
