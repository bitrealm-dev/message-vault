# Contact Groups and Message Tags by id Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address Contact Groups and Message Tags by their integer id on the HTTP interface, put membership under the collection, implement both route groups from one module, and keep the web app's name-based interface by resolving ids inside the feature module.

**Architecture:** `named_membership.rs` gains id-addressed DB functions beside the name-addressed ones the import path still uses. A new `named_set_api.rs` holds the shared request and response types and six functions over `&'static MembershipSpec`; `contact_groups_api.rs` and `message_tags_api.rs` become twelve three-line handlers carrying the `#[utoipa::path]` attributes. On the web, `vaultApi.ts` gets twelve route functions named by the `<action><Resource>` rule, and `nameCollection.ts` resolves a name to an id from its cached list before each write and invalidates the lists that show the name.

**Tech Stack:** Rust (Axum, sqlx Any over SQLite and Postgres, utoipa), React 19 + TypeScript, TanStack Query, Vitest.

**Spec:** `docs/superpowers/specs/2026-09-02-contact-groups-and-message-tags-by-id-design.md`. Decision record: `docs/adr/0003-resources-are-addressed-by-id-on-the-http-interface.md`.

## Global Constraints

- Work on the `named-sets-by-id` branch. Never commit to `main`. Never create or push tags.
- The twelve routes are exactly those in the spec's routes table; no other route changes. All twelve take `FullAccess`.
- Response shapes: list `{items}`, create and update the `NamedSet`, delete `204` with no body, members list `{items}`, members update `{added, removed}`. No response carries `ok`.
- Status codes: `400` empty, over-long, or reserved name, or a members patch with nothing in `add` or `remove`; `404` unknown set id, unknown member id, or another account's id; `409` duplicate name ignoring case. A case-only change of the same name is allowed.
- Names on the wire are trimmed; `MAX_NAME_LEN` is `80` and stays in `named_membership.rs`.
- Web routes, the search language (`group:`, `within:`, `label:`, `tag:`), Saved Search text, both reserved-name lists, and `kind` on `contact_groups` do not change.
- OpenAPI-visible changes regenerate `docs/src/assets/openapi.json` in the same commit: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json`. The test `committed_openapi_matches_dump` in `crates/vault/server/src/openapi.rs` fails otherwise.
- After the JSON changes, regenerate `web/src/lib/vaultApi.types.ts` with `cd web && npm run gen:api` and commit it. `scripts/check-pr.sh` fails on a diff.
- Biome gates `web/`: prefix unused bindings with `_`; prefer a real fix over `biome-ignore`.
- Tests use invented data only (`Family`, `Work`, `Holiday`, `alice`, `bob`, `hunter2hunter2`). Never commit real message data.
- Commit messages are conventional commits whose body says what changed and why in plain language.
- The Rust and TypeScript snippets below were written against the current sources but not compiled before this plan was written. Where wiring details differ, the compiler and the existing tests are authoritative; keep the names and types the Interfaces blocks state.

---

### Task 1: Id-addressed functions in `named_membership.rs`

**Files:**
- Modify: `crates/vault/server/src/named_membership.rs` (add functions after `set_membership`; add a `#[cfg(test)] mod tests` at the end)

**Interfaces:**
- Consumes: existing `MembershipSpec`, `MembershipError`, `find_id`, `normalize_name`, `is_reserved`, `member_exists`, `engine_of`, `order_by_name_ci`.
- Produces, all `pub` and all taking `spec: &MembershipSpec, conn: &mut AnyConnection, account_id: &str` first:
  - `list_sets(…) -> Result<Vec<(i64, String)>, MembershipError>`
  - `get_set(…, id: i64) -> Result<(i64, String), MembershipError>`
  - `create_set(…, name: &str) -> Result<(i64, String), MembershipError>`
  - `rename_set(…, id: i64, name: &str) -> Result<String, MembershipError>`
  - `delete_set(…, id: i64) -> Result<(), MembershipError>`
  - `list_member_ids_of(…, id: i64) -> Result<Vec<i64>, MembershipError>`
  - `patch_members(…, id: i64, add: &[i64], remove: &[i64]) -> Result<(u64, u64), MembershipError>`
  - Task 2 calls all seven.

- [ ] **Step 1: Write the failing tests**

Append to `crates/vault/server/src/named_membership.rs`:

```rust
#[cfg(test)]
mod tests {
    use sqlx::AnyConnection;

    use super::{
        MembershipError, create_set, delete_set, get_set, group_spec, list_member_ids_of,
        list_sets, names_for_item, patch_members, rename_set, set_membership, tag_spec,
    };
    use crate::db::{engine, schema};

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir, String) {
        let (pool, dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let account = "00000000-0000-4000-8000-0000000000c9".to_string();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();
        (pool, dir, account)
    }

    async fn insert_contact(conn: &mut AnyConnection, account: &str, name: &str) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(account)
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn create_and_list_sets_answer_ids_and_names_a_to_z() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let (work_id, work) = create_set(group_spec(), &mut conn, &account, " Work ")
            .await
            .unwrap();
        assert_eq!(work, "Work");
        let (family_id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        assert_ne!(work_id, family_id);
        assert_eq!(
            list_sets(group_spec(), &mut conn, &account).await.unwrap(),
            vec![(family_id, "Family".to_string()), (work_id, "Work".to_string())]
        );
        assert_eq!(
            get_set(group_spec(), &mut conn, &account, work_id)
                .await
                .unwrap(),
            (work_id, "Work".to_string())
        );
    }

    #[tokio::test]
    async fn create_set_refuses_duplicates_and_reserved_names() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        let err = create_set(group_spec(), &mut conn, &account, "family")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::Conflict(_)));
        let err = create_set(group_spec(), &mut conn, &account, "Trash")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::BadRequest(_)));
    }

    #[tokio::test]
    async fn rename_set_allows_a_case_change_and_refuses_another_sets_name() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let (family_id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        let (work_id, _) = create_set(group_spec(), &mut conn, &account, "Work")
            .await
            .unwrap();
        assert_eq!(
            rename_set(group_spec(), &mut conn, &account, family_id, "FAMILY")
                .await
                .unwrap(),
            "FAMILY"
        );
        let err = rename_set(group_spec(), &mut conn, &account, work_id, "family")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::Conflict(_)));
        let err = rename_set(group_spec(), &mut conn, &account, 999_999, "Anything")
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_set_drops_its_memberships_and_refuses_an_unknown_id() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let a = insert_contact(&mut conn, &account, "Ada").await;
        let (id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        patch_members(group_spec(), &mut conn, &account, id, &[a], &[])
            .await
            .unwrap();
        delete_set(group_spec(), &mut conn, &account, id)
            .await
            .unwrap();
        assert!(list_sets(group_spec(), &mut conn, &account).await.unwrap().is_empty());
        assert!(
            names_for_item(group_spec(), &mut conn, &account, a)
                .await
                .unwrap()
                .is_empty()
        );
        let err = delete_set(group_spec(), &mut conn, &account, id)
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::NotFound(_)));
    }

    #[tokio::test]
    async fn patch_members_adds_and_removes_in_one_call() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let a = insert_contact(&mut conn, &account, "Ada").await;
        let b = insert_contact(&mut conn, &account, "Ben").await;
        let (id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        assert_eq!(
            patch_members(group_spec(), &mut conn, &account, id, &[a, b, b], &[])
                .await
                .unwrap(),
            (2, 0)
        );
        assert_eq!(
            list_member_ids_of(group_spec(), &mut conn, &account, id)
                .await
                .unwrap(),
            vec![a, b]
        );
        assert_eq!(
            patch_members(group_spec(), &mut conn, &account, id, &[a], &[b])
                .await
                .unwrap(),
            (0, 1)
        );
        assert_eq!(
            list_member_ids_of(group_spec(), &mut conn, &account, id)
                .await
                .unwrap(),
            vec![a]
        );
        let err = patch_members(group_spec(), &mut conn, &account, id, &[], &[])
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::BadRequest(_)));
    }

    #[tokio::test]
    async fn patch_members_with_a_foreign_member_writes_nothing() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let a = insert_contact(&mut conn, &account, "Ada").await;
        let (id, _) = create_set(group_spec(), &mut conn, &account, "Family")
            .await
            .unwrap();
        let err = patch_members(group_spec(), &mut conn, &account, id, &[a, 999_999], &[])
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::NotFound(_)));
        assert!(
            list_member_ids_of(group_spec(), &mut conn, &account, id)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn another_accounts_set_is_not_found() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let other = "00000000-0000-4000-8000-0000000000ca";
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'bob')")
            .bind(other)
            .execute(&mut *conn)
            .await
            .unwrap();
        let (id, _) = create_set(tag_spec(), &mut conn, other, "Holiday")
            .await
            .unwrap();
        let err = get_set(tag_spec(), &mut conn, &account, id).await.unwrap_err();
        assert!(matches!(err, MembershipError::NotFound(_)));
        assert!(list_sets(tag_spec(), &mut conn, &account).await.unwrap().is_empty());
    }

    /// The import path still fills groups by name through `set_membership`.
    #[tokio::test]
    async fn set_membership_by_name_still_creates_and_fills_a_group() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let a = insert_contact(&mut conn, &account, "Ada").await;
        assert_eq!(
            set_membership(group_spec(), &mut conn, &account, &[a], "Family", true)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            names_for_item(group_spec(), &mut conn, &account, a)
                .await
                .unwrap(),
            vec!["Family"]
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p message-vault-server named_membership::tests`
Expected: compile error, `cannot find function 'create_set'` and the other six.

