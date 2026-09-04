# Named Sets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Delete `contact_groups_api.rs` and `message_tags_api.rs` by stamping their twelve handlers out of one macro in `named_set_api.rs`, leaving every route and every byte of `docs/src/assets/openapi.json` exactly as it is.

**Architecture:** `named_set_api.rs` already holds the six operations once, over `MembershipSpec`. What is still duplicated is the HTTP surface: twelve near-identical handler stubs, each three to six lines of body under a `#[utoipa::path]` attribute, differing only in path strings, tag, doc comment, function name, and `group_spec()` versus `tag_spec()`. A single `macro_rules!` macro takes those differences as parameters and emits all six handlers for one collection. Two invocations replace two files.

**Tech Stack:** Rust, Axum 0.8.9, utoipa 5.5 + utoipa-axum 0.2, `macro_rules!`.

**Spec:** `docs/superpowers/specs/2026-09-03-http-interface-repair-design.md`, section "Named sets". Roadmap: `docs/superpowers/plans/2026-09-03-http-interface-repair-roadmap.md`, PR 8.

## Global Constraints

- **`docs/src/assets/openapi.json` must not change at all.** After regenerating, `git diff -- docs/src/assets/openapi.json` shows nothing. If it shows something, the fold changed a route and the fold is wrong. The same goes for `web/src/lib/vaultApi.types.ts`.
- **Two things pin the generated document to the code you are moving.** `operationId` comes from the *function name*, and `summary` comes from the *doc comment's first line*. Every generated function therefore keeps the exact name and the exact doc comment its handwritten twin had, character for character, including the en dash in "A–Z".
- `openapi::tests::committed_openapi_matches_dump` pins the committed JSON to the annotations; `scripts/check-generated-api-types.sh` pins the TypeScript to the JSON.
- ADR-0005: every route answers in the one shape, and the status carries the meaning. Nothing here changes a status.
- Use the fixture at `crates/vault/server/src/test_support.rs` if you add a test — `serve()`, `test_vault()`, `TestVault::account()`, `seed_conversation()`. Do not write a `fn setup()` and do not bind a listener.
- `./scripts/check-pr.sh` passes on the head commit.
- Issue #281 is closed by this pull request.

## Feasibility, already proven

Do not spike this again; it was checked before the plan was written, on this branch, and then reverted.

`#[utoipa::path]` works inside a `macro_rules!` expansion with `path = $path` and `tag = $tag` as `literal` metavariables. `utoipa_axum::routes!` resolves the type the attribute generates for a macro-produced function from another module. `cargo run -p message-vault-server -- dump-openapi` emitted a complete, correct path entry for the spiked route — tag, summary from the `#[doc]`, `operationId` from the function name, and all three responses.

The one thing that does **not** work: `path = concat!("/v1/", $base)`. utoipa parses the attribute before `concat!` would expand, so it needs a real string literal. That is why the macro takes all three paths spelled out rather than building them from a base.

## File Structure

- **Modify:** `crates/vault/server/src/named_set_api.rs` — gains the macro and both invocations. Its module doc changes: it is now the whole HTTP surface, not the shared half of one.
- **Delete:** `crates/vault/server/src/contact_groups_api.rs`, `crates/vault/server/src/message_tags_api.rs`.
- **Modify:** `crates/vault/server/src/lib.rs` — drop the two `pub(crate) mod` lines.
- **Modify:** `crates/vault/server/src/openapi.rs:88-103` — the twelve `routes!` entries point at `crate::named_set_api::` instead.
- **Untouched:** `crates/vault/server/src/named_membership.rs`, every `db/` module, all of `web/`, and `docs/src/assets/openapi.json`.

---

### Task 1: The macro, and Contact Groups through it

**Files:**
- Modify: `crates/vault/server/src/named_set_api.rs`
- Delete: `crates/vault/server/src/contact_groups_api.rs`
- Modify: `crates/vault/server/src/lib.rs:13`
- Modify: `crates/vault/server/src/openapi.rs:88-97`

**Interfaces:**
- Consumes: `named_set_api::{list, create, update, delete, members_list, members_update}` — already in the file, unchanged; `crate::named_membership::group_spec`.
- Produces: `macro_rules! named_set_routes`, invoked once per collection. Task 2 invokes it a second time and changes nothing about it.

- [ ] **Step 1: Read what you are replacing**

