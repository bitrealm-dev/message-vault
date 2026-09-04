# One Query Builder on the Web Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every search term the web sends is built in one module, and a
committed fixture proves the server can parse each one.

**Architecture:** `web/src/lib/searchQuery.ts` gains one `quote` and a builder
per term the web composes. Five files stop composing query text by hand. A test
in that module writes every builder's output to a fixture, and the server's own
test suite reads that fixture back and asserts each line parses on the list it
names — so the web cannot invent a query the vault would refuse without a test
going red on both sides.

**Tech Stack:** TypeScript (React 19, Vitest, Biome), Rust (the search language
and its tests), Astro Starlight (the docs pages).

**Spec:** `docs/superpowers/specs/2026-09-03-http-interface-repair-design.md`,
section "Query text on the web". Pull request 6 of eight in
`docs/superpowers/plans/2026-09-03-http-interface-repair-roadmap.md`.

## What the inventory found

**Query text is composed in six places, with two independent quoting rules.**

| Where | Builds | Quoting |
| --- | --- | --- |
| `web/src/lib/nameCollection.ts:100-110` | `group:X`, `tag:X` | its own `/\s/.test(name)` |
| `web/src/components/AppLayout.tsx:29-40` | `handle:`, `with:#`, `kind:` | its own `/\s/.test(h)` |
| `web/src/components/AppLayout.tsx:189` | `trashed:yes …` | none |
| `web/src/screens/TrashScreen.tsx:31` | `trashed:yes …` | none — a duplicate of the line above |
| `web/src/components/advancedSearch/buildAdvancedQuery.ts` | `handle:`, `kind:`, `participants:`, `messages:`, `name:`, `service:`, date bounds | inconsistent — see below |
| `web/src/lib/useSearchSuggestions.ts:46` | `word:value` for the suggestion list | none |

The inconsistency worth naming: `buildAdvancedQuery` writes
`handle:${input.handle.trim()}` for messages and `handle:"${input.handle.trim()}"`
for contacts. The same input produces a quoted term on one screen and a bare one
on the other, and neither path escapes an embedded quote.

**`in:#` and `date:` are already gone from the web.** Pull request 4 moved the
message screen onto the conversation read routes, so the spec's `year(q, year)`
builder has nothing left to build — the year is a route parameter now, not a
search term. It is not in this plan.

**`docs/src/content/docs/vault/developer/reference/api.md` documents seven
operators the language does not have.** I checked each against
`crates/vault/server/src/search/fields.rs`: `has:`, `after:`, `before:`, `is:`,
`people:`, `within:` and `label:` are not registered words. The page is
describing an older query language. Rewriting it from the registry is not
tidying — it is correcting a reference that would send someone down a path the
vault refuses.