- [ ] **Step 3: Add the functions**

Insert after `set_membership` and before `names_for_item` in `named_membership.rs`:

```rust
/// Drop non-positive ids, sort, and dedupe a caller's member id list.
fn clean_ids(ids: &[i64]) -> Vec<i64> {
    let mut out: Vec<i64> = ids.iter().copied().filter(|id| *id > 0).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Sets for this account with their ids, A–Z, excluding reserved leftovers.
pub async fn list_sets(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Vec<(i64, String)>, MembershipError> {
    let order = order_by_name_ci(engine_of(conn), "name");
    let sql = format!(
        "SELECT id, name FROM {table} WHERE account_id = $1 {order}",
        table = spec.table
    );
    let rows = sqlx::query_as::<_, (i64, String)>(&sql)
        .bind(account_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows
        .into_iter()
        .filter(|(_, name)| !is_reserved(spec, name))
        .collect())
}

/// One set by id, or `NotFound` when it is not this account's.
pub async fn get_set(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<(i64, String), MembershipError> {
    let sql = format!(
        "SELECT id, name FROM {table} WHERE id = $1 AND account_id = $2",
        table = spec.table
    );
    sqlx::query_as::<_, (i64, String)>(&sql)
        .bind(id)
        .bind(account_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| MembershipError::NotFound(format!("{} not found", spec.label)))
}

/// Create a set and answer its id and trimmed name. Fails when the name is
/// taken (ignoring case) or reserved.
pub async fn create_set(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<(i64, String), MembershipError> {
    let name = normalize_name(spec, name)?;
    if find_id(spec, conn, account_id, &name).await?.is_some() {
        return Err(MembershipError::Conflict(format!(
            "{} already exists",
            spec.label
        )));
    }
    let sql = format!(
        "INSERT INTO {table} (account_id, name) VALUES ($1, $2) RETURNING id",
        table = spec.table
    );
    let id: i64 = sqlx::query_scalar(&sql)
        .bind(account_id)
        .bind(&name)
        .fetch_one(&mut *conn)
        .await?;
    Ok((id, name))
}

/// Rename a set by id. A case-only change of its own name is allowed; another
/// set's name (ignoring case) is a conflict.
pub async fn rename_set(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
    name: &str,
) -> Result<String, MembershipError> {
    let (_, old_name) = get_set(spec, conn, account_id, id).await?;
    let new_name = normalize_name(spec, name)?;
    if old_name == new_name {
        return Ok(new_name);
    }
    if let Some(other) = find_id(spec, conn, account_id, &new_name).await?
        && other != id
    {
        return Err(MembershipError::Conflict(format!(
            "{} already exists",
            spec.label
        )));
    }
    let sql = format!(
        "UPDATE {table} SET name = $1 WHERE id = $2 AND account_id = $3",
        table = spec.table
    );
    sqlx::query(&sql)
        .bind(&new_name)
        .bind(id)
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(new_name)
}

/// Delete a set by id, and its memberships.
pub async fn delete_set(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<(), MembershipError> {
    get_set(spec, conn, account_id, id).await?;
    let members_sql = format!(
        "DELETE FROM {mt} WHERE {nc} = $1",
        mt = spec.members_table,
        nc = spec.name_column
    );
    sqlx::query(&members_sql)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    let sql = format!(
        "DELETE FROM {table} WHERE id = $1 AND account_id = $2",
        table = spec.table
    );
    sqlx::query(&sql)
        .bind(id)
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Member ids of one set, ascending.
pub async fn list_member_ids_of(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<Vec<i64>, MembershipError> {
    get_set(spec, conn, account_id, id).await?;
    let sql = format!(
        "SELECT {mc} FROM {mt} WHERE {nc} = $1 ORDER BY {mc}",
        mc = spec.member_column,
        mt = spec.members_table,
        nc = spec.name_column,
    );
    let rows = sqlx::query_scalar::<_, i64>(&sql)
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// Add and remove members of one set in one call, answering
/// `(added, removed)`. Every id is checked before anything is written, so a
/// foreign or unknown member id leaves the set as it was.
pub async fn patch_members(
    spec: &MembershipSpec,
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
    add: &[i64],
    remove: &[i64],
) -> Result<(u64, u64), MembershipError> {
    get_set(spec, conn, account_id, id).await?;
    let add = clean_ids(add);
    let remove = clean_ids(remove);
    if add.is_empty() && remove.is_empty() {
        return Err(MembershipError::BadRequest(format!(
            "{} ids required",
            spec.member_label
        )));
    }
    for member in add.iter().chain(remove.iter()) {
        if !member_exists(spec, conn, account_id, *member).await? {
            return Err(MembershipError::NotFound(format!(
                "{} {member} not found",
                spec.member_label
            )));
        }
    }

    let insert_sql = format!(
        "INSERT INTO {mt} ({mc}, {nc})
         SELECT id, $1 FROM {member_table} WHERE id = $2 AND account_id = $3
         ON CONFLICT DO NOTHING",
        mt = spec.members_table,
        mc = spec.member_column,
        nc = spec.name_column,
        member_table = spec.member_table,
    );
    let delete_sql = format!(
        "DELETE FROM {mt}
         WHERE {mc} = $1 AND {nc} = $2
           AND EXISTS (
             SELECT 1 FROM {member_table}
             WHERE {member_table}.id = {mt}.{mc}
               AND {member_table}.account_id = $3
           )",
        mt = spec.members_table,
        mc = spec.member_column,
        nc = spec.name_column,
        member_table = spec.member_table,
    );

    let mut added = 0u64;
    for member in add {
        let n = sqlx::query(&insert_sql)
            .bind(id)
            .bind(member)
            .bind(account_id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
        if n > 0 {
            added += 1;
            if let Some(hook) = spec.on_change {
                hook(conn, account_id, member)
                    .await
                    .map_err(|e| MembershipError::Internal(e.to_string()))?;
            }
        }
    }
    let mut removed = 0u64;
    for member in remove {
        let n = sqlx::query(&delete_sql)
            .bind(member)
            .bind(id)
            .bind(account_id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
        if n > 0 {
            removed += 1;
            if let Some(hook) = spec.on_change {
                hook(conn, account_id, member)
                    .await
                    .map_err(|e| MembershipError::Internal(e.to_string()))?;
            }
        }
    }
    Ok((added, removed))
}
```

Then, in `set_membership`, replace its first four lines (`let mut ids … ids.dedup();`) with `let ids = clean_ids(member_ids);` so the two share one cleaner.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p message-vault-server named_membership::tests`
Expected: 8 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/named_membership.rs
git commit -m "feat(vault): id-addressed functions for named sets

Contact Groups and Message Tags will be addressed by id on the HTTP
interface. This adds the seven functions that route layer needs beside
the name-addressed ones the import path still uses, with tests at the
DB level for each. Nothing calls them yet."
```

---

### Task 2: One route module for both collections, tested over HTTP

**Files:**
- Create: `crates/vault/server/src/named_set_api.rs`
- Rewrite: `crates/vault/server/src/contact_groups_api.rs`, `crates/vault/server/src/message_tags_api.rs`
- Modify: `crates/vault/server/src/lib.rs:7-36` (add `pub(crate) mod named_set_api;` in alphabetical order), `crates/vault/server/src/openapi.rs:84-125` (route registrations), `crates/vault/server/src/server.rs:760-764` (remove `MembershipChangedResponse`) and `:1690-1730` (the sibling-route test), `crates/vault/server/src/test_support.rs` (add `patch_json`)
- Regenerate: `docs/src/assets/openapi.json`

**Interfaces:**
- Consumes: Task 1's seven functions; `crate::server::{ApiError, AppState, ErrorBody, FullAccess}`; `crate::named_membership::{MembershipSpec, group_spec, tag_spec}`.
- Produces: `pub(crate)` types `NamedSet { id: i64, name: String }`, `NamedSetList { items: Vec<NamedSet> }`, `NamedSetBody { name: String }`, `MemberIdList { items: Vec<i64> }`, `MembersPatch { add: Vec<i64>, remove: Vec<i64> }`, `MembersChanged { added: u64, removed: u64 }`; twelve handlers named `contact_groups_list`, `contact_groups_create`, `contact_groups_update`, `contact_groups_delete`, `contact_group_members_list`, `contact_group_members_update`, and the same six with `message_tags` / `message_tag`. These names are the OpenAPI operationIds the web types in Task 5 are generated from.

- [ ] **Step 1: Add `patch_json` to `test_support.rs`**

After `patch_status`:

```rust
/// PATCH a JSON body with a Bearer token and decode the JSON response.
pub async fn patch_json<T: DeserializeOwned>(
    state: &AppState,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> T {
    let response = request(state, reqwest::Method::PATCH, path, Some(token), Some(body)).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "PATCH {path} must succeed"
    );
    response.json().await.unwrap()
}
```

