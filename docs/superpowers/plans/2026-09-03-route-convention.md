# Route Convention Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every route on the vault's HTTP interface follows one convention: lists take `offset` and `limit` and return `{items, total, limit, offset}`, failures return `{error}` with the status (Axum's own rejections included), no response carries `ok`, every id is an integer, Export pages by offset with no `source=` parameter, and the pull library pages by offset.

**Architecture:** One `paging` module on the server owns the page envelope `Page<T>`, the query shape, and the one validator that turns `limit`/`offset` into a 400 instead of a silent clamp. One `extract` module wraps Axum's `Query`, `Path`, and `Json` so a malformed request is answered in the vault's own error body. Every route file swaps its imports and its envelope; the OpenAPI document and the web's generated types follow, and the web then compiles against the new shapes. The three client crates (`vault-pull`, `vault-push`, `vault-http`) stop reading `ok` and decide success by HTTP status.

**Tech Stack:** Rust 2024, Axum 0.8.9 (no `macros` feature), utoipa 5.5 (generic `ToSchema` is supported), sqlx `AnyConnection`; React 19 + TypeScript + TanStack Query, Vitest; `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json` and `(cd web && npm run gen:api)` regenerate the OpenAPI document and the web types.

**Spec:** `docs/superpowers/specs/2026-09-03-http-interface-repair-design.md`, section "Interface convention (ADR-0005)", and `docs/adr/0005-one-shape-for-every-route-on-the-http-interface.md`. This is pull request 2 of the eight in the spec's "Delivery" section.

## Global Constraints

