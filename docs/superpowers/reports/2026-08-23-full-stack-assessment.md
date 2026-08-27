# Full-stack assessment — 2026-08-23

Independent read-only audit of documentation, code, CI, build process, and code
quality, with the move toward a hosted offering in mind. Scope covers the Rust
workspace, `src-tauri`, `web/`, `.github/`, `scripts/`, `docker/`, and `docs/`.

Verified at `edc22755`. `tsc`, `cargo clippy`, and `cargo test` were executed
locally; every claim below cites a file or a command. No files were modified.

This report deliberately does **not** restate
`docs/superpowers/reports/2026-08-23-rust-audit.md`. That audit already covers
Rust duplication and doc coverage in detail. Where findings overlap they are
marked *(already known)*.

## Summary

The code is ahead of the pipeline that ships it.

The Rust is genuinely well-built: clippy is clean across the workspace, 609
tests pass, there is not a single `TODO`/`FIXME` in the tree, and the server
contains exactly three `unwrap`/`expect` calls, all provably unreachable.
Blocking SQLite work is uniformly wrapped in `spawn_blocking` — the most common
way AI-written Axum code goes wrong, and it is correct here throughout.

The security work is real rather than cargo-culted: Argon2id with per-password
salts, a dummy-hash run on unknown usernames so login timing does not leak
account existence, `O_NOFOLLOW` opens with symlink rejection on the asset path,
digest re-verification on read, parameterized SQL with no injection surface
found, CORS defaulting to same-origin, and scoped API tokens hashed at rest.

Documentation is stronger than expected: an accuracy pass checked 28 documented
claims against source and found 26 correct.

The weaknesses cluster in delivery, not design. One item is actively broken.

### Numbers

| Metric | Value |
|---|---|
| Product LOC | ~97,800 (Rust 78.7k excl. legacy, web 19.1k) |
| Legacy LOC still in repo | 46,201 (`web-next/` 42,089, Slint GUI 4,112) |
| Commits / span / authors | 903 / 45 days / 1 |
| Rust tests | 609, all passing |
| Web tests | 138 across 30 files, all passing |
| Clippy warnings | 0, workspace-wide |
| OpenAPI paths | 37, guarded by a staleness test |
| Doc pages | 79 |

## Verdicts

| Dimension | Verdict | Note |
|---|---|---|
| Rust code quality | Strong | Clean clippy, coherent modules, correct async discipline. Eight server files over 1,500 lines are the main wart. |
| Security posture | Strong | Hardened auth, path handling, token model. Gaps are rate-limit coverage and absent scanning, not the crypto. |
| Documentation | Strong | 26/28 claims verified correct; OpenAPI drift test-guarded. Missing privacy, security, backup, changelog history. |
| Test coverage | Adequate | Real HTTP-level route tests and guest-isolation tests. Frontend thin; no E2E. |
| Data layer | Adequate | Multi-tenant schema, good indexes, real FTS5. No versioned migration mechanism. |
| Frontend | Mixed | Zero `any`, disciplined hooks. No runtime validation, no error boundary, unvirtualized threads. |
| CI & release | Weak | Builds the dead code, skips the shipping code, nothing required to merge. |
| SaaS readiness | Mixed | Multi-tenant schema is a major head start. Single-writer concurrency and no observability are the work. |

## Blocking items

Seven items, ordered by damage per hour of fix. None requires an architectural
change.

### 1. The release pipeline is armed to fail

`web/` does not type-check at `HEAD`:

```
src/components/AttachmentLightbox.tsx(56,11): error TS2322: Property 'onKeyDown'
  does not exist on type 'IntrinsicAttributes & DialogProps & ...'
src/components/AttachmentLightbox.tsx(56,23): error TS7006: Parameter 'e'
  implicitly has an 'any' type.
```

Introduced 2026-08-22 by PR #82 (`36f4d8e6`, "fix: clear Clippy and Biome
warnings in product trees"). No gate catches it:

- `.github/workflows/ci.yml` runs Biome and Vitest for web — never `tsc`.
- `npm run build` (`tsc && vite build`) appears in CI only in the `docker` and
  `release` jobs, both gated `if: startsWith(github.ref, 'refs/tags/v')`
  (`ci.yml:136`, `ci.yml:186`).
- `scripts/check-pr.sh:25` runs `npm run lint` only. It builds the **docs** site
  (`check-pr.sh:34-36`) but never the product SPA.