- [ ] **Step 2: Write the failing HTTP tests**

Create `crates/vault/server/src/named_set_api.rs` with only this test module for now (the implementation comes in Step 4):

```rust
//! One HTTP surface for Contact Groups and Message Tags.
//!
//! Both are a named set the account owns plus a membership of contact or
//! conversation ids. The request and response types and the six operations
//! live here once, over [`MembershipSpec`]; `contact_groups_api.rs` and
//! `message_tags_api.rs` keep one three-line handler per route so every path
//! stays greppable and utoipa has a concrete function to describe.

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};

    use crate::server::AppState;
    use crate::test_support::{
        RegisteredAccount, delete_status, get_json, get_status, patch_json, patch_status,
        post_json, post_status, register_via_api, test_vault,
    };

    /// Which collection a case runs against. Every case runs for both.
    #[derive(Clone, Copy)]
    enum Kind {
        Groups,
        Tags,
    }

    impl Kind {
        fn base(self) -> &'static str {
            match self {
                Kind::Groups => "/v1/contact-groups",
                Kind::Tags => "/v1/message-tags",
            }
        }

        /// Insert one row a set of this kind can hold, answering its id.
        async fn member(self, state: &AppState, account_id: &str) -> i64 {
            let mut conn = state.db.acquire().await.unwrap();
            match self {
                Kind::Groups => sqlx::query_scalar(
                    "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Ada') RETURNING id",
                )
                .bind(account_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap(),
                Kind::Tags => {
                    let handle_id: i64 = sqlx::query_scalar(
                        "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
                         VALUES ($1, $2, $2, 'phone', 'phone') RETURNING id",
                    )
                    .bind(account_id)
                    .bind(format!("+1555{}", rand_suffix()))
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap();
                    sqlx::query_scalar(
                        "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, source_file)
                         VALUES ($1, $2, 'individual', 'seed.jsonl') RETURNING id",
                    )
                    .bind(account_id)
                    .bind(handle_id)
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap()
                }
            }
        }
    }

    /// Distinct handle text per inserted conversation.
    fn rand_suffix() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    async fn alice(state: &AppState) -> RegisteredAccount {
        register_via_api(state, "alice", "hunter2hunter2").await
    }

    async fn create(state: &AppState, kind: Kind, token: &str, name: &str) -> i64 {
        let set: Value = post_json(state, kind.base(), token, json!({ "name": name })).await;
        set["id"].as_i64().unwrap()
    }

    async fn names(state: &AppState, kind: Kind, token: &str) -> Vec<String> {
        let list: Value = get_json(state, kind.base(), token).await;
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap().to_string())
            .collect()
    }

    async fn member_ids(state: &AppState, kind: Kind, token: &str, id: i64) -> Vec<i64> {
        let list: Value = get_json(state, &format!("{}/{id}/members", kind.base()), token).await;
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect()
    }

    #[tokio::test]
    async fn create_list_update_and_delete_a_set() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;

            let created: Value =
                post_json(state, kind.base(), &user.token, json!({ "name": " Family " })).await;
            assert_eq!(created["name"], "Family");
            let id = created["id"].as_i64().unwrap();
            assert!(created.get("ok").is_none());

            create(state, kind, &user.token, "Work").await;
            assert_eq!(names(state, kind, &user.token).await, vec!["Family", "Work"]);

            let updated: Value = patch_json(
                state,
                &format!("{}/{id}", kind.base()),
                &user.token,
                json!({ "name": "Fam" }),
            )
            .await;
            assert_eq!(updated, json!({ "id": id, "name": "Fam" }));

            let case_only: Value = patch_json(
                state,
                &format!("{}/{id}", kind.base()),
                &user.token,
                json!({ "name": "fam" }),
            )
            .await;
            assert_eq!(case_only["name"], "fam");
            assert_eq!(names(state, kind, &user.token).await, vec!["fam", "Work"]);

            assert_eq!(
                delete_status(state, &format!("{}/{id}", kind.base()), &user.token).await,
                StatusCode::NO_CONTENT
            );
            assert_eq!(names(state, kind, &user.token).await, vec!["Work"]);
            assert_eq!(
                delete_status(state, &format!("{}/{id}", kind.base()), &user.token).await,
                StatusCode::NOT_FOUND
            );
        }
    }

    #[tokio::test]
    async fn create_and_update_refuse_duplicate_empty_and_reserved_names() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;
            create(state, kind, &user.token, "Family").await;
            let work = create(state, kind, &user.token, "Work").await;

            assert_eq!(
                post_status(state, kind.base(), &user.token, json!({ "name": "family" })).await,
                StatusCode::CONFLICT
            );
            assert_eq!(
                post_status(state, kind.base(), &user.token, json!({ "name": "Trash" })).await,
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                post_status(state, kind.base(), &user.token, json!({ "name": "  " })).await,
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                patch_status(
                    state,
                    &format!("{}/{work}", kind.base()),
                    &user.token,
                    json!({ "name": "FAMILY" })
                )
                .await,
                StatusCode::CONFLICT
            );
            assert_eq!(
                patch_status(
                    state,
                    &format!("{}/{work}", kind.base()),
                    &user.token,
                    json!({ "name": "" })
                )
                .await,
                StatusCode::BAD_REQUEST
            );
        }
    }

    #[tokio::test]
    async fn an_unknown_id_answers_404_on_every_route() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;
            let base = kind.base();
            assert_eq!(
                patch_status(state, &format!("{base}/999"), &user.token, json!({ "name": "X" }))
                    .await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                delete_status(state, &format!("{base}/999"), &user.token).await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                get_status(state, &format!("{base}/999/members"), &user.token).await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                patch_status(
                    state,
                    &format!("{base}/999/members"),
                    &user.token,
                    json!({ "add": [1] })
                )
                .await,
                StatusCode::NOT_FOUND
            );
        }
    }

    #[tokio::test]
    async fn members_patch_adds_and_removes_in_one_call() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;
            let a = kind.member(state, &user.account_id).await;
            let b = kind.member(state, &user.account_id).await;
            let id = create(state, kind, &user.token, "Family").await;
            let members = format!("{}/{id}/members", kind.base());

            let changed: Value =
                patch_json(state, &members, &user.token, json!({ "add": [a, b] })).await;
            assert_eq!(changed, json!({ "added": 2, "removed": 0 }));
            assert_eq!(member_ids(state, kind, &user.token, id).await, vec![a, b]);

            let changed: Value = patch_json(
                state,
                &members,
                &user.token,
                json!({ "add": [a], "remove": [b] }),
            )
            .await;
            assert_eq!(changed, json!({ "added": 0, "removed": 1 }));
            assert_eq!(member_ids(state, kind, &user.token, id).await, vec![a]);

            assert_eq!(
                patch_status(state, &members, &user.token, json!({})).await,
                StatusCode::BAD_REQUEST
            );
        }
    }

    #[tokio::test]
    async fn members_patch_with_a_foreign_member_writes_nothing() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;
            let a = kind.member(state, &user.account_id).await;
            let id = create(state, kind, &user.token, "Family").await;
            let members = format!("{}/{id}/members", kind.base());
            assert_eq!(
                patch_status(state, &members, &user.token, json!({ "add": [a, 999999] })).await,
                StatusCode::NOT_FOUND
            );
            assert!(member_ids(state, kind, &user.token, id).await.is_empty());
        }
    }

    #[tokio::test]
    async fn another_accounts_set_is_not_visible() {
        for kind in [Kind::Groups, Kind::Tags] {
            let vault = test_vault().await;
            let state = &vault.state;
            let user = alice(state).await;
            let bob = register_via_api(state, "bob", "hunter2hunter2").await;
            let id = create(state, kind, &bob.token, "Holiday").await;
            let base = kind.base();

            assert!(names(state, kind, &user.token).await.is_empty());
            assert_eq!(
                patch_status(state, &format!("{base}/{id}"), &user.token, json!({ "name": "X" }))
                    .await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                delete_status(state, &format!("{base}/{id}"), &user.token).await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                get_status(state, &format!("{base}/{id}/members"), &user.token).await,
                StatusCode::NOT_FOUND
            );
            assert_eq!(names(state, kind, &bob.token).await, vec!["Holiday"]);
        }
    }
}
```

- [ ] **Step 3: Register the module and run the tests to verify they fail**

Add `pub(crate) mod named_set_api;` to `crates/vault/server/src/lib.rs` between `named_membership` and `openapi`.

Run: `cargo test -p message-vault-server named_set_api::tests`
Expected: the tests compile, then five of the six fail, because the routes are not registered yet. `create_list_update_and_delete_a_set` panics at `created["id"].as_i64().unwrap()` (the old route answers `{name, groups}` with no id), and the members cases get 404 on the `/{id}/members` paths. `an_unknown_id_answers_404_on_every_route` passes already, since an unregistered path is also a 404; it earns its place once the routes exist.

- [ ] **Step 4: Write the shared module**

Insert above the test module in `named_set_api.rs`:

```rust
use axum::Json;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::named_membership::{self, MembershipSpec};
use crate::server::{ApiError, AppState};

/// One Contact Group or Message Tag: its id and name.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct NamedSet {
    pub(crate) id: i64,
    pub(crate) name: String,
}

/// The account's sets of one kind, A–Z.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct NamedSetList {
    pub(crate) items: Vec<NamedSet>,
}

/// A name to create, or the new name for an existing set.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct NamedSetBody {
    pub(crate) name: String,
}

/// Member ids of one set, ascending.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct MemberIdList {
    pub(crate) items: Vec<i64>,
}

/// Members to put in and take out of one set, in one request.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct MembersPatch {
    #[serde(default)]
    pub(crate) add: Vec<i64>,
    #[serde(default)]
    pub(crate) remove: Vec<i64>,
}

/// How many memberships a patch created and how many it removed.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct MembersChanged {
    pub(crate) added: u64,
    pub(crate) removed: u64,
}

pub(crate) async fn list(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
) -> Result<Json<NamedSetList>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let items = named_membership::list_sets(spec, &mut conn, account_id)
        .await?
        .into_iter()
        .map(|(id, name)| NamedSet { id, name })
        .collect();
    Ok(Json(NamedSetList { items }))
}

pub(crate) async fn create(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
    body: NamedSetBody,
) -> Result<Json<NamedSet>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let (id, name) = named_membership::create_set(spec, &mut conn, account_id, &body.name).await?;
    Ok(Json(NamedSet { id, name }))
}

pub(crate) async fn update(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
    id: i64,
    body: NamedSetBody,
) -> Result<Json<NamedSet>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let name = named_membership::rename_set(spec, &mut conn, account_id, id, &body.name).await?;
    Ok(Json(NamedSet { id, name }))
}

pub(crate) async fn delete(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
    id: i64,
) -> Result<StatusCode, ApiError> {
    let mut conn = state.db.acquire().await?;
    named_membership::delete_set(spec, &mut conn, account_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn members_list(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
    id: i64,
) -> Result<Json<MemberIdList>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let items = named_membership::list_member_ids_of(spec, &mut conn, account_id, id).await?;
    Ok(Json(MemberIdList { items }))
}

pub(crate) async fn members_update(
    spec: &'static MembershipSpec,
    state: &AppState,
    account_id: &str,
    id: i64,
    body: MembersPatch,
) -> Result<Json<MembersChanged>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let (added, removed) =
        named_membership::patch_members(spec, &mut conn, account_id, id, &body.add, &body.remove)
            .await?;
    Ok(Json(MembersChanged { added, removed }))
}
```

- [ ] **Step 5: Rewrite `contact_groups_api.rs` as twelve-line handlers**

Replace the whole file with:

```rust
//! Contact Groups over HTTP: one handler per route, each a call into
//! [`crate::named_set_api`] with [`group_spec`].

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::named_membership::group_spec;
use crate::named_set_api::{
    self, MemberIdList, MembersChanged, MembersPatch, NamedSet, NamedSetBody, NamedSetList,
};
use crate::server::{ApiError, AppState, ErrorBody, FullAccess};

/// The account's Contact Groups, A–Z.
#[utoipa::path(
    get,
    path = "/v1/contact-groups",
    tag = "Contacts",
    security(("bearer" = [])),
    responses(
        (status = 200, body = NamedSetList),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody)
    )
)]
pub(crate) async fn contact_groups_list(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
) -> Result<Json<NamedSetList>, ApiError> {
    named_set_api::list(group_spec(), &state, &auth.account_id).await
}

/// Create a Contact Group.
#[utoipa::path(
    post,
    path = "/v1/contact-groups",
    tag = "Contacts",
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
pub(crate) async fn contact_groups_create(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Json(body): Json<NamedSetBody>,
) -> Result<Json<NamedSet>, ApiError> {
    named_set_api::create(group_spec(), &state, &auth.account_id, body).await
}

/// Rename a Contact Group.
#[utoipa::path(
    patch,
    path = "/v1/contact-groups/{id}",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact Group id")),
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
pub(crate) async fn contact_groups_update(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
    Json(body): Json<NamedSetBody>,
) -> Result<Json<NamedSet>, ApiError> {
    named_set_api::update(group_spec(), &state, &auth.account_id, id, body).await
}

/// Delete a Contact Group and its memberships.
#[utoipa::path(
    delete,
    path = "/v1/contact-groups/{id}",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact Group id")),
    responses(
        (status = 204),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub(crate) async fn contact_groups_delete(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    named_set_api::delete(group_spec(), &state, &auth.account_id, id).await
}

/// Contact ids in one Contact Group.
#[utoipa::path(
    get,
    path = "/v1/contact-groups/{id}/members",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact Group id")),
    responses(
        (status = 200, body = MemberIdList),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub(crate) async fn contact_group_members_list(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
) -> Result<Json<MemberIdList>, ApiError> {
    named_set_api::members_list(group_spec(), &state, &auth.account_id, id).await
}

/// Put contacts in and take contacts out of one Contact Group.
#[utoipa::path(
    patch,
    path = "/v1/contact-groups/{id}/members",
    tag = "Contacts",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Contact Group id")),
    request_body = MembersPatch,
    responses(
        (status = 200, body = MembersChanged),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub(crate) async fn contact_group_members_update(
    State(state): State<AppState>,
    FullAccess(auth): FullAccess,
    Path(id): Path<i64>,
    Json(body): Json<MembersPatch>,
) -> Result<Json<MembersChanged>, ApiError> {
    named_set_api::members_update(group_spec(), &state, &auth.account_id, id, body).await
}
```

- [ ] **Step 6: Rewrite `message_tags_api.rs` the same way**

Replace the whole file with the same six handlers, changed as follows and nothing else: module doc says "Message Tags"; import `tag_spec` instead of `group_spec` and pass `tag_spec()`; `tag = "Message tags"`; every path starts `/v1/message-tags`; handler names are `message_tags_list`, `message_tags_create`, `message_tags_update`, `message_tags_delete`, `message_tag_members_list`, `message_tag_members_update`; path param descriptions say "Message Tag id"; doc comments say "Message Tag" and "Conversation ids in one Message Tag" / "Put conversations in and take conversations out of one Message Tag".

- [ ] **Step 7: Register the twelve handlers and remove the old registrations**

In `crates/vault/server/src/openapi.rs`, delete every `.routes(routes!(crate::contact_groups_api::…_handler))` and `.routes(routes!(crate::message_tags_api::…_handler))` line (twelve of them; the `message_tags_membership_handler` one sits after the saved-search lines). In the place where the contact-group lines were, add:

```rust
        .routes(routes!(crate::contact_groups_api::contact_groups_list))
        .routes(routes!(crate::contact_groups_api::contact_groups_create))
        .routes(routes!(crate::contact_groups_api::contact_groups_update))
        .routes(routes!(crate::contact_groups_api::contact_groups_delete))
        .routes(routes!(
            crate::contact_groups_api::contact_group_members_list
        ))
        .routes(routes!(
            crate::contact_groups_api::contact_group_members_update
        ))
        .routes(routes!(crate::message_tags_api::message_tags_list))
        .routes(routes!(crate::message_tags_api::message_tags_create))
        .routes(routes!(crate::message_tags_api::message_tags_update))
        .routes(routes!(crate::message_tags_api::message_tags_delete))
        .routes(routes!(crate::message_tags_api::message_tag_members_list))
        .routes(routes!(
            crate::message_tags_api::message_tag_members_update
        ))
```

Also remove the `MembershipChangedResponse` struct and its doc comment from `server.rs` (around line 760). `openapi.rs` has no hand-listed schema block; the schemas come from the route macros, so registering the twelve handlers is all the document needs.

- [ ] **Step 8: Update the sibling-route test in `server.rs`**

In `literal_contact_routes_are_not_captured_by_the_id_route` (around line 1690), remove `"/v1/contacts/groups",` from the array, and change the doc comment's first sentence to: `/// `/v1/contacts/{id}` takes an `i64`, and three literal routes sit beside it: `summaries`, `match`, and `address-book`. All three are `POST`, …` (keep the rest).

- [ ] **Step 9: Build, run the tests, and regenerate the OpenAPI document**

Run: `cargo build -p message-vault-server`
Expected: builds. Unused-function warnings for `list_names`, `create_name`, `rename_name`, `delete_name`, and `list_member_ids` in `named_membership.rs` are expected here; Task 3 removes them.

Run: `cargo test -p message-vault-server named_set_api::tests`
Expected: 6 passed.

Run: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json`
Then: `cargo test -p message-vault-server openapi`
Expected: `committed_openapi_matches_dump` passes. Open the JSON and confirm the twelve operationIds read `contact_groups_list`, …, `message_tag_members_update`, and that `/v1/contacts/groups`, `/v1/conversations/tags`, `/v1/contact-groups/members`, and `/v1/message-tags/members` are gone.

Run: `cargo test -p message-vault-server`
Expected: all pass. The only tests that referenced the removed routes were the two DB-level modules that lived in the rewritten files and the sibling-route test edited in Step 8.

- [ ] **Step 10: Format and commit**

```bash
cargo fmt --all
git add crates/vault/server/src docs/src/assets/openapi.json
git commit -m "feat(vault): address Contact Groups and Message Tags by id