- A list is `{items, total, limit, offset}`. A response that is nothing but a list is keyed `items` even when it is not paged (saved searches, import sessions, a conversation's sources, contact summaries). A report that carries a list among other facts keeps that field's descriptive name (`ImportContactsResponse.contacts`, `AccountStorageResponse.top_attachments`).
- `limit` above `MAX_LIST_LIMIT` (500) or equal to 0 is a 400. `offset` above `MAX_LIST_OFFSET` (50 000) is a 400 on the Contacts and Conversations lists. Export has no offset cap: its job is to walk the whole set, and an offset past the end returns an empty page with the true `total`.
- `MAX_LIST_LIMIT` is one number, 500, for every paged list. The conversation list's cap rises from 100 to 500 with it. The contact summaries body cap gets its own name, `MAX_CONTACT_SUMMARY_IDS`.
- A failure is `{"error": "<sentence>"}` with the status. No `ok` field on any response, success or failure. A route whose success body would be empty after `ok` goes returns `204 No Content`.
- Every id on the interface is an integer. API token ids and account ids are opaque strings and stay strings; they were never integers.
- `CompleteImportBody.ok` is a request field (the client reporting whether its push succeeded) and is out of scope.
- Breaking wire changes are accepted (ADR-0005, "Consequences"). Nothing is kept for compatibility.
- The server crate takes no new dependency. The rejection mapping is three hand-written extractors.
- After every server task: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json`, then `cargo test -p message-vault-server`. The test `committed_openapi_matches_dump` fails otherwise.
- Before the final commit: `./scripts/check-pr.sh` from the repo root.
- Commit messages: conventional commit subject, plain-English body explaining what and why, ending with the two attribution lines this session uses.

---

## File map

| File | Responsibility after this plan |
| --- | --- |
| `crates/vault/server/src/paging.rs` (create; replaces `page_limits.rs`) | `Page<T>`, `PageQuery`, `PageParams`, `page_params()`, and the paging constants. |
| `crates/vault/server/src/extract.rs` (create) | `Query`, `Path`, `Json` wrappers whose rejections are `ApiError::BadRequest`. |
| `crates/vault/server/src/server.rs` | `ErrorBody { error }`, `ApiError::MethodNotAllowed`, JSON 404 for unknown `/v1/*`, JSON 405, the banner. `ListPageQuery` deleted. |
| `crates/vault/server/src/conversations_api.rs` | `Page<ConversationSummary>` with `id: i64`; `ConversationSourcesPage { items }`; validation through `page_params`. |
| `crates/vault/server/src/contacts_api.rs` | `Page<ContactSummary>`; `ContactSummariesPage { items }` capped by `MAX_CONTACT_SUMMARY_IDS`. |
| `crates/vault/server/src/export_api.rs` | `Page<ExportMessage>` by offset; no cursor, no `source=`, no `ok`, no `query` echo; count response without `ok`. |
| `crates/vault/server/src/saved_searches_api.rs` | `{items}` list; create and update return the `SavedSearch`; delete is 204. |
| `crates/vault/server/src/{auth,profile,api_tokens_api,assets,admin_api}.rs`, `import/mod.rs` | `ok` gone; empty acknowledgements are 204; `ImportsListResponse { items }`. |
| `crates/vault/server/tests/search_parity.rs` | Builds `ExportPageOpts` without `cursor`. |
| `docs/src/assets/openapi.json`, `web/src/lib/vaultApi.types.ts` | Regenerated. |
| `web/src/lib/vaultApi.ts` | Numeric conversation id on the sources route; `void` returns for 204 routes; no `source` on the count route. |
| `web/src/lib/savedSearches.ts` | Reads `items`; writes invalidate the list instead of storing the answer. |
| `web/src/screens/message/useConversationMessages.ts` | Reads `items` and `total` from the export page; no count call; numeric id. |
| `web/src/lib/fetchConversationById.ts`, `messagesLocationState.ts`, `vaultKeys.ts`, `components/{MessageRoute,ConversationRow,SourcesPanel}.tsx`, `screens/{ConversationList,MessageView}.tsx` | Conversation id is a `number`. |
| `web/src/screens/settings/storage/useStorageData.ts`, `components/{SourcesPanel,CheckedContactsPanel}.tsx` | Read `items`. |
| `crates/libs/vault-pull/src/{http,run,lib}.rs` | Offset paging, page size 500, no `source`, no `compose_query`, true module doc. |
| `src-tauri/src/commands/pull.rs` | No `source` field. |
| `crates/libs/vault-push/src/http.rs`, `tests/push_mock.rs` | Success by status; `{error}` on failure; fixtures without `ok`. |
| `crates/libs/vault-http/src/session.rs` | Auth check without `ok`/`account_ok`. |
| `scripts/test/smoke-{export-api,import-api,vault-push}.sh` | Assert on real fields, not `"ok":true`. |
| `CHANGELOG.md` | Changed and Removed entries dated 2026-09-03. |

---

### Task 1: The `paging` module

**Files:**
- Create: `crates/vault/server/src/paging.rs`
- Delete: `crates/vault/server/src/page_limits.rs`
- Modify: `crates/vault/server/src/lib.rs:30` (`pub(crate) mod page_limits;` → `pub(crate) mod paging;`)
- Modify: `crates/vault/server/src/contacts_api.rs:20`, `conversations_api.rs:18-20`, `export_api.rs:17` (the `use crate::page_limits::…` lines → `crate::paging::…`)
- Test: unit tests inside `paging.rs`

**Interfaces:**
- Produces: `crate::paging::{Page<T>, PageQuery, PageParams, page_params, DEFAULT_LIST_LIMIT, DEFAULT_EXPORT_LIMIT, MAX_LIST_LIMIT, MAX_LIST_OFFSET, MAX_CONTACT_SUMMARY_IDS}`. `MAX_CONVERSATION_LIST_LIMIT`, `MAX_EXPORT_LIMIT`, and `MAX_EXPORT_OFFSET` stay in this task only so the crate keeps compiling; Tasks 2 and 3 delete them.

- [ ] **Step 1: Write the failing tests**

Create `crates/vault/server/src/paging.rs`:

```rust
//! One shape for every paged list on the HTTP interface (ADR-0005).
//!
//! A list takes `?offset=&limit=` and answers `{items, total, limit, offset}`.
//! A `limit` above the cap or a zero `limit` is a 400, never a silent clamp,
//! so a caller learns the rule the first time it breaks it.

use serde::{Deserialize, Serialize};

use crate::server::ApiError;

/// Default page size for the Contacts and Conversations lists.
pub const DEFAULT_LIST_LIMIT: usize = 40;
/// Default page size for `GET /v1/export/messages`.
pub const DEFAULT_EXPORT_LIMIT: usize = 100;
/// The largest page any list route returns. One number, one meaning.
pub const MAX_LIST_LIMIT: usize = 500;
/// Cap on `OFFSET` skips for the Contacts and Conversations lists. Export has
/// no cap: it walks the whole set.
pub const MAX_LIST_OFFSET: usize = 50_000;
/// Most contact ids one `POST /v1/contacts/summaries` body may carry, so the
/// `IN` list stays under SQLite's variable cap.
pub const MAX_CONTACT_SUMMARY_IDS: usize = 500;

// Kept only until Task 2 (conversations) and Task 3 (export) land.
pub const MAX_CONVERSATION_LIST_LIMIT: usize = 100;
pub const MAX_EXPORT_LIMIT: usize = 500;
pub const MAX_EXPORT_OFFSET: usize = 50_000;

/// One page of a list.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Page<T> {
    /// The rows on this page.
    pub items: Vec<T>,
    /// Rows matching the query across every page.
    pub total: u64,
    /// Page size used.
    pub limit: usize,
    /// Page offset used.
    pub offset: usize,
}

/// The query string every plain list route takes.
#[derive(Debug, Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// A validated `limit` and `offset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    pub limit: usize,
    pub offset: usize,
}

/// Turn the raw `limit` and `offset` into a page, or a 400 that says which
/// one is wrong. `max_offset` is `None` for a route that may walk the whole set.
pub fn page_params(
    limit: Option<usize>,
    offset: Option<usize>,
    default_limit: usize,
    max_offset: Option<usize>,
) -> Result<PageParams, ApiError> {
    let limit = limit.unwrap_or(default_limit);
    if limit == 0 {
        return Err(ApiError::BadRequest("limit must be at least 1".into()));
    }
    if limit > MAX_LIST_LIMIT {
        return Err(ApiError::BadRequest(format!(
            "limit exceeds maximum of {MAX_LIST_LIMIT}"
        )));
    }
    let offset = offset.unwrap_or(0);
    if let Some(max) = max_offset {
        if offset > max {
            return Err(ApiError::BadRequest(format!(
                "offset exceeds maximum of {max}"
            )));
        }
    }
    Ok(PageParams { limit, offset })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_fill_in_when_nothing_is_sent() {
        let p = page_params(None, None, DEFAULT_LIST_LIMIT, Some(MAX_LIST_OFFSET)).unwrap();
        assert_eq!(p, PageParams { limit: 40, offset: 0 });
    }

    #[test]
    fn a_limit_above_the_cap_is_refused_not_clamped() {
        let err = page_params(Some(501), None, 40, None).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(m) if m == "limit exceeds maximum of 500"));
        let p = page_params(Some(500), None, 40, None).unwrap();
        assert_eq!(p.limit, 500);
    }

    #[test]
    fn a_zero_limit_is_refused() {
        let err = page_params(Some(0), None, 40, None).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(m) if m == "limit must be at least 1"));
    }

    #[test]
    fn an_offset_past_the_cap_is_refused_only_when_a_cap_is_given() {
        let err = page_params(None, Some(50_001), 40, Some(MAX_LIST_OFFSET)).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(m) if m == "offset exceeds maximum of 50000"));
        let p = page_params(None, Some(50_001), 40, None).unwrap();
        assert_eq!(p.offset, 50_001);
    }

    #[test]
    fn a_page_serializes_with_the_four_agreed_keys() {
        let page = Page { items: vec![1, 2], total: 9, limit: 2, offset: 4 };
        let json = serde_json::to_value(&page).unwrap();
        assert_eq!(json, serde_json::json!({"items": [1, 2], "total": 9, "limit": 2, "offset": 4}));
    }
}
```

- [ ] **Step 2: Wire the module and delete the old one**

Delete `crates/vault/server/src/page_limits.rs`. In `crates/vault/server/src/lib.rs` replace `pub(crate) mod page_limits;` with `pub(crate) mod paging;`. Update the three importers:

- `contacts_api.rs:20`: `pub use crate::paging::{DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT, MAX_LIST_OFFSET};`
- `conversations_api.rs:18-20`: `pub use crate::paging::{DEFAULT_LIST_LIMIT, MAX_CONVERSATION_LIST_LIMIT as MAX_LIST_LIMIT, MAX_LIST_OFFSET};`
- `export_api.rs:17`: `pub use crate::paging::{DEFAULT_EXPORT_LIMIT, MAX_EXPORT_LIMIT, MAX_EXPORT_OFFSET};`

- [ ] **Step 3: Run the tests**

Run: `cargo test -p message-vault-server paging::`
Expected: 5 tests pass. Then `cargo test -p message-vault-server` passes in full (nothing else changed).

- [ ] **Step 4: Commit**

```bash
git add crates/vault/server/src/paging.rs crates/vault/server/src/lib.rs crates/vault/server/src/contacts_api.rs crates/vault/server/src/conversations_api.rs crates/vault/server/src/export_api.rs
git rm crates/vault/server/src/page_limits.rs
git commit -m "feat(vault-server): one page shape and one page validator for every list"
```

---

### Task 2: Conversations and Contacts on `Page<T>`, with integer conversation ids

**Files:**
- Modify: `crates/vault/server/src/conversations_api.rs` (`ConversationListPage` at 100-110, `ConversationSummary.id` at 134-138, `RawConversation` → summary at 300, `list_conversations_sorted` at 185-201, `ConversationsPageQuery` at 539-553, the handler at 556-610, `ConversationSourcesPage`, and the tests)
- Modify: `crates/vault/server/src/contacts_api.rs` (`ContactListPage` at 22-32, `ContactSummariesPage` at 177-181, `list_contacts` at 244-253, `get_contact_summaries` at 505-523, the handlers at 1104-1172, and the tests)
- Modify: `crates/vault/server/src/server.rs:745-753` (delete `ListPageQuery`)
- Modify: `crates/vault/server/src/paging.rs` (delete `MAX_CONVERSATION_LIST_LIMIT`)
- Test: HTTP tests appended to the test modules of both files

**Interfaces:**
- Consumes: `crate::paging::{Page, PageQuery, page_params, DEFAULT_LIST_LIMIT, MAX_LIST_OFFSET, MAX_CONTACT_SUMMARY_IDS}`.
- Produces: `list_conversations_sorted(conn, account_id, q, order, limit, offset, today) -> Result<Page<ConversationSummary>, ApiError>` and `list_contacts(conn, account_id, q, limit, offset, today) -> Result<Page<ContactSummary>, ApiError>`, both trusting their `limit`/`offset` (validation happens at the handler). `ConversationSummary.id: i64`. `ConversationSourcesPage { items }`. `ContactSummariesPage { items }`.

- [ ] **Step 1: Write the failing HTTP tests**

Append to the `mod tests` in `conversations_api.rs` (it already has HTTP tests near line 680 using `crate::test_support`; reuse their setup helper by name; the one at 660-692 is the model):

```rust
    #[tokio::test]
    async fn the_conversation_list_is_a_page_with_integer_ids() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;

        let page: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/conversations?limit=10", &user.token).await;
        assert_eq!(page["total"], 1);
        assert_eq!(page["limit"], 10);
        assert_eq!(page["offset"], 0);
        assert!(page["items"][0]["id"].is_i64(), "id must be an integer: {page}");
        assert!(page.get("conversations").is_none());
        assert!(page.get("ok").is_none());
    }

    #[tokio::test]
    async fn a_limit_past_the_cap_or_an_offset_past_the_cap_is_a_400() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;

        for path in [
            "/v1/conversations?limit=501",
            "/v1/conversations?limit=0",
            "/v1/conversations?offset=50001",
        ] {
            let status = crate::test_support::get_status(&state, path, &user.token).await;
            assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{path}");
        }
    }
```

Append to the `mod tests` in `contacts_api.rs` (its HTTP tests near line 2660 show the setup):

```rust
    #[tokio::test]
    async fn the_contact_list_is_a_page_and_summaries_are_items() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;

        let page: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/contacts?limit=5", &user.token).await;
        assert_eq!(page["total"], 0);
        assert_eq!(page["limit"], 5);
        assert!(page["items"].is_array());
        assert!(page.get("contacts").is_none());

        let status =
            crate::test_support::get_status(&state, "/v1/contacts?limit=501", &user.token).await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);

        let summaries: serde_json::Value = crate::test_support::post_json(
            &state,
            "/v1/contacts/summaries",
            &user.token,
            serde_json::json!({ "ids": [] }),
        )
        .await;
        assert!(summaries["items"].is_array());
        assert!(summaries.get("contacts").is_none());
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p message-vault-server the_conversation_list_is_a_page the_contact_list_is_a_page a_limit_past_the_cap`
Expected: all three fail (`items` missing, `id` is a string, 200 where 400 expected).

- [ ] **Step 3: Conversations**

In `conversations_api.rs`:

1. Delete `ConversationListPage` (lines 100-110). Replace every `ConversationListPage` with `Page<ConversationSummary>` and every construction `ConversationListPage { conversations, total, limit, offset }` with `Page { items, total, limit, offset }`. Add `use crate::paging::{Page, page_params, DEFAULT_LIST_LIMIT, MAX_LIST_OFFSET};` and delete the `pub use crate::paging::{…}` alias line.
2. `ConversationSummary.id: i64`, doc comment `/// The conversation's id; search for it as \`in:#<id>\`.` At line 300, `id: row.id,`.
3. In `list_conversations_sorted` delete the four lines `let limit = limit.clamp(1, MAX_LIST_LIMIT);` and the `if offset > MAX_LIST_OFFSET { … }` block. The function trusts its arguments.
4. In `conversations_list_handler` replace
   ```rust
   let limit = query.limit.unwrap_or(DEFAULT_LIST_LIMIT);
   let offset = query.offset.unwrap_or(0);
   ```
   with
   ```rust
   let page = page_params(query.limit, query.offset, DEFAULT_LIST_LIMIT, Some(MAX_LIST_OFFSET))?;
   ```
   and pass `page.limit, page.offset` to `list_conversations_sorted`. The `#[utoipa::path]` response becomes `(status = 200, body = crate::paging::Page<crate::conversations_api::ConversationSummary>)`; the `limit` description becomes `"Page size, default 40, max 500"` and `offset` `"Page offset, max 50000"`.
5. `ConversationSourcesPage`: rename the field `sources` to `items` (struct and the construction at line 535).
6. Tests: run `grep -n '\.conversations\b\|conversations: \|MAX_LIST_LIMIT\|\.sources\b' crates/vault/server/src/conversations_api.rs` and at every hit inside `mod tests`: `page.conversations` → `page.items`; a JSON assertion `["conversations"]` → `["items"]`; the clamp test at 1035-1038 (`clamped.limit == MAX_LIST_LIMIT`) is deleted, since validation now lives in the handler and is covered by Step 1; string id comparisons such as `c.id == "1"` become `c.id == 1`; `["sources"]` → `["items"]`.

- [ ] **Step 4: Contacts**

In `contacts_api.rs`:

1. Delete `ContactListPage` (22-32); use `Page<ContactSummary>` and `Page { items, total, limit, offset }` at the construction site. Imports: `use crate::paging::{Page, PageQuery, page_params, DEFAULT_LIST_LIMIT, MAX_LIST_OFFSET, MAX_CONTACT_SUMMARY_IDS};` replacing the `pub use` line.
2. `ContactSummariesPage { items: Vec<ContactSelectionSummary> }`; the handler builds `ContactSummariesPage { items }`.
3. In `list_contacts` delete the clamp and the offset block (lines 249-253).
4. `contacts_list_handler`: `Query(query): Query<PageQuery>` (was `crate::server::ListPageQuery`), and
   ```rust
   let page = page_params(query.limit, query.offset, DEFAULT_LIST_LIMIT, Some(MAX_LIST_OFFSET))?;
   ```
   passing `page.limit, page.offset`. utoipa response `Page<ContactSummary>`, descriptions as in Step 3.
5. `contact_summaries_handler`: the cap check uses `MAX_CONTACT_SUMMARY_IDS` with message `"at most {MAX_CONTACT_SUMMARY_IDS} contact ids"`. `get_contact_summaries`: the `unique.len() == MAX_LIST_LIMIT` guard and its doc comment use `MAX_CONTACT_SUMMARY_IDS`.
6. Tests: `grep -n '\.contacts\b\|contacts: \|MAX_LIST_LIMIT' crates/vault/server/src/contacts_api.rs` and inside `mod tests`: `.contacts` on a page → `.items`; `["contacts"]` → `["items"]`; the clamp test at 1560-1566 is deleted.

- [ ] **Step 5: Delete the old pieces**

Delete `ListPageQuery` from `server.rs:745-753` (and its `Deserialize` import if now unused). Delete `MAX_CONVERSATION_LIST_LIMIT` from `paging.rs`.

- [ ] **Step 6: Regenerate and test**

Run: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json && cargo test -p message-vault-server`
Expected: all pass, including the three new tests.

- [ ] **Step 7: Commit**

```bash
git add crates/vault/server/src docs/src/assets/openapi.json
git commit -m "feat(vault-server): the conversation and contact lists are pages with integer ids"
```

---

### Task 3: Export pages by offset, without `source=`, cursor, `ok`, or an echoed query

**Files:**
- Modify: `crates/vault/server/src/export_api.rs` (structs at 19-79, `PageCursor` at 200-222, `source_word`/`with_source_word` at 224-272, `export_messages` at 290-457, `export_message_count` at 470-527, the query structs at 686-710, both handlers at 712-800, tests)
- Modify: `crates/vault/server/tests/search_parity.rs:190-240`
- Modify: `crates/vault/server/src/paging.rs` (delete `MAX_EXPORT_LIMIT`, `MAX_EXPORT_OFFSET`)
- Modify: `crates/vault/server/src/server.rs:534,539` (banner lines)
- Test: unit tests in `export_api.rs`, one new HTTP test

**Interfaces:**
- Produces: `ExportPageOpts { account_id: &str, query: &str, limit: usize, offset: usize, today }`, `export_messages(conn, opts) -> Result<Page<ExportMessage>, ApiError>`, `ExportCountResponse { messages, conversations, attachments, total_bytes }`, and `count_matching_messages(conn, &Filter) -> Result<u64, ApiError>` shared by both.

- [ ] **Step 1: Rewrite the paging test and add the HTTP test**

Replace `export_pages_with_cursor_and_limit` (1411-1460) with:

```rust
    #[tokio::test]
    async fn export_pages_by_offset_and_reports_the_total() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO messages (
                id, conversation_id, account_id, source, service, timestamp,
                is_from_me, sort_order, body
             ) VALUES (
                3, 1, 'a1', 'sms', 'sms', '2020-01-03T00:00:00Z',
                0, 0, 'third'
             )",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let page = |limit: usize, offset: usize| {
            let pool = pool.clone();
            async move {
                let mut conn = pool.acquire().await.unwrap();
                export_messages(
                    &mut conn,
                    ExportPageOpts {
                        account_id: "a1",
                        query: "",
                        limit,
                        offset,
                        today: crate::search::tests::today(),
                    },
                )
                .await
                .unwrap()
            }
        };

        let first = page(2, 0).await;
        assert_eq!(first.items.len(), 2);
        assert_eq!((first.total, first.limit, first.offset), (3, 2, 0));

        let second = page(2, 2).await;
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].id, 3);
        assert_eq!(second.total, 3);

        // Past the end is an empty page with the true total, not an error.
        let past = page(2, 10).await;
        assert!(past.items.is_empty());
        assert_eq!(past.total, 3);
    }

    #[tokio::test]
    async fn the_export_route_answers_a_page_and_refuses_a_bad_limit() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        crate::test_support::seed_one_message(&state, &user.account_id).await;

        let page: serde_json::Value = crate::test_support::get_json(
            &state,
            "/v1/export/messages?q=&limit=10",
            &user.token,
        )
        .await;
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"].as_array().unwrap().len(), 1);
        for gone in ["ok", "query", "messages", "next_cursor", "truncated"] {
            assert!(page.get(gone).is_none(), "{gone} must be gone: {page}");
        }

        let status = crate::test_support::get_status(
            &state,
            "/v1/export/messages?q=&limit=501",
            &user.token,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);

        let count: serde_json::Value = crate::test_support::get_json(
            &state,
            "/v1/export/messages/count?q=",
            &user.token,
        )
        .await;
        assert_eq!(count["messages"], 1);
        assert!(count.get("ok").is_none());
        assert!(count.get("query").is_none());
    }
```

In `rejects_an_oversized_query_and_offset` (1297-1337) delete the second half (the `MAX_EXPORT_OFFSET + 1` call) and rename the test `rejects_an_oversized_query`. Delete the three tests `source_param_maps_to_the_source_word`, `the_source_param_becomes_a_leading_source_word`, and `the_source_param_narrows_the_whole_query_not_just_its_first_branch` (858-957); the `source:` search word they exercised belongs to the search module and is tested there. Rename `export_takes_the_search_language_and_source_param` to `export_takes_the_search_language`.

- [ ] **Step 2: Run to see them fail to compile**

Run: `cargo test -p message-vault-server export_`
Expected: compile errors on `ExportPageOpts.cursor`, `.items`, `.total`.

- [ ] **Step 3: Rewrite the types**

Replace lines 19-79 of `export_api.rs` with:

```rust
/// One page of `GET /v1/export/messages`.
#[derive(Debug, Clone)]
pub struct ExportPageOpts<'a> {
    pub account_id: &'a str,
    pub query: &'a str,
    /// Already validated by the handler: 1..=MAX_LIST_LIMIT.
    pub limit: usize,
    pub offset: usize,
    pub today: chrono::NaiveDate,
}

/// Arguments for `GET /v1/export/messages/count`.
#[derive(Debug, Clone)]
pub struct ExportCountOpts<'a> {
    pub account_id: &'a str,
    pub query: &'a str,
    pub today: chrono::NaiveDate,
}

/// Match counts for an export query.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ExportCountResponse {
    /// Matching messages.
    pub messages: u64,
    /// Distinct conversations with at least one matching message.
    pub conversations: u64,
    /// Unique attachment fingerprints among matching messages.
    pub attachments: u64,
    /// Sum of known `size_bytes` for those unique fingerprints (unknown sizes omitted).
    pub total_bytes: u64,
}
```

(Keep whatever `ExportCountOpts` already looks like if it differs only in doc comments.) Delete `ExportMessagesResponse` and `PageCursor` (200-222) and `source_word` and `with_source_word` (224-272). Add `use crate::paging::{Page, page_params, DEFAULT_EXPORT_LIMIT};` and delete the `pub use crate::paging::{DEFAULT_EXPORT_LIMIT, MAX_EXPORT_LIMIT, MAX_EXPORT_OFFSET};` line.

- [ ] **Step 4: Rewrite `export_messages` and share the count**

Add, above `export_messages`:

```rust
/// `COUNT(*)` of the messages a compiled filter matches.
async fn count_matching_messages(
    conn: &mut AnyConnection,
    filter: &crate::search::Filter,
) -> Result<u64, ApiError> {
    let sql = format!(
        "SELECT COUNT(*)
         {messages_from_sql}
         WHERE {where_sql}",
        messages_from_sql = messages_from_sql(),
        where_sql = filter.where_sql(),
    );
    let n: i64 = (&mut *conn)
        .fetch_one(bind_all(&renumber_placeholders(&sql), filter.params()))
        .await?
        .try_get(0)?;
    Ok(n.max(0) as u64)
}
```

`export_messages` becomes (the `SELECT … FROM … WHERE` text and the `RawRow` mapping are unchanged from today; only the parts shown change):

```rust
pub async fn export_messages(
    conn: &mut AnyConnection,
    opts: ExportPageOpts<'_>,
) -> Result<Page<ExportMessage>, ApiError> {
    let filter = message_filter(engine_of(conn), opts.account_id, opts.query, opts.today)?;
    let total = count_matching_messages(conn, &filter).await?;

    let mut sql = format!(
        "SELECT m.id, m.conversation_id, m.source, m.service, m.guid, m.timestamp, m.timestamp_utc,
                m.sort_order, m.is_from_me, hs.raw AS sender, m.subject, m.body,
                m.is_announcement, m.is_reply, m.thread_originator_guid,
                m.thread_originator_part, m.num_replies,
                hc.raw AS chat_identifier, c.conversation_type, c.group_title
         {messages_from_sql}
         WHERE {where_sql}",
        messages_from_sql = messages_from_sql(),
        where_sql = filter.where_sql(),
    );
    let mut params = filter.params().to_vec();
    sql.push_str(" ORDER BY m.timestamp ASC, m.sort_order ASC, m.id ASC LIMIT ? OFFSET ?");
    params.push(SqlParam::Int(opts.limit as i64));
    params.push(SqlParam::Int(opts.offset as i64));

    let sql = renumber_placeholders(&sql);
    let rows = (&mut *conn).fetch_all(bind_all(&sql, &params)).await?;
    let page_rows: Vec<RawRow> = rows
        .iter()
        .map(|row| { /* unchanged RawRow mapping */ })
        .collect::<Result<Vec<RawRow>, ApiError>>()?;

    // (participants, attachments, tapbacks, and the ExportMessage mapping are unchanged)

    Ok(Page {
        items: messages,
        total,
        limit: opts.limit,
        offset: opts.offset,
    })
}
```

The doc comment on the function drops "or cursor". In `export_message_count`, replace the first `msg_sql` block (the `COUNT(*)` query and its `fetch_one`) with `let messages = count_matching_messages(conn, &filter).await?;` and build `ExportCountResponse { messages, conversations: …, attachments: …, total_bytes: … }` without `ok` and `query`.

- [ ] **Step 5: The query structs and handlers**

```rust
#[derive(Debug, Deserialize)]
pub(crate) struct ExportMessagesQuery {
    #[serde(default)]
    pub(crate) q: String,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) offset: Option<usize>,
    #[serde(default)]
    pub(crate) account: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExportMessagesCountQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    account: Option<String>,
}
```

Count handler: remove the `source` param line from `#[utoipa::path]`; `let q = query.q;` replaces the `with_source_word` call. Messages handler: doc comment `/// Export messages matching a query in the search language, a page at a time.`; params are `q`, `limit` ("Page size, default 100, max 500"), `offset` ("Page offset; no cap, an offset past the end is an empty page"), `account`; response `(status = 200, body = crate::paging::Page<crate::export_api::ExportMessage>)`; body:

```rust
    let account = resolve_import_account(&auth, query.account.as_deref(), &state.db).await?;
    let page = page_params(query.limit, query.offset, DEFAULT_EXPORT_LIMIT, None)?;
    let today = chrono::Local::now().date_naive();

    let mut conn = state.db.acquire().await?;
    let body = export_api::export_messages(
        &mut conn,
        ExportPageOpts {
            account_id: &account,
            query: &query.q,
            limit: page.limit,
            offset: page.offset,
            today,
        },
    )
    .await?;
    Ok(Json(body))
```

Return type `Result<Json<Page<export_api::ExportMessage>>, ApiError>`.

- [ ] **Step 6: Every other caller**

- `crates/vault/server/tests/search_parity.rs`: both `ExportPageOpts { … offset: None, cursor: None, … }` become `offset: 0,` with no `cursor`; `resp.messages` → `resp.items`.
- In `export_api.rs` tests, run `grep -n 'cursor: None\|offset: None\|\.messages\b\|truncated\|next_cursor' crates/vault/server/src/export_api.rs`: `offset: None, cursor: None,` → `offset: 0,`; `page.messages` → `page.items` (leave `count.messages`, which is the count).
- `lib.rs:49` re-exports `ExportPageOpts`; nothing to change unless it also names a deleted item.
- `paging.rs`: delete `MAX_EXPORT_LIMIT` and `MAX_EXPORT_OFFSET`.
- `server.rs` banner: line 534 → `eprintln!("  GET  /v1/export/messages?q=&limit=&offset=&account=  (download messages, a page at a time)");` and line 539 → `eprintln!("  GET  /v1/export/messages/count?q=&account=  (export match counts)");`.