Read `crates/vault/server/src/contact_groups_api.rs` end to end (149 lines). Every string in it — paths, tag, `description`, doc comments — has to survive into the macro invocation unchanged.

- [ ] **Step 2: Add the two imports the macro needs**

In `crates/vault/server/src/named_set_api.rs`, the existing line

```rust
use crate::server::{ApiError, AppState};
```

becomes

```rust
use crate::server::{ApiError, AppState, ErrorBody, FullAccess};
```

`ErrorBody` is named in the response annotations exactly as the two handwritten files named it. `Path` stays unimported; the macro writes `crate::extract::Path` in full.

- [ ] **Step 3: Add the macro**

Append to `crates/vault/server/src/named_set_api.rs`, after the six operation functions and **before** `mod tests`:

```rust
/// One collection's six HTTP handlers.
///
/// Contact Groups and Message Tags are the same six operations over
/// [`MembershipSpec`]; what differs is the paths, the tag, the noun in the
/// prose, and which spec function to call. utoipa needs a concrete function
/// per route with literal strings in its attribute — it cannot describe a
/// generic, and it cannot see through `concat!` — so the handlers are stamped
/// out here rather than written twice.
///
/// The function names and doc comments are load-bearing: `operationId` comes
/// from the name and `summary` from the first line of the doc, so both appear
/// in `docs/src/assets/openapi.json`.
macro_rules! named_set_routes {
    (
        spec: $spec:path,
        tag: $tag:literal,
        id_description: $id_description:literal,
        root_path: $root_path:literal,
        id_path: $id_path:literal,
        members_path: $members_path:literal,
        list: $list_fn:ident, $list_doc:literal,
        create: $create_fn:ident, $create_doc:literal,
        update: $update_fn:ident, $update_doc:literal,
        delete: $delete_fn:ident, $delete_doc:literal,
        members_list: $members_list_fn:ident, $members_list_doc:literal,
        members_update: $members_update_fn:ident, $members_update_doc:literal,
    ) => {
        #[doc = $list_doc]
        #[utoipa::path(
            get,
            path = $root_path,
            tag = $tag,
            security(("bearer" = [])),
            responses(
                (status = 200, body = NamedSetList),
                (status = 401, body = ErrorBody),
                (status = 403, body = ErrorBody)
            )
        )]
        pub(crate) async fn $list_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
        ) -> Result<Json<NamedSetList>, ApiError> {
            list($spec(), &state, &auth.account_id).await
        }

        #[doc = $create_doc]
        #[utoipa::path(
            post,
            path = $root_path,
            tag = $tag,
            security(("bearer" = [])),
            request_body = NamedSetBody,
            responses(
                (status = 200, body = NamedSet),
                (status = 400, body = ErrorBody),
                (status = 401, body = ErrorBody),
                (status = 403, body = ErrorBody),
                (status = 409, body = ErrorBody)
            )
        )]
        pub(crate) async fn $create_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
            Json(body): Json<NamedSetBody>,
        ) -> Result<Json<NamedSet>, ApiError> {
            create($spec(), &state, &auth.account_id, body).await
        }

        #[doc = $update_doc]
        #[utoipa::path(
            patch,
            path = $id_path,
            tag = $tag,
            security(("bearer" = [])),
            params(("id" = i64, Path, description = $id_description)),
            request_body = NamedSetBody,
            responses(
                (status = 200, body = NamedSet),
                (status = 400, body = ErrorBody),
                (status = 401, body = ErrorBody),
                (status = 403, body = ErrorBody),
                (status = 404, body = ErrorBody),
                (status = 409, body = ErrorBody)
            )
        )]
        pub(crate) async fn $update_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
            crate::extract::Path(id): crate::extract::Path<i64>,
            Json(body): Json<NamedSetBody>,
        ) -> Result<Json<NamedSet>, ApiError> {
            update($spec(), &state, &auth.account_id, id, body).await
        }

        #[doc = $delete_doc]
        #[utoipa::path(
            delete,
            path = $id_path,
            tag = $tag,
            security(("bearer" = [])),
            params(("id" = i64, Path, description = $id_description)),
            responses(
                (status = 204),
                (status = 401, body = ErrorBody),
                (status = 403, body = ErrorBody),
                (status = 404, body = ErrorBody)
            )
        )]
        pub(crate) async fn $delete_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
            crate::extract::Path(id): crate::extract::Path<i64>,
        ) -> Result<StatusCode, ApiError> {
            delete($spec(), &state, &auth.account_id, id).await
        }

        #[doc = $members_list_doc]
        #[utoipa::path(
            get,
            path = $members_path,
            tag = $tag,
            security(("bearer" = [])),
            params(("id" = i64, Path, description = $id_description)),
            responses(
                (status = 200, body = MemberIdList),
                (status = 401, body = ErrorBody),
                (status = 403, body = ErrorBody),
                (status = 404, body = ErrorBody)
            )
        )]
        pub(crate) async fn $members_list_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
            crate::extract::Path(id): crate::extract::Path<i64>,
        ) -> Result<Json<MemberIdList>, ApiError> {
            members_list($spec(), &state, &auth.account_id, id).await
        }

        #[doc = $members_update_doc]
        #[utoipa::path(
            patch,
            path = $members_path,
            tag = $tag,
            security(("bearer" = [])),
            params(("id" = i64, Path, description = $id_description)),
            request_body = MembersPatch,
            responses(
                (status = 200, body = MembersChanged),
                (status = 400, body = ErrorBody),
                (status = 401, body = ErrorBody),
                (status = 403, body = ErrorBody),
                (status = 404, body = ErrorBody)
            )
        )]
        pub(crate) async fn $members_update_fn(
            axum::extract::State(state): axum::extract::State<AppState>,
            FullAccess(auth): FullAccess,
            crate::extract::Path(id): crate::extract::Path<i64>,
            Json(body): Json<MembersPatch>,
        ) -> Result<Json<MembersChanged>, ApiError> {
            members_update($spec(), &state, &auth.account_id, id, body).await
        }
    };
}
```