Last tag was `v0.7.3` on 2026-08-13; the break landed 08-22. It is latent. The
next `v*` tag push fails the Docker job and all three desktop installer jobs at
once.

**Fix:** move the handler off the react-aria `Dialog` (to the `Modal`, or a
wrapper with `onKeyDownCapture`) and type the parameter. Then add
`tsc --noEmit` to the `web-lint` job and `(cd web && npm run build)` to
`check-pr.sh`.

### 2. Your repo states two mutually exclusive licenses

- `LICENSE.md:1` and `README.md:95` — **Fair Core License 1.0, ALv2 future**
  (source-available, converts to Apache-2.0 after two years).
- All **26** `Cargo.toml` files and `web/package.json:5` — `AGPL-3.0-only`.
- `docs/src/assets/openapi.json` `info.license` — `AGPL-3.0-only`, because
  utoipa injects `CARGO_PKG_LICENSE`. Your published API spec advertises AGPL.

These are not compatible, and the difference is exactly the one that matters for
the business model: FCL exists to stop a competitor hosting your software; AGPL
explicitly permits it provided they publish source. A contributor cannot know
what they are signing over; a customer's counsel cannot tell what they bought.

**Fix:** decide which you mean — for the SaaS path this is almost certainly FCL
— then correct Cargo metadata, `package.json`, and the regenerated spec
together, and add a CI check so they cannot diverge. Do this before accepting
an outside contribution.

Note also that `README.md:113` describes the project as "open source". FCL is
source-available, not OSI-approved. That wording is worth making precise, since
it affects distro packaging and some corporate adoption.

### 3. CI compiles the dead code and skips the shipping code

- `crates/message-vault-io-gui` — the legacy Slint GUI, which `CLAUDE.md` calls
  "not the product path" — is a workspace member (`Cargo.toml:27`), so every PR
  builds and tests it.
- `src-tauri` — the desktop app you actually ship — is in `exclude`
  (`Cargo.toml:30`) and is only *format*-checked (`ci.yml:56`). Its first real
  compile happens inside the 13–19 minute release job.

**Fix:** add `cargo check --manifest-path src-tauri/Cargo.toml` on every PR.
Separately, drop the Slint crate from workspace members.

### 4. Nothing is required to merge, and `.env` is tracked

`gh api repos/.../branches/main/protection` returns 404 — `main` has no branch
protection at all, so CI is advisory and a red build can land. A `v*` tag push
ships a release with no guardrail.

Separately, `.env` is tracked in git and `.gitignore` has no rule for it.
Contents are benign today (`COMPOSE_FILE`, `UID`, `GID`, `DEMO_DATA`,
`VAULT_AUTH`), but that is exactly where a Hanko key or payment secret lands the
day you start billing. It is also absent from `.dockerignore`, so it rides into
the build context.

**Fix:** require status checks on `main` and protect the `v*` tag pattern.
`git rm --cached .env`, add it to `.gitignore` and `.dockerignore`, commit a
`.env.example`.

### 5. No vulnerability disclosure path, no dependency scanning

There is no `SECURITY.md`, so a researcher who finds a bug in a product holding
private message archives has nowhere to report it but a public issue. There is
also no Dependabot, no `cargo-audit`/`cargo-deny`, no `npm audit`, and no
CodeQL — 837 crates and 278 npm packages with nothing watching them.

**Fix:** roughly an hour total. `SECURITY.md` with a contact and response
window; `dependabot.yml` for cargo, npm, and actions; `cargo-deny` in the PR
gate.

### 6. No versioned migration mechanism

All DDL is `CREATE TABLE IF NOT EXISTS`, plus four hand-written `ensure_column`
patches covering the accounts tables only (`db/schema.rs:323-370`) and one
bespoke rename (`migrate_contact_labels_to_groups`, `schema.rs:299`). There is
no `PRAGMA user_version`, no ordered migration list, and no data-migration path.

Add a column to `schema/sql/messages.sql` and existing vaults never receive it:
`IF NOT EXISTS` no-ops and nothing backfills. The schema-contract test checks
fresh vaults only.

Nothing has broken because every change so far has been additive and you
remembered. That is a process guarantee, not an engineering one, and it does not
survive a fleet you cannot inspect.

**Fix:** adopt `user_version` with an ordered migration list, and add a test
that opens a DB created from an older schema snapshot and upgrades it. The
committed schema fixture can be extended into a version series.

### 7. Desktop installers ship unsigned