- [ ] **Step 7: Regenerate and test**

Run: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json && cargo test -p message-vault-server && cargo test -p message-vault-server --test search_parity`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add crates/vault/server docs/src/assets/openapi.json
git commit -m "feat(vault-server): export pages by offset and drops its cursor, source= parameter, and ok flag"
```

---

### Task 4: Every failure is `{error}`, including Axum's own

**Files:**
- Create: `crates/vault/server/src/extract.rs`
- Modify: `crates/vault/server/src/lib.rs` (add `pub(crate) mod extract;`)
- Modify: `crates/vault/server/src/server.rs` (`ErrorBody` 263-268, `ApiError` 271-320, `http_app` 429-459)
- Modify: the `use axum::…` lines in `api_tokens_api.rs:3-4`, `auth.rs:9-10`, `assets.rs:18-19`, `contact_groups_api.rs:4-5`, `conversations_api.rs:5-6`, `admin_api.rs:8-9`, `export_api.rs:4-5`, `named_set_api.rs:9`, `message_tags_api.rs:4-5`, `saved_searches_api.rs:8-9`, `profile.rs:4-5`, `contacts_api.rs:8-9`, `search_api.rs:4-5`, `import/mod.rs:21-22`
- Test: `crates/vault/server/src/extract.rs` (HTTP tests inside the file)