**The existing docs test does not check applicability.**
`crates/vault/server/src/search/tests.rs` has `mod docs`, whose
`the_page_lists_every_word_and_nothing_else` asserts that `search.mdx` lists
every registered word and no others. It says nothing about the `<ListTiles on=…>`
column, which is why `trashed:` sat at `on="C V"` after pull request 5 registered
it for Messages, with CI green throughout (issue #328).

## Global Constraints

- **ADR-0002.** One way to fetch data in `web/`. This plan changes what query
  text screens build, not how they fetch.
- **No `<word>:` template literal survives outside `searchQuery.ts`.** That is
  the check the pull request is judged by.
- **The fixture is the contract.** `tests/fixtures/search/web-queries.txt` is
  written by a web test and read by a Rust test. Neither may skip it.
- Biome gates lint and format; `npx tsc --noEmit` must be clean including tests.
- **Verification.** `./scripts/check-pr.sh` passes on the head commit.

## Decisions this plan makes

**No `year()` builder.** The spec lists one; pull request 4 removed the only
caller when the message screen stopped composing `in:#id date:YYYY`. Adding a
builder with no caller would be dead code on arrival.

**`quote` quotes on whitespace, parentheses, or a quote, and escapes an embedded
quote.** That is the spec's rule verbatim. Both existing implementations only
check whitespace, so both are wrong for a name containing a parenthesis — which
the search language treats as grouping — and neither escapes. Fixing that is
part of the point, not a side effect.

## File Structure

**Created**

- `web/src/lib/searchQuery.ts` — `quote` and one builder per term.
- `web/src/lib/searchQuery.test.ts` — the builders' tests, and the test that
  writes the fixture.
- `tests/fixtures/search/web-queries.txt` — committed, one query per line, each
  prefixed by the list it belongs to.

**Modified — web**

- `web/src/lib/nameCollection.ts`, `web/src/components/AppLayout.tsx`,
  `web/src/screens/TrashScreen.tsx`,
  `web/src/components/advancedSearch/buildAdvancedQuery.ts`,
  `web/src/lib/useSearchSuggestions.ts` — call the builders.
- `web/src/lib/savedSearches.ts` — the carried-over type parameter.

**Modified — server and docs**

- `crates/vault/server/src/search/tests.rs` — read the fixture; extend `mod docs`.
- `docs/src/content/docs/vault/developer/reference/api.md` — rewritten from the
  registry.

---

### Task 1: `searchQuery.ts`

**Files:**
- Create: `web/src/lib/searchQuery.ts`, `web/src/lib/searchQuery.test.ts`

**Interfaces:**
- Produces: `quote(value: string): string`, and builders
  `forGroup(name)`, `forTag(name)`, `forHandle(handle)`, `forContact(id)`,
  `withKind(q, kind)`, `trashed(search)`, `advancedMessages(input)`,
  `advancedContacts(input)`, `suggestion(word, value)`.
  Tasks 2 and 3 consume these; keep the names exactly.

Take the two advanced-query input types from `buildAdvancedQuery.ts` as they
are — this task moves that logic, it does not redesign it, except for the
quoting inconsistency named below.

- [ ] **Step 1: Write the failing tests**

Cover `quote` first, because everything else depends on it:

- a plain word is returned bare
- a value with a space is quoted
- a value with `(` or `)` is quoted — the search language treats parentheses as
  grouping, so an unquoted one changes the query's meaning
- a value containing `"` is quoted **and** the inner quote escaped
- an empty string round-trips to something the language accepts

Then one test per builder, including: `forHandle` produces the same text whether
the caller is a messages screen or a contacts screen (the bug named in the
inventory), and `withKind(q, "all")` returns `q` unchanged rather than appending
an empty term.

- [ ] **Step 2: Run and watch them fail**

`cd web && npx vitest run src/lib/searchQuery.test.ts`

- [ ] **Step 3: Write the module**

Move the logic from the six sites listed in the inventory. Where two sites
disagree, the correct behaviour is the one that quotes and escapes.

Do not import from the components — this module is a leaf. If a builder needs a
type that currently lives in `buildAdvancedQuery.ts`, move the type here and let
that file import it.

- [ ] **Step 4: Verify and commit**

```bash
cd web && npm run lint && npx vitest run src/lib/searchQuery.test.ts
git add web/src/lib/searchQuery.ts web/src/lib/searchQuery.test.ts
git commit -m "feat(web): one module builds every search term"
```

---

### Task 2: Every screen calls it

**Files:**
- Modify: `web/src/lib/nameCollection.ts`, `web/src/components/AppLayout.tsx`,
  `web/src/screens/TrashScreen.tsx`,
  `web/src/components/advancedSearch/buildAdvancedQuery.ts`,
  `web/src/lib/useSearchSuggestions.ts`

**Interfaces:**
- Consumes: Task 1's builders.

- [ ] **Step 1: Replace each site**

Work through the inventory table. `AppLayout.tsx` and `TrashScreen.tsx` build
the same trash query independently — both call `trashed(search)` now, and the
duplication goes.

`buildAdvancedQuery.ts` keeps its two exported functions and their input types
as the screens' entry point, but their bodies become calls into
`searchQuery.ts`. Do not change what the advanced form produces except where the
quoting was inconsistent.

- [ ] **Step 2: The check this pull request is judged by**

```bash
grep -rnE '`[^`]*(in:|group:|tag:|contact:|handle:|kind:|date:|trashed:|source:|with:|from:|to:|service:|messages:|name:|participants:)' web/src --include=*.ts --include=*.tsx | grep -v '\.test\.' | grep -v vaultApi.types | grep -v searchQuery.ts
```
Expected: nothing but comments. Any code hit is a site you missed.

- [ ] **Step 3: Verify and commit**

```bash
cd web && npm run lint && npm test && npx tsc --noEmit
git add web/src
git commit -m "refactor(web): every screen builds its search terms in one place"
```

Watch for tests that asserted on hand-built query strings. If a test's subject
was the old text, update the expectation to the builder's output — but if the
output differs because the old text was wrong (unescaped, inconsistently
quoted), say so in your report rather than quietly matching the new value.

---

### Task 3: The fixture, written by the web and read by the vault

**Files:**
- Modify: `web/src/lib/searchQuery.test.ts`, `crates/vault/server/src/search/tests.rs`
- Create: `tests/fixtures/search/web-queries.txt`

This is the task that makes the module worth having: it stops the web inventing
a query the vault would refuse.

- [ ] **Step 1: Write the fixture from the web side**

A test in `searchQuery.test.ts` calls every builder with a fixed set of inputs —
including the awkward ones: a name with a space, a name with a parenthesis, a
name with a quote — and writes one line per result, sorted, to
`tests/fixtures/search/web-queries.txt`. Each line is
`contacts|conversations|messages`, a tab, then the query.

The test **fails if the committed file differs** from what it just produced.
Follow how `scripts/check-generated-api-types.sh` frames the same idea: the file
is committed, and drift is a failure with a message saying how to regenerate.

- [ ] **Step 2: Read it from the vault side**

In `crates/vault/server/src/search/tests.rs`, add a test that
`include_str!`s the fixture, and for every line asserts the query compiles on
the list its first column names. Reuse whatever helper the file's existing tests
use to compile a query for a list — do not add a second way.

A line that fails to parse should say which line and which list, because the
person reading that failure is looking at a fixture, not at code.

- [ ] **Step 3: Prove the loop closes**

Change a builder to emit something the language refuses — an unquoted value with
a space is the easy one — regenerate the fixture, and confirm the **Rust** test
goes red. Then revert. Put the failing output in your report: that is the
evidence this task did what it exists to do.

- [ ] **Step 4: Verify and commit**

```bash
cd web && npx vitest run src/lib/searchQuery.test.ts
cargo test -p message-vault-server search
git add web/src tests/fixtures/search crates/vault/server/src/search/tests.rs
git commit -m "test: the vault parses every query the web can build"
```

---

### Task 4: `api.md` rewritten from the registry

**Files:**
- Modify: `docs/src/content/docs/vault/developer/reference/api.md`,
  `crates/vault/server/src/search/tests.rs`

`api.md`'s search section documents **seven operators the language does not
have** — `has:`, `after:`, `before:`, `is:`, `people:`, `within:`, `label:`. I
verified each against `fields.rs`. It is describing an older query language, so
someone following it writes a query the vault refuses.

- [ ] **Step 1: Rewrite the section from `fields.rs`**

Every word the registry marks as applying to the Messages list, with its real
name and what it takes. Keep the "One shape for every route" section written in
pull request 2. Keep the note that export runs a metadata subset rather than the
full-text path, if it is still true — check `export_api.rs` rather than
assuming.

**This repo's published docs must read like a person wrote them** — plain,
direct English, no AI-flavoured hedging, no clipped telegraphese. Match the
surrounding pages.

- [ ] **Step 2: Extend the docs test to cover this page**

`mod docs` in `crates/vault/server/src/search/tests.rs` already asserts that
`search.mdx` lists every word and nothing else. Add the same assertion for
`api.md`, scoped to words the registry marks as applying to Messages, since that
is what export answers.

- [ ] **Step 3: Close the applicability gap (issue #328)**

The existing test checks *which words* the page lists, not *which lists* it says
each applies to — which is why `trashed:` sat at `on="C V"` after it gained
Messages, with CI green.

Extend `the_page_lists_every_word_and_nothing_else`, or add a sibling, to parse
each row's `<ListTiles on="…" />` and assert it matches `spec.lists`. The tile
letters map `C` → Contacts, `V` → Conversations, `M` → Messages; read
`search.mdx` to confirm before relying on it.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p message-vault-server search::tests::docs
cd docs && npm run check && npm run build
git add docs crates
git commit -m "docs: the API reference describes the query language the vault has"
```

---

### Task 5: The carried-over type parameter

**Files:**
- Modify: `web/src/lib/savedSearches.ts`

Carried from the pull request 2 review: `useSavedSearchWrite` returns
`UseMutationResult<unknown, …>` for all three writes, so create and update lose
their `SavedSearch` result type at every call site.

- [ ] **Step 1:** Give it a result type parameter so create and update stay
typed as `SavedSearch` and delete stays as whatever it actually returns.

- [ ] **Step 2:** Check the call sites — if any was compensating with a cast or
a non-null assertion, that compensation goes too.

- [ ] **Step 3:**

```bash
cd web && npx tsc --noEmit && npm run lint && npm test
git add web/src/lib/savedSearches.ts web/src
git commit -m "fix(web): saved-search writes keep their result type"
```

---

### Task 6: The pull request

- [ ] **Step 1:** `./scripts/check-pr.sh` — expect `All pre-PR checks passed.`

- [ ] **Step 2: Confirm the roadmap's Done-when**

Run Task 2 Step 2's grep again, confirm `tests/fixtures/search/web-queries.txt`
is committed and non-empty, and confirm both the web and Rust fixture tests run
in CI.

- [ ] **Step 3:** Open the pull request, wait for CI, squash-merge.

- [ ] **Step 4:** Update the roadmap's Status table on a branch — row 6 merged
with its number, row 7 **next**, this plan added, and anything carried forward
written into the row that inherits it.

## Self-Review

**Spec coverage.** One module with the builders and `quote` → Task 1; every
screen calling it with no surviving template literal → Task 2; the fixture
written by the web and parsed by the vault → Task 3; `api.md` rewritten from the
registry with the docs test covering it → Task 4. `vault-pull`'s `compose_query`
was already deleted in pull request 2.

**Deliberately not covered.** The spec's `year(q, year)` builder — pull request 4
removed its only caller when the message screen moved onto the read routes, so
it would be dead on arrival.

**Risk to watch.** Task 2 changes the text screens send. A builder that quotes
differently from the hand-written version it replaces changes what the vault
matches, and a test that simply adopts the new string would hide it. Task 2's
instruction is to report any output difference rather than absorb it, and Task
3's fixture is what makes such a difference visible on the server side.