`ci.yml:200-208` declares `WINDOWS_CERTIFICATE_*`, `MACOS_CERTIFICATE_*`, and
`NOTARY_*` as environment variables. None of those secrets are set (`gh secret
list` shows only the two Docker Hub secrets), and **no step consumes them** —
there is no signing or notarization anywhere in the workflow.

Every macOS user meets Gatekeeper; every Windows user meets SmartScreen. The
docs are honest about this, which helps, but it is a conversion tax on a paid
product.

**Fix:** implement signing, or delete the env block — as written it reads as
done.

## SaaS readiness

The headline is better than expected: **the schema is already multi-tenant.**
Every table carries `account_id` with `ON DELETE CASCADE` and composite indexes
(`schema/sql/*.sql`). The single most expensive SaaS retrofit — bolting tenancy
onto a single-user model — is a problem you do not have.

Tenant isolation is enforced by threading `auth.account_id` into each query by
hand. That is correct in the paths traced, and
`list_conversation_source_stats` (`conversations_api.rs:768`) shows the right
pattern: verify ownership, then query by id. But nothing structural prevents one
forgotten predicate, and the blast radius is one customer reading another's
private messages.

**Recommendation:** add a dedicated cross-tenant isolation test suite — for each
read endpoint, assert that account B cannot reach account A's rows by id. This
is the highest-value test you can write before hosting.

### Carries over as-is

- Multi-tenant schema with cascade deletes and per-account indexes
- Scoped, revocable, hash-at-rest API tokens with expiry
- Opaque session tokens — nothing forgeable if the database leaks
- Content-addressed assets, which makes object storage a clean swap
- Resumable multipart upload, already implemented
- Real FTS5 search over body, subject, and attachment transcriptions

### Needs work

- **Write concurrency ceiling.** All writes serialize through a single
  `Arc<StdMutex<Connection>>` (`server.rs:154`). Correct, and a hard ceiling.
- **Per-request DDL.** `resolve_auth` opens a fresh connection *and* runs
  `ensure_accounts_schema` on every authenticated request (`server.rs:655`).
  Schema-ensure already runs at startup (`server.rs:459`), so this is waste.
  Dropping it plus a connection pool is likely the highest-leverage performance
  change available.
- **Rate limiting** is auth-endpoint-only, in-memory per process, and resets on
  restart. The Hanko bucket is a single global 20/min (`auth.rs:639`) — a
  capacity bug at scale. Data routes are unthrottled, so a leaked token can be
  scraped freely.
- **Assets are local disk only.** Single node, needs a persistent volume. The
  content-addressed design makes the eventual swap clean.
- **SIGTERM is unhandled.** `shutdown_signal()` awaits only `ctrl_c()`
  (`server.rs:592`). `docker stop` sends SIGTERM, so the graceful shutdown wired
  into Axum never fires in a container — Docker waits its grace period then
  SIGKILLs mid-import.
- **No observability.** 167 raw `eprintln!`/`println!` calls in the server and
  no `tracing`, `log`, metrics, or error reporting. `/health` returns a static
  string with no dependency check, and the Dockerfile has no `HEALTHCHECK`.
- **Guest-clone completeness is unchecked** (`guest_clone.rs:91-1330`) — ~20
  tables hand-cloned with no test asserting full coverage, so a new table
  silently degrades the hosted demo.

### An architectural decision still ahead

Decide deliberately whether the hosted tier is per-tenant SQLite files or a
shared Postgres. Per-tenant SQLite suits your content-addressed, cascade-scoped
design remarkably well and sidesteps the write-mutex ceiling entirely. Either
way the choice gets much more expensive after the first hundred customers.

## Frontend

TypeScript hygiene is better than most professional codebases: **zero `any`** in
product code, zero `@ts-ignore`, zero non-null assertions, ~28 `as` casts, and
hand-rolled fetch hooks that correctly use `AbortController` and refs to avoid
stale closures. Biome runs `useExhaustiveDependencies` at error level. The PR
#82 exhaustive-deps regressions noted in
`docs/superpowers/plans/2026-08-22-fix-pr82-exhaustive-deps-regressions.md` are
fixed in the current tree.

Two gaps are product-shaped, and both are worst on exactly the data this app
exists for — a decade-long thread.

- **Message threads are not virtualized.** `MessageThread.tsx:39` maps over
  every message. Year mode loads a full calendar year in sequential 500-message
  fetches (`useConversationMessages.ts:53-75`). A busy year is 10k+ DOM nodes.
  The contacts list is already virtualized — reuse `VirtualList`.