**Interfaces:**
- Produces: `crate::extract::{Query<T>, Path<T>, Json<T>}`, drop-in for Axum's, whose rejection is `ApiError::BadRequest(<Axum's sentence>)`; `Json<T: Serialize>` also implements `IntoResponse`. `ErrorBody { error: String }`. `ApiError::MethodNotAllowed(String)` → 405. Unknown `/v1/*` → `{"error": "no route at /v1/…"}` with 404.

- [ ] **Step 1: Write the failing tests**

Create `crates/vault/server/src/extract.rs` with the tests first:

```rust
//! Axum's `Query`, `Path`, and `Json`, answering in the vault's own error body.
//!
//! Axum's extractors reject a bad request with a plain-text body. Every other
//! failure on this interface is `{"error": "<sentence>"}` with the status, so
//! these three wrappers turn each rejection into an [`ApiError::BadRequest`]
//! carrying Axum's sentence. Handlers use these names in place of Axum's.

use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::server::ApiError;

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::test_support::{register_via_api, test_vault};

    /// GET a path and return the status and the parsed JSON body.
    async fn get(state: &crate::server::AppState, path: &str, token: &str) -> (StatusCode, serde_json::Value) {
        let (status, text) = crate::test_support::get_raw(state, path, token).await;
        let body = serde_json::from_str(&text).unwrap_or_else(|_| panic!("{path} answered non-JSON: {text}"));
        (status, body)
    }

    #[tokio::test]
    async fn a_query_parameter_of_the_wrong_type_is_a_json_400() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let user = register_via_api(&state, "alice", "hunter2hunter2").await;
        let (status, body) = get(&state, "/v1/conversations?limit=ten", &user.token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("limit"), "{body}");
        assert!(body.get("ok").is_none());
    }

    #[tokio::test]
    async fn a_path_id_that_is_not_a_number_is_a_json_400() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let user = register_via_api(&state, "alice", "hunter2hunter2").await;
        let (status, body) = get(&state, "/v1/conversations/abc/sources", &user.token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].is_string(), "{body}");
    }

    #[tokio::test]
    async fn a_json_body_missing_a_field_is_a_json_400() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let user = register_via_api(&state, "alice", "hunter2hunter2").await;
        let (status, text) = crate::test_support::post_raw(
            &state,
            "/v1/saved-searches",
            &user.token,
            "application/json",
            r#"{"name": "only a name"}"#,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(body["error"].as_str().unwrap().contains("query"), "{body}");
    }

    #[tokio::test]
    async fn an_unknown_api_path_is_a_json_404_and_a_wrong_method_a_json_405() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let user = register_via_api(&state, "alice", "hunter2hunter2").await;
        let (status, body) = get(&state, "/v1/no-such-thing", &user.token).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "no route at /v1/no-such-thing");

        let status = crate::test_support::delete_status(&state, "/v1/conversations", &user.token).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    }
}
```

Add `get_raw` to `test_support.rs`, next to `post_raw`, returning `(StatusCode, String)` for a GET with a Bearer token (same listener boilerplate as `post_raw`, method GET, no body, no content type).

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p message-vault-server extract::`
Expected: compile error (no `get_raw`), then after adding it: the query test fails with a non-JSON body panic, the 404 test gets the static-file fallback.

- [ ] **Step 3: The three extractors**

Above the test module in `extract.rs`:

```rust
/// Axum's `Query`, rejecting as `{error}`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Query<T>(pub T);

impl<T, S> FromRequestParts<S> for Query<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(value)) => Ok(Query(value)),
            Err(rejection) => Err(ApiError::BadRequest(rejection.body_text())),
        }
    }
}

/// Axum's `Path`, rejecting as `{error}`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Path<T>(pub T);

impl<T, S> FromRequestParts<S> for Path<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Path(value)) => Ok(Path(value)),
            Err(rejection) => Err(ApiError::BadRequest(rejection.body_text())),
        }
    }
}

/// Axum's `Json`, rejecting as `{error}` and answering as JSON.
#[derive(Debug, Clone, Copy, Default)]
pub struct Json<T>(pub T);

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(value)) => Ok(Json(value)),
            Err(rejection) => Err(ApiError::BadRequest(rejection.body_text())),
        }
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}
```

Add `pub(crate) mod extract;` to `lib.rs`.

- [ ] **Step 4: Swap the imports**

In each listed file replace the Axum extractor imports with the crate's, keeping any alias the file already uses so handler bodies do not change:

| File | Old | New |
| --- | --- | --- |
| `api_tokens_api.rs:3-4` | `use axum::Json;` / `use axum::extract::{Path as AxumPath, State};` | `use axum::extract::State;` / `use crate::extract::{Json, Path as AxumPath};` |
| `auth.rs:9-10` | `use axum::Json;` / `use axum::extract::{Query, State};` | `use axum::extract::State;` / `use crate::extract::{Json, Query};` |
| `assets.rs:18-19` | `use axum::Json;` / `use axum::extract::{Path as AxumPath, Query, Request, State};` | `use axum::extract::{Request, State};` / `use crate::extract::{Json, Path as AxumPath, Query};` |
| `contact_groups_api.rs:4-5`, `message_tags_api.rs:4-5`, `admin_api.rs:8-9`, `saved_searches_api.rs:8-9` | `use axum::Json;` / `use axum::extract::{Path, State};` | `use axum::extract::State;` / `use crate::extract::{Json, Path};` |
| `conversations_api.rs:5-6`, `contacts_api.rs:8-9` | `use axum::Json;` / `use axum::extract::{Path as AxumPath, Query, State};` | `use axum::extract::State;` / `use crate::extract::{Json, Path as AxumPath, Query};` |
| `export_api.rs:4-5` | `use axum::Json;` / `use axum::extract::{Query, State};` | `use axum::extract::State;` / `use crate::extract::{Json, Query};` |
| `named_set_api.rs:9` | `use axum::Json;` | `use crate::extract::Json;` |
| `profile.rs:4-5` | `use axum::Json;` / `use axum::extract::State;` | `use axum::extract::State;` / `use crate::extract::Json;` |
| `search_api.rs:4-5` | `use axum::Json;` / `use axum::extract::Query;` | `use crate::extract::{Json, Query};` |
| `import/mod.rs:21-22` | `use axum::Json;` / `use axum::extract::{FromRequest, Multipart, Path as AxumPath, Query, Request, State};` | `use axum::extract::{FromRequest, Multipart, Request, State};` / `use crate::extract::{Json, Path as AxumPath, Query};` |

`server.rs:17` keeps `axum::Json` for `ApiError::into_response`. Any test module that does `use axum::Json` for building a request value can keep it; a test that pattern-matches `Json(x)` on a handler's return needs `crate::extract::Json`. Let the compiler list them.

utoipa's `axum_extras` feature reads the identifiers `Path` and `Query` in a handler's signature; the names are unchanged, so the generated document is unchanged except where this plan changes it.

- [ ] **Step 5: `ErrorBody`, 405, and the `/v1` fallbacks**

In `server.rs`:

```rust
/// The body of every failure: one sentence, with the HTTP status carrying the meaning.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    /// Human-readable description of the failure.
    pub error: String,
}
```

Add to `ApiError`: `/// \`405\` — the path exists but not for this method.` `MethodNotAllowed(String),` and in `into_response` the arm `Self::MethodNotAllowed(m) => (StatusCode::METHOD_NOT_ALLOWED, m),`. The `Json(ErrorBody { ok: false, error: message })` becomes `Json(ErrorBody { error: message })`.

Add two handlers above `http_app`:

```rust
/// A `/v1/…` path no route claims. Static files answer everything else.
async fn api_not_found(uri: axum::http::Uri) -> ApiError {
    ApiError::NotFound(format!("no route at {}", uri.path()))
}

/// A route that exists, asked with a method it does not take.
async fn api_method_not_allowed(method: axum::http::Method, uri: axum::http::Uri) -> ApiError {
    ApiError::MethodNotAllowed(format!("{method} is not allowed at {}", uri.path()))
}
```

and in `http_app`:

```rust
    let mut api = Router::new()
        .merge(doc_router)
        .merge(auth_small)
        .route("/v1/{*rest}", axum::routing::any(api_not_found))
        .method_not_allowed_fallback(api_method_not_allowed)
        .fallback_service(ServeDir::new("static"))
        .layer(build_cors_layer(&cors_origins))
        .layer(RequestBodyLimitLayer::new(state.max_body_bytes));
```

`ApiError` must implement `IntoResponse` (it does) for the handlers to return it directly.

- [ ] **Step 6: Regenerate and test**

Run: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json && cargo test -p message-vault-server`
Expected: all pass. The import tests at `import/mod.rs:3215-3254` already assert `err["error"]` and keep passing.

- [ ] **Step 7: Commit**

```bash
git add crates/vault/server docs/src/assets/openapi.json
git commit -m "feat(vault-server): every failure is {error} with the status, Axum's own rejections included"
```

---

### Task 5: No `ok` on any response

**Files:**
- Modify: `crates/vault/server/src/auth.rs` (`AuthCheckResponse` 233-243 and 282-289, `ChangePasswordResponse` 480-486, `DeleteAccountResponse` 499-503, `LogoutResponse` 506-510, and their handlers)
- Modify: `crates/vault/server/src/profile.rs:313-320,386-391` (`DeleteMessagesResponse`)
- Modify: `crates/vault/server/src/api_tokens_api.rs:120-124,135-145,220-290`
- Modify: `crates/vault/server/src/assets.rs:613-630,895-925,960-980,1025-1035,1100-1125`
- Modify: `crates/vault/server/src/admin_api.rs:322-332,400-410,595-632,986`
- Modify: `crates/vault/server/src/import/mod.rs:570-580,625-630,735-745,750-753,935-945,970-980,1248-1280,1310-1320,1352-1395` and the tests at 1705, 2398
- Modify: `crates/vault/server/src/server.rs` (banner; any test constructing a response with `ok`)
- Test: `admin_api.rs` tests at 595-632 rewritten; new assertions in `auth.rs` tests

**Interfaces:**
- Produces: these routes now answer `204 No Content`: `POST /v1/auth/logout`, `POST /v1/auth/delete-account`, `DELETE /v1/account/api-tokens/{id}`, `DELETE /v1/assets/{sha256}/uploads/{upload_id}`, `PUT /v1/admin/users/{id}/password`, `DELETE /v1/admin/users/{id}`. Every other response loses `ok` (and `AuthCheckResponse` loses `account_ok`, another spelling of the same flag). `ImportsListResponse { items }`.

- [ ] **Step 1: Rewrite the two admin tests and add an auth test**

`admin_api.rs:595-632`: the delete-messages test expects `sorted_keys(&body) == vec!["attachments", "conversations"]`. Replace the delete-account test body with:

```rust
        let status = delete_status(
            &state,
            &format!("/v1/admin/users/{}", victim.account_id),
            &admin.token,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "delete-account is an acknowledgement with no body");
```

(import `delete_status` and `StatusCode` as the module's other tests do). Update the doc comment at 986 that mentions `{"ok": true}` to say "answer 204".

In `auth.rs`'s test module add:

```rust
    #[tokio::test]
    async fn auth_check_names_the_account_without_an_ok_flag_and_logout_is_204() {
        let vault = crate::test_support::test_vault().await;
        let state = vault.state.clone();
        let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;
        let body: serde_json::Value =
            crate::test_support::get_json(&state, "/v1/auth/check", &user.token).await;
        assert_eq!(body["username"], "alice");
        assert!(body.get("ok").is_none() && body.get("account_ok").is_none(), "{body}");
        let status = crate::test_support::post_status(
            &state,
            "/v1/auth/logout",
            &user.token,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    }
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p message-vault-server auth_check_names_the_account delete_account_response delete_messages_response`
Expected: fail on `ok` present / 200 instead of 204.

- [ ] **Step 3: Remove the field everywhere**

The rule for each struct: delete the `ok: bool` field, its doc line, and `ok: true,` at every construction. When nothing is left, delete the struct, make the handler return `Result<StatusCode, ApiError>` with `Ok(StatusCode::NO_CONTENT)`, and set the utoipa response to `(status = 204, description = "…")`.

| Struct | Becomes |
| --- | --- |
| `auth::AuthCheckResponse` | `{ sources, account_id, username, admin }`; delete `account_ok` too and its `Some(true)`. |
| `auth::ChangePasswordResponse` | `{ token }` |
| `auth::DeleteAccountResponse`, `auth::LogoutResponse` | deleted; 204, description "Signed out" / "Account deleted" |
| `profile::DeleteMessagesResponse` | `{ conversations, attachments }` |
| `api_tokens_api::DeleteApiTokenResponse` | deleted; 204 "Token deleted" |
| `api_tokens_api::RenameApiTokenResponse` | `{ id, label }` |
| `assets::AssetPutResponse` | `{ sha256, assets_path, already_present }` |
| `assets::AssetUploadStartResponse` | without `ok` |
| `assets::AssetUploadPartResponse` | `{ part, bytes }` |
| `assets::AssetUploadAbortResponse` | deleted; 204 "Upload aborted" |
| `admin_api` set-password (`json!({"ok": true})` at 330) | `Ok(StatusCode::NO_CONTENT)`, return type `Result<StatusCode, ApiError>` |
| `admin_api` delete-user (409) | same |
| `import::ImportResponse` | without `ok` |
| `import::CreateImportResponse` | `{ id }` |
| `import::CompleteImportResponse` | without `ok` |
| `import::ActiveImportResponse` | `{ session }` |
| `import::SetImportStageResponse` | `{ stage }` |
| `import::DiscardImportResponse` | `{ id, status }` |
| `import::ImportsListResponse` | `{ items }` (rename the field; the handler builds `ImportsListResponse { items }`) |

`grep -rn 'ok: true' crates/vault/server/src` afterwards must list only `CompleteImportBody` constructions in tests (`server.rs:1234,1303,1331,1368`) and `db/vault_imports.rs` (`CompleteImportArgs`, a storage type). Tests that construct a response struct with `ok` (`import/mod.rs:1705, 2398`) lose the field. Any test reading `body["ok"]` on a success is deleted or changed to assert the field it actually cares about.

- [ ] **Step 4: Regenerate and test**

Run: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json && cargo test -p message-vault-server && grep -rn '"ok"' docs/src/assets/openapi.json | grep -v CompleteImportBody`
Expected: tests pass; the grep prints nothing.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server docs/src/assets/openapi.json
git commit -m "feat(vault-server): no response carries an ok flag; empty acknowledgements are 204"
```

---

### Task 6: Saved searches on the convention

**Files:**
- Modify: `crates/vault/server/src/saved_searches_api.rs` (whole file)
- Test: new `mod tests` in the same file (there is none today)

**Interfaces:**
- Produces: `GET /v1/saved-searches` → `SavedSearchesListResponse { items }`; `POST` → 200 `SavedSearch`; `PATCH /{id}` → 200 `SavedSearch`; `DELETE /{id}` → 204. No camelCase field anywhere.

- [ ] **Step 1: Write the failing test**

Append to `saved_searches_api.rs`:

```rust
#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::test_support::{delete_status, get_json, patch_json, post_json, register_via_api, test_vault};

    #[tokio::test]
    async fn saved_searches_list_as_items_and_each_write_answers_the_row_or_204() {
        let vault = test_vault().await;
        let state = vault.state.clone();
        let user = register_via_api(&state, "alice", "hunter2hunter2").await;

        let created: serde_json::Value = post_json(
            &state,
            "/v1/saved-searches",
            &user.token,
            serde_json::json!({ "name": "Family", "query": "group:Family" }),
        )
        .await;
        assert_eq!(created["name"], "Family");
        assert!(created["id"].is_i64());
        assert!(created.get("savedSearch").is_none() && created.get("savedSearches").is_none());
        let id = created["id"].as_i64().unwrap();

        let renamed: serde_json::Value = patch_json(
            &state,
            &format!("/v1/saved-searches/{id}"),
            &user.token,
            serde_json::json!({ "name": "Kin", "query": "group:Family" }),
        )
        .await;
        assert_eq!(renamed["name"], "Kin");

        let list: serde_json::Value = get_json(&state, "/v1/saved-searches", &user.token).await;
        assert_eq!(list["items"][0]["name"], "Kin");
        assert!(list.get("savedSearches").is_none());

        let status = delete_status(&state, &format!("/v1/saved-searches/{id}"), &user.token).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let list: serde_json::Value = get_json(&state, "/v1/saved-searches", &user.token).await;
        assert_eq!(list["items"].as_array().unwrap().len(), 0);
    }
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p message-vault-server saved_searches_list_as_items`
Expected: fails on `savedSearch` present / `items` missing.

- [ ] **Step 3: Rewrite the route file**

Replace the structs and handlers (keep the module doc and the `use` lines, with `use crate::extract::{Json, Path};` from Task 4 and `use axum::http::StatusCode;` added):

```rust
/// A saved search's name and query.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct SavedSearchBody {
    name: String,
    query: String,
}

/// The account's saved searches, A–Z.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct SavedSearchesListResponse {
    items: Vec<SavedSearch>,
}

/// List the account's saved searches, A–Z.
#[utoipa::path(
    get,
    path = "/v1/saved-searches",
    tag = "Saved searches",
    security(("bearer" = [])),
    responses(
        (status = 200, body = SavedSearchesListResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn saved_searches_list_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
) -> Result<Json<SavedSearchesListResponse>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let items = saved_searches::list(&mut conn, &auth.account_id).await?;
    Ok(Json(SavedSearchesListResponse { items }))
}

/// Create a saved search and return it.
#[utoipa::path(
    post,
    path = "/v1/saved-searches",
    tag = "Saved searches",
    security(("bearer" = [])),
    request_body = SavedSearchBody,
    responses(
        (status = 200, body = SavedSearch),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 409, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn saved_searches_create_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Json(body): Json<SavedSearchBody>,
) -> Result<Json<SavedSearch>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let row = saved_searches::create(
        &mut conn,
        &auth.account_id,
        &body.name,
        &body.query,
        SavedSearchKind::Manual,
    )
    .await?;
    Ok(Json(row))
}

/// Replace a saved search's name and query, and return it.
#[utoipa::path(
    patch,
    path = "/v1/saved-searches/{id}",
    tag = "Saved searches",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Saved search id")),
    request_body = SavedSearchBody,
    responses(
        (status = 200, body = SavedSearch),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody),
        (status = 409, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn saved_searches_update_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
    Json(body): Json<SavedSearchBody>,
) -> Result<Json<SavedSearch>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let row =
        saved_searches::update(&mut conn, &auth.account_id, id, &body.name, &body.query).await?;
    Ok(Json(row))
}

/// Delete a saved search.
///
/// Deleting an import-created saved search removes the shortcut only. The
/// `vault_imports` row it pointed at is the account's permanent record of that
/// run and is never touched here.
#[utoipa::path(
    delete,
    path = "/v1/saved-searches/{id}",
    tag = "Saved searches",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Saved search id")),
    responses(
        (status = 204, description = "Saved search deleted"),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn saved_searches_delete_handler(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let mut conn = state.db.acquire().await?;
    saved_searches::delete(&mut conn, &auth.account_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

`SavedSearch` in `db/saved_searches.rs:39` must derive `utoipa::ToSchema` (check; add it if missing).

- [ ] **Step 4: Regenerate and test**

Run: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json && cargo test -p message-vault-server`
Expected: pass. `grep -c savedSearch docs/src/assets/openapi.json` prints 0.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server docs/src/assets/openapi.json
git commit -m "feat(vault-server): saved searches list as items and each write answers the row"
```

---

### Task 7: The web reads the new shapes

**Files:**
- Regenerate: `web/src/lib/vaultApi.types.ts`
- Modify: `web/src/lib/vaultApi.ts` (74-87, 140-143, 237-255, 445-472; and `getConversationSources` is Task 8)
- Modify: `web/src/lib/api.ts:40-46` (doc comment only)
- Modify: `web/src/lib/savedSearches.ts`
- Modify: `web/src/screens/message/useConversationMessages.ts:52-72,105-117`
- Modify: `web/src/screens/ConversationList.tsx:63-81`, `web/src/screens/ContactList.tsx:98-105,145-157`
- Modify: `web/src/screens/settings/storage/useStorageData.ts:21`, `web/src/components/SourcesPanel.tsx:19`, `web/src/components/CheckedContactsPanel.tsx:148`
- Test: `web/src/lib/savedSearches.test.ts`, `web/src/screens/message/useConversationMessages.test.tsx`, `web/src/screens/ConversationList.test.tsx:43-49`, `web/src/screens/ContactList.test.tsx:50-63`, `web/src/lib/api.test.ts`, `web/src/lib/vaultApi.test.ts`

**Interfaces:**
- Consumes: the regenerated `Schema` types: `Page_ConversationSummary`, `Page_ContactSummary`, `Page_ExportMessage` (utoipa names a generic instance `Page_<T>`; confirm the exact names in the regenerated file and use those), `SavedSearchesListResponse { items }`, `ImportsListResponse { items }`, `ConversationSourcesPage { items }`, `ContactSummariesPage { items }`.
- Produces: `logout`, `deleteAccount`, `deleteApiToken`, `deleteUser`, `setUserPassword` return `Promise<void>`; `countExportMessages(params: { q: string })`; `useSavedSearches()` unchanged in shape for its callers.

- [ ] **Step 1: Regenerate the types and let the compiler list the work**

Run: `(cd web && npm run gen:api && npx tsc --noEmit -p tsconfig.json)`
Expected: errors at every site named in this task's file list (and the id sites of Task 8, which can wait; note them and move on).

- [ ] **Step 2: Update the tests first**

`savedSearches.test.ts`: every `list.mockResolvedValue({ savedSearches: [...] })` becomes `{ items: [...] }` (lines 63, 99, 107, 117, 125, 132, 139, 147, 178). Delete the test "treats a response without a list as empty rather than throwing" (the type now guarantees the field). Each write mock resolves the row (`create`/`update`) or `undefined` (`remove`), and the assertion after a write is that the list is asked again and shows the vault's new answer, in this shape:

```ts
  it("re-reads the list after a create", async () => {
    list.mockResolvedValueOnce({ items: [] }).mockResolvedValueOnce({ items: [search(3, "Work")] });
    create.mockResolvedValue(search(3, "Work"));
    const { result } = renderHook(() => ({ list: useSavedSearches(), actions: useSavedSearchActions() }), { wrapper });
    await waitFor(() => expect(result.current.list.savedSearches).toEqual([]));
    await act(() => result.current.actions.create("Work", "kind:group Work"));
    await waitFor(() => expect(result.current.list.savedSearches).toEqual([search(3, "Work")]));
    expect(list).toHaveBeenCalledTimes(2);
  });
```

Apply the same pattern to the existing update and remove cases (the `update.mockResolvedValue({ savedSearches: [...] })` at line 178 becomes `update.mockResolvedValue(search(3, "Renamed"))` with a second `list` answer).

`useConversationMessages.test.tsx`: `routeGets` and the two `deferred<…>()` calls use the page shape:

```ts
type MessagePage = { items: Message[]; total: number; limit: number; offset: number };
function page(items: Message[]): MessagePage {
  return { items, total: items.length, limit: 50, offset: 0 };
}
function routeGets(slow: Promise<MessagePage>) {
  getMessages.mockImplementation(((params: { q: string }) =>
    params.q.includes("in:#1")
      ? slow
      : Promise.resolve(page([message(2)]))) as unknown as typeof exportMessages);
}
```

`deferred<MessagePage>()`, `slow.resolve(page([message(1)]))`, and the `getCount` mock and its `mockReset` go (the hook stops calling the count route). The ids in `initialProps` become numbers in Task 8; for now keep the hook's parameter type as-is and use `"1"`/`"2"`.

`ConversationList.test.tsx:43-49`: `conversations: [...]` → `items: [...]`, and add `limit: 40, offset: 0`. `ContactList.test.tsx:50-63`: `contacts: [...]` → `items: [...]`.

`api.test.ts`: the three `'{"ok":false,"error":"…"}'` bodies become `'{"error":"…"}'`; the blank-error case `'{"error":"  "}'` still returns the raw text.

`vaultApi.test.ts:124-132`: delete any `source` argument to `countExportMessages`.

- [ ] **Step 3: Run the tests to see them fail**

Run: `(cd web && npx vitest run src/lib/savedSearches.test.ts src/screens/message/useConversationMessages.test.tsx src/screens/ConversationList.test.tsx src/screens/ContactList.test.tsx)`
Expected: fail (production code still reads the old fields).

- [ ] **Step 4: Production code**

`vaultApi.ts`:

```ts
export function logout(opts?: VaultRequestOptions): Promise<void> {
  return apiClient.post<void>("/v1/auth/logout", {}, opts);
}
export function deleteAccount(body: Schema["DeleteAccountRequest"]): Promise<void> {
  return apiClient.post<void>("/v1/auth/delete-account", body);
}
export function deleteApiToken(id: string): Promise<void> {
  return apiClient.delete<void>(`/v1/account/api-tokens/${encodeURIComponent(id)}`);
}
export function countExportMessages(
  params: { q: string },
  opts?: VaultRequestOptions,
): Promise<Schema["ExportCountResponse"]> {
  return apiClient.get<Schema["ExportCountResponse"]>(
    withQuery("/v1/export/messages/count", query(params)),
    opts,
  );
}
```

`exportMessages` returns `Promise<Schema["Page_ExportMessage"]>` (exact generated name), `listConversations` `Schema["Page_ConversationSummary"]`, `listContacts` `Schema["Page_ContactSummary"]`. The admin `deleteUser` and `setUserPassword` route functions return `Promise<void>` (find them with `grep -n 'admin/users' web/src/lib/vaultApi.ts`). `api.ts:40-46` doc comment: "The vault answers `{"error":"..."}`".

`savedSearches.ts`:

```ts
/** The account's saved searches, A–Z as the vault orders them. */
export function useSavedSearches(): { savedSearches: SavedSearch[]; loading: boolean } {
  const { data, isPending } = useVaultQuery(
    keys.savedSearches.all,
    async (signal) => (await listSavedSearches({ signal })).items,
  );
  return { savedSearches: data ?? [], loading: isPending };
}