Both collections are now PATCH and DELETE /v1/<collection>/{id}, with
membership at /v1/<collection>/{id}/members instead of under /v1/contacts
and /v1/conversations. One module, named_set_api.rs, implements the six
operations for both over MembershipSpec; the two route files keep one
three-line handler per route so every path stays greppable. Responses
follow one rule: a list answers {items}, a mutation answers the set,
delete answers 204. The twelve routes are tested over real HTTP for
both collections. See docs/adr/0003."
```

---

### Task 3: Remove the name-addressed functions nothing calls

**Files:**
- Modify: `crates/vault/server/src/named_membership.rs`

**Interfaces:**
- Keeps: `set_membership`, `names_for_item`, `names_for_items`, `is_reserved`, `ensure_id`, `find_id`, `normalize_name`, `member_exists`, `MAX_NAME_LEN`, and Task 1's seven functions.
- Removes: `list_names`, `create_name`, `rename_name`, `delete_name`, `list_member_ids`.

- [ ] **Step 1: Confirm nothing outside the file calls them**

Run: `grep -rn "list_names\|create_name\|rename_name\|delete_name\|list_member_ids\b" crates/vault/server/src --include=*.rs | grep -v named_membership.rs`
Expected: no output. If a line appears, that caller moves to the id-addressed function of the same purpose before deleting.

- [ ] **Step 2: Delete the five functions**

Remove `list_names`, `create_name`, `rename_name`, `delete_name`, and `list_member_ids` from `named_membership.rs`, doc comments included. Update the module doc's first paragraph to read:

```rust
//! Shared storage for named sets (Message Tags and Contact Groups).
//!
//! Both domains store a named set (rows in a names table) whose members are
//! conversation or contact ids. The HTTP layer addresses a set by id through
//! `list_sets`, `get_set`, `create_set`, `rename_set`, `delete_set`,
//! `list_member_ids_of`, and `patch_members`. The import path still fills a
//! group by name through `set_membership`, which creates the name on demand.
//! The operations are identical apart from table and column names, reserved
//! names, and one post-change hook, so this module implements them once
//! behind [`MembershipSpec`].
```

- [ ] **Step 3: Build and test**

Run: `cargo build -p message-vault-server 2>&1 | grep -c "never used"`
Expected: `0`.

Run: `cargo test -p message-vault-server`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all
git add crates/vault/server/src/named_membership.rs
git commit -m "refactor(vault): drop the name-addressed set functions

The HTTP layer addresses sets by id now, so list, create, rename, delete,
and member listing by name have no callers. set_membership stays for the
import path, which creates and fills a group by name."
```

---

### Task 4: `api.ts` answers `undefined` for a 204

**Files:**
- Modify: `web/src/lib/api.ts:70-95` (the `request` function)
- Test: `web/src/lib/api.test.ts`

**Interfaces:**
- Produces: `apiClient.delete<void>(path)` resolves `undefined` on a `204`. Task 5's `deleteContactGroup` and `deleteMessageTag` rely on it.

- [ ] **Step 1: Write the failing test**

Add to the `describe("apiClient errors", …)` block's parent, as a new `describe`, in `web/src/lib/api.test.ts`:

```ts
describe("apiClient no-content", () => {
  it("resolves undefined for a 204 rather than parsing an empty body", async () => {
    const json = vi.fn().mockRejectedValue(new SyntaxError("Unexpected end of JSON input"));
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({ ok: true, status: 204, json, text: async () => "" }),
    );
    await expect(apiClient.delete("/v1/contact-groups/7")).resolves.toBeUndefined();
    expect(json).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/lib/api.test.ts`
Expected: FAIL, the promise rejects with the `SyntaxError`.

- [ ] **Step 3: Handle the status**

In `request<T>` in `web/src/lib/api.ts`, between the `if (!res.ok)` block and `return res.json()`:

```ts
  // A 204 has no body by definition; asking for JSON would throw.
  if (res.status === 204) {
    return undefined as T;
  }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd web && npx vitest run src/lib/api.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/api.ts web/src/lib/api.test.ts
git commit -m "fix(web): let the API client accept a 204

Deleting a Contact Group or Message Tag answers 204 with no body. The
client parsed every successful response as JSON, which throws on an
empty body."
```

---

### Task 5: Twelve route functions in `vaultApi.ts`

**Files:**
- Regenerate: `web/src/lib/vaultApi.types.ts`
- Modify: `web/src/lib/vaultApi.ts:311-375` (the Contact Groups and Message Tags sections)
- Test: `web/src/lib/vaultApi.test.ts`

**Interfaces:**
- Consumes: `Schema["NamedSet"]`, `Schema["NamedSetList"]`, `Schema["NamedSetBody"]`, `Schema["MemberIdList"]`, `Schema["MembersPatch"]`, `Schema["MembersChanged"]` from the regenerated types; `apiClient` from Task 4.
- Produces, exported from `vaultApi.ts`:
  - `listContactGroups(opts?: VaultRequestOptions): Promise<Schema["NamedSetList"]>`
  - `createContactGroup(body: Schema["NamedSetBody"], opts?): Promise<Schema["NamedSet"]>`
  - `updateContactGroup(id: number, body: Schema["NamedSetBody"], opts?): Promise<Schema["NamedSet"]>`
  - `deleteContactGroup(id: number, opts?): Promise<void>`
  - `listContactGroupMembers(id: number, opts?): Promise<Schema["MemberIdList"]>`
  - `updateContactGroupMembers(id: number, body: Schema["MembersPatch"], opts?): Promise<Schema["MembersChanged"]>`
  - the same six as `listMessageTags`, `createMessageTag`, `updateMessageTag`, `deleteMessageTag`, `listMessageTagMembers`, `updateMessageTagMembers`.
  - Removed: `renameContactGroup`, `setContactGroupMembership`, `renameMessageTag`, `setMessageTagMembership`. Task 7 consumes the new twelve.

- [ ] **Step 1: Regenerate the types**

Run: `cd web && npm run gen:api`
Expected: `src/lib/vaultApi.types.ts` changes; `git diff --stat` shows it. The old schemas (`ContactGroupNameBody`, `MessageTagNamedListResponse`, `MembershipChangedResponse`, and the rest) are gone and the six `NamedSet…`/`Member…` schemas are present.

- [ ] **Step 2: Write the failing tests**

In `web/src/lib/vaultApi.test.ts`, add the twelve functions to the import from `./vaultApi` and add:

```ts
describe("Contact Groups and Message Tags are addressed by id", () => {
  it("lists and creates on the collection", async () => {
    await listContactGroups();
    expect(lastPath(get)).toBe("/v1/contact-groups");
    await createMessageTag({ name: "Holiday" });
    expect(post).toHaveBeenCalledWith("/v1/message-tags", { name: "Holiday" }, undefined);
  });

  it("renames with PATCH on the id and deletes with DELETE on the id", async () => {
    await updateContactGroup(12, { name: "Fam" });
    expect(patch).toHaveBeenCalledWith("/v1/contact-groups/12", { name: "Fam" }, undefined);
    await deleteMessageTag(7);
    expect(del).toHaveBeenCalledWith("/v1/message-tags/7", undefined, undefined);
  });

  it("reads and patches membership under the set", async () => {
    await listMessageTagMembers(7);
    expect(lastPath(get)).toBe("/v1/message-tags/7/members");
    await updateContactGroupMembers(12, { add: [1, 2], remove: [3] });
    expect(patch).toHaveBeenCalledWith(
      "/v1/contact-groups/12/members",
      { add: [1, 2], remove: [3] },
      undefined,
    );
  });

  it("passes the abort options through on a write", async () => {
    const controller = new AbortController();
    await deleteContactGroup(12, { signal: controller.signal });
    expect(del).toHaveBeenCalledWith("/v1/contact-groups/12", undefined, {
      signal: controller.signal,
    });
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd web && npx vitest run src/lib/vaultApi.test.ts`
Expected: FAIL, `updateContactGroup` and the others are not exported.

- [ ] **Step 4: Replace the two sections in `vaultApi.ts`**

Replace everything from `// ── Contact Groups ───` through the end of `setMessageTagMembership` with:

```ts
// ── Contact Groups ──────────────────────────────────────────────────────────
//
// A Contact Group is addressed by its id. Screens hold names; the lookup from
// a name to an id lives in `nameCollection.ts`, not here.

export function listContactGroups(opts?: VaultRequestOptions): Promise<Schema["NamedSetList"]> {
  return apiClient.get<Schema["NamedSetList"]>("/v1/contact-groups", opts);
}

export function createContactGroup(
  body: Schema["NamedSetBody"],
  opts?: VaultRequestOptions,
): Promise<Schema["NamedSet"]> {
  return apiClient.post<Schema["NamedSet"]>("/v1/contact-groups", body, opts);
}

export function updateContactGroup(
  id: number,
  body: Schema["NamedSetBody"],
  opts?: VaultRequestOptions,
): Promise<Schema["NamedSet"]> {
  return apiClient.patch<Schema["NamedSet"]>(`/v1/contact-groups/${id}`, body, opts);
}

export function deleteContactGroup(id: number, opts?: VaultRequestOptions): Promise<void> {
  return apiClient.delete<void>(`/v1/contact-groups/${id}`, undefined, opts);
}

export function listContactGroupMembers(
  id: number,
  opts?: VaultRequestOptions,
): Promise<Schema["MemberIdList"]> {
  return apiClient.get<Schema["MemberIdList"]>(`/v1/contact-groups/${id}/members`, opts);
}

export function updateContactGroupMembers(
  id: number,
  body: Schema["MembersPatch"],
  opts?: VaultRequestOptions,
): Promise<Schema["MembersChanged"]> {
  return apiClient.patch<Schema["MembersChanged"]>(`/v1/contact-groups/${id}/members`, body, opts);
}

// ── Message Tags ────────────────────────────────────────────────────────────

export function listMessageTags(opts?: VaultRequestOptions): Promise<Schema["NamedSetList"]> {
  return apiClient.get<Schema["NamedSetList"]>("/v1/message-tags", opts);
}

export function createMessageTag(
  body: Schema["NamedSetBody"],
  opts?: VaultRequestOptions,
): Promise<Schema["NamedSet"]> {
  return apiClient.post<Schema["NamedSet"]>("/v1/message-tags", body, opts);
}

export function updateMessageTag(
  id: number,
  body: Schema["NamedSetBody"],
  opts?: VaultRequestOptions,
): Promise<Schema["NamedSet"]> {
  return apiClient.patch<Schema["NamedSet"]>(`/v1/message-tags/${id}`, body, opts);
}

export function deleteMessageTag(id: number, opts?: VaultRequestOptions): Promise<void> {
  return apiClient.delete<void>(`/v1/message-tags/${id}`, undefined, opts);
}

export function listMessageTagMembers(
  id: number,
  opts?: VaultRequestOptions,
): Promise<Schema["MemberIdList"]> {
  return apiClient.get<Schema["MemberIdList"]>(`/v1/message-tags/${id}/members`, opts);
}

export function updateMessageTagMembers(
  id: number,
  body: Schema["MembersPatch"],
  opts?: VaultRequestOptions,
): Promise<Schema["MembersChanged"]> {
  return apiClient.patch<Schema["MembersChanged"]>(`/v1/message-tags/${id}/members`, body, opts);
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd web && npx vitest run src/lib/vaultApi.test.ts`
Expected: PASS. `npx tsc --noEmit -p .` will fail in `contactGroups.ts` and `messageTags.ts` until Task 7; that is expected at this step.

- [ ] **Step 6: Commit**

```bash
cd web && npx biome format --write src/lib/vaultApi.ts src/lib/vaultApi.test.ts && cd ..
git add web/src/lib/vaultApi.ts web/src/lib/vaultApi.test.ts web/src/lib/vaultApi.types.ts
git commit -m "feat(web): route functions for Contact Groups and Message Tags by id

Twelve functions named by the <action><Resource> rule replace the ten
that carried a name in the body. Every one takes opts last, so a write
can be cancelled like a read. The feature module does not compile until
the next commit switches it over."
```

---

### Task 6: Cache readers in `vaultQuery.ts`

**Files:**
- Modify: `web/src/lib/vaultQuery.ts` (add two hooks after `useVaultSetCached`)
- Test: `web/src/lib/vaultQuery.test.tsx`

**Interfaces:**
- Produces:
  - `useVaultCached(): <T>(key: VaultQueryKey) => T | undefined` — what the cache holds for the account-scoped key, without fetching.
  - `useVaultFetchFresh(): <T>(key: VaultQueryKey, queryFn: (signal: AbortSignal) => Promise<T>) => Promise<T>` — always asks the vault, stores the answer under the key, and resolves it.
  - Task 7 consumes both.

- [ ] **Step 1: Write the failing tests**

Add to `web/src/lib/vaultQuery.test.tsx`, importing `useVaultCached` and `useVaultFetchFresh` from `./vaultQuery`:

```ts
describe("useVaultCached and useVaultFetchFresh", () => {
  it("reads what the cache holds for the signed-in account, and nothing for another", () => {
    client.setQueryData(["vault", "account-1", "contact-groups"], [{ id: 1, name: "Family" }]);
    const { result } = renderHook(() => useVaultCached(), { wrapper });
    expect(result.current<{ id: number; name: string }[]>(["contact-groups"])).toEqual([
      { id: 1, name: "Family" },
    ]);
    account.current = "account-2";
    const other = renderHook(() => useVaultCached(), { wrapper });
    expect(other.result.current(["contact-groups"])).toBeUndefined();
  });

  it("always asks the vault and stores the answer under the account's key", async () => {
    client.setQueryData(["vault", "account-1", "contact-groups"], [{ id: 1, name: "Old" }]);
    const fetchGroups = vi.fn(async () => [{ id: 1, name: "New" }]);
    const { result } = renderHook(() => useVaultFetchFresh(), { wrapper });
    await expect(result.current(["contact-groups"], fetchGroups)).resolves.toEqual([
      { id: 1, name: "New" },
    ]);
    expect(fetchGroups).toHaveBeenCalledTimes(1);
    expect(client.getQueryData(["vault", "account-1", "contact-groups"])).toEqual([
      { id: 1, name: "New" },
    ]);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd web && npx vitest run src/lib/vaultQuery.test.tsx`
Expected: FAIL, the two hooks are not exported.

- [ ] **Step 3: Add the hooks**

After `useVaultSetCached` in `web/src/lib/vaultQuery.ts`:

```ts
/**
 * What the cache holds for a key, or `undefined`, without fetching.
 *
 * For a lookup that a write needs before it can be sent — a Contact Group's
 * id behind the name a screen holds — where the list is almost always in the
 * cache already.
 */
export function useVaultCached(): <T>(key: VaultQueryKey) => T | undefined {
  const client = useQueryClient();
  const account = useAccountScope();
  return useCallback(
    <T>(key: VaultQueryKey) => client.getQueryData<T>(vaultQueryKey(account, key)),
    [client, account],
  );
}

/**
 * Ask the vault now, store the answer under the key, and resolve it.
 *
 * The one case `useVaultCached` cannot cover: the cache has no entry, or has
 * one that does not hold what the caller is looking for. `staleTime: 0` makes
 * the library fetch rather than hand back the entry it already has.
 */
export function useVaultFetchFresh(): <T>(
  key: VaultQueryKey,
  queryFn: (signal: AbortSignal) => Promise<T>,
) => Promise<T> {
  const client = useQueryClient();
  const account = useAccountScope();
  return useCallback(
    <T>(key: VaultQueryKey, queryFn: (signal: AbortSignal) => Promise<T>) =>
      client.fetchQuery<T>({
        queryKey: vaultQueryKey(account, key),
        queryFn: ({ signal }) => queryFn(signal),
        staleTime: 0,
      }),
    [client, account],
  );
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx vitest run src/lib/vaultQuery.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/vaultQuery.ts web/src/lib/vaultQuery.test.tsx
git commit -m "feat(web): read the cache and force a fetch, account-scoped

Two small hooks over the query client, so the feature module can look a
name up in the list it already holds and ask the vault when it is not
there. Both name the account in the key like every other cache access."
```

---

### Task 7: `nameCollection.ts` resolves names to ids and invalidates the chip lists

**Files:**
- Rewrite: `web/src/lib/nameCollection.ts`
- Modify: `web/src/lib/contactGroups.ts:1-8, 67-80`, `web/src/lib/messageTags.ts:1-10, 37-50`, `web/src/screens/ContactList.tsx:348, 360, 386, 400`, `web/src/screens/ConversationList.tsx:128, 143`
- Create: `web/src/lib/nameCollection.test.ts`

**Interfaces:**
- Consumes: Task 5's twelve route functions; Task 6's `useVaultCached`, `useVaultFetchFresh`; existing `useVaultQuery`, `useVaultInvalidate`.
- Produces, exported from `nameCollection.ts`:
  - `type NamedSet = { id: number; name: string }`
  - `type MembersPatch = { add?: number[]; remove?: number[] }` — what a screen passes to `setMembers`; the module fills the missing side with `[]` before calling the route, whose generated body type requires both.
  - `NameCollectionRoutes` with `list`, `create`, `update`, `remove`, `updateMembers(id, body: { add: number[]; remove: number[] })`
  - `NameCollectionConfig` gains `label: string` and `invalidates: readonly (readonly [string])[]`; loses `responseKey`
  - `useNameCollection(collection): { names: string[]; loading: boolean }` (unchanged)
  - `useNameCollectionActions(collection): { create, rename, remove, setMembers, invalidate }` where `setMembers(name: string, patch: MembersPatch): Promise<{ added: number; removed: number }>` replaces `setMembership(ids, name, enable)`.

- [ ] **Step 1: Write the failing tests**

Create `web/src/lib/nameCollection.test.ts`:

```ts
/** @vitest-environment jsdom */

/**
 * The name-based interface over id-addressed routes.
 *
 * Screens hold names; the vault addresses a set by id. Everything about that
 * translation — where the id comes from, what happens when the name is not
 * there, and which lists go stale after a write — is this module's, so it is
 * tested here at the interface with the routes faked by name.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createNameCollection,
  type NameCollectionRoutes,
  useNameCollection,
  useNameCollectionActions,
} from "./nameCollection";

vi.mock("./auth", () => ({
  useAuth: () => ({ accountId: "account-1" }),
}));

let client: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

function fakeRoutes(): { [K in keyof NameCollectionRoutes]: ReturnType<typeof vi.fn> } {
  return {
    list: vi.fn().mockResolvedValue({ items: [] }),
    create: vi.fn(),
    update: vi.fn(),
    remove: vi.fn().mockResolvedValue(undefined),
    updateMembers: vi.fn().mockResolvedValue({ added: 0, removed: 0 }),
  };
}

function groupsOver(routes: NameCollectionRoutes) {
  return createNameCollection({
    routes,
    cacheKey: "contact-groups",
    invalidates: [["contacts"], ["contact-detail"]],
    label: "group",
    queryToken: "group",
    reservedNames: new Set(["trash"]),
    reservedError: (name) => `${name} is reserved`,
  });
}

const KEY = ["vault", "account-1", "contact-groups"];

beforeEach(() => {
  client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
  });
});

describe("useNameCollection", () => {
  it("answers the names in the vault's order", async () => {
    const routes = fakeRoutes();
    routes.list.mockResolvedValue({
      items: [
        { id: 2, name: "Family" },
        { id: 1, name: "Work" },
      ],
    });
    const { result } = renderHook(() => useNameCollection(groupsOver(routes)), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.names).toEqual(["Family", "Work"]);
  });
});

describe("useNameCollectionActions", () => {
  it("renames by the id the cache holds and invalidates the lists that show the name", async () => {
    const routes = fakeRoutes();
    routes.update.mockResolvedValue({ id: 12, name: "Fam" });
    client.setQueryData(KEY, [{ id: 12, name: "Family" }]);
    const invalidate = vi.spyOn(client, "invalidateQueries");

    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    await expect(result.current.rename("Family", "Fam")).resolves.toBe("Fam");

    expect(routes.update).toHaveBeenCalledWith(12, { name: "Fam" });
    expect(routes.list).not.toHaveBeenCalled();
    const keys = invalidate.mock.calls.map((call) => call[0]?.queryKey);
    expect(keys).toEqual(
      expect.arrayContaining([
        ["vault", "account-1", "contact-groups"],
        ["vault", "account-1", "contacts"],
        ["vault", "account-1", "contact-detail"],
      ]),
    );
  });

  it("matches a name without regard to letter case", async () => {
    const routes = fakeRoutes();
    client.setQueryData(KEY, [{ id: 12, name: "Family" }]);
    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    await result.current.remove("family");
    expect(routes.remove).toHaveBeenCalledWith(12);
  });

  it("asks the vault once when the cache does not hold the name", async () => {
    const routes = fakeRoutes();
    routes.list.mockResolvedValue({ items: [{ id: 7, name: "Holiday" }] });
    routes.updateMembers.mockResolvedValue({ added: 2, removed: 0 });
    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });

    await expect(result.current.setMembers("Holiday", { add: [1, 2] })).resolves.toEqual({
      added: 2,
      removed: 0,
    });
    expect(routes.list).toHaveBeenCalledTimes(1);
    expect(routes.updateMembers).toHaveBeenCalledWith(7, { add: [1, 2], remove: [] });
    expect(client.getQueryData(KEY)).toEqual([{ id: 7, name: "Holiday" }]);
  });

  it("throws without a request when the vault has no set of that name", async () => {
    const routes = fakeRoutes();
    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    await expect(result.current.remove("Nope")).rejects.toThrow("group not found");
    expect(routes.list).toHaveBeenCalledTimes(1);
    expect(routes.remove).not.toHaveBeenCalled();
  });

  it("refuses a reserved name before asking the vault", async () => {
    const routes = fakeRoutes();
    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    await expect(result.current.create("Trash")).rejects.toThrow("Trash is reserved");
    expect(routes.create).not.toHaveBeenCalled();
  });

  it("answers the created name and invalidates its own list", async () => {
    const routes = fakeRoutes();
    routes.create.mockResolvedValue({ id: 3, name: "Work" });
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    await expect(result.current.create(" Work ")).resolves.toBe("Work");
    expect(routes.create).toHaveBeenCalledWith({ name: "Work" });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: KEY });
  });
});
```