- **Opening one conversation costs O(N) requests.** There is no
  `GET /v1/export/conversations/{id}`; only the list and a `/sources` sub-route.
  So `fetchConversationById.ts:15-34` pages the entire list 100 at a time until
  it finds the id. Add the by-id endpoint.

Also:

- **No runtime validation.** `api.ts:45` does `res.json() as Promise<T>` for
  every endpoint. Types are hand-written mirrors of the server's. Desktop app
  and server version independently, so a renamed field renders wrong rather than
  throwing. Add zod at the client edge.
- **No error boundary** anywhere in the tree — a render crash is a white screen.
- **No 401 handling** anywhere (zero matches for `401`/`Unauthorized` in
  `web/src`). The token is validated once at startup (`auth.tsx:130-179`); when
  the 30-day session expires mid-use the UI degrades into stray `401:` strings.
- **No code splitting.** `App.tsx` statically imports all 12 screens.
- Token lives in `localStorage` (`auth.tsx:48-92`) — XSS-reachable. Worth
  revisiting httpOnly cookies before hosting.
- `InfiniteOffsetList.tsx` contains two virtualization implementations
  (`RacVirtualList` at :79, `TanStackVirtualList` at :168) dispatched at :447.

## Documentation

Stronger than the rest of the delivery story, and worth defending.

An accuracy pass verified 28 documented claims against source: **26 accurate**,
one minor drift, one contradiction (the license, §2 above). Env-var tables,
rate-limit constants, search operators, import defaults, demo credentials,
schema version, and every route in the API guide match the code. Documented
commands work as printed.

Two structural strengths:

- The OpenAPI spec is generated by utoipa and **guarded by a test** —
  `committed_openapi_matches_dump` (`openapi.rs:319`) fails the build if the
  committed spec drifts from the routes. Verified passing. Routes cannot silently
  diverge from the published reference.
- The docs admit what does not exist. `how-to/trash.md` states outright that
  restore and empty-trash routes "are not part of the current vault server.
  Treat Trash as a placeholder until those endpoints ship." Calibrated
  incompleteness is rare and valuable.

Zero AI-filler markers appear in the technical content.

Gaps that matter:

- **No privacy, security, or backup documentation.** No privacy policy, no
  retention statement, no telemetry disclosure, no threat model, no TLS or
  reverse-proxy guidance. Backup is one sentence in `update.md`. For a product
  built to preserve people's private messages, these are table stakes before a
  hosted tier.
- **No release has ever been recorded in the changelog.** `CHANGELOG.md` has
  only `[Unreleased]`; eight shipped tags through 0.7.3 have no entries, and the
  site's changelog page is a stub. Self-hosters pulling `latest` cannot see what
  changed.
- **`CLAUDE.md` and `AGENTS.md` describe the wrong auth system.** Both say "JWT
  session tokens". Local sessions are opaque `mv-user-` random tokens, SHA-256
  hashed at rest with a 30-day TTL (`db/session_tokens.rs`); `jsonwebtoken`
  appears only to verify Hanko RS256 tokens via cached JWKS
  (`auth.rs:654-693`). Your actual design is the better one. This matters more
  than a normal doc bug because these two files prime every AI session on the
  repo — two independent audit passes in this very review inherited the error
  before it was checked against source. One-line fix in each.
- **Zero compilable doc examples.** Rustdoc coverage is 87% (654/753 public
  items) and the descriptions carry real contracts, but
  `cargo test --doc --workspace` runs 0 tests across all 24 suites, while
  `rustdoc-style.md` mandates "examples when behavior is non-obvious."
- **CLI reference pages are ungated.** Unlike `openapi.json`, the clap-generated
  pages have no staleness test and are regenerated manually.
- `README.md:35` — "Pry **digitial** conversations out of apps" — typo in the
  tagline directly under the logo, the most-read line in the project.
  `README.md:109` has "and and". Several blocks of unused Best-README-Template
  scaffolding remain commented in.
- `CLAUDE.md` is untracked in git.
- Four `docs/superpowers/plans/` files are uncommitted on disk, and finished
  plans carry no status marker distinguishing them from open ones.

## Dead weight

| Tree | LOC | Status | Cost |
|---|---|---|---|
| `web-next/` | 42,089 | Legacy Next.js browse UI | Not in CI, not served, ships its own auth stack (`jose`, `@node-rs/argon2`) — an un-audited surface if ever pointed at a vault |
| `crates/message-vault-io-gui/` | 4,112 | Legacy Slint desktop GUI | Workspace member, so compiled and tested on every PR |