/** Every write is followed by one fresh read of the list the sidebar shows. */
function useSavedSearchWrite<V>(write: (vars: V) => Promise<unknown>): UseMutationResult<unknown, Error, V> {
  const cache = useVaultCache();
  return useMutation<unknown, Error, V>({
    mutationFn: write,
    onSettled: () => cache.invalidate(keys.savedSearches.all),
  });
}
```

`useCreateSavedSearch`/`useUpdateSavedSearch`/`useDeleteSavedSearch` return `UseMutationResult<unknown, Error, …>`; delete `ListResponse` and `listFrom`. Update the module comment's last paragraph: "Every write is followed by one fresh read" replaces "Every write answers with the whole list". `SavedSearch` can become `export type SavedSearch = Schema["SavedSearch"];` (import `Schema` as `vaultApi.ts` does).

`useConversationMessages.ts`: `fetchAllMessagesForQuery` no longer calls the count route:

```ts
async function fetchAllMessagesForQuery(
  q: string,
  signal: AbortSignal,
): Promise<{ messages: Message[]; total: number }> {
  const collected: Message[] = [];
  let offset = 0;
  let total = 0;
  while (true) {
    const page = await exportMessages({ q, offset, limit: YEAR_FETCH_LIMIT }, { signal });
    total = page.total;
    collected.push(...page.items);
    offset += page.items.length;
    if (page.items.length === 0 || offset >= total) break;
  }
  return { messages: collected, total };
}
```

and `fetchConversationPage` reads one page:

```ts
        const page = await exportMessages({ q, offset: newOffset, limit: PAGE_SIZE }, { signal });
        if (signal.aborted) return;
        setMessages(page.items);
        setTotal(page.total);
        setOffset(newOffset);
```

Delete the `countExportMessages` import. `ConversationList.tsx:76-77` → `items: res.items, total: res.total,`; `ContactList.tsx:150` → `items: normalizeContacts(res.items)` and `normalizeContacts(rows: Contact-page items type)` with `(rows || [])` dropped to `rows`. `useStorageData.ts:21` → `importsRes.items`; `SourcesPanel.tsx:19` → `res.items`; `CheckedContactsPanel.tsx:148` → `page.items`.

- [ ] **Step 5: Run the web checks**

Run: `(cd web && npx tsc --noEmit -p tsconfig.json && npm run lint && npm test)`
Expected: type errors remain only at the conversation-id sites (Task 8); the four test files from Step 3 pass. If `tsc` blocks on the id sites, do Task 8's Step 3 first and come back.

- [ ] **Step 6: Commit**

```bash
git add web docs/src/assets/openapi.json
git commit -m "feat(web): read lists as pages of items and drop the ok flag from every response"
```

---

### Task 8: The conversation id is a number in the web

**Files:**
- Modify: `web/src/lib/vaultApi.ts:225-233`, `web/src/lib/fetchConversationById.ts`, `web/src/lib/messagesLocationState.ts:24-27`, `web/src/lib/vaultKeys.ts:36`, `web/src/components/MessageRoute.tsx`, `web/src/components/ConversationRow.tsx:135`, `web/src/components/SourcesPanel.tsx:12`, `web/src/screens/ConversationList.tsx:35,42,116`, `web/src/screens/message/useConversationMessages.ts:27,80`
- Test: `web/src/lib/fetchConversationById.test.ts`, `web/src/lib/messagesLocationState.test.ts`, `web/src/lib/vaultApi.test.ts:91-93,184-187`, `web/src/screens/ConversationList.test.tsx`, `web/src/screens/message/useConversationMessages.test.tsx`

**Interfaces:**
- Produces: `getConversationSources(conversationId: number)`, `fetchConversationById(conversationId: number)`, `useConversationMessages(conversationId: number)`, `ConversationList({ selectedId: number | null })`, `ConversationRow.onCheckChange?: (id: number) => void`, `SourcesPanel({ conversationId: number | null })`, `keys.conversations.sources(id: number | null)`. The router path stays `/messages/:conversationId`; `MessageRoute` turns the string into a positive integer once, at the top, and shows "Conversation not found." for anything else.

- [ ] **Step 1: Update the tests**

`fetchConversationById.test.ts`: `conv(id: number)` with `id` numeric, `label: \`Chat ${id}\``; fixtures `{ items: [conv(1), conv(2)], total: 2, limit: 100, offset: 0 }`; calls `fetchConversationById(2)`, `(99)`, `(7)` for the missing case, `(1, controller.signal)`.

