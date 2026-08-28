# Authorization Model Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Message Vault's non-existent authorization model with enforced per-account permissions, an administrator role, and a user-management API, while deleting Hanko and the guest-account system.

**Architecture:** One permission set (`Permissions { import, export, delete }`) is stored identically on `accounts` and `account_api_tokens`. `resolve_auth` loads the account's permissions, intersects them with the presented credential's, and hands every guard a single resolved value. An administrator flag on the account gates a new `/v1/admin/*` route group that manages accounts and their storage but never returns message content.

**Tech Stack:** Rust (axum, sqlx `Any`, utoipa), SQLite + Postgres dual schema, React 19 + TypeScript (Vite, Vitest, Testing Library, Biome).

**Spec:** `docs/superpowers/specs/2026-08-28-authorization-model-design.md`

## Global Constraints

- **Branch from current `main`** (at or after `c42e3c6c`, the PR #219 merge). This plan's web tasks edit files that merge changed.
- **Dual-engine schema.** Every change to `schema/sql/accounts.sql` has a twin in `schema/sql/pg_accounts.sql`. Both change in the same commit.
- **Column comments are mandatory.** Every column needs a `--` comment on the line directly above it, or `scripts/check-sql-column-comments.mjs` fails.
- **`SCHEMA_VERSION` becomes `3`** in `crates/vault/server/src/db/schema.rs`. It is bumped exactly once, in Task 3. Do not bump it again.
- **Hand-numbered placeholders.** sqlx `Any` does no rewriting: write `$1, $2, …` even for SQLite.
- **Breaking changes are accepted.** Databases at any other schema version are rebuilt empty and re-imported. Do not write migrations or preserve columns for compatibility.
- **Multitenancy is inviolable.** No query added by this plan may read message data across an `account_id` boundary.
- **No admin endpoint returns message content.** Counts, sizes, usernames, and flags only.
- **`web-next/` is legacy.** Do not touch it. `scripts/sync-vault-schema.mjs` will regenerate its schema copy; that is the only change it should receive.
- **Verify with:** `cargo test -p message-vault-server`, `cargo build --workspace`, and in `web/`: `npm test`, `npx tsc --noEmit`, `npx biome ci .`.

---

### Task 1: Remove Hanko from the server and the web client

Hanko is a second sign-in mechanism selected by `VAULT_AUTH`. Removing it collapses `AuthMode` to a single value, which makes the enum, the environment variable, and the conditional route registration all dead. The `hanko_user_id` **column stays for now** — Task 3 owns every schema change — but nothing writes it after this task.

PR #219 already removed the `<hanko-auth>` render branch from `LoginScreen.tsx`, leaving only an explanatory comment. Do not expect to find rendering code there.

**Files:**
- Modify: `crates/vault/server/src/auth.rs` — delete the Hanko handler and its helpers
- Modify: `crates/vault/server/src/config.rs` — delete `AuthMode`
- Modify: `crates/vault/server/src/server.rs` — delete `limited_auth_router`'s mode parameter
- Modify: `crates/vault/server/src/openapi.rs` — delete `SpecAuth` and the Hanko route
- Modify: `crates/vault/server/src/db/account_profile.rs` — drop the `hanko_user_id` parameter and `lookup_account_by_hanko`
- Modify: `web/package.json` — remove `@teamhanko/hanko-elements`
- Modify: `web/src/lib/authGuards.ts` — remove `AuthMode` and `isAuthMode`
- Modify: `web/src/lib/authGuards.test.ts` — remove their cases
- Modify: `web/src/screens/LoginScreen.tsx` — remove the leftover comment and `AuthModeResponse` usage
- Modify: `.env.example` — remove the Hanko block

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `account_profile::insert_account(conn, id, username, password_hash, preferred_name, read_only) -> Result<()>` — six parameters, the `hanko_user_id: Option<&str>` argument removed. Every later task and every existing test calls this new signature.

- [ ] **Step 1: Write the failing test**

In `crates/vault/server/src/auth.rs`, inside `mod tests`, replace nothing yet — add this test proving the six-argument signature exists:

```rust
    #[tokio::test]
    async fn insert_account_takes_no_hanko_id() {
        let (_dir, mut conn) = test_conn().await;
        account_profile::insert_account(&mut conn, TEST_ACCOUNT, "alice", None, None, false)
            .await
            .unwrap();
        assert_eq!(
            account_profile::username_for_account(&mut conn, TEST_ACCOUNT)
                .await
                .unwrap()
                .as_deref(),
            Some("alice")
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p message-vault-server insert_account_takes_no_hanko_id`
Expected: FAIL to compile — "this function takes 7 arguments but 6 arguments were supplied".

- [ ] **Step 3: Change the `insert_account` signature**

In `crates/vault/server/src/db/account_profile.rs`, replace the function at line 329:

```rust
/// Insert a new account row. All fields except id and username are optional.
pub async fn insert_account(
    conn: &mut AnyConnection,
    id: &str,
    username: &str,
    password_hash: Option<&str>,
    preferred_name: Option<&str>,
    read_only: bool,
) -> Result<()> {
    schema::ensure_accounts_schema(conn).await?;
    sqlx::query(
        "INSERT INTO accounts (id, username, read_only, password_hash, preferred_name) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(username)
    .bind(read_only as i32)
    .bind(password_hash)
    .bind(preferred_name)
    .execute(&mut *conn)
    .await
    .with_context(|| format!("insert account {username}"))?;
    Ok(())
}
```

Delete `lookup_account_by_hanko` (line 300) entirely.

- [ ] **Step 4: Update every caller**

Run `cargo build -p message-vault-server 2>&1 | grep "arguments"` to list them. Each call site drops its `None, // hanko_user_id` argument. Known sites: `auth.rs:561` (register), `auth.rs:762`, `auth.rs:1210`, `auth.rs:1221`, `auth.rs:1464`, `auth.rs:1494`, `auth.rs:1590`.

- [ ] **Step 5: Delete the Hanko handler and helpers**

In `crates/vault/server/src/auth.rs`, delete:
- `HankoSessionRequest`
- `hanko_session_handler`
- the JWKS fetch and its cache
- `username_from_hanko_email_or_id`
- `unique_hanko_username`
- `MAX_HANKO_JWT_BYTES`
- the `check_auth_rate_limit("hanko:session")` call site
- every `mod tests` case naming Hanko

Update the module doc comment at line 1 to read:

```rust
//! Authentication handlers: register, login, session check, and logout.
```

- [ ] **Step 6: Delete `AuthMode`**

In `crates/vault/server/src/config.rs`, delete the `AuthMode` enum, its `impl`, and `from_env`. In `crates/vault/server/src/auth.rs`, delete `AuthModeResponse` and `auth_mode_handler`'s `mode` field — but **keep the handler and the route**; Task 7 retires the endpoint after moving the web probe off it. The handler becomes:

```rust
/// Sign-in mode for clients. Retained only as the sign-in card's reachability
/// probe until it moves to GET /health; see Task 7.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct AuthModeResponse {
    /// Always "local". The vault has one sign-in mechanism.
    pub mode: String,
}

#[utoipa::path(
    get,
    path = "/v1/auth/mode",
    tag = "Auth",
    responses((status = 200, description = "Sign-in mode", body = AuthModeResponse))
)]
pub(crate) async fn auth_mode_handler() -> Json<AuthModeResponse> {
    Json(AuthModeResponse {
        mode: "local".into(),
    })
}
```

- [ ] **Step 7: Simplify the routers**

In `crates/vault/server/src/openapi.rs`, delete `SpecAuth` and replace `auth_public_openapi`:

```rust
/// Unauthenticated auth JSON (register and login).
pub fn auth_public_openapi() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(crate::auth::register_handler))
        .routes(routes!(crate::auth::login_handler))
}
```

Remove `use crate::config::AuthMode;`. In the `#[openapi(info(description = ...))]` attribute, replace the description with:

```
"HTTP API for a local Message Vault. Bearer session tokens come from login. API tokens come from Settings → Account."
```

In `crates/vault/server/src/server.rs`, replace `limited_auth_router`:

```rust
fn limited_auth_router() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    let (router, spec) = crate::openapi::auth_public_openapi().split_for_parts();
    (
        // Auth JSON is tiny; keep a tight limit so Argon2 abuse cannot ship 512 MiB bodies.
        router.layer(RequestBodyLimitLayer::new(32 * 1024)),
        spec,
    )
}
```

In `http_app`, delete `let mode = AuthMode::from_env();` and call `limited_auth_router()`. Delete the `auth_route_status` tests that assert on `AuthMode::Hanko`.

- [ ] **Step 8: Remove Hanko from the web client**

`web/src/lib/authGuards.ts` — delete the `AuthMode` type and `isAuthMode`. Keep `isTryDemoEnabled` for now; Task 2 removes it.

`web/src/lib/authGuards.test.ts` — delete the `isAuthMode` cases.

`web/src/screens/LoginScreen.tsx` — inside `connect()`, replace the two-line Hanko comment with nothing. The call itself stays until Task 7.

`web/package.json` — remove the `"@teamhanko/hanko-elements": "^3"` dependency line, then run `npm install` in `web/` to update the lockfile.

`.env.example` — delete the six Hanko lines (the `hanko` mode note, `HANKO_API_URL`, and `NEXT_PUBLIC_HANKO_API_URL`), and remove `hanko` from the `VAULT_AUTH` description. Since `VAULT_AUTH` no longer does anything, delete that variable too.

- [ ] **Step 9: Verify nothing Hanko-shaped remains outside `web-next/`**

Run: `grep -ril hanko crates schema web/src web/package.json .env.example`
Expected: no output.

- [ ] **Step 10: Run the suites**

Run: `cargo test -p message-vault-server`
Expected: PASS.
Run: `cd web && npm test && npx tsc --noEmit && npx biome ci .`
Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "refactor: remove Hanko sign-in"
```

---

### Task 2: Remove try-demo and the guest pool

`POST /v1/auth/try-demo` is unreachable from the product — `web/src/lib/tryDemo.ts` is its only mention in `web/` and nothing imports that file. Behind it sit 2,490 lines of pool and clone machinery. The `guest_status` column stays until Task 3.

The demo *account* is unaffected: `reset-demo` seeding is a separate path and `./scripts/run-vault-dev.sh --reset-demo` must still work when this task is done.

**Files:**
- Delete: `crates/vault/server/src/guest_pool.rs`
- Delete: `crates/vault/server/src/guest_clone.rs`
- Delete: `web/src/lib/tryDemo.ts`
- Modify: `crates/vault/server/src/auth.rs` — delete `try_demo_handler` and the guest branch in logout
- Modify: `crates/vault/server/src/server.rs` — delete `reject_if_guest`, `reject_if_guest_account`, `GuestPoolState` from `AppState`
- Modify: `crates/vault/server/src/lib.rs` — drop both `pub mod` lines
- Modify: `crates/vault/server/src/openapi.rs` — drop the try-demo route
- Modify: `crates/vault/server/src/assets.rs`, `api_tokens_api.rs`, `import/mod.rs` — drop the guest guard calls
- Modify: `crates/vault/server/src/db/account_profile.rs` — delete `guest_status`, `is_guest_account`, `insert_guest_account`, `set_guest_status`
- Modify: `crates/vault/server/src/profile.rs` — drop `is_guest` from the profile response
- Modify: `web/src/lib/authGuards.ts` — delete `isTryDemoEnabled`

**Interfaces:**
- Consumes: `insert_account`'s six-parameter signature from Task 1.
- Produces: `AppState` without a `guest_pool` field; `AccountProfileResponse` without `is_guest`.

- [ ] **Step 1: Write the failing test**

In `crates/vault/server/src/server.rs`, inside `mod tests`:

```rust
    #[tokio::test]
    async fn try_demo_route_is_gone() {
        let state = test_state().await;
        let response = get_path(state, "/v1/auth/try-demo").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
```

If `get_path` only issues GETs and the route was a POST, a 404 is still the correct assertion for a removed route: axum returns 404 for an unknown path regardless of method, and 405 only for a known path with the wrong method. That distinction is exactly what this test checks.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p message-vault-server try_demo_route_is_gone`
Expected: FAIL — asserts 405 (route exists, wrong method), not 404.

- [ ] **Step 3: Delete the modules**

```bash
git rm crates/vault/server/src/guest_pool.rs crates/vault/server/src/guest_clone.rs web/src/lib/tryDemo.ts
```

In `crates/vault/server/src/lib.rs`, remove the `pub mod guest_pool;` and `pub mod guest_clone;` lines. The file has a deliberate two-block structure — `pub mod` declarations first, then `use` re-exports. Remove from the first block only.

- [ ] **Step 4: Remove the guest guards**

In `crates/vault/server/src/server.rs`, delete `reject_if_guest` and `reject_if_guest_account` entirely, and remove the `guest_pool` field from `AppState` plus its construction sites.

Delete the call sites. They are:
- `assets.rs:864`, `assets.rs:988`, `assets.rs:1054`, `assets.rs:1103`
- `api_tokens_api.rs:188`, `api_tokens_api.rs:284`
- `import/mod.rs:787`

and their `use` imports at `assets.rs:26`, `api_tokens_api.rs:11`, `import/mod.rs:48`.

Each call is a standalone line of the form `reject_if_guest_account(&state.db, &auth.account_id).await?;` — delete the line. The capability guard that already sits beside it (`require_import_access`, `require_full_access`) remains and is now the only check.

- [ ] **Step 5: Remove the handler and the account helpers**

In `crates/vault/server/src/auth.rs`, delete `try_demo_handler`, its request/response types, and the `is_guest_account` branch in `logout_on_conn` (around line 1016) along with any test that exercised guest logout.

In `crates/vault/server/src/db/account_profile.rs`, delete `guest_status`, `is_guest_account`, `insert_guest_account`, and `set_guest_status`.

In `crates/vault/server/src/openapi.rs`, remove `.routes(routes!(crate::auth::try_demo_handler))`.

- [ ] **Step 6: Drop `is_guest` from the profile response**

In `crates/vault/server/src/profile.rs`, remove the `is_guest` field from `AccountProfileResponse` and the line computing it in `load_response`.

In `web/src/lib/authGuards.ts`, delete `isTryDemoEnabled` and its doc comment; delete its cases from `authGuards.test.ts`.

- [ ] **Step 7: Run the test to verify it passes**

Run: `cargo test -p message-vault-server try_demo_route_is_gone`
Expected: PASS.

- [ ] **Step 8: Verify the demo path still works**

Run: `./scripts/run-vault-dev.sh --reset-demo`
Expected: seeds without error and serves on `http://127.0.0.1:8080`. Sign in as `demo` with an empty password. Stop the server afterwards.

- [ ] **Step 9: Run the suites**

Run: `cargo test -p message-vault-server && cargo build --workspace`
Expected: PASS.
Run: `cd web && npm test && npx tsc --noEmit && npx biome ci .`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "refactor: remove try-demo and the guest account pool"
```

---

### Task 3: The permission model and the schema that stores it

This is the one schema task. It drops three dead columns from `accounts`, adds five live ones, replaces `account_api_tokens.scopes` with three columns matching the account's, and bumps `SCHEMA_VERSION` once. Behavior does not change yet — `resolve_auth` starts resolving permissions and the guards start reading them, but every existing account and token gets the permissions it effectively had before.

**Files:**
- Create: `crates/vault/server/src/db/permissions.rs`
- Modify: `schema/sql/accounts.sql`, `schema/sql/pg_accounts.sql`
- Modify: `crates/vault/server/src/db/schema.rs` — `SCHEMA_VERSION` to 3
- Modify: `crates/vault/server/src/db/mod.rs` — declare the new module
- Modify: `crates/vault/server/src/db/api_tokens.rs` — replace `ApiTokenScopes`
- Modify: `crates/vault/server/src/db/account_profile.rs` — load account auth, drop `account_is_read_only`
- Modify: `crates/vault/server/src/server.rs` — `AuthCapability`, `resolve_auth`, the guards
- Modify: `crates/vault/server/src/api_tokens_api.rs` — request and response shape
- Modify: `crates/vault/server/src/profile.rs` — drop `read_only`, add the flags
- Modify: `crates/vault/server/src/reset_demo.rs` — the demo account's flags
- Test: inline `mod tests` in `permissions.rs`, `server.rs`, `api_tokens.rs`

**Interfaces:**
- Consumes: the six-parameter `insert_account` from Task 1.
- Produces:
  - `db::permissions::Permissions { import: bool, export: bool, delete: bool }` with `all()`, `none()`, `intersect(self, Permissions) -> Permissions`
  - `db::account_profile::AccountAuth { is_admin: bool, disabled: bool, permissions: Permissions }`
  - `db::account_profile::load_account_auth(conn, account_id) -> Result<Option<AccountAuth>>`
  - `server::AuthIdentity::permissions(&self) -> Permissions`
  - `server::AuthIdentity::is_admin(&self) -> bool`
  - `server::AuthIdentity::is_session(&self) -> bool`
  - `server::resolve_auth_on_conn(conn: &mut AnyConnection, token: &str) -> Result<AuthIdentity, ApiError>` — the whole resolution, minus acquiring a connection. `resolve_auth` keeps its `(headers, state)` signature and delegates to it. Tasks 5 and 6 test through this function, so it is a required deliverable of this task, not an optional refactor.
  - `insert_account(conn, id, username, password_hash, preferred_name) -> Result<()>` — the `read_only` parameter is gone

- [ ] **Step 1: Write the failing test**

Create `crates/vault/server/src/db/permissions.rs` containing only its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersect_keeps_only_what_both_allow() {
        let account = Permissions {
            import: true,
            export: true,
            delete: false,
        };
        let token = Permissions {
            import: true,
            export: false,
            delete: true,
        };
        let effective = account.intersect(token);
        assert!(effective.import);
        assert!(!effective.export, "token withheld export");
        assert!(!effective.delete, "account withheld delete");
    }

    #[test]
    fn none_grants_nothing_and_all_grants_everything() {
        let none = Permissions::none();
        assert!(!none.import && !none.export && !none.delete);
        let all = Permissions::all();
        assert!(all.import && all.export && all.delete);
    }
}
```

Add `pub mod permissions;` to `crates/vault/server/src/db/mod.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p message-vault-server permissions::`
Expected: FAIL to compile — "cannot find type `Permissions`".

- [ ] **Step 3: Write the type**

At the top of `crates/vault/server/src/db/permissions.rs`, above the test module:

```rust
//! What a credential may do. One set, stored identically on `accounts` and on
//! `account_api_tokens`, so an account's grant and a token's grant intersect
//! field by field rather than through a translation.

/// Operations a credential is allowed to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    /// May call the import endpoints.
    pub import: bool,
    /// May call the export endpoints.
    pub export: bool,
    /// May destroy message data: trash, purge, delete-messages, attachments.
    pub delete: bool,
}

impl Permissions {
    /// Everything allowed. The default for a newly registered account.
    pub const fn all() -> Self {
        Self {
            import: true,
            export: true,
            delete: true,
        }
    }

    /// Nothing allowed.
    pub const fn none() -> Self {
        Self {
            import: false,
            export: false,
            delete: false,
        }
    }

    /// What both sides allow. A token can narrow its owner's grant, never widen it.
    pub const fn intersect(self, other: Self) -> Self {
        Self {
            import: self.import && other.import,
            export: self.export && other.export,
            delete: self.delete && other.delete,
        }
    }

    /// Read from three integer columns as stored by both engines.
    pub fn from_ints(import: i64, export: i64, delete: i64) -> Self {
        Self {
            import: import != 0,
            export: export != 0,
            delete: delete != 0,
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p message-vault-server permissions::`
Expected: PASS, two tests.

- [ ] **Step 5: Change both schema twins**

In `schema/sql/accounts.sql`, replace the `accounts` table with:

```sql
-- Vault login account (web UI + API owner).
CREATE TABLE IF NOT EXISTS accounts (
    -- Stable account id (opaque string primary key).
    id TEXT PRIMARY KEY,
    -- Login user id; unique case-insensitively.
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    -- Password verifier hash; NULL when password auth is unused.
    password_hash TEXT,
    -- Display name for “you” in the UI.
    preferred_name TEXT,
    -- 1 = may manage users through /v1/admin/*; 0 = ordinary account.
    is_admin INTEGER NOT NULL DEFAULT 0,
    -- 1 = may not sign in and existing sessions are refused; 0 = active.
    disabled INTEGER NOT NULL DEFAULT 0,
    -- 1 = may call the import endpoints.
    can_import INTEGER NOT NULL DEFAULT 1,
    -- 1 = may call the export endpoints.
    can_export INTEGER NOT NULL DEFAULT 1,
    -- 1 = may destroy message data (trash, purge, delete-messages, attachments).
    can_delete INTEGER NOT NULL DEFAULT 1
);
```

Delete `CREATE UNIQUE INDEX ... ix_accounts_hanko_user_id` and its three lines.

In the same file, replace the `scopes` column of `account_api_tokens` with three columns:

```sql
    -- 1 = this token may call the import endpoints.
    can_import INTEGER NOT NULL DEFAULT 1,
    -- 1 = this token may call the export endpoints.
    can_export INTEGER NOT NULL DEFAULT 1,
    -- 1 = this token may destroy message data. Off unless asked for.
    can_delete INTEGER NOT NULL DEFAULT 0,
```

and update the table's header comment to `-- Named CLI API tokens (many per account). Prefix: mv-api-`.

Make the identical changes in `schema/sql/pg_accounts.sql`, keeping its existing differences: `username TEXT NOT NULL` without `COLLATE NOCASE` (uniqueness stays on `ix_accounts_username_ci`), and the same `-- ` comment on every column.

- [ ] **Step 6: Bump the schema version**

In `crates/vault/server/src/db/schema.rs`:

```rust
pub const SCHEMA_VERSION: i64 = 3;
```

- [ ] **Step 7: Run the column-comment and schema-sync scripts**

Run: `node scripts/check-sql-column-comments.mjs`
Expected: PASS.
Run: `node scripts/sync-vault-schema.mjs`
Expected: regenerates the `web-next/` copy and `tests/fixtures/schema/`.

- [ ] **Step 8: Replace `ApiTokenScopes`**

In `crates/vault/server/src/db/api_tokens.rs`, delete the `ApiTokenScopes` enum and its `impl` entirely. Replace every use of it with `Permissions`. The row read at line 183 becomes:

```rust
    let row: Option<(String, i64, i64, i64, Option<String>, i64)> = sqlx::query_as(
        "SELECT account_id, can_import, can_export, can_delete, expires_at, disabled
         FROM account_api_tokens WHERE token_hash = $1",
    )
    .bind(&hash)
    .fetch_optional(&mut *conn)
    .await?;
```

and builds `Permissions::from_ints(can_import, can_export, can_delete)`. `create_api_token` takes `permissions: Permissions` in place of `scopes: ApiTokenScopes` and binds the three integers. Update `ApiTokenRow` to carry `permissions: Permissions` instead of `scopes`.

Update the module doc at line 1 to `//! Named CLI API tokens (`mv-api-…`); many per account, with per-token permissions.`

- [ ] **Step 9: Load account auth**

In `crates/vault/server/src/db/account_profile.rs`, delete `account_is_read_only`, drop the `read_only` parameter from `insert_account` (the INSERT drops the column too), and add:

```rust
/// An account's administrative flag, disabled flag, and permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountAuth {
    /// May manage users.
    pub is_admin: bool,
    /// May not sign in; existing sessions are refused.
    pub disabled: bool,
    /// What this account may do.
    pub permissions: crate::db::permissions::Permissions,
}

/// Load one account's authorization row. `None` when the account is gone.
pub async fn load_account_auth(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Option<AccountAuth>> {
    schema::ensure_accounts_schema(conn).await?;
    let row: Option<(i64, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT is_admin, disabled, can_import, can_export, can_delete
         FROM accounts WHERE id = $1",
    )
    .bind(account_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(is_admin, disabled, import, export, delete)| AccountAuth {
        is_admin: is_admin != 0,
        disabled: disabled != 0,
        permissions: crate::db::permissions::Permissions::from_ints(import, export, delete),
    }))
}
```

- [ ] **Step 10: Rewrite `AuthCapability` and `resolve_auth`**

In `crates/vault/server/src/server.rs`:

```rust
/// What a Bearer credential is allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthCapability {
    /// Signed-in session. Carries the account's own permissions.
    Session {
        /// The account may manage users.
        is_admin: bool,
        /// What the account may do.
        permissions: Permissions,
    },
    /// Named API token. Already intersected with its owner's permissions.
    ApiToken(Permissions),
}

impl AuthIdentity {
    /// What this credential may do, account and token already intersected.
    pub fn permissions(&self) -> Permissions {
        match self.capability {
            AuthCapability::Session { permissions, .. } => permissions,
            AuthCapability::ApiToken(permissions) => permissions,
        }
    }

    /// True only for a signed-in administrator, never for an API token.
    pub fn is_admin(&self) -> bool {
        matches!(self.capability, AuthCapability::Session { is_admin: true, .. })
    }

    /// True when the credential is a signed-in session rather than a token.
    pub fn is_session(&self) -> bool {
        matches!(self.capability, AuthCapability::Session { .. })
    }
}
```

In `resolve_auth`, after finding the account for a session token, load its auth row and build `Session`. For an API token, load the owner's auth row and intersect:

```rust
    let resolved = if let Some(account_id) =
        session_tokens::lookup_account_for_token(&mut conn, &token)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        let auth = account_profile::load_account_auth(&mut conn, &account_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::Unauthorized("account no longer exists".into()))?;
        Some(AuthIdentity {
            account_id,
            capability: AuthCapability::Session {
                is_admin: auth.is_admin,
                permissions: auth.permissions,
            },
        })
    } else if let Some(tok) = api_tokens::lookup_account_for_api_token(&mut conn, &token)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        let auth = account_profile::load_account_auth(&mut conn, &tok.account_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::Unauthorized("account no longer exists".into()))?;
        Some(AuthIdentity {
            account_id: tok.account_id,
            capability: AuthCapability::ApiToken(auth.permissions.intersect(tok.permissions)),
        })
    } else {
        None
    };
```

- [ ] **Step 11: Rewrite the guards**

Replace the three scope guards in `crates/vault/server/src/server.rs` and add the delete guard:

```rust
/// Allow a credential that may import.
///
/// # Errors
///
/// Returns forbidden when import is not permitted.
pub fn require_import_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    if auth.permissions().import {
        return Ok(());
    }
    Err(ApiError::Forbidden("import is not permitted".into()))
}

/// Allow a credential that may export.
///
/// # Errors
///
/// Returns forbidden when export is not permitted.
pub fn require_export_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    if auth.permissions().export {
        return Ok(());
    }
    Err(ApiError::Forbidden("export is not permitted".into()))
}

/// Allow a credential that may import or export, for asset probes.
///
/// # Errors
///
/// Returns forbidden when neither is permitted.
pub fn require_import_or_export_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    let p = auth.permissions();
    if p.import || p.export {
        return Ok(());
    }
    Err(ApiError::Forbidden("this credential cannot access assets".into()))
}

/// Allow a credential that may destroy message data.
///
/// # Errors
///
/// Returns forbidden when deletion is not permitted.
pub fn require_delete_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    if auth.permissions().delete {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "deleting messages is not permitted for this account".into(),
    ))
}
```

`require_full_access` keeps its current body but matches the new variant:

```rust
pub fn require_full_access(auth: &AuthIdentity) -> Result<(), ApiError> {
    if auth.is_session() {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "this endpoint requires a signed-in session; use an API token only for import/export".into(),
    ))
}
```

- [ ] **Step 12: Update the API token endpoints and the profile response**

In `crates/vault/server/src/api_tokens_api.rs`, replace `scopes: String` in `CreateApiTokenRequest` with three optional booleans, defaulting delete to off:

```rust
/// Body for creating a token: label, permissions, optional expiry.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateApiTokenRequest {
    /// User-chosen label shown in Settings.
    pub label: String,
    /// May call the import endpoints. Default true.
    #[serde(default = "default_true")]
    pub can_import: bool,
    /// May call the export endpoints. Default true.
    #[serde(default = "default_true")]
    pub can_export: bool,
    /// May destroy message data. Default false — asked for, never inherited.
    #[serde(default)]
    pub can_delete: bool,
    /// Days until expiry. Omit for the default (365 days). Pass `0` for no expiry.
    #[serde(default)]
    pub expires_in_days: Option<u64>,
}

const fn default_true() -> bool {
    true
}
```

Delete `default_scopes`. `CreateApiTokenResponse` and the list response replace `scopes: String` with the same three booleans.

In `crates/vault/server/src/profile.rs`, remove `read_only` from `AccountProfileResponse` and add the flags the UI needs:

```rust
    /// May manage users.
    pub is_admin: bool,
    /// May call the import endpoints.
    pub can_import: bool,
    /// May call the export endpoints.
    pub can_export: bool,
    /// May destroy message data.
    pub can_delete: bool,
```

populated in `load_response` from `account_profile::load_account_auth`.

- [ ] **Step 13: Give the demo account its flags**

In `crates/vault/server/src/reset_demo.rs`, `seed_demo_account_on_conn` currently binds `seed.account.read_only`. Replace the INSERT with:

```rust
    sqlx::query(
        r#"
        INSERT INTO accounts (
            id, username, password_hash, preferred_name, can_import, can_export, can_delete
        )
        VALUES ($1, $2, NULL, $3, 0, 0, 0)
        ON CONFLICT(id) DO UPDATE SET
            username = excluded.username,
            preferred_name = excluded.preferred_name,
            can_import = excluded.can_import,
            can_export = excluded.can_export,
            can_delete = excluded.can_delete
        "#,
    )
    .bind(account_id)
    .bind(&seed.account.username)
    .bind(&seed.owner.display_name)
```

Delete the `read_only` field from the `DemoAccount` struct and the `read_only = true` line from the seed TOML fixture in the same file.

- [ ] **Step 14: Fix every test insert**

Roughly a dozen test modules insert accounts with a `read_only` column. Run `grep -rn "INSERT INTO accounts" crates/vault/server/src` and rewrite each to the new column list. Known files: `thread_tags_api.rs`, `process_assets.rs`, `export_api.rs`, `conversations_api.rs`, `contact_groups_api.rs`, `contacts_api.rs`, `dedupe.rs`, `named_membership.rs`, `reset_demo.rs`.

- [ ] **Step 15: Add the intersection test**

In `crates/vault/server/src/server.rs`, inside `mod tests`:

```rust
    #[tokio::test]
    async fn api_token_cannot_exceed_its_owner() {
        let (_dir, mut conn) = test_conn().await;
        account_profile::insert_account(&mut conn, TEST_ACCOUNT, "alice", None, None)
            .await
            .unwrap();
        sqlx::query("UPDATE accounts SET can_import = 0 WHERE id = $1")
            .bind(TEST_ACCOUNT)
            .execute(&mut conn)
            .await
            .unwrap();
        let created = api_tokens::create_api_token(
            &mut conn,
            TEST_ACCOUNT,
            "tool",
            Permissions::all(),
            None,
        )
        .await
        .unwrap();

        let identity = resolve_auth_on_conn(&mut conn, &created.5).await.unwrap();

        assert!(
            !identity.permissions().import,
            "the account lost import, so its token must not have it"
        );
        assert!(identity.permissions().export);
    }
```

Extract `resolve_auth_on_conn` as part of this step — Tasks 5 and 6 test through it. `resolve_auth` keeps its public signature and becomes a thin wrapper:

```rust
pub async fn resolve_auth(headers: &HeaderMap, state: &AppState) -> Result<AuthIdentity, ApiError> {
    let token = bearer_token(headers)?;
    let mut conn = state
        .db
        .acquire()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    resolve_auth_on_conn(&mut conn, &token).await
}

/// Resolve a Bearer credential on an existing connection.
///
/// # Errors
///
/// Unauthorized when the token matches nothing; forbidden when the account is
/// disabled.
pub async fn resolve_auth_on_conn(
    conn: &mut AnyConnection,
    token: &str,
) -> Result<AuthIdentity, ApiError> {
    schema::ensure_accounts_schema(conn)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    // ... the session / API-token branches written in Step 10 move here ...
}
```

The comment marks where the Step 10 body goes; write that body here rather than leaving the comment in the code.

- [ ] **Step 16: Run the suites**

Run: `cargo test -p message-vault-server && cargo build --workspace`
Expected: PASS.
Run: `docker compose -f docker-compose.pg.yml up -d && MV_TEST_POSTGRES_URL=postgres://vault:vault@127.0.0.1:5432/vault cargo test -p message-vault-server`
Expected: PASS, including the Postgres engine tests that were skipped before.
Run: `cd web && npx tsc --noEmit`
Expected: FAIL where the web reads `read_only` or `scopes` — Tasks 8 and 9 fix those. Record the failures; do not fix them here.

- [ ] **Step 17: Commit**

```bash
git add -A
git commit -m "feat: per-account permissions shared with API tokens"
```

---

### Task 4: The first administrator and the last-admin guard

The first account created through `POST /v1/auth/register` that is not the demo account becomes an administrator. The reverse rule protects it: the only remaining administrator cannot be demoted, disabled, or deleted, because the first-administrator rule will not fire again on a vault that has accounts.

**Files:**
- Modify: `crates/vault/server/src/auth.rs` — `register_handler`
- Modify: `crates/vault/server/src/db/account_profile.rs` — the two count helpers
- Test: inline `mod tests` in `auth.rs`

**Interfaces:**
- Consumes: `insert_account` (five parameters), `AccountAuth`, `Permissions` from Task 3.
- Produces:
  - `account_profile::vault_has_no_real_accounts(conn) -> Result<bool>`
  - `account_profile::is_last_admin(conn, account_id) -> Result<bool>`
  - `account_profile::set_admin(conn, account_id, is_admin) -> Result<()>`

- [ ] **Step 1: Write the failing test**

In `crates/vault/server/src/auth.rs`, inside `mod tests`:

```rust
    #[tokio::test]
    async fn first_real_account_becomes_admin_and_second_does_not() {
        let (_dir, mut conn) = test_conn().await;

        // The demo account exists first and must not count.
        account_profile::insert_account(
            &mut conn,
            account_profile::DEMO_ACCOUNT_ID,
            "demo",
            None,
            None,
        )
        .await
        .unwrap();
        assert!(
            account_profile::vault_has_no_real_accounts(&mut conn)
                .await
                .unwrap(),
            "the demo account must not occupy first place"
        );

        account_profile::insert_account(&mut conn, "acct-1", "alice", None, None)
            .await
            .unwrap();
        assert!(
            !account_profile::vault_has_no_real_accounts(&mut conn)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn last_admin_is_protected() {
        let (_dir, mut conn) = test_conn().await;
        account_profile::insert_account(&mut conn, "acct-1", "alice", None, None)
            .await
            .unwrap();
        account_profile::set_admin(&mut conn, "acct-1", true)
            .await
            .unwrap();
        account_profile::insert_account(&mut conn, "acct-2", "bob", None, None)
            .await
            .unwrap();

        assert!(account_profile::is_last_admin(&mut conn, "acct-1").await.unwrap());
        assert!(!account_profile::is_last_admin(&mut conn, "acct-2").await.unwrap());

        account_profile::set_admin(&mut conn, "acct-2", true)
            .await
            .unwrap();
        assert!(
            !account_profile::is_last_admin(&mut conn, "acct-1").await.unwrap(),
            "with two admins neither is the last"
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p message-vault-server first_real_account_becomes_admin last_admin_is_protected`
Expected: FAIL to compile — the three functions do not exist.

- [ ] **Step 3: Write the helpers**

In `crates/vault/server/src/db/account_profile.rs`:

```rust
/// True when the vault holds no account a person registered — the demo account
/// does not count, so a `--reset-demo` vault still grants admin to its first
/// real user.
pub async fn vault_has_no_real_accounts(conn: &mut AnyConnection) -> Result<bool> {
    schema::ensure_accounts_schema(conn).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE id != $1")
        .bind(DEMO_ACCOUNT_ID)
        .fetch_one(&mut *conn)
        .await?;
    Ok(count == 0)
}

/// True when `account_id` is an administrator and no other account is.
pub async fn is_last_admin(conn: &mut AnyConnection, account_id: &str) -> Result<bool> {
    schema::ensure_accounts_schema(conn).await?;
    let others: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE is_admin = 1 AND id != $1")
            .bind(account_id)
            .fetch_one(&mut *conn)
            .await?;
    if others > 0 {
        return Ok(false);
    }
    let self_admin: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE is_admin = 1 AND id = $1")
            .bind(account_id)
            .fetch_one(&mut *conn)
            .await?;
    Ok(self_admin > 0)
}

/// Grant or revoke the administrative flag.
pub async fn set_admin(conn: &mut AnyConnection, account_id: &str, is_admin: bool) -> Result<()> {
    schema::ensure_accounts_schema(conn).await?;
    sqlx::query("UPDATE accounts SET is_admin = $1 WHERE id = $2")
        .bind(is_admin as i32)
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p message-vault-server first_real_account_becomes_admin last_admin_is_protected`
Expected: PASS.

- [ ] **Step 5: Wire the rule into register**

In `crates/vault/server/src/auth.rs`, in `register_handler`, between the username-taken check and `insert_account`:

```rust
    let first_account = account_profile::vault_has_no_real_accounts(&mut tx)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    account_profile::insert_account(
        &mut tx,
        &account_id,
        &username,
        password_hash.as_deref(),
        preferred_name.as_deref(),
    )
    .await
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    if first_account {
        account_profile::set_admin(&mut tx, &account_id, true)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
```

The count and the insert share the register transaction because that is where the decision belongs.

- [ ] **Step 6: Add the end-to-end register test**

```rust
    #[tokio::test]
    async fn register_grants_admin_to_the_first_user_only() {
        let state = test_state().await;

        let first = register_via_api(&state, "alice", "hunter2hunter2").await;
        let second = register_via_api(&state, "bob", "hunter2hunter2").await;

        let mut conn = state.db.acquire().await.unwrap();
        assert!(
            account_profile::load_account_auth(&mut conn, &first.account_id)
                .await
                .unwrap()
                .unwrap()
                .is_admin
        );
        assert!(
            !account_profile::load_account_auth(&mut conn, &second.account_id)
                .await
                .unwrap()
                .unwrap()
                .is_admin
        );
    }
```

`register_via_api` is a test helper posting to `/v1/auth/register` and parsing `AuthTokenResponse`. If `auth.rs`'s test module has no such helper, write one beside the test.

- [ ] **Step 7: Run the suite**

Run: `cargo test -p message-vault-server`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: first registered account becomes the administrator"
```

---

### Task 5: Disabled accounts are refused on every request

Session tokens live 30 days (`SESSION_TTL_SECS`), so checking `disabled` only at login would leave a disabled user working for a month. The check goes in `resolve_auth`, which already loads the account row.

**Files:**
- Modify: `crates/vault/server/src/server.rs` — `resolve_auth`
- Modify: `crates/vault/server/src/auth.rs` — `login_handler`
- Test: inline `mod tests` in `server.rs`

**Interfaces:**
- Consumes: `AccountAuth` and `load_account_auth` from Task 3.
- Produces: no new public functions.

- [ ] **Step 1: Write the failing test**

In `crates/vault/server/src/server.rs`, inside `mod tests`:

```rust
    #[tokio::test]
    async fn disabling_an_account_kills_its_live_session() {
        let (_dir, mut conn) = test_conn().await;
        account_profile::insert_account(&mut conn, TEST_ACCOUNT, "alice", None, None)
            .await
            .unwrap();
        let token = session_tokens::insert_account_session_token(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();

        // The token works while the account is active.
        resolve_auth_on_conn(&mut conn, &token).await.unwrap();

        sqlx::query("UPDATE accounts SET disabled = 1 WHERE id = $1")
            .bind(TEST_ACCOUNT)
            .execute(&mut conn)
            .await
            .unwrap();

        let err = resolve_auth_on_conn(&mut conn, &token).await.unwrap_err();
        assert!(
            matches!(err, ApiError::Forbidden(_)),
            "a disabled account's existing token must stop working, got {err:?}"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p message-vault-server disabling_an_account_kills_its_live_session`
Expected: FAIL — the second `resolve_auth_on_conn` succeeds.

- [ ] **Step 3: Add the check**

In `resolve_auth_on_conn`, immediately after loading `AccountAuth` for either credential kind:

```rust
    if auth.disabled {
        return Err(ApiError::Forbidden("this account is disabled".into()));
    }
```

Apply it on both branches — a disabled account's API tokens must stop working too.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p message-vault-server disabling_an_account_kills_its_live_session`
Expected: PASS.

- [ ] **Step 5: Refuse the login itself**

In `crates/vault/server/src/auth.rs`, in `login_handler`, after the password check and before issuing a token:

```rust
    let auth = account_profile::load_account_auth(&mut conn, &account_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("invalid username or password".into()))?;
    if auth.disabled {
        return Err(ApiError::Forbidden("this account is disabled".into()));
    }
```

- [ ] **Step 6: Add the login test**

```rust
    #[tokio::test]
    async fn disabled_account_cannot_sign_in() {
        let state = test_state().await;
        let created = register_via_api(&state, "alice", "hunter2hunter2").await;

        let mut conn = state.db.acquire().await.unwrap();
        sqlx::query("UPDATE accounts SET disabled = 1 WHERE id = $1")
            .bind(&created.account_id)
            .execute(&mut conn)
            .await
            .unwrap();

        let status = login_status(&state, "alice", "hunter2hunter2").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
```

`login_status` posts to `/v1/auth/login` and returns the status; write it beside the test if the module lacks one.

- [ ] **Step 7: Run the suite**

Run: `cargo test -p message-vault-server`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: disabled accounts are refused on every request"
```

---

### Task 6: Deletion respects the delete permission

`delete-messages` currently calls `require_full_access`, which rejects API tokens outright. It moves to `require_delete_access`. Trash and purge paths gain the same guard. Account deletion, password change, token management, and the admin routes stay on `require_full_access`.

**Note on call sites:** trash mutations do not live in a dedicated route group. Trashed rows are written from handlers under the `/v1/export/*` namespace — `export_api.rs:1797` inserts into `trashed_conversations`. Enumerate the call sites by searching, not by assuming a router exists.

**Files:**
- Modify: `crates/vault/server/src/profile.rs` — `delete_messages_handler`
- Modify: `crates/vault/server/src/export_api.rs` — the trash and purge handlers
- Test: inline `mod tests` in `profile.rs`

**Interfaces:**
- Consumes: `require_delete_access` from Task 3.
- Produces: no new public functions.

- [ ] **Step 1: Find the call sites**

Run: `grep -rn "trashed_conversations\|trashed_handles\|trashed_contacts" crates/vault/server/src --include='*.rs' | grep -v "^.*tests\?::"`

Record which handler owns each write. Conversations and their messages are gated; **contacts and handles are not** — identity management stays open to every account.

- [ ] **Step 2: Write the failing test**

In `crates/vault/server/src/profile.rs`, inside `mod tests`:

```rust
    #[tokio::test]
    async fn delete_messages_needs_the_delete_permission() {
        let state = test_state().await;
        let created = register_via_api(&state, "alice", "hunter2hunter2").await;

        let mut conn = state.db.acquire().await.unwrap();
        sqlx::query("UPDATE accounts SET can_delete = 0 WHERE id = $1")
            .bind(&created.account_id)
            .execute(&mut conn)
            .await
            .unwrap();

        let status = post_status(
            &state,
            "/v1/account/delete-messages",
            &created.token,
            serde_json::json!({ "confirm": true }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p message-vault-server delete_messages_needs_the_delete_permission`
Expected: FAIL — returns 200, because only `require_full_access` is checked and a session always passes it.

- [ ] **Step 4: Swap the guard**

In `crates/vault/server/src/profile.rs`, in `delete_messages_handler`, replace:

```rust
    require_full_access(&auth)?;
```

with:

```rust
    require_delete_access(&auth)?;
```

and update the import. Apply `require_delete_access` to each conversation trash and purge handler found in Step 1.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p message-vault-server delete_messages_needs_the_delete_permission`
Expected: PASS.

- [ ] **Step 6: Prove the token path works and the session-only paths do not**

```rust
    #[tokio::test]
    async fn a_token_with_delete_may_delete_but_may_not_close_the_account() {
        let state = test_state().await;
        let created = register_via_api(&state, "alice", "hunter2hunter2").await;
        let mut conn = state.db.acquire().await.unwrap();
        let token = api_tokens::create_api_token(
            &mut conn,
            &created.account_id,
            "tool",
            Permissions::all(),
            None,
        )
        .await
        .unwrap()
        .5;

        let deleted = post_status(
            &state,
            "/v1/account/delete-messages",
            &token,
            serde_json::json!({ "confirm": true }),
        )
        .await;
        assert_eq!(deleted, StatusCode::OK);

        let closed = post_status(
            &state,
            "/v1/auth/delete-account",
            &token,
            serde_json::json!({ "confirm": true }),
        )
        .await;
        assert_eq!(
            closed,
            StatusCode::FORBIDDEN,
            "closing the account stays session-only"
        );
    }
```

- [ ] **Step 7: Run the suite**

Run: `cargo test -p message-vault-server`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: gate message deletion on the delete permission"
```

---

### Task 7: The admin API

A new route group. Existing routes are untouched and stay scoped to the caller's own account. Every response carries counts and metadata; none carries message content.

**Files:**
- Create: `crates/vault/server/src/admin_api.rs`
- Modify: `crates/vault/server/src/lib.rs` — declare the module
- Modify: `crates/vault/server/src/openapi.rs` — register the routes and add an "Admin" tag
- Modify: `crates/vault/server/src/server.rs` — add `require_admin`
- Test: inline `mod tests` in `admin_api.rs`

**Interfaces:**
- Consumes: `AuthIdentity::is_admin`, `require_full_access`, `load_account_auth`, `is_last_admin`, `set_admin`, `Permissions`, `delete_all_messages_for_account`, `remove_account_asset_trees`.
- Produces: `server::require_admin(auth: &AuthIdentity) -> Result<(), ApiError>`.

- [ ] **Step 1: Write the failing test**

Create `crates/vault/server/src/admin_api.rs` with only its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn non_admins_are_refused() {
        let state = test_state().await;
        let _admin = register_via_api(&state, "alice", "hunter2hunter2").await;
        let ordinary = register_via_api(&state, "bob", "hunter2hunter2").await;

        let status = get_status(&state, "/v1/admin/users", &ordinary.token).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn the_admin_sees_every_account_but_no_messages() {
        let state = test_state().await;
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
        let _other = register_via_api(&state, "bob", "hunter2hunter2").await;

        let body: ListUsersResponse =
            get_json(&state, "/v1/admin/users", &admin.token).await;

        assert_eq!(body.items.len(), 2);
        let bob = body.items.iter().find(|u| u.username == "bob").unwrap();
        assert_eq!(bob.message_count, 0);
        assert!(!bob.is_admin);
        assert!(!bob.disabled);
    }

    #[tokio::test]
    async fn the_last_admin_cannot_be_demoted_disabled_or_deleted() {
        let state = test_state().await;
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
        let path = format!("/v1/admin/users/{}", admin.account_id);

        let demoted = patch_status(
            &state,
            &path,
            &admin.token,
            serde_json::json!({ "is_admin": false }),
        )
        .await;
        assert_eq!(demoted, StatusCode::BAD_REQUEST);

        let disabled = patch_status(
            &state,
            &path,
            &admin.token,
            serde_json::json!({ "disabled": true }),
        )
        .await;
        assert_eq!(disabled, StatusCode::BAD_REQUEST);

        let deleted = delete_status(&state, &path, &admin.token).await;
        assert_eq!(deleted, StatusCode::BAD_REQUEST);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p message-vault-server admin_api::`
Expected: FAIL to compile — the module has no handlers or types.

- [ ] **Step 3: Add the admin guard**

In `crates/vault/server/src/server.rs`:

```rust
/// Reject anything that is not a signed-in administrator.
///
/// # Errors
///
/// Returns forbidden for ordinary sessions and for every API token.
pub fn require_admin(auth: &AuthIdentity) -> Result<(), ApiError> {
    if auth.is_admin() {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "this endpoint requires an administrator session".into(),
    ))
}
```

`is_admin()` is false for `ApiToken` by construction, so tokens are refused without a separate check.

- [ ] **Step 4: Write the list endpoint**

At the top of `crates/vault/server/src/admin_api.rs`:

```rust
//! Administrator user management. Every route requires an administrator
//! session. Responses carry account metadata, counts, and storage sizes —
//! never message content.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::db::account_profile;
use crate::db::permissions::Permissions;
use crate::server::{require_admin, resolve_auth, ApiError, AppState};

/// One account as an administrator sees it.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AdminUser {
    /// Account id.
    pub account_id: String,
    /// Login username.
    pub username: String,
    /// May manage users.
    pub is_admin: bool,
    /// May not sign in.
    pub disabled: bool,
    /// May call the import endpoints.
    pub can_import: bool,
    /// May call the export endpoints.
    pub can_export: bool,
    /// May destroy message data.
    pub can_delete: bool,
    /// Messages this account owns.
    pub message_count: i64,
    /// Attachment bytes this account owns.
    pub storage_bytes: i64,
}

/// Every account in the vault.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListUsersResponse {
    /// One row per account.
    pub items: Vec<AdminUser>,
}

/// List every account with its flags, message count, and storage use.
#[utoipa::path(
    get,
    path = "/v1/admin/users",
    tag = "Admin",
    security(("bearer" = [])),
    responses(
        (status = 200, body = ListUsersResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub async fn list_users_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ListUsersResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_admin(&auth)?;

    let mut conn = state.db.acquire().await?;
    let rows: Vec<(String, String, i64, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT id, username, is_admin, disabled, can_import, can_export, can_delete
         FROM accounts ORDER BY username",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut items = Vec::with_capacity(rows.len());
    for (id, username, is_admin, disabled, import, export, delete) in rows {
        let message_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE account_id = $1",
        )
        .bind(&id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
        let storage_bytes: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(a.byte_size), 0)
             FROM attachments a
             JOIN messages m ON m.id = a.message_id
             WHERE m.account_id = $1",
        )
        .bind(&id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
        items.push(AdminUser {
            account_id: id,
            username,
            is_admin: is_admin != 0,
            disabled: disabled != 0,
            can_import: import != 0,
            can_export: export != 0,
            can_delete: delete != 0,
            message_count,
            storage_bytes,
        });
    }
    Ok(Json(ListUsersResponse { items }))
}
```

Confirm the attachments size column name before running — `grep -n "byte_size\|size_bytes" schema/sql/messages.sql` — and use whichever the schema defines.

- [ ] **Step 5: Write create, patch, and the two deletes**

```rust
/// Body for creating an account as an administrator.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateUserRequest {
    /// Login username.
    pub username: String,
    /// Initial password. Must satisfy the vault's password policy.
    pub password: String,
    /// Grant the administrative flag. Default false.
    #[serde(default)]
    pub is_admin: bool,
}

/// Body for changing an account's flags. Omitted fields are left alone.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PatchUserRequest {
    /// Grant or revoke administration.
    pub is_admin: Option<bool>,
    /// Disable or re-enable sign-in.
    pub disabled: Option<bool>,
    /// Allow or forbid import.
    pub can_import: Option<bool>,
    /// Allow or forbid export.
    pub can_export: Option<bool>,
    /// Allow or forbid deleting message data.
    pub can_delete: Option<bool>,
}
```

`patch_user_handler` refuses with `ApiError::BadRequest` when the request would demote or disable the last administrator:

```rust
    if (req.is_admin == Some(false) || req.disabled == Some(true))
        && account_profile::is_last_admin(&mut conn, &target)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        return Err(ApiError::BadRequest(
            "this is the only administrator; promote another account first".into(),
        ));
    }
```

`delete_user_handler` makes the same check, then reuses the existing deletion path:

```rust
    account_profile::delete_account(&mut conn, &target)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let account_root = state.cfg.paths.data_dir.join(&target);
    if account_root.exists() {
        tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&account_root))
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
```

`delete_user_messages_handler` reuses the two functions the self-service endpoint already calls:

```rust
    let stats = account_profile::delete_all_messages_for_account(&mut conn, &target)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    crate::profile::remove_account_asset_trees(
        &state.cfg.paths.data_dir,
        &target,
        &state.cfg.paths.assets_dir,
        &state.cfg.paths.assets_converted_dir,
    )
    .map_err(|e| ApiError::Internal(e.to_string()))?;
```

`remove_account_asset_trees` is currently private to `profile.rs`; make it `pub(crate)`.

`create_user_handler` mirrors `register_handler`'s validation but never grants the first-account administrator flag — an administrator already exists, or this endpoint could not have been called:

```rust
pub async fn create_user_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<AdminUser>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_admin(&auth)?;

    let username = crate::auth::normalize_username(&req.username);
    if !crate::auth::is_valid_username(&username) {
        return Err(ApiError::BadRequest(
            "username must be 1–128 chars (alphanumeric, _, -, .)".into(),
        ));
    }
    crate::auth::validate_password_policy(&req.password)?;
    let password_hash = crate::auth::hash_password(&req.password)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut conn = state.db.acquire().await?;
    if account_profile::lookup_account_ref(&mut conn, &username)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
        .is_some()
    {
        return Err(ApiError::BadRequest(format!(
            "username already taken: {username}"
        )));
    }

    let account_id = uuid::Uuid::new_v4().to_string();
    account_profile::insert_account(
        &mut conn,
        &account_id,
        &username,
        Some(&password_hash),
        None,
    )
    .await
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    if req.is_admin {
        account_profile::set_admin(&mut conn, &account_id, true)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    Ok(Json(AdminUser {
        account_id,
        username,
        is_admin: req.is_admin,
        disabled: false,
        can_import: true,
        can_export: true,
        can_delete: true,
        message_count: 0,
        storage_bytes: 0,
    }))
}
```

`normalize_username`, `is_valid_username`, `validate_password_policy`, and `hash_password` are private to `auth.rs` today; make each `pub(crate)`.

`set_user_password_handler` takes `{ "password": "…" }`, applies the same policy, and writes through the existing helper:

```rust
/// Body for an administrator setting someone's password.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetPasswordRequest {
    /// The new password. Must satisfy the vault's password policy.
    pub password: String,
}

pub async fn set_user_password_handler(
    State(state): State<AppState>,
    Path(target): Path<String>,
    headers: HeaderMap,
    Json(req): Json<SetPasswordRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_admin(&auth)?;
    crate::auth::validate_password_policy(&req.password)?;
    let hash = crate::auth::hash_password(&req.password)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut conn = state.db.acquire().await?;
    account_profile::update_password_hash(&mut conn, &target, &hash)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
```

Setting a password does not invalidate that account's existing session. If you want it to, that is a separate decision and not in this spec.

- [ ] **Step 6: Register the routes**

In `crates/vault/server/src/lib.rs`, add `pub mod admin_api;` to the `pub mod` block.

In `crates/vault/server/src/openapi.rs`, add to the `tags(...)` list:

```rust
        (name = "Admin", description = "User management for administrators"),
```

and to `api_openapi()`:

```rust
        .routes(routes!(crate::admin_api::list_users_handler))
        .routes(routes!(crate::admin_api::create_user_handler))
        .routes(routes!(crate::admin_api::patch_user_handler))
        .routes(routes!(crate::admin_api::set_user_password_handler))
        .routes(routes!(crate::admin_api::delete_user_messages_handler))
        .routes(routes!(crate::admin_api::delete_user_handler))
```

Add the six paths to the `openapi.rs` route-presence test list.

- [ ] **Step 7: Add the multitenancy assertion**

```rust
    #[tokio::test]
    async fn deleting_one_users_messages_leaves_the_others_alone() {
        let state = test_state().await;
        let admin = register_via_api(&state, "alice", "hunter2hunter2").await;
        let victim = register_via_api(&state, "bob", "hunter2hunter2").await;
        seed_one_message(&state, &victim.account_id).await;
        seed_one_message(&state, &admin.account_id).await;

        let status = delete_status(
            &state,
            &format!("/v1/admin/users/{}/messages", victim.account_id),
            &admin.token,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let body: ListUsersResponse = get_json(&state, "/v1/admin/users", &admin.token).await;
        let bob = body.items.iter().find(|u| u.username == "bob").unwrap();
        let alice = body.items.iter().find(|u| u.username == "alice").unwrap();
        assert_eq!(bob.message_count, 0);
        assert_eq!(alice.message_count, 1, "the other tenant is untouched");
    }
```

`seed_one_message` inserts a conversation and a message for the given account; write it beside the test.

- [ ] **Step 8: Run the tests and the suite**

Run: `cargo test -p message-vault-server admin_api::`
Expected: PASS.
Run: `cargo test -p message-vault-server && cargo build --workspace`
Expected: PASS.

- [ ] **Step 9: Regenerate the OpenAPI document**

Run: `cargo run -p message-vault-server -- openapi > docs/src/assets/openapi.json`

If that subcommand does not exist, find the generator: `grep -rn "openapi.json" crates/vault/server/src scripts`.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "feat: /v1/admin/users for managing accounts"
```

---

### Task 8: Move the sign-in probe to /health and retire the mode endpoint

PR #219 made `GET /v1/auth/mode` the sign-in card's reachability probe. It discards the body: a successful answer means connected, a failure or an eight-second timeout means disconnected. With one sign-in mechanism the endpoint reports nothing, so the probe moves to `GET /health`, which already exists at `server.rs:535`.

**Read `connect()` in `LoginScreen.tsx` before starting.** This task edits code that shipped recently.

**Files:**
- Modify: `web/src/screens/LoginScreen.tsx`
- Modify: `web/src/screens/LoginScreen.test.tsx`
- Modify: `crates/vault/server/src/auth.rs` — delete `auth_mode_handler` and `AuthModeResponse`
- Modify: `crates/vault/server/src/openapi.rs` — drop the route
- Modify: `web/src/lib/api.ts` — drop the `AuthModeResponse` type if it lives there

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: no new exports. `LoginScreen`'s `authMode` state is deleted.

- [ ] **Step 1: Write the failing test**

In `web/src/screens/LoginScreen.test.tsx`, add to the existing suite:

```tsx
  it("probes /health rather than the auth mode endpoint", async () => {
    render(<LoginScreen />, { wrapper: Wrapper });

    await waitFor(() => {
      expect(screen.getByText(/connected/i)).toBeInTheDocument();
    });

    const calls = fetchMock.mock.calls.map(([url]) => String(url));
    expect(calls.some((url) => url.endsWith("/health"))).toBe(true);
    expect(calls.some((url) => url.endsWith("/v1/auth/mode"))).toBe(false);
  });
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web && npx vitest run src/screens/LoginScreen.test.tsx -t "probes /health"`
Expected: FAIL — `/v1/auth/mode` is still called.

- [ ] **Step 3: Swap the probe**

In `web/src/screens/LoginScreen.tsx`, inside `connect()`, replace the mode request:

```tsx
        await apiClient.get<unknown>("/health", {
          signal: probeTimeoutSignal(),
        });
        setAddress(trimmed);
        setDraft(trimmed);
        setAuthServer(trimmed);
        setState("connected");
```

Delete the `authMode` state declaration at line 41 and every read of it, along with the `AuthModeResponse` import. `probeTimeoutSignal()` and the eight-second timeout stay exactly as they are.

- [ ] **Step 4: Update the test mocks**

Throughout `LoginScreen.test.tsx`, the helper answers both `/v1/auth/mode` and `/health`. Delete the `/v1/auth/mode` branches; the `/health` branches already exist and become the ones that matter.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd web && npx vitest run src/screens/LoginScreen.test.tsx`
Expected: PASS, all cases.

- [ ] **Step 6: Delete the endpoint**

In `crates/vault/server/src/auth.rs`, delete `auth_mode_handler` and `AuthModeResponse`. In `openapi.rs`, remove `.routes(routes!(crate::auth::auth_mode_handler))` and drop `/v1/auth/mode` from the route-presence test list. Remove any `AuthModeResponse` type from `web/src/lib/api.ts`.

- [ ] **Step 7: Run both suites**

Run: `cargo test -p message-vault-server`
Expected: PASS.
Run: `cd web && npm test && npx tsc --noEmit && npx biome ci .`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: probe /health for reachability and drop /v1/auth/mode"
```

---

### Task 9: Per-token permission checkboxes

`useApiTokens.ts` posts `scopes: "both"` as a literal — there is no scope selector anywhere in Settings, so every token created from the UI holds every permission that exists. That is tolerable while permissions mean import and export. Once delete exists it is not: every token would silently gain the right to destroy message data.

**Files:**
- Modify: `web/src/screens/settings/useApiTokens.ts`
- Modify: `web/src/screens/settings/ApiTokenForms.tsx`
- Modify: `web/src/screens/settings/apiTokensUtils.ts`
- Modify: `web/src/screens/settings/ApiTokensTable.tsx`
- Test: `web/src/screens/settings/apiTokensUtils.test.ts` (create)

**Interfaces:**
- Consumes: the `can_import` / `can_export` / `can_delete` request and response fields from Task 3.
- Produces: `permissionsLabel(item: ApiTokenItem): string`; `ApiTokenItem` carrying the three booleans instead of `scopes`.

- [ ] **Step 1: Write the failing test**

Create `web/src/screens/settings/apiTokensUtils.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { permissionsLabel } from "./apiTokensUtils";

describe("permissionsLabel", () => {
  it("lists what the token may do", () => {
    expect(
      permissionsLabel({ can_import: true, can_export: true, can_delete: false }),
    ).toBe("Import / Export");
    expect(
      permissionsLabel({ can_import: true, can_export: true, can_delete: true }),
    ).toBe("Import / Export / Delete");
    expect(
      permissionsLabel({ can_import: false, can_export: true, can_delete: false }),
    ).toBe("Export");
  });

  it("says so when a token may do nothing", () => {
    expect(
      permissionsLabel({ can_import: false, can_export: false, can_delete: false }),
    ).toBe("None");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web && npx vitest run src/screens/settings/apiTokensUtils.test.ts`
Expected: FAIL — `permissionsLabel` is not exported.

- [ ] **Step 3: Replace `scopesLabel`**

In `web/src/screens/settings/apiTokensUtils.ts`, delete `scopesLabel` and add:

```ts
/** What an API token is allowed to do, as a readable list. */
export function permissionsLabel(token: {
  can_import: boolean;
  can_export: boolean;
  can_delete: boolean;
}): string {
  const parts: string[] = [];
  if (token.can_import) parts.push("Import");
  if (token.can_export) parts.push("Export");
  if (token.can_delete) parts.push("Delete");
  return parts.length > 0 ? parts.join(" / ") : "None";
}
```

Update `ApiTokenItem`, replacing `scopes: string` with:

```ts
  can_import: boolean;
  can_export: boolean;
  can_delete: boolean;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web && npx vitest run src/screens/settings/apiTokensUtils.test.ts`
Expected: PASS.

- [ ] **Step 5: Add the selector to the create form**

In `web/src/screens/settings/useApiTokens.ts`, add state and send it:

```ts
  const [canImport, setCanImport] = useState(true);
  const [canExport, setCanExport] = useState(true);
  const [canDelete, setCanDelete] = useState(false);
```

and in `create()`:

```ts
      const res = await apiClient.post<{
        id: string;
        label: string;
        can_import: boolean;
        can_export: boolean;
        can_delete: boolean;
        created_at: string;
        token: string;
      }>("/v1/account/api-tokens", {
        label: trimmed,
        can_import: canImport,
        can_export: canExport,
        can_delete: canDelete,
      });
```

Reset the three to `true, true, false` in `cancelCompose` and after a successful create. Return them and their setters from the hook.

In `ApiTokenForms.tsx`, render three checkboxes inside `ApiTokenCreateForm` labeled **Import**, **Export**, and **Delete messages and attachments**. A checkbox whose permission the signed-in account does not hold is disabled, with the line "Your account cannot do this." beneath it — read the account's flags from the profile response added in Task 3. Follow the existing checkbox component the settings panels already use rather than a bare `<input type="checkbox">`.

- [ ] **Step 6: Show the permissions in the table**

In `ApiTokensTable.tsx`, replace `scopesLabel(item.scopes)` with `permissionsLabel(item)` and update the prop name from `scopesLabel` to `permissionsLabel` at both the definition and the call site.

- [ ] **Step 7: Run the suites**

Run: `cd web && npm test && npx tsc --noEmit && npx biome ci .`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(web): choose what an API token may do"
```

---

### Task 10: The Users panel

Settings is a tabbed screen driven by `SETTINGS_TABS`. A **Users** tab joins it, rendered only when the signed-in account is an administrator.

**Files:**
- Create: `web/src/screens/settings/AdminUsersPanel.tsx`
- Create: `web/src/screens/settings/useAdminUsers.ts`
- Create: `web/src/screens/settings/AdminUsersPanel.test.tsx`
- Modify: `web/src/screens/SettingsScreen.tsx`

**Interfaces:**
- Consumes: `/v1/admin/users` from Task 7; `is_admin` on the profile response from Task 3; the existing `useResource` and `useAsyncAction` hooks.
- Produces: `AdminUsersPanel` (default export), `useAdminUsers()`.

- [ ] **Step 1: Write the failing test**

Create `web/src/screens/settings/AdminUsersPanel.test.tsx`:

```tsx
import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AdminUsersPanel } from "./AdminUsersPanel";

describe("AdminUsersPanel", () => {
  it("lists each account with its counts and flags", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          items: [
            {
              account_id: "a1",
              username: "alice",
              is_admin: true,
              disabled: false,
              can_import: true,
              can_export: true,
              can_delete: true,
              message_count: 1200,
              storage_bytes: 4096,
            },
            {
              account_id: "a2",
              username: "bob",
              is_admin: false,
              disabled: true,
              can_import: false,
              can_export: true,
              can_delete: false,
              message_count: 0,
              storage_bytes: 0,
            },
          ],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );

    render(<AdminUsersPanel />);

    await waitFor(() => {
      expect(screen.getByText("alice")).toBeInTheDocument();
    });
    expect(screen.getByText("bob")).toBeInTheDocument();
    expect(screen.getByText(/1,200/)).toBeInTheDocument();
    expect(screen.getByText(/disabled/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web && npx vitest run src/screens/settings/AdminUsersPanel.test.tsx`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write the hook**

Create `web/src/screens/settings/useAdminUsers.ts`:

```ts
import { useCallback } from "react";
import { apiClient } from "../../lib/api";
import { useAsyncAction } from "../../lib/useAsyncAction";
import { useResource } from "../../lib/useResource";

export type AdminUser = {
  account_id: string;
  username: string;
  is_admin: boolean;
  disabled: boolean;
  can_import: boolean;
  can_export: boolean;
  can_delete: boolean;
  message_count: number;
  storage_bytes: number;
};

const fetchUsers = (signal: AbortSignal) =>
  apiClient
    .get<{ items: AdminUser[] }>("/v1/admin/users", { signal })
    .then((res) => res.items ?? []);

/** The administrator's view of every account, plus the actions on one. */
export function useAdminUsers() {
  const { data, loading, error: loadError, reload } = useResource("admin/users", fetchUsers);
  const { busy, error: actionError, run, clearError } = useAsyncAction();

  const patch = useCallback(
    (id: string, changes: Partial<Pick<AdminUser, "is_admin" | "disabled" | "can_import" | "can_export" | "can_delete">>) =>
      run(async () => {
        await apiClient.patch(`/v1/admin/users/${encodeURIComponent(id)}`, changes);
        reload();
      }),
    [run, reload],
  );

  const deleteMessages = useCallback(
    (id: string) =>
      run(async () => {
        await apiClient.delete(`/v1/admin/users/${encodeURIComponent(id)}/messages`);
        reload();
      }),
    [run, reload],
  );

  const deleteUser = useCallback(
    (id: string) =>
      run(async () => {
        await apiClient.delete(`/v1/admin/users/${encodeURIComponent(id)}`);
        reload();
      }),
    [run, reload],
  );

  return {
    users: data ?? [],
    loading,
    loadError,
    busy,
    actionError,
    clearError,
    patch,
    deleteMessages,
    deleteUser,
  };
}
```

- [ ] **Step 4: Write the panel**

Create `web/src/screens/settings/AdminUsersPanel.tsx`:

```tsx
import { useState } from "react";
import { formatBytes } from "../../lib/formatBytes";
import { tdClass, tdMuted, thClass } from "./apiTokensUtils";
import { type AdminUser, useAdminUsers } from "./useAdminUsers";

/** Every account in the vault, with the actions an administrator has on one. */
export function AdminUsersPanel() {
  const { users, loading, loadError, busy, actionError, patch, deleteMessages, deleteUser } =
    useAdminUsers();
  const [confirming, setConfirming] = useState<{ user: AdminUser; kind: "messages" | "account" } | null>(
    null,
  );

  if (loading) return <p className="text-[0.875rem] text-muted">Loading accounts…</p>;
  if (loadError) return <p className="text-[0.875rem] text-danger">{loadError}</p>;

  return (
    <section>
      <h3 className="m-0 text-text">Users</h3>
      <p className="mt-[0.35rem] text-[0.875rem] text-muted">
        Everyone with an account on this vault. You can change what they may do, disable them, or
        delete their messages. You cannot read them.
      </p>

      {actionError ? (
        <p className="mt-3 text-[0.875rem] text-danger" role="alert">
          {actionError}
        </p>
      ) : null}

      <table className="mt-4 w-full border-collapse">
        <thead>
          <tr>
            <th className={thClass}>User</th>
            <th className={thClass}>Status</th>
            <th className={thClass}>Messages</th>
            <th className={thClass}>Storage</th>
            <th className={thClass}>Import</th>
            <th className={thClass}>Export</th>
            <th className={thClass}>Delete</th>
            <th className={thClass}>Actions</th>
          </tr>
        </thead>
        <tbody>
          {users.map((user) => (
            <tr key={user.account_id}>
              <td className={tdClass}>
                {user.username}
                {user.is_admin ? <span className="ml-2 text-muted">(admin)</span> : null}
              </td>
              <td className={tdMuted}>{user.disabled ? "Disabled" : "Active"}</td>
              <td className={tdMuted}>{user.message_count.toLocaleString()}</td>
              <td className={tdMuted}>{formatBytes(user.storage_bytes)}</td>
              <td className={tdClass}>
                <input
                  type="checkbox"
                  aria-label={`Allow importing messages for ${user.username}`}
                  checked={user.can_import}
                  disabled={busy}
                  onChange={(e) => patch(user.account_id, { can_import: e.target.checked })}
                />
              </td>
              <td className={tdClass}>
                <input
                  type="checkbox"
                  aria-label={`Allow exporting messages for ${user.username}`}
                  checked={user.can_export}
                  disabled={busy}
                  onChange={(e) => patch(user.account_id, { can_export: e.target.checked })}
                />
              </td>
              <td className={tdClass}>
                <input
                  type="checkbox"
                  aria-label={`Allow deleting messages and attachments for ${user.username}`}
                  checked={user.can_delete}
                  disabled={busy}
                  onChange={(e) => patch(user.account_id, { can_delete: e.target.checked })}
                />
              </td>
              <td className={tdClass}>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => patch(user.account_id, { disabled: !user.disabled })}
                >
                  {user.disabled ? "Enable" : "Disable"}
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => setConfirming({ user, kind: "messages" })}
                >
                  Delete messages
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => setConfirming({ user, kind: "account" })}
                >
                  Delete account
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      {confirming ? (
        <ConfirmDialog
          user={confirming.user}
          kind={confirming.kind}
          busy={busy}
          onCancel={() => setConfirming(null)}
          onConfirm={async () => {
            const { user, kind } = confirming;
            await (kind === "messages" ? deleteMessages(user.account_id) : deleteUser(user.account_id));
            setConfirming(null);
          }}
        />
      ) : null}
    </section>
  );
}
```

`ConfirmDialog` wraps the existing `ModalShell` used by `ApiTokenForms.tsx` and states the count rather than asking for a blind yes:

```tsx
function ConfirmDialog({
  user,
  kind,
  busy,
  onCancel,
  onConfirm,
}: {
  user: AdminUser;
  kind: "messages" | "account";
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const count = user.message_count.toLocaleString();
  const body =
    kind === "messages"
      ? `This permanently deletes ${count} messages belonging to ${user.username}, and their attachments. It cannot be undone.`
      : `This permanently deletes ${user.username}'s account along with ${count} messages and their attachments. It cannot be undone.`;
  return (
    <ModalShell title={kind === "messages" ? "Delete messages" : "Delete account"} onClose={onCancel}>
      <p className="text-[0.875rem] text-text">{body}</p>
      <DialogFooter>
        <Button onPress={onCancel} isDisabled={busy}>
          Cancel
        </Button>
        <Button onPress={onConfirm} isDisabled={busy} variant="danger">
          Delete
        </Button>
      </DialogFooter>
    </ModalShell>
  );
}
```

Two substitutions to make while writing this file. First, locate the byte formatter `StorageSection` already uses — `grep -rn "formatBytes\|Bytes(" web/src/lib web/src/screens/settings` — and import that one rather than adding a second. Second, the settings panels render checkboxes through a shared component rather than a bare `<input type="checkbox">`; find it with `grep -rn "type=\"checkbox\"\|Checkbox" web/src/components` and use it, keeping the `aria-label` text above verbatim so the test's queries still match.

Labels are written for the reader, not after the columns:

| Field | Label |
|---|---|
| `is_admin` | Allow this user to manage the vault |
| `disabled` | Disable this user |
| `can_import` | Allow importing messages |
| `can_export` | Allow exporting messages |
| `can_delete` | Allow deleting messages and attachments |

Follow `ApiTokensTable.tsx` for table styling — it already exports `thClass`, `tdClass`, and `tdMuted` from `apiTokensUtils.ts`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cd web && npx vitest run src/screens/settings/AdminUsersPanel.test.tsx`
Expected: PASS.

- [ ] **Step 6: Add the tab**

In `web/src/screens/SettingsScreen.tsx`, read the profile's `is_admin` and build the tab list conditionally:

```tsx
const BASE_TABS = ["account", "profile", "storage", "system", "appearance"] as const;
const ADMIN_TABS = ["account", "profile", "users", "storage", "system", "appearance"] as const;
```

`tabFromSearchParam` validates against whichever list applies, so a non-administrator landing on `?tab=users` falls back to `account` rather than rendering a panel that will 403. Add the `TabPanel`:

```tsx
        <TabPanel id="users" className="mt-6">
          <AdminUsersPanel />
        </TabPanel>
```

and the `{ id: "users", label: "Users" }` entry, inserted only for administrators. Update the header sentence to mention users when the tab is present.

- [ ] **Step 7: Run the suites**

Run: `cd web && npm test && npx tsc --noEmit && npx biome ci .`
Expected: PASS.

- [ ] **Step 8: Verify in the browser**

Run the vault and the web dev server:

```bash
./scripts/run-vault-dev.sh
cd web && npm run dev
```

Open `http://127.0.0.1:5173`, register the first account (it becomes the administrator), register a second in a private window, then check Settings → Users as the administrator. Confirm the second account appears, its permission checkboxes save, disabling it logs it out on its next request, and the delete confirmations name the account.

Per `.cursor/rules/playwright-mcp.mdc`, use the Playwright MCP for this if it is connected. If it is not, do the pass by hand and say so in the commit body rather than claiming a verification that did not happen.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(web): Users panel for administrators"
```

---

### Task 11: Full-repo verification

**Files:** none — this task runs the gate and fixes whatever it finds.

- [ ] **Step 1: Run the repo's own PR gate**

Run: `./scripts/check-pr.sh`
Expected: PASS. It stops on the first failure, so re-run after each fix.

- [ ] **Step 2: Run Clippy, which CI does not gate**

Run: `./scripts/lint-all.sh`
Expected: no warnings in changed crates.

- [ ] **Step 3: Run the Postgres engine tests**

```bash
docker compose -f docker-compose.pg.yml up -d
MV_TEST_POSTGRES_URL=postgres://vault:vault@127.0.0.1:5432/vault cargo test -p message-vault-server
```

Expected: PASS. These cover the `pg_accounts.sql` twin, which the SQLite-only run never exercises.

- [ ] **Step 4: Confirm the removals are complete**

```bash
grep -ril "hanko\|try_demo\|try-demo\|guest_status\|guest_pool\|read_only" crates schema web/src .env.example
```

Expected: no output. Hits in `web-next/` are expected and out of scope; the command above does not search it.

- [ ] **Step 5: Confirm the schema version and the fixture snapshot agree**

Run: `node scripts/sync-vault-schema.mjs && git diff --exit-code tests/fixtures/schema/`
Expected: no diff — the snapshot was regenerated in Task 3 and nothing since should have changed it.

- [ ] **Step 6: Commit any fixes**

```bash
git add -A
git commit -m "chore: repo gate fixes for the authorization model"
```