Together, roughly a third of the codebase. Both are already documented as
not-the-product-path, so this is bookkeeping rather than a decision. Move them
to an archive branch, keep the one `searchQuery` parser that
`scripts/deprecated/regen-search-goldens-worker.ts` still imports from
`web-next`, and reclaim the CI time plus the overhead of two dead auth
implementations sitting in the tree.

## Other CI notes

Not blocking, but worth folding in.

- **Caching is good.** `actions/cache@v5` over cargo registry, git, and `target`
  keyed on `Cargo.lock`; npm caching on all web jobs; Docker BuildKit `gha`
  cache `mode=max`. PR runs are 3–5 minutes; a full tag run is ~26 minutes.
- **Docs are not gated on PRs.** `docs.yml` has no `pull_request` trigger, so a
  PR that breaks the Astro build passes CI and fails the post-merge deploy.
- **No test coverage measurement** anywhere (`vitest run` without `--coverage`,
  no llvm-cov).
- **Release reproducibility is weak.** No `rust-toolchain.toml`; CI floats on
  `dtolnay/rust-toolchain@stable`. All actions are floating major tags rather
  than pinned SHAs, with no Dependabot to bump them.
- **Docker image is fatter than needed.** The runtime stage is `node:20-bookworm-slim`
  plus ffmpeg, but the server is a compiled Rust binary that serves its own
  static files — the Node runtime is waste. Single-arch amd64 only.
- **Self-host footgun.** Both compose files use `user: "${UID}:${GID}"`. A
  self-hoster who saves only the published `compose.yml` without the repo `.env`
  gets empty substitution and a container that will not start.
- **`DEMO_DATA: true` by default** in both compose files — fine for self-host,
  a trap to carry into SaaS.
- **Version lockstep is manual.** All four files are at `0.7.3` today, but
  nothing checks it. Installer filenames derive from the tag itself
  (`ci.yml:254`), so a mismatch produces a release whose artifacts and metadata
  disagree.
- **Orphaned maintenance scripts.** `scripts/check-sql-column-comments.mjs` and
  `scripts/sync-vault-schema.mjs` run nowhere in CI or `check-pr.sh`.
- Scripts themselves are solid: all use `set -euo pipefail` and resolve paths
  from `$0`. `check-pr.sh` is stricter than CI in most respects — the web build
  is the one gap.

## Calibration

The common failure mode of AI-assisted solo projects is impressive-looking
breadth over a hollow core: plausible code that does not cohere, tests that
assert nothing, docs describing software that was never written. **That is not
what this is.**

The evidence is specific. 609 tests that exercise the HTTP layer against a real
router on an ephemeral port. Zero clippy warnings across 82k lines. A
hand-written FTS query parser with a golden-file corpus. Guest-isolation tests
that run auth → handler → database. A generated API spec with a test that fails
the build on drift. And 26 of 28 documented claims verified true against source.

Two separate audits — yours from earlier today and this one — went looking for
something alarming in the Rust and neither found it. Your own audit's three
"high" findings are all documentation issues.

The weaknesses cluster tightly, and it is a recognizable cluster: the things a
solo developer has no colleague to be annoyed by. Nobody's release got broken by
your merge, so nothing forced a build gate. Nobody was paged at 3am, so there is
no structured logging. Nobody inherited an unfamiliar schema, so migrations
stayed manual. Nobody's lawyer asked which license applied, so two answers
coexisted.

Every one of those is cheap now and expensive once customers exist. The seven
blocking items are roughly two focused days. That work moves this from
impressive personal project to defensible commercial foundation, and none of it
requires touching the architecture — which is the real compliment.

## Suggested order

1. Fix `AttachmentLightbox.tsx:56`; add `tsc` to CI and the web build to
   `check-pr.sh`. *(unblocks releasing at all)*
2. Resolve the license contradiction. *(unblocks outside contribution)*
3. Branch protection on `main`; untrack `.env`.
4. `SECURITY.md` + Dependabot + `cargo-deny`.
5. `cargo check` for `src-tauri` in PR CI; drop the Slint crate from the
   workspace.
6. Cross-tenant isolation test suite.
7. `user_version` migrations with an old-DB upgrade test.
8. Drop per-request DDL from `resolve_auth`; add a connection pool.
9. Virtualize message threads; add the conversation by-id endpoint.
10. Privacy, backup, and data-handling docs; backfill the changelog.