`messagesLocationState.test.ts:4-14`: the conversation fixture `id: 42`. Add:

```ts
  it("rejects a conversation whose id is not a positive integer", () => {
    expect(asMessagesLocationState({ conversation: { ...conversation, id: "42" } })).toBeNull();
    expect(asMessagesLocationState({ conversation: { ...conversation, id: 0 } })).toBeNull();
  });
```

`vaultApi.test.ts:91-93`: `getConversationSources(12)` → `/v1/conversations/12/sources`; delete the "escapes a conversation id containing a space" test at 184-187. `ConversationList.test.tsx:43-49`: `id: 1`, `id: 2`. `useConversationMessages.test.tsx`: `({ id }: { id: number })`, `initialProps: { id: 1 }`, `rerender({ id: 2 })`.

- [ ] **Step 2: Run to see them fail**

Run: `(cd web && npx vitest run src/lib/fetchConversationById.test.ts src/lib/messagesLocationState.test.ts src/lib/vaultApi.test.ts)`
Expected: type and assertion failures.

- [ ] **Step 3: Production code**

`vaultApi.ts`:

```ts
export function getConversationSources(
  conversationId: number,
  opts?: VaultRequestOptions,
): Promise<Schema["ConversationSourcesPage"]> {
  return apiClient.get<Schema["ConversationSourcesPage"]>(
    `/v1/conversations/${conversationId}/sources`,
    opts,
  );
}
```

`fetchConversationById.ts`:

```ts
export async function fetchConversationById(
  conversationId: number,
  signal?: AbortSignal,
): Promise<Conversation | null> {
  let offset = 0;
  while (true) {
    const page = await listConversations({ q: "", limit: PAGE_SIZE, offset }, { signal });
    const match = page.items.find((c) => c.id === conversationId);
    if (match) return match;
    offset += PAGE_SIZE;
    if (offset >= page.total || page.items.length === 0) return null;
  }
}
```

`messagesLocationState.ts`:

```ts
/** True when the value looks like a conversation with a positive integer id. */
function isConversation(value: unknown): value is Conversation {
  if (!isRecord(value)) return false;
  return typeof value.id === "number" && Number.isInteger(value.id) && value.id > 0;
}
```

`vaultKeys.ts:36`: `sources: (id: number | null) => ["conversations", "sources", String(id)] as const,`.

`MessageRoute.tsx`: replace the first line of the body with

```ts
  const { conversationId: conversationParam } = useParams<{ conversationId: string }>();
  const conversationId = positiveInteger(conversationParam);
```

and add at module level:

```ts
/** The route's `:conversationId` as a number, or null when it is not a positive integer. */
function positiveInteger(raw: string | undefined): number | null {
  if (raw === undefined || !/^\d+$/.test(raw)) return null;
  const n = Number(raw);
  return Number.isSafeInteger(n) && n > 0 ? n : null;
}
```

In the effect, `if (stateConversation || conversationId === null)` guards the early return, and the `fetchConversationById(conversationId, …)` call is unchanged in shape. Below the effect, when `conversationParam !== undefined && conversationId === null`, render the same "Conversation not found." block the fetch error uses (set `fetchError` in the effect's early-return branch: `setFetchError(conversationParam === undefined ? null : "Conversation not found.")`). `selectedId={conversationId}`.

`ConversationList.tsx`: `selectedId: number | null`; `useState<Set<number>>`; `applyMembership` ids: `const ids = targetConversations.map((c) => c.id);` (delete the `Number(...)` map and the `isFinite` filter). `ConversationRow.tsx:135`: `onCheckChange?: (id: number) => void;`. `SourcesPanel.tsx:12`: `conversationId: number | null;`. `useConversationMessages.ts`: `yearQuery(conversationId: number, …)` and `useConversationMessages(conversationId: number)`. `MessageView.tsx` needs no change: `conversation.id` is now a number by type.

- [ ] **Step 4: Run every web check**

Run: `(cd web && npx tsc --noEmit -p tsconfig.json && npm run lint && npm test)`
Expected: clean. Then `./scripts/check-generated-api-types.sh` from the repo root prints nothing and exits 0.

- [ ] **Step 5: Commit**

```bash
git add web
git commit -m "feat(web): a conversation's id is a number everywhere, as the vault now sends it"
```

---

### Task 9: `vault-pull` pages by offset

**Files:**
- Modify: `crates/libs/vault-pull/src/http.rs:18-28,123-236`
- Modify: `crates/libs/vault-pull/src/run.rs:20-42,81-95,184-252`
- Modify: `crates/libs/vault-pull/src/lib.rs:1-19`
- Modify: `src-tauri/src/commands/pull.rs:59`
- Test: unit tests in `http.rs` (URL builder) and a new offset-loop test in `run.rs`

**Interfaces:**
- Produces: `ExportMessagesPage { items: Vec<ExportMessage>, total: u64 }`, `ExportMessagesArgs { base_url, key, q, limit, offset, account }`, `DEFAULT_PAGE_LIMIT = 500`, `MAX_PAGE_LIMIT = 500`, `VaultPullConfig` without `source`, no `compose_query`.

- [ ] **Step 1: Write the failing tests**

In `http.rs`'s test module (create one if there is none) add:

```rust
    #[test]
    fn the_export_url_carries_q_limit_offset_and_account_only() {
        let url = export_url(ExportUrl {
            base_url: "http://127.0.0.1:8080/",
            q: "from:me",
            limit: 500,
            offset: 1000,
            account: " alice ",
        })
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:8080/v1/export/messages?q=from%3Ame&limit=500&offset=1000&account=alice"
        );
    }

    #[test]
    fn a_page_parses_without_an_ok_flag_and_a_failure_body_yields_its_sentence() {
        let page: ExportMessagesPage =
            serde_json::from_str(r#"{"items":[],"total":7,"limit":500,"offset":0}"#).unwrap();
        assert_eq!((page.items.len(), page.total), (0, 7));
        assert_eq!(error_sentence(r#"{"error":"limit exceeds maximum of 500"}"#, 400), "limit exceeds maximum of 500");
        assert_eq!(error_sentence("<html>", 502), "<html>");
    }
```

In `run.rs` add a test of the loop's stopping rule, written against a pure helper so it needs no server:

```rust
#[cfg(test)]
mod paging_tests {
    use super::next_offset;

    #[test]
    fn paging_stops_at_the_total_or_on_an_empty_page() {
        assert_eq!(next_offset(0, 500, 1200), Some(500));
        assert_eq!(next_offset(1000, 200, 1200), None);
        assert_eq!(next_offset(1000, 0, 1200), None, "an empty page ends the walk even under total");
    }
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p vault-pull`
Expected: compile errors (`ExportMessagesPage`, `offset`, `error_sentence`, `next_offset` do not exist).

- [ ] **Step 3: `http.rs`**

Replace `ExportMessagesResponse` (18-28) with:

```rust
/// One page from `GET /v1/export/messages`: `{items, total, limit, offset}`.
#[derive(Debug, Deserialize)]
pub struct ExportMessagesPage {
    #[serde(default)]
    pub items: Vec<ExportMessage>,
    #[serde(default)]
    pub total: u64,
}

/// The vault's failure body: `{error}`.
#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: Option<String>,
}

/// The sentence to show for a failed response: the body's `error` when it has
/// one, otherwise the body itself, clipped.
fn error_sentence(body: &str, _status: u16) -> String {
    match serde_json::from_str::<ErrorBody>(body) {
        Ok(ErrorBody { error: Some(message) }) if !message.trim().is_empty() => message,
        _ => truncate(body, 300),
    }
}
```

`ExportUrl` drops `path`, `cursor`, `source`, and `limit` becomes `usize` with `offset: usize`; `export_url` appends `q`, `limit`, `offset`, and `account` (when non-blank) to `{base}/v1/export/messages`. `ExportMessagesArgs { base_url, key, q, limit: usize, offset: usize, account }`. `export_messages` returns `Result<ExportMessagesPage>`; the non-success branch uses `error_sentence(&body, status.as_u16())`; the `if !parsed.ok` block is deleted.

- [ ] **Step 4: `run.rs`, `lib.rs`, `pull.rs`**

`run.rs`: `pub const DEFAULT_PAGE_LIMIT: usize = 500;` with doc "Page size for GET /v1/export/messages; the vault's maximum." and `pub const MAX_PAGE_LIMIT: usize = 500;`. Delete `source` from `VaultPullConfig` and `compose_query` (81-95). Add:

```rust
/// Where the next page starts, or `None` when the walk is over: the vault said
/// this was the last of `total`, or it sent nothing (a stale total must not spin).
fn next_offset(offset: usize, fetched: usize, total: u64) -> Option<usize> {
    if fetched == 0 {
        return None;
    }
    let next = offset + fetched;
    (u64::try_from(next).unwrap_or(u64::MAX) < total).then_some(next)
}
```

The loop (184-252):

```rust
    let mut offset = 0usize;
    // (by_conv, assets, total_messages unchanged)
    loop {
        check_cancel(cfg.cancel.as_ref())?;
        let page = with_retries(MAX_RETRIES, || {
            crate::http::export_messages(
                &session,
                ExportMessagesArgs {
                    base_url: &cfg.base_url,
                    key: &cfg.key,
                    q: &q,
                    limit: cfg.page_limit.clamp(1, MAX_PAGE_LIMIT),
                    offset,
                    account: &account,
                },
            )
        })?;
        let fetched = page.items.len();
        total_messages += fetched as u64;
        emit(&mut on_progress, ProgressEvent::Page { messages: fetched, total_so_far: total_messages });

        for msg in page.items {
            // (unchanged body)
        }

        match next_offset(offset, fetched, page.total) {
            Some(next) => offset = next,
            None => break,
        }
    }
```

`lib.rs`: the module doc becomes "Pulls messages out of a running vault, a page at a time, through `GET /v1/export/messages?offset=&limit=`, and writes them as chat files." Remove `compose_query` from the re-export. `src-tauri/src/commands/pull.rs:59`: delete `source: None,`.

- [ ] **Step 5: Test both crates**

Run: `cargo test -p vault-pull && cargo build --manifest-path src-tauri/Cargo.toml`
Expected: pass; the desktop shell builds.

- [ ] **Step 6: Commit**

```bash
git add crates/libs/vault-pull src-tauri/src/commands/pull.rs
git commit -m "feat(vault-pull): page the export by offset and drop the dead query composer"
```

---

### Task 10: `vault-push` and `vault-http` decide success by status

**Files:**
- Modify: `crates/libs/vault-push/src/http.rs:19-53,93-157,240-257,340-350,625-665`
- Modify: `crates/libs/vault-push/tests/push_mock.rs` (every `"ok": …` fixture)
- Modify: `crates/libs/vault-http/src/session.rs:119-126,141-154`
- Test: the rewritten `ok_json` tests in `http.rs`; `push_mock.rs`

**Interfaces:**
- Produces: `ok_json<T: DeserializeOwned>(status, text) -> Result<T>`: a 2xx parses `T`; anything else is a `VaultHttpError` with the body's `error` sentence, or `HTTP {status}: {body}`. The four response structs lose `ok` and `error`. `AuthCheckResponse` in `vault-http` loses `ok`, `account_ok`, `error`.

- [ ] **Step 1: Rewrite the three `ok_json` tests**

```rust
    #[test]
    fn ok_json_prefers_the_body_error_sentence() {
        let err = ok_json::<AssetPutResponse>(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":"sha mismatch"}"#,
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "sha mismatch");
    }

    #[test]
    fn ok_json_falls_back_to_status_and_body() {
        let err = ok_json::<AssetPutResponse>(reqwest::StatusCode::BAD_GATEWAY, "{}").unwrap_err();
        assert!(err.to_string().starts_with("HTTP 502"));
        let err = ok_json::<AssetPutResponse>(reqwest::StatusCode::BAD_GATEWAY, "gateway text").unwrap_err();
        assert!(err.to_string().contains("gateway text"));
    }

    #[test]
    fn ok_json_trusts_the_status_not_a_flag() {
        let parsed = ok_json::<AssetPutResponse>(
            reqwest::StatusCode::OK,
            r#"{"sha256":"abc","assets_path":"a/b","already_present":true}"#,
        )
        .unwrap();
        assert!(parsed.already_present);
        assert!(ok_json::<AssetPutResponse>(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "{}").is_err());
        // A success whose body cannot be read is a failure that names the problem.
        let err = ok_json::<AssetPutResponse>(reqwest::StatusCode::OK, "not json").unwrap_err();
        assert!(err.to_string().contains("not json"));
    }
```

(`AssetPutResponse` fields must match what the server now sends: `already_present: bool` with `#[serde(default)]`; `sha256`/`assets_path` are ignored unless the struct declares them.)

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p vault-push ok_json`
Expected: the first passes by accident, the others fail.

- [ ] **Step 3: `http.rs`**

Delete the `OkEnvelope` trait and its three impls, and the `ok`/`error` fields from `AssetPutResponse`, `ImportResponse`, `UploadStartResponse`, `ImportSessionResponse`. Replace `ok_json` with:

```rust
/// The vault's failure body: `{error}`.
#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: Option<String>,
}