Name the file `nameCollection.test.tsx` (it renders JSX in `wrapper`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd web && npx vitest run src/lib/nameCollection.test.tsx`
Expected: FAIL, `invalidates` and `label` are not accepted, `setMembers` does not exist, and `list` answers no `items`.

- [ ] **Step 3: Rewrite `nameCollection.ts`**

Replace the whole file with:

```ts
import { useMemo } from "react";
import {
  useVaultCached,
  useVaultFetchFresh,
  useVaultInvalidate,
  useVaultQuery,
  type VaultQueryKey,
} from "./vaultQuery";

/**
 * Contact Groups and Message Tags are the same feature over different nouns: a
 * named set the account owns, and a membership that puts rows in or out of it.
 * This builds one from a description of the nouns so the two do not drift
 * apart.
 *
 * The vault addresses a set by its id; screens, the sidebar, and the router
 * hold names. The lookup from one to the other lives here and nowhere else:
 * the id comes from the cached list, or from the vault once when the cached
 * list does not hold the name, and a name the vault does not know is an error
 * before any request is sent. See `docs/adr/0003`.
 */

/** One set as the vault answers it. */
export type NamedSet = { id: number; name: string };

/** Members to put in and take out of a set, in one request. Either side may be left off. */
export type MembersPatch = { add?: number[]; remove?: number[] };

/** The vault calls one of these collections is built from. */
export type NameCollectionRoutes = {
  list: (opts?: { signal?: AbortSignal }) => Promise<{ items: NamedSet[] }>;
  create: (body: { name: string }) => Promise<NamedSet>;
  update: (id: number, body: { name: string }) => Promise<NamedSet>;
  remove: (id: number) => Promise<void>;
  updateMembers: (
    id: number,
    body: { add: number[]; remove: number[] },
  ) => Promise<{ added: number; removed: number }>;
};

export type NameCollectionConfig = {
  routes: NameCollectionRoutes;
  /** Name of this collection in a cache key, e.g. `contact-groups`. */
  cacheKey: string;
  /**
   * Cache keys of the lists that show these names as chips, invalidated after
   * every write. Matched by prefix, so `["contacts"]` covers every page and
   * every search of the contact list.
   */
  invalidates: readonly (readonly [string])[];
  /** What one of these is called in an error, e.g. `group`. */
  label: string;
  /** Search token used in list queries, e.g. `group` for `group:Family`. */
  queryToken: string;
  reservedNames: ReadonlySet<string>;
  reservedError: (name: string) => string;
};

export type NameCollection = {
  /** Cache key parts, before the account is put in front of them. */
  key: readonly [string];
  routes: NameCollectionRoutes;
  invalidates: readonly (readonly [string])[];
  label: string;
  isReserved: (name: string) => boolean;
  reservedError: (name: string) => string;
  /** Build the list query for one page of this collection plus a typed search. */
  listQuery: (name: string | "none" | null, search: string) => string;
};

export function createNameCollection(config: NameCollectionConfig): NameCollection {
  const isReserved = (name: string) => config.reservedNames.has(name.trim().toLowerCase());

  function listQuery(name: string | "none" | null, search: string): string {
    const parts: string[] = [];
    if (name === "none") {
      parts.push(`${config.queryToken}:none`);
    } else if (name) {
      parts.push(
        /\s/.test(name) ? `${config.queryToken}:"${name}"` : `${config.queryToken}:${name}`,
      );
    }
    const extra = search.trim();
    if (extra) parts.push(extra);
    return parts.join(" ");
  }

  return {
    key: [config.cacheKey] as const,
    routes: config.routes,
    invalidates: config.invalidates,
    label: config.label,
    isReserved,
    reservedError: config.reservedError,
    listQuery,
  };
}

/** The cache holds the vault's list as it came, ids included. */
async function fetchSets(collection: NameCollection, signal: AbortSignal): Promise<NamedSet[]> {
  return (await collection.routes.list({ signal })).items;
}

/** Live list of one collection's names for the signed-in account. */
export function useNameCollection(collection: NameCollection): {
  names: string[];
  loading: boolean;
} {
  const { data, isPending } = useVaultQuery(collection.key, (signal) =>
    fetchSets(collection, signal),
  );
  const names = useMemo(() => (data ?? []).map((set) => set.name), [data]);
  return { names, loading: isPending };
}

/**
 * The four things a person can do to one of these collections.
 *
 * Every write invalidates the collection's own list and the lists that show
 * its names as chips, so a renamed or deleted group disappears from contact
 * rows without anyone reloading.
 */
export function useNameCollectionActions(collection: NameCollection): {
  create: (name: string) => Promise<string>;
  rename: (from: string, to: string) => Promise<string>;
  remove: (name: string) => Promise<void>;
  setMembers: (name: string, patch: MembersPatch) => Promise<{ added: number; removed: number }>;
  invalidate: () => Promise<void>;
} {
  const cached = useVaultCached();
  const fetchFresh = useVaultFetchFresh();
  const invalidate = useVaultInvalidate();

  // One stable object, so a caller can list it as a dependency without
  // rebuilding every callback that uses it on each render.
  return useMemo(() => {
    const findId = (sets: NamedSet[] | undefined, name: string): number | undefined => {
      const wanted = name.trim().toLowerCase();
      return sets?.find((set) => set.name.toLowerCase() === wanted)?.id;
    };

    /**
     * The id behind a name: from the cache, else from the vault once, else an
     * error and no request. The vault-once path covers creating a set and
     * adding to it before the invalidated list has come back.
     */
    async function idOf(name: string): Promise<number> {
      const hit = findId(cached<NamedSet[]>(collection.key), name);
      if (hit !== undefined) return hit;
      const fresh = findId(
        await fetchFresh(collection.key, (signal) => fetchSets(collection, signal)),
        name,
      );
      if (fresh !== undefined) return fresh;
      throw new Error(`${collection.label} not found`);
    }

    const staleKeys: readonly VaultQueryKey[] = [collection.key, ...collection.invalidates];
    const changed = async () => {
      await Promise.all(staleKeys.map((key) => invalidate(key)));
    };

    const checkName = (name: string): string => {
      const trimmed = name.trim();
      if (!trimmed) throw new Error("name required");
      if (collection.isReserved(trimmed)) throw new Error(collection.reservedError(trimmed));
      return trimmed;
    };

    return {
      async create(name: string) {
        const trimmed = checkName(name);
        const created = await collection.routes.create({ name: trimmed });
        await changed();
        return created.name;
      },
      async rename(from: string, to: string) {
        const trimmed = checkName(to);
        const id = await idOf(from);
        const updated = await collection.routes.update(id, { name: trimmed });
        await changed();
        return updated.name;
      },
      async remove(name: string) {
        const id = await idOf(name);
        await collection.routes.remove(id);
        await changed();
      },
      async setMembers(name: string, patch: MembersPatch) {
        const id = await idOf(name);
        const result = await collection.routes.updateMembers(id, {
          add: patch.add ?? [],
          remove: patch.remove ?? [],
        });
        await changed();
        return result;
      },
      invalidate: () => invalidate(collection.key),
    };
  }, [collection, cached, fetchFresh, invalidate]);
}
```

- [ ] **Step 4: Point the two configs at the new routes**

In `web/src/lib/contactGroups.ts`, replace the import block and the `contactGroups` definition:

```ts
import { createNameCollection, useNameCollectionActions } from "./nameCollection";
import {
  createContactGroup,
  deleteContactGroup,
  listContactGroups,
  updateContactGroup,
  updateContactGroupMembers,
} from "./vaultApi";
```

```ts
export const contactGroups = createNameCollection({
  routes: {
    list: listContactGroups,
    create: createContactGroup,
    update: updateContactGroup,
    remove: deleteContactGroup,
    updateMembers: updateContactGroupMembers,
  },
  cacheKey: "contact-groups",
  // Contact rows and the contact drawer show group names as chips.
  invalidates: [["contacts"], ["contact-detail"]],
  label: "group",
  queryToken: "group",
  reservedNames: RESERVED_GROUP_NAMES,
  reservedError: reservedGroupError,
});
```

In `web/src/lib/messageTags.ts`, the same with:

```ts
import {
  createMessageTag,
  deleteMessageTag,
  listMessageTags,
  updateMessageTag,
  updateMessageTagMembers,
} from "./vaultApi";
```

```ts
export const messageTags = createNameCollection({
  routes: {
    list: listMessageTags,
    create: createMessageTag,
    update: updateMessageTag,
    remove: deleteMessageTag,
    updateMembers: updateMessageTagMembers,
  },
  cacheKey: "message-tags",
  // Conversation rows and the Trash count show tag names as chips.
  invalidates: [["conversations"], ["trash-count"]],
  label: "tag",
  queryToken: "tag",
  reservedNames: RESERVED_TAG_NAMES,
  reservedError: reservedTagError,
});
```

- [ ] **Step 5: Switch the two list screens to `setMembers`**

`web/src/screens/ContactList.tsx`:
- line 348: `await groupActions.setMembership(ids, name, enable);` → `await groupActions.setMembers(name, enable ? { add: ids } : { remove: ids });`
- line 360 (the `useCallback` deps): `groupActions.setMembership` → `groupActions.setMembers`
- line 386: `[...names].map((name) => groupActions.setMembership(ids, name, false)),` → `[...names].map((name) => groupActions.setMembers(name, { remove: ids })),`
- line 400 (deps): `groupActions.setMembership` → `groupActions.setMembers`

`web/src/screens/ConversationList.tsx`:
- line 128: `await tagActions.setMembership(ids, name, enable);` → `await tagActions.setMembers(name, enable ? { add: ids } : { remove: ids });`
- line 143 (deps): `tagActions.setMembership` → `tagActions.setMembers`

Line numbers are from before this task; find the lines by the text.

- [ ] **Step 6: Type-check, run the web tests, and verify they pass**

Run: `cd web && npx tsc --noEmit -p . && npm run lint && npm test`
Expected: type-check clean; Biome clean; all Vitest suites pass, including the new `nameCollection.test.tsx`. If `LeftPanel.test.tsx`, `AddressBookSection.test.tsx`, or `MessageTagsNav.test.tsx` fail, they mock `useContactGroups`, `useContactGroupActions`, or `useAuth` by name and should still pass; read the failure before changing anything.

- [ ] **Step 7: Check it in the browser**

Start the vault and the web app:

```bash
./scripts/run-vault-dev.sh --reset-demo   # terminal 1
cd web && npm run dev                     # terminal 2
```

Sign in as `demo` with an empty password at `http://127.0.0.1:5173`. With the Playwright MCP or by hand:
1. In the sidebar, create a Contact Group `Family`. It appears in the sidebar and the URL becomes `/group/Family`.
2. Open Contacts, check two contacts, open the groups menu, tick `Family`. Both rows show the chip.
3. In the sidebar, rename `Family` to `Fam`. Within a second the two contact rows read `Fam` without a reload. This is the invalidation Task 7 adds; before it, they read `Family` until a remount.
4. In the groups menu, type a new name `Close` and press Enter. The group is created and the checked contacts are in it in one gesture. This is the create-then-add path through `idOf`'s vault-once branch.
5. Delete `Fam` from the sidebar. The chips go and the URL falls back.
6. Repeat 1, 3, and 5 for a Message Tag on the conversation list.

- [ ] **Step 8: Commit**

```bash
cd web && npx biome format --write src && cd ..
git add web/src/lib/nameCollection.ts web/src/lib/nameCollection.test.tsx web/src/lib/contactGroups.ts web/src/lib/messageTags.ts web/src/screens/ContactList.tsx web/src/screens/ConversationList.tsx
git commit -m "feat(web): resolve group and tag names to ids inside the feature module

Screens keep passing names. nameCollection.ts looks the id up in the
cached list, asks the vault once when the name is not there yet, and
refuses with no request when the vault does not know it. Every write
now invalidates the contact and conversation lists that show the name
as a chip, so a rename shows on every row without a reload."
```

---

### Task 8: Whole-repo check and the review's follow-ups

**Files:**
- Modify: `docs/superpowers/specs/2026-09-02-contact-groups-and-message-tags-by-id-design.md` only if something in the implementation had to differ from it (record the difference under "What else changes").

- [ ] **Step 1: Run the full gate**

Run: `./scripts/check-pr.sh`
Expected: rustfmt, workspace build and test, `src-tauri` build, Biome `ci`, Vitest, and the OpenAPI type drift check all pass.

- [ ] **Step 2: Confirm the old routes are gone everywhere**

Run: `grep -rn "contacts/groups\|conversations/tags\|contact-groups/members\|message-tags/members\|setContactGroupMembership\|setMessageTagMembership\|renameContactGroup\|renameMessageTag\|MembershipChangedResponse" crates web/src docs/src --include=*.rs --include=*.ts --include=*.tsx --include=*.md --include=*.json`
Expected: no output.

- [ ] **Step 3: Open the pull request**

```bash
git push -u origin named-sets-by-id
gh pr create --title "feat: address Contact Groups and Message Tags by id" --body "$(cat <<'EOF'
Contact Groups and Message Tags are addressed by their integer id on the HTTP interface, membership lives under the collection, and one server module implements both route groups. The web app keeps its name-based interface and resolves ids inside the feature module, which now also invalidates the contact and conversation lists after every write.

Spec: docs/superpowers/specs/2026-09-02-contact-groups-and-message-tags-by-id-design.md
Decision: docs/adr/0003-resources-are-addressed-by-id-on-the-http-interface.md

Routes, per collection:
- GET/POST /v1/contact-groups
- PATCH/DELETE /v1/contact-groups/{id}
- GET/PATCH /v1/contact-groups/{id}/members

Removed: POST /v1/contacts/groups, POST /v1/conversations/tags, GET /v1/contact-groups/members, GET /v1/message-tags/members.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Then `gh pr checks --watch` until green. Do not merge.