Note the `Path` inside `params(...)` is utoipa's own parameter-location keyword, not the extractor — leave it as the bare word `Path`, exactly as the handwritten files had it.

- [ ] **Step 4: Invoke it for Contact Groups**

Immediately after the macro definition:

```rust
named_set_routes! {
    spec: crate::named_membership::group_spec,
    tag: "Contacts",
    id_description: "Contact Group id",
    root_path: "/v1/contact-groups",
    id_path: "/v1/contact-groups/{id}",
    members_path: "/v1/contact-groups/{id}/members",
    list: contact_groups_list, "The account's Contact Groups, A–Z.",
    create: contact_groups_create, "Create a Contact Group.",
    update: contact_groups_update, "Rename a Contact Group.",
    delete: contact_groups_delete, "Delete a Contact Group and its memberships.",
    members_list: contact_group_members_list, "Contact ids in one Contact Group.",
    members_update: contact_group_members_update,
        "Put contacts in and take contacts out of one Contact Group.",
}
```

Check each string against `contact_groups_api.rs` before moving on. "A–Z" uses an en dash (U+2013), not a hyphen.

- [ ] **Step 5: Delete the file and its module line**

```bash
git rm crates/vault/server/src/contact_groups_api.rs
```

Remove `pub(crate) mod contact_groups_api;` from `crates/vault/server/src/lib.rs:13`.

- [ ] **Step 6: Repoint the six route registrations**

In `crates/vault/server/src/openapi.rs`, lines 88-97, change `crate::contact_groups_api::` to `crate::named_set_api::` in all six entries. Leave their order alone — it decides nothing in the JSON, but an unnecessary reordering makes the diff harder to trust.

- [ ] **Step 7: Build**

Run: `cargo build -p message-vault-server`
Expected: clean. A "cannot find function" here means a name in the invocation does not match what `openapi.rs` asks for.

- [ ] **Step 8: Prove the document did not move**

```bash
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
git diff --stat -- docs/src/assets/openapi.json
```

Expected: **no output from the diff.** If there is any, stop and read it — a changed `operationId` means a renamed function, a changed `summary` means an altered doc comment, and a changed path means a mistyped literal. Fix the invocation, do not accept the new JSON.

- [ ] **Step 9: Run the tests**

