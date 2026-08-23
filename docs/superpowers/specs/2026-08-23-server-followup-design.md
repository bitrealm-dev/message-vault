# Server Crate Follow-up Design

> Status: approved design. The implementation plan is written from this spec.

**Goal:** Fix the 14 server-crate findings from the rust audit in one sequenced
project: HTTP docs first, then structure splits, then error and surface
cleanups.

**Why:** The [rust audit report](../reports/2026-08-23-rust-audit.md) found 14
findings in `crates/vault/server/` (2 high, 7 medium, 5 low). This spec turns
them into three workstreams that land on one branch. The libs, exporters, CLI
tools, and Tauri host groups are separate follow-up projects with their own
specs.

## Findings addressed

High:

- `api_tokens_api.rs` handler docs echo the route path and use `# Errors`
  sections; the utoipa summaries leak rustdoc headings into the HTTP catalog.
- `server.rs` has ~30 route handlers with no utoipa summary at all.

Medium:

- `lib.rs` has no `missing_docs` gate; many public items are undocumented.
- `import.rs` modules have no `//!` intros.
- `contacts_api.rs` `ToSchema` structs have no docs.
- `server.rs` is a 3,452-line monolith mixing router assembly, shared state,
  and route handlers.
- `import.rs` is a 3,039-line monolith.
- `thread_tags_api.rs` and `contact_groups_api.rs` implement near-identical
  named-membership CRUD.
- `api_tokens_api.rs` classifies label-validation errors by substring match on
  anyhow messages.

Low:

- Pagination-limit constants are duplicated across three API modules.
- `asset_uploads.rs` calls `libc::flock` with no SAFETY comment.
- `lib.rs` re-exports every module as `pub`.
- `thread_tags_api.rs` exposes a test-only `pub` function.
- `import.rs` has a dead public field.

## Workstream 1: HTTP docs

Every item below follows the rustdoc style guide
(`docs/src/content/docs/vault/developer/rustdoc-style.md`). That guide is the
authority for wording; when it conflicts with anything here, it wins.

- Every handler registered in `openapi.rs` gets a one-line summary in plain
  prose that says what the route does. Add a short description only when it
  explains when or why to use the route. No summary may echo the route path.
  No `# Errors` headings — error conditions become prose in the description.
- Every `ToSchema` struct gets a one-line doc comment; it becomes the
  component description in the HTTP catalog.
- Every module gets a `//!` intro. For modules that Workstream 2 will split,
  the intro names the future submodule responsibilities so it does not have to
  be rewritten; the split task carries each submodule's own intro with its
  code.
- Add `#![warn(missing_docs)]` to `lib.rs` and fix every warning it produces.
  No `#[allow(missing_docs)]` anywhere.
- Regenerate `docs/src/assets/openapi.json` with `dump-openapi` and commit it.
  The stale-spec test must pass.

## Workstream 2: Structure

- **Move handlers out of `server.rs`.** Each handler moves to its domain
  module (`contacts_api`, `conversations_api`, `assets`, `import`, `auth`,
  `export`, and so on). `server.rs` keeps router assembly, `AppState`, auth
  resolution, and shared layers. A domain move is one unit of work: the
  handler, its utoipa annotation, its `openapi.rs` `routes!()` entry, and the
  tests that import it change in the same task, and the crate compiles and
  passes tests after each unit.
- **Split `import.rs`** into `staging`, `promote`, and `contact-name`
  submodules, one responsibility each.
- **Share named-membership CRUD.** Extract one generic helper for thread tags
  and contact groups (reserved names, normalization, list/rename/delete/
  membership). `thread_tags_api.rs` and `contact_groups_api.rs` become thin
  wrappers over it.
- **Define pagination limits once** in a shared module; delete the three
  duplicated constant groups.

## Workstream 3: Errors and surface

- **Typed label validation.** Replace `map_label_error` substring matching
  with a typed error from the validation layer. Status codes and JSON bodies
  stay exactly as they are today: invalid label → 400 with the same message,
  everything else → 500.
- **Safe file locks.** Replace the `libc::flock` call in `asset_uploads.rs`
  with `fs2::FileExt::try_lock_exclusive`, matching `operation_lock.rs`.
- **Curated re-exports.** Server modules become `pub(crate)` or private.
  `lib.rs` re-exports the intended surface: `cli`, `config`, the server entry
  points, and the key public types.
- **Visibility fixes.** The test-only `pub` function in `thread_tags_api.rs`
  becomes `pub(crate)`. Remove the dead public field in `import.rs`.

## Non-goals

- The "unsafe attachment path" string contract between the server and
  `ir-format` — that belongs to the libs follow-up.
- Changes under `web/` or `src-tauri/`.
- A product version bump.
- Changing the utoipa architecture (`OpenApiRouter` stays).
- Any behavior change: this project is docs, structure, and error plumbing.
  Requests, responses, status codes, and error strings are untouched.

## Constraints

- **Behavior-preserving.** Status codes, JSON bodies, and error strings do
  not change. Every existing server test and the smoke scripts must pass on
  the final branch.
- **Crate green after every task.** `cargo fmt --check`, clippy clean, and
  `cargo test -p message-vault-server` pass after each task; no mid-project
  broken states.
- **Doc standard.** The committed rustdoc style guide governs every comment
  written here.
- **Sequencing.** Workstream 1 before 2 before 3. Workstream 1 lands the two
  high findings first so docs improvements are in place before code moves.
- **CHANGELOG.** One `[Unreleased]` entry describing the cleanup at the end.

## Testing

- Existing server tests stay green throughout.
- New tests:
  - Typed-error mapping: each former substring case asserts the same status
    code and body it produces today (invalid label → 400, other failures →
    500).
  - The named-membership helper: both routes behave identically (list, create
    with reserved names rejected, rename, delete, membership).
  - Lock behavior for the `fs2` swap: acquiring the lock twice fails with the
    same observable result as the `flock` path.
- `dump-openapi` output matches the committed `openapi.json` after
  Workstream 1 (stale-spec test).

## Deliverables

- `crates/vault/server/**` — the three workstreams above.
- `docs/src/assets/openapi.json` — regenerated.
- `CHANGELOG.md` — one `[Unreleased]` entry.
- This spec and the implementation plan under `docs/superpowers/`.

## Execution

Subagent-driven development against the implementation plan, on one branch,
tasks in workstream order. The workstreams are sequencing inside one plan,
not separate plans or branches.