/// Parse a vault JSON response body, or bail with the server's error text.
///
/// A 2xx status is a success and the body is `T`. Anything else is a
/// [`VaultHttpError`] carrying the body's `error` sentence when it has one,
/// else `HTTP {status}: {body}`.
fn ok_json<T: DeserializeOwned>(status: reqwest::StatusCode, text: &str) -> Result<T> {
    if status.is_success() {
        return serde_json::from_str::<T>(text).map_err(|e| {
            VaultHttpError::new(status.as_u16(), format!("could not read the vault's answer ({e}): {text}")).into()
        });
    }
    let message = match serde_json::from_str::<ErrorBody>(text) {
        Ok(ErrorBody { error: Some(m) }) if !m.trim().is_empty() => m,
        _ => format!("HTTP {status}: {text}"),
    };
    Err(VaultHttpError::new(status.as_u16(), message).into())
}
```

`head_asset` (240-257): the synthesized `assumed_present` is `AssetPutResponse { already_present: true }` (plus any other fields the struct keeps, defaulted); the guard becomes `if !parsed.already_present { return Ok(None); }`. `put_asset_multipart` (345-350): `AssetPutResponse { already_present: true, .. }` likewise. The abort closure at 361-375 sends a DELETE and ignores the body; confirm it does not parse JSON (a 204 now comes back).

- [ ] **Step 4: `push_mock.rs` and `session.rs`**

`push_mock.rs`: delete every `"ok": true,` line (the `json!` fixtures at 124, 133, 138, 151, 195, 204, 211, 224, 263, 275, 314, 327, 339, 368, 389, 434, 443, 481, 499, 506, 642, 672, 714, 748, 760, 787, 799, 806, 823 and the one-liners at 662, 667, 742, 755, 813, which become `json!({ "already_present": … })`). The two `"ok": false` fixtures at 380 and 492 become `{"error": "<the same message they carry>"}` on the non-2xx status they already use. `grep -c '"ok"' crates/libs/vault-push/tests/push_mock.rs` must print 0.

`session.rs`: delete the `if !parsed.ok { … }` block (119-126) and the `ok`, `account_ok`, `error` fields of `AuthCheckResponse`. `AuthError::Rejected` stays (retry classification and its tests use it).

- [ ] **Step 5: Test the three crates**

Run: `cargo test -p vault-push -p vault-http -p vault-pull`
Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add crates/libs/vault-push crates/libs/vault-http
git commit -m "feat(vault-push): trust the HTTP status, not an ok flag, and read {error} on failure"
```

---

### Task 11: Smoke scripts, changelog, and the full check

**Files:**
- Modify: `scripts/test/smoke-export-api.sh:70,77,83,90,95`, `scripts/test/smoke-import-api.sh:60`, `scripts/test/smoke-vault-push.sh:67,76,84,89,104`
- Modify: `CHANGELOG.md` (`[Unreleased]`)
- Modify: `docs/src/content/docs/vault/developer/reference/api.md` (one short section)

- [ ] **Step 1: Smoke scripts assert on fields the responses still carry**

| Line | Old | New |
| --- | --- | --- |
| `smoke-export-api.sh:70`, `smoke-vault-push.sh:67` | `grep -q '"ok":true'` on `/v1/auth/check` | `grep -q '"account_id"'` |
| `smoke-export-api.sh:77` | asset PUT `grep -q '"ok":true'` | `grep -q '"sha256"'` |
| `smoke-export-api.sh:83,90`, `smoke-import-api.sh:60`, `smoke-vault-push.sh:76,84,104` | import `grep -q '"ok":true'` | `grep -q '"messages_appended"'` (an `ImportStats` field; confirm the name with `grep -n messages_appended crates/vault/server/src/import/mod.rs`) |
| `smoke-export-api.sh:95` | export `grep -q '"ok":true'` | `grep -q '"items"'` |
| `smoke-vault-push.sh:89` | `grep -q '"account_ok":true'` | `grep -q "\"account_id\":\"${ACCOUNT}\""` |

- [ ] **Step 2: Changelog and the API page**

Under `## [Unreleased]` in `CHANGELOG.md`:

`### Changed`:
- `2026-09-03: Every list on the HTTP interface answers `{items, total, limit, offset}` and takes `offset` and `limit`; a `limit` above 500 or an `offset` above 50 000 is a 400 instead of a silent clamp. Conversation ids are integers. Every failure is `{error}` with the status, including a malformed query parameter, path, or JSON body, an unknown `/v1` path, and a wrong method. No response carries an `ok` flag; acknowledgements with nothing else to say are 204. Saved searches list as `items`, and creating or renaming one answers the row. (ADR-0005)`
- `2026-09-03: `GET /v1/export/messages` pages by `offset` and `limit`, reports `total`, and has no offset cap. The desktop Export walks it in pages of 500.`

`### Removed`:
- `2026-09-03: The export cursor, the `source=` parameter on `GET /v1/export/messages` and `/count` (write `source:imessage` in the query instead), the `savedSearches` and `savedSearch` fields, the `ok` and `account_ok` flags, and `vault-pull`'s unused `compose_query`.`

In `docs/src/content/docs/vault/developer/reference/api.md`, after the introduction, add:

```markdown
## One shape for every route

- A list takes `?offset=&limit=` and answers `{items, total, limit, offset}`. `limit` is at most 500; `offset` is at most 50 000 on the Contacts and Conversations lists and unlimited on Export.
- A failure answers `{"error": "<sentence>"}` with the HTTP status. There is no `ok` field on any response.
- Every id is an integer.

Why: [ADR-0005](https://github.com/bitrealm-io/message-vault/blob/main/docs/adr/0005-one-shape-for-every-route-on-the-http-interface.md).
```

- [ ] **Step 3: The full check**

Run from the repo root: `./scripts/check-pr.sh`
Expected: every step passes: rustfmt (workspace and `src-tauri`), build, tests, Biome, Vitest, the generated-types check, and the docs check. Fix anything it names before committing. Then run `./scripts/lint-all.sh` and clear any Clippy warning in files this plan touched.

- [ ] **Step 4: Commit**

```bash
git add scripts/test CHANGELOG.md docs/src/content/docs/vault/developer/reference/api.md
git commit -m "docs: record the route convention and make the smoke scripts assert on real fields"
```

---

## Self-review

**Spec coverage.** Offset paging and `Page<T>` on the server: Tasks 1-3. `Page<T>` in the web: Task 7 (the generated per-instance types satisfy the web's existing `OffsetPage<T>` structurally; the two adapters collapse). `{error}` with Axum rejections mapped in `server.rs`: Task 4 (the mapping lives in `extract.rs`, registered from `server.rs`'s router). No `ok` anywhere: Tasks 3, 5, 6. Integer ids: Tasks 2 and 8. camelCase off Saved Searches: Task 6. One `MAX_LIST_LIMIT` and a named summaries cap: Tasks 1-2. `source=` off Export: Task 3. `vault-pull` on offset paging and `compose_query` deleted: Task 9. Web types regenerated: Task 7. Read routes taking `FullAccess` is pull request 4, not this one.

**Placeholder scan.** Every code step shows the code. The RawRow mapping in Task 3 Step 4 is marked unchanged and is present in the file at the lines given.

**Type consistency.** `page_params(limit, offset, default_limit, max_offset)` is called with the same argument order in Tasks 2 and 3. `Page { items, total, limit, offset }` matches the web's reads (`page.items`, `page.total`) in Tasks 7-9. `ExportMessagesArgs.offset: usize` matches `next_offset(offset: usize, …)`. `ErrorBody { error }` on the server matches the `ErrorBody { error: Option<String> }` readers in both client crates.