Run: `cargo test -p message-vault-server 2>&1 | tail -5`
Expected: all pass, including `openapi::tests::committed_openapi_matches_dump`. The count should be unchanged from `main`.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "refactor(server): one macro stamps out a named set's six routes"
```

---

### Task 2: Message Tags through the same macro

**Files:**
- Modify: `crates/vault/server/src/named_set_api.rs`
- Delete: `crates/vault/server/src/message_tags_api.rs`
- Modify: `crates/vault/server/src/lib.rs:25`
- Modify: `crates/vault/server/src/openapi.rs:98-103`

**Interfaces:**
- Consumes: `named_set_routes!` from Task 1, unchanged. If this task needs to edit the macro, something in Task 1 was wrong for the general case — say so rather than special-casing.

- [ ] **Step 1: Invoke the macro for Message Tags**

Read `crates/vault/server/src/message_tags_api.rs` first, then add after the Contact Groups invocation in `named_set_api.rs`:

```rust
named_set_routes! {
    spec: crate::named_membership::tag_spec,
    tag: "Message tags",
    id_description: "Message Tag id",
    root_path: "/v1/message-tags",
    id_path: "/v1/message-tags/{id}",
    members_path: "/v1/message-tags/{id}/members",
    list: message_tags_list, "The account's Message Tags, A–Z.",
    create: message_tags_create, "Create a Message Tag.",
    update: message_tags_update, "Rename a Message Tag.",
    delete: message_tags_delete, "Delete a Message Tag and its memberships.",
    members_list: message_tag_members_list, "Conversation ids in one Message Tag.",
    members_update: message_tag_members_update,
        "Put conversations in and take conversations out of one Message Tag.",
}
```

The tag is `"Message tags"` — lower-case second word. That is what the committed document says, so it is what this must say.

- [ ] **Step 2: Delete the file and its module line**

```bash
git rm crates/vault/server/src/message_tags_api.rs
```

Remove `pub(crate) mod message_tags_api;` from `crates/vault/server/src/lib.rs:25`.

- [ ] **Step 3: Repoint the six route registrations**

In `crates/vault/server/src/openapi.rs`, lines 98-103, change `crate::message_tags_api::` to `crate::named_set_api::`.

- [ ] **Step 4: Rewrite the module doc**

`crates/vault/server/src/named_set_api.rs` opens with a comment that describes an arrangement this pull request ends:

```rust
//! One HTTP surface for Contact Groups and Message Tags.
//!
//! Both are a named set the account owns plus a membership of contact or
//! conversation ids. The request and response types and the six operations
//! live here once, over [`MembershipSpec`]; `contact_groups_api.rs` and
//! `message_tags_api.rs` keep one three-line handler per route so every path
//! stays greppable and utoipa has a concrete function to describe.
```

Replace the second paragraph so it describes what the file now is: the request and response types, the six operations over `MembershipSpec`, and the macro that stamps both collections' twelve routes out of them. Say why the macro exists — utoipa needs a concrete function with literal strings per route — and say that the route paths are greppable in the two invocations. Keep the first line.

- [ ] **Step 5: Confirm nothing still names the deleted modules**

```bash
grep -rn 'contact_groups_api\|message_tags_api' --include='*.rs' crates/
```

Expected: no output. If a doc comment elsewhere in the crate names either file, fix it — the same slip cost PR 7 a review finding.

- [ ] **Step 6: Build and prove the document still did not move**

```bash
cargo build -p message-vault-server
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
git diff --stat -- docs/src/assets/openapi.json
(cd web && npm run gen:api)
git diff --stat -- web/src/lib/vaultApi.types.ts
```

Expected: both diffs empty. This is the task's real test — the routes are unchanged, so the generated artifacts must be byte-identical to what is on main.

- [ ] **Step 7: Run the full gate**

```bash
cargo test -p message-vault-server 2>&1 | tail -5
./scripts/check-pr.sh
```

Expected: tests pass with the same count as main; `check-pr.sh` exits 0.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(server): message tags come from the same macro, and both route files go"
```

---

## Verification

The whole pull request is verified by four things:

```bash
ls crates/vault/server/src | grep -E 'contact_groups_api|message_tags_api'   # nothing
git diff origin/main --stat -- docs/src/assets/openapi.json                   # nothing
git diff origin/main --stat -- web/src/lib/vaultApi.types.ts                  # nothing
./scripts/check-pr.sh                                                         # exit 0
```

Compare against `origin/main`, not `main`. This repository's local `main` ref
is stale — the checkout that owns it sits well behind the remote — so
`git diff main` reports a large false difference in both generated files.

The second and third are the ones that matter. Every route in this pull request already had tests and an entry in the committed document; if that document is byte-identical and the tests still pass, the twelve handlers behave as they did.
