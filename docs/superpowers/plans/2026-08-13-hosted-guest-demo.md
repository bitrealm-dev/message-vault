# Hosted Guest Demo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hosted Try it signs a browser visitor into a private, editable copy of the sample dataset from a ready pool, without the desktop app; self-hosted still uses the shared `demo` login.

**Architecture:** `reset-demo` still fills the template `demo` account. A background worker clones that account (SQL row copy with remapped integer ids, hard-linked attachment files) into unused guest rows. `POST /v1/auth/try-demo` assigns one guest and issues a 24-hour session. The existing `web/` SPA calls that endpoint; Import/Export/Extract/Format stay hidden when not Tauri and when `is_guest`.

**Tech Stack:** Rust (`message-vault-server`, rusqlite, axum), SQLite, Vite React SPA, Docker Compose env.

**Spec:** [docs/superpowers/specs/2026-08-13-hosted-guest-demo-design.md](../specs/2026-08-13-hosted-guest-demo-design.md)

## Global Constraints

- Guest session lifetime default: `86400` seconds (24 hours). Normal accounts stay at `SESSION_TTL_SECS` (30 days).
- Unused ready pool floor default `2`, ceiling default `20`.
- `GUEST_DEMO_POOL` default `false` (self-hosted unchanged).
- Truthy env values (case-insensitive): `true`, `1`, `yes`.
- Do not copy `account_emails`, `account_session_tokens`, or `account_api_tokens`.
- Do not block `GET /v1/export/messages` (that is the conversation browse API).
- Hosted Try it must not tell the visitor to install the desktop app.
- `read_only` stays `0` on guests (edits and deletes allowed). Template `demo` stays `read_only`.
- One clone at a time; wait on the existing vault operation lock used by `reset-demo`.
- Communication in user-facing copy: no “we/us/our”; say “Try it” and “sample account.”

## File map

| File | Role |
|------|------|
| `schema/sql/accounts.sql` | `guest_status` on `accounts` |
| `crates/vault/server/src/db/schema.rs` | Additive `ensure_column` for `guest_status` |
| `crates/vault/server/src/db/account_profile.rs` | Guest helpers; `insert_guest_account` |
| `crates/vault/server/src/db/session_tokens.rs` | Session insert with custom TTL |
| `crates/vault/server/src/config.rs` | `GuestDemoSettings::from_env()` |
| `crates/vault/server/src/guest_clone.rs` | **Create.** SQL clone + hard-link assets |
| `crates/vault/server/src/guest_pool.rs` | **Create.** Assign, refill, sweep |
| `crates/vault/server/src/auth.rs` | `try-demo`, reject `demo` login when pool on, logout deletes guest |
| `crates/vault/server/src/server.rs` | Route, `try_demo` on `/v1/auth/mode`, AppState, worker, 403 gates |
| `crates/vault/server/src/profile.rs` | `is_guest` on profile JSON |
| `crates/vault/server/src/reset_demo.rs` | Drop unused ready guests after a successful reset |
| `crates/vault/server/src/main.rs` | `mod guest_clone;` `mod guest_pool;` |
| `crates/vault/server/src/api_tokens_api.rs` | 403 for guests |
| `web/src/lib/account.ts` | `is_guest` |
| `web/src/lib/authGuards.ts` | `try_demo` on mode response |
| `web/src/screens/LoginScreen.tsx` | Try it button |
| `web/src/components/LeftPanel.tsx` | Hide Import/Export for guests |
| `web/src/App.tsx` | Redirect `/import` `/export` for guests and non-Tauri |
| `web/src/screens/settings/AccountSettingsPanel.tsx` | Hide password + API tokens for guests |
| `compose-release.yml` / hosted comments | Document `GUEST_DEMO_POOL` |
| `docs/src/content/docs/get-started/try-the-vault.md` | Hosted vs self-hosted sign-in |

---

### Task 1: `guest_status` column and helpers

**Files:**
- Modify: `schema/sql/accounts.sql`
- Modify: `crates/vault/server/src/db/schema.rs`
- Modify: `crates/vault/server/src/db/account_profile.rs`
- Test: `crates/vault/server/src/db/schema.rs` (existing `mod tests`)
- Test: `crates/vault/server/src/db/account_profile.rs` (existing `mod tests`)

**Interfaces:**
- Consumes: existing `ensure_accounts_schema`, `insert_account`
- Produces:
  - `accounts.guest_status` — `NULL` | `'ready'` | `'assigned'`
  - `pub fn guest_status(conn: &Connection, account_id: &str) -> Result<Option<String>>`
  - `pub fn is_guest_account(conn: &Connection, account_id: &str) -> Result<bool>`
  - `pub fn insert_guest_account(conn: &Connection, id: &str, username: &str, preferred_name: Option<&str>) -> Result<()>`
  - `pub fn set_guest_status(conn: &Connection, account_id: &str, status: &str) -> Result<()>`

- [ ] **Step 1: Write the failing schema test**

Add to `schema.rs` `mod tests`:

```rust
#[test]
fn guest_status_column_exists_and_defaults_null() {
    let conn = Connection::open_in_memory().unwrap();
    ensure_accounts_schema(&conn).unwrap();
    conn.execute(
        "INSERT INTO accounts (id, username) VALUES (?1, 'alice')",
        params![A1],
    )
    .unwrap();
    let status: Option<String> = conn
        .query_row(
            "SELECT guest_status FROM accounts WHERE id = ?1",
            params![A1],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, None);
}
```

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cargo test -p message-vault-server guest_status_column_exists_and_defaults_null -- --nocapture`

Expected: FAIL (`no such column: guest_status`)

- [ ] **Step 3: Add the column**

In `schema/sql/accounts.sql`, after `hanko_user_id`, add:

```sql
    -- 'ready' | 'assigned' for hosted guest copies; NULL for every other account.
    guest_status TEXT
```

In `ensure_accounts_schema`, after the existing `ensure_column` calls:

```rust
    ensure_column(
        conn,
        "accounts",
        "guest_status",
        "ALTER TABLE accounts ADD COLUMN guest_status TEXT",
    )?;
```

Add helpers on `account_profile.rs`:

```rust
pub fn guest_status(conn: &Connection, account_id: &str) -> Result<Option<String>> {
    schema::ensure_accounts_schema(conn)?;
    let status: Option<String> = conn
        .query_row(
            "SELECT guest_status FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(status.filter(|s| !s.is_empty()))
}

pub fn is_guest_account(conn: &Connection, account_id: &str) -> Result<bool> {
    Ok(guest_status(conn, account_id)?.is_some())
}

pub fn insert_guest_account(
    conn: &Connection,
    id: &str,
    username: &str,
    preferred_name: Option<&str>,
) -> Result<()> {
    schema::ensure_accounts_schema(conn)?;
    conn.execute(
        r#"
        INSERT INTO accounts (
            id, username, read_only, password_hash, preferred_name, guest_status
        ) VALUES (?1, ?2, 0, NULL, ?3, 'ready')
        "#,
        params![id, username, preferred_name],
    )?;
    Ok(())
}

pub fn set_guest_status(conn: &Connection, account_id: &str, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE accounts SET guest_status = ?2 WHERE id = ?1",
        params![account_id, status],
    )?;
    Ok(())
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p message-vault-server guest_status_column_exists_and_defaults_null -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add schema/sql/accounts.sql crates/vault/server/src/db/schema.rs crates/vault/server/src/db/account_profile.rs
git commit -m "$(cat <<'EOF'
feat(vault): add guest_status on accounts

Hosted Try it needs a column that marks a private sample copy as unused or assigned without using read_only.
EOF
)"
```

---

### Task 2: `GuestDemoSettings` from env

**Files:**
- Modify: `crates/vault/server/src/config.rs`

**Interfaces:**
- Consumes: env `GUEST_DEMO_POOL`, `GUEST_POOL_MIN`, `GUEST_POOL_MAX`, `GUEST_SESSION_SECS`
- Produces:

```rust
#[derive(Debug, Clone, Copy)]
pub struct GuestDemoSettings {
    pub enabled: bool,
    pub pool_min: u32,
    pub pool_max: u32,
    pub session_secs: u64,
}

impl GuestDemoSettings {
    pub fn from_env() -> Self { /* ... */ }
    pub fn disabled() -> Self {
        Self { enabled: false, pool_min: 2, pool_max: 20, session_secs: 86_400 }
    }
}
```

- [ ] **Step 1: Write failing tests in `config.rs` `mod tests`**

```rust
#[test]
fn guest_demo_settings_default_disabled() {
    // Call the parse helper with empty values, not from_env(), so other tests stay isolated.
    let s = GuestDemoSettings::parse("", "", "", "");
    assert!(!s.enabled);
    assert_eq!(s.pool_min, 2);
    assert_eq!(s.pool_max, 20);
    assert_eq!(s.session_secs, 86_400);
}

#[test]
fn guest_demo_settings_truthy_and_clamps() {
    let s = GuestDemoSettings::parse("true", "0", "100", "60");
    assert!(s.enabled);
    assert_eq!(s.pool_min, 1); // floor at 1
    assert_eq!(s.pool_max, 100);
    assert_eq!(s.session_secs, 60);
    let s = GuestDemoSettings::parse("yes", "5", "3", "not-a-number");
    assert!(s.enabled);
    assert_eq!(s.pool_max, 5); // max raised to min
    assert_eq!(s.session_secs, 86_400);
}
```

- [ ] **Step 2: Run tests and confirm they fail**

Run: `cargo test -p message-vault-server guest_demo_settings -- --nocapture`

Expected: FAIL (`GuestDemoSettings` not found)

- [ ] **Step 3: Implement `parse` + `from_env`**

```rust
fn env_truthy(raw: &str) -> bool {
    matches!(raw.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

impl GuestDemoSettings {
    pub fn from_env() -> Self {
        Self::parse(
            &std::env::var("GUEST_DEMO_POOL").unwrap_or_default(),
            &std::env::var("GUEST_POOL_MIN").unwrap_or_default(),
            &std::env::var("GUEST_POOL_MAX").unwrap_or_default(),
            &std::env::var("GUEST_SESSION_SECS").unwrap_or_default(),
        )
    }

    pub(crate) fn parse(pool: &str, min: &str, max: &str, secs: &str) -> Self {
        let enabled = env_truthy(pool);
        let mut pool_min = min.parse::<u32>().unwrap_or(2).max(1);
        let mut pool_max = max.parse::<u32>().unwrap_or(20).max(1);
        if pool_max < pool_min {
            pool_max = pool_min;
        }
        let session_secs = secs.parse::<u64>().unwrap_or(86_400).max(60);
        let _ = pool_min; // keep names for the struct
        Self { enabled, pool_min, pool_max, session_secs }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server guest_demo_settings -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/config.rs
git commit -m "$(cat <<'EOF'
feat(vault): parse hosted guest demo pool settings from env

Self-hosted stays off unless GUEST_DEMO_POOL is set, so local Docker behavior does not change.
EOF
)"
```

---

### Task 3: Session token with custom TTL

**Files:**
- Modify: `crates/vault/server/src/db/session_tokens.rs`

**Interfaces:**
- Consumes: existing `insert_account_session_token`
- Produces: `pub fn insert_account_session_token_with_ttl(conn: &Connection, account_id: &str, ttl_secs: u64) -> Result<String>`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn insert_session_with_ttl_sets_expires_at() {
    let conn = setup(); // reuse the existing test helper in this file
    let before = now_unix_secs();
    let token = insert_account_session_token_with_ttl(&conn, "a1", 120).unwrap();
    assert!(token.starts_with("mv-user-"));
    let expires: String = conn
        .query_row(
            "SELECT expires_at FROM account_session_tokens WHERE account_id = 'a1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let exp: u64 = expires.parse().unwrap();
    assert!(exp >= before + 120);
    assert!(exp <= before + 130);
}
```

- [ ] **Step 2: Run test and confirm it fails**

Run: `cargo test -p message-vault-server insert_session_with_ttl_sets_expires_at -- --nocapture`

Expected: FAIL (function missing)

- [ ] **Step 3: Implement**

Refactor the existing insert to take `ttl_secs`. Keep `insert_account_session_token` as a wrapper that passes `SESSION_TTL_SECS`.

```rust
pub fn insert_account_session_token_with_ttl(
    conn: &Connection,
    account_id: &str,
    ttl_secs: u64,
) -> Result<String> {
    let token = generate_session_token()?;
    let token_hash = hash_api_token(&token);
    let created_at = unix_secs_string();
    let expires_at = format!("{}", now_unix_secs().saturating_add(ttl_secs));
    conn.execute(
        "INSERT INTO account_session_tokens (account_id, token_hash, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![account_id, token_hash, created_at, expires_at],
    )?;
    Ok(token)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server --lib session_tokens -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/db/session_tokens.rs
git commit -m "$(cat <<'EOF'
feat(vault): allow a shorter session lifetime for guest accounts

Hosted sample copies should expire in 24 hours instead of the 30-day GUI session used for real accounts.
EOF
)"
```

---

### Task 4: Clone template account into a guest

**Files:**
- Create: `crates/vault/server/src/guest_clone.rs`
- Modify: `crates/vault/server/src/main.rs` (add `mod guest_clone;`)

**Interfaces:**
- Consumes: `insert_guest_account`, `DEMO_ACCOUNT_ID`, `PathsConfig::assets_dir_for_account`
- Produces:

```rust
pub fn clone_template_to_guest(
    conn: &mut Connection,
    cfg: &Config,
    template_account_id: &str,
) -> Result<String> // new guest account id (UUID)
```

Clone order (integer ids remapped in Rust maps; do not copy old ids):

1. Insert guest account (`guest_status='ready'`, username `guest-<8 hex chars>`, `read_only=0`, same `preferred_name` as template).
2. Copy `handles` → map old handle id → new.
3. Copy `contacts` → map contact ids.
4. Copy `contact_handles`, `account_handles` (skip if handle missing), `contact_labels` + `contact_label_members`, trash tables.
5. Copy `vault_imports` → map import ids; copy `vault_import_issues`.
6. Copy `conversations` (rewrite `chat_handle_id`) → map conversation ids.
7. Copy `participants` (rewrite `conversation_id`, `handle_id`, `contact_id`).
8. Copy `messages` with `duplicate_of` NULL; then `UPDATE` `duplicate_of` and `import_id` from maps. Rewrite `conversation_id`, `sender_handle_id`, `account_id`.
9. Copy `attachments` and `tapbacks` (rewrite `message_id`, `sender_handle_id`).
10. Copy staging tables the same way if the template has any rows.
11. Copy `account_prefs`.
12. Do **not** copy `account_emails`, session tokens, or API tokens.
13. Hard-link (fallback copy) every file under `data/<template>/<source>/{assets,assets_converted}` into `data/<guest>/<source>/...`.

`messages_fts` updates via existing INSERT triggers. Leave triggers on.

- [ ] **Step 1: Write a failing clone test in `guest_clone.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, params};
    use crate::db::schema;

    const T: &str = "00000000-0000-0000-0000-00000000d001";

    fn tiny_template(conn: &Connection) {
        schema::ensure_vault_schema(conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only, preferred_name)
             VALUES (?1, 'demo', 1, 'Alex Demo')",
            params![T],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES (?1, '+15555550100', '+15555550100', 'phone', 'phone')",
            params![T],
        )
        .unwrap();
        let hid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO account_handles (account_id, handle_id) VALUES (?1, ?2)",
            params![T, hid],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, source_file)
             VALUES (?1, ?2, 'individual', 'a.jsonl')",
            params![T, hid],
        )
        .unwrap();
        let cid = conn.last_insert_rowid();
        conn.execute(
            r#"INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order, body
            ) VALUES (?1, ?2, 'imessage', 'g1', '2020-01-01T00:00:00Z', 1, 0, 'hello')"#,
            params![cid, T],
        )
        .unwrap();
    }

    #[test]
    fn clone_copies_rows_and_leaves_template() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        tiny_template(&conn);
        let cfg = test_config(); // temp data_dir
        let guest = clone_template_to_guest(&mut conn, &cfg, T).unwrap();
        assert_ne!(guest, T);
        let t_msgs: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages WHERE account_id = ?1", params![T], |r| r.get(0))
            .unwrap();
        let g_msgs: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages WHERE account_id = ?1", params![guest], |r| r.get(0))
            .unwrap();
        assert_eq!(t_msgs, 1);
        assert_eq!(g_msgs, 1);
        let emails: i64 = conn
            .query_row("SELECT COUNT(*) FROM account_emails WHERE account_id = ?1", params![guest], |r| r.get(0))
            .unwrap();
        assert_eq!(emails, 0);
        let status: String = conn
            .query_row("SELECT guest_status FROM accounts WHERE id = ?1", params![guest], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "ready");
    }

    #[test]
    fn second_clone_does_not_collide() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        tiny_template(&conn);
        let cfg = test_config();
        let a = clone_template_to_guest(&mut conn, &cfg, T).unwrap();
        let b = clone_template_to_guest(&mut conn, &cfg, T).unwrap();
        assert_ne!(a, b);
    }
}
```

`test_config()` builds a `Config` whose `paths.data_dir` is a `tempfile::TempDir` kept alive for the test (store the `TempDir` in a struct or `Box::leak` the path for the test process).

Add a third test that writes one file under the template assets dir and asserts the guest path is a hard link (`same FileId` / equal `ino` on Unix) or, if the platform cannot hard-link, that the bytes match.

- [ ] **Step 2: Run tests and confirm they fail**

Run: `cargo test -p message-vault-server --lib guest_clone -- --nocapture`

Expected: FAIL (module missing)

- [ ] **Step 3: Implement `clone_template_to_guest`**

Hard-link helper:

```rust
fn link_or_copy(src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::hard_link(src, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(src, dest)?;
            Ok(())
        }
    }
}
```

Walk `cfg.paths.data_dir.join(template_id)` recursively. For each file, write the same relative path under `data_dir.join(guest_id)`.

Hold the vault operation lock around the whole clone (`operation_lock::acquire_for_reset` is exclusive and would block serve — do **not** use that). Use a process-local `std::sync::Mutex<()>` passed in from AppState (Task 5/8). For this unit test, pass no lock or a local mutex.

Signature adjustment so tests do not need the HTTP lock:

```rust
pub fn clone_template_to_guest(
    conn: &mut Connection,
    cfg: &Config,
    template_account_id: &str,
) -> Result<String>
```

Use a transaction (`conn.transaction()?`) for all SQL; hard-link files after commit. If file linking fails, delete the guest account (cascade) and return the error.

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server --lib guest_clone -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/guest_clone.rs crates/vault/server/src/main.rs
git commit -m "$(cat <<'EOF'
feat(vault): clone the demo template account into a guest

A private sample inbox is a row copy with remapped ids and hard-linked media, not a second reset-demo import.
EOF
)"
```

---

### Task 5: Pool assign, refill, and sweep

**Files:**
- Create: `crates/vault/server/src/guest_pool.rs`
- Modify: `crates/vault/server/src/main.rs` (`mod guest_pool;`)

**Interfaces:**
- Consumes: `clone_template_to_guest`, `GuestDemoSettings`, `insert_account_session_token_with_ttl`, `set_guest_status`, `delete_account`
- Produces:

```rust
pub fn count_ready(conn: &Connection) -> Result<u32>

pub fn assign_ready_guest(
    conn: &mut Connection,
    session_secs: u64,
) -> Result<Option<(String, String, String)>>
// (account_id, username, token)

pub fn refill_pool(
    conn: &mut Connection,
    cfg: &Config,
    settings: GuestDemoSettings,
    assignments_last_15m: u32,
) -> Result<u32> // clones created

pub fn sweep_expired_guests(conn: &Connection, data_dir: &Path) -> Result<u32>

pub fn drop_ready_guests(conn: &Connection, data_dir: &Path) -> Result<u32>
```

Assignment SQL (one transaction, `BEGIN IMMEDIATE`):

```sql
SELECT id, username FROM accounts
WHERE guest_status = 'ready'
ORDER BY id
LIMIT 1
```

Then `set_guest_status(..., "assigned")`, `insert_account_session_token_with_ttl`, commit.

Refill target: `max(pool_min, assignments_last_15m)` capped at `pool_max`. If `count_ready > pool_max`, delete oldest ready guests (`ORDER BY id` is fine).

Sweep: assigned guests whose session row is missing or `expires_at <= now`. Delete account row (cascade) and `remove_dir_all(data_dir.join(account_id))`.

Record assignments for the 15-minute window in a small in-memory `VecDeque<(Instant, u32)>` on a `GuestPoolState` struct, or a table. Prefer memory on `GuestPoolState` held in `AppState` (Task 8). For unit tests, pass `assignments_last_15m` into `refill_pool`.

- [ ] **Step 1: Write failing tests in `guest_pool.rs`**

```rust
#[test]
fn assign_marks_assigned_and_issues_token() { /* clone twice, assign once, count_ready == 1 */ }

#[test]
fn two_assigns_never_share_a_guest() {
    // Sequential in one test is enough; also run two threads on one file DB
    // with BEGIN IMMEDIATE to prove the LIMIT 1 pick is serialized.
}

#[test]
fn sweep_deletes_expired_assigned_only() { /* ready stays; expired assigned gone */ }

#[test]
fn refill_respects_max() {
    // settings pool_min=1 pool_max=2, assignments_last_15m=50 → create at most 2 ready
}
```

- [ ] **Step 2: Run tests and confirm they fail**

Run: `cargo test -p message-vault-server --lib guest_pool -- --nocapture`

Expected: FAIL (module missing)

- [ ] **Step 3: Implement the four functions**

`assign_ready_guest` returns `Ok(None)` when no ready row exists (caller clones on demand).

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server --lib guest_pool -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/guest_pool.rs crates/vault/server/src/main.rs
git commit -m "$(cat <<'EOF'
feat(vault): assign, refill, and expire hosted guest demo accounts

Try it is a token handoff from a ready copy; abandoned assigned copies are deleted so disk does not grow without bound.
EOF
)"
```

---

### Task 6: Auth — `try-demo`, hosted `demo` login reject, logout deletes guest

**Files:**
- Modify: `crates/vault/server/src/auth.rs`
- Modify: `crates/vault/server/src/server.rs` (route + `auth_mode_handler`)

**Interfaces:**
- Consumes: `GuestDemoSettings` on `AppState` (add the field in this task if Task 8 has not landed; add `guest: GuestDemoSettings` and `guest_clone_lock: Arc<Mutex<()>>` to `AppState` here)
- Produces:
  - `POST /v1/auth/try-demo` → `AuthTokenResponse`
  - `GET /v1/auth/mode` includes `"try_demo": bool`
  - When `guest.enabled`, `POST /v1/auth/login` as username `demo` returns 401/400

Behavior of `try_demo_handler`:

1. `check_auth_rate_limit("try-demo")`.
2. If `!state.guest.enabled`: issue a session for `DEMO_ACCOUNT_ID` via `AuthTokenResponse::for_existing_account` (self-hosted Try it). If that account is missing, 503 with a clear message.
3. If enabled: `assign_ready_guest`. If `None`, take `guest_clone_lock`, `clone_template_to_guest`, then assign. If clone exceeds 60 seconds or fails, 503.
4. Return `{ token, account_id, username }`.

Logout: after `revoke_session_token`, if `is_guest_account`, `delete_account` and `remove_dir_all(data_dir.join(account_id))`.

Login: after resolving `account_id`, if `guest.enabled && username == "demo"`, bail `"use Try it to open a sample account"`.

- [ ] **Step 1: Write failing auth tests**

Follow the existing `auth.rs` `mod tests` style (in-memory DB + handler helpers if present). If handlers are hard to call without a full `AppState`, test the extracted functions:

```rust
pub(crate) fn reject_demo_password_login(enabled: bool, username: &str) -> bool {
    enabled && username.eq_ignore_ascii_case("demo")
}
```

Plus an integration-style test that builds `AppState` the same way `server.rs` tests already do (`auth_route_status` pattern around line 2066). Add:

```rust
#[tokio::test]
async fn try_demo_route_exists() {
    // POST /v1/auth/try-demo is registered (not 404) on the local auth router
}
```

And a unit test that logout deletes a guest row (call the blocking body extracted as `logout_on_conn`).

- [ ] **Step 2: Run tests and confirm they fail**

Run: `cargo test -p message-vault-server try_demo -- --nocapture`

Expected: FAIL

- [ ] **Step 3: Implement handlers and wire the route**

In `auth_public_router` (always, both Hanko and Local):

```rust
.route("/v1/auth/try-demo", post(crate::auth::try_demo_handler))
```

Update `auth_mode_handler` to take `State(state)` and include `try_demo: state.guest.enabled`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server --lib auth -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/auth.rs crates/vault/server/src/server.rs
git commit -m "$(cat <<'EOF'
feat(vault): add try-demo sign-in and expire guest copies on logout

The hosted button can hand out a private sample session without sharing the template demo login.
EOF
)"
```

---

### Task 7: Guest 403s and `is_guest` on the profile

**Files:**
- Modify: `crates/vault/server/src/profile.rs`
- Modify: `crates/vault/server/src/server.rs` (`require_import_access` / asset PUT / imports create)
- Modify: `crates/vault/server/src/api_tokens_api.rs`
- Modify: `crates/vault/server/src/auth.rs` (`change_password_handler`)
- Modify: `web/src/lib/account.ts`

**Interfaces:**
- Consumes: `is_guest_account`
- Produces: `AccountProfileResponse.is_guest: bool`

Add:

```rust
pub fn reject_if_guest(conn: &Connection, account_id: &str) -> Result<(), ApiError> {
    if account_profile::is_guest_account(conn, account_id).map_err(|e| ApiError::Internal(e.to_string()))? {
        return Err(ApiError::Forbidden(
            "sample accounts cannot import, export backups, or create API tokens".into(),
        ));
    }
    Ok(())
}
```

Call it at the start of:

- `imports_create_handler` / `import_handler` (the POST bodies)
- `asset_put_handler` and multipart start (`asset_upload_start_handler`)
- every API-token create/rename handler
- `change_password_handler`

Do **not** call it on `GET /v1/export/messages`.

- [ ] **Step 1: Write failing tests**

In `profile.rs` tests, assert `is_guest` is true for a row with `guest_status='assigned'`.

In `server.rs` or `api_tokens_api.rs` tests, create a guest, obtain a session, `POST /v1/imports` → 403, `GET /v1/export/messages?q=...` → 200 (or 400 if query missing — not 403).

- [ ] **Step 2: Run tests and confirm they fail**

Run: `cargo test -p message-vault-server is_guest -- --nocapture`

Expected: FAIL (`is_guest` missing)

- [ ] **Step 3: Implement the field and the gates**

Update `web/src/lib/account.ts`:

```ts
  is_demo?: boolean;
  is_guest?: boolean;
  read_only?: boolean;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server --lib profile -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/profile.rs crates/vault/server/src/server.rs crates/vault/server/src/api_tokens_api.rs crates/vault/server/src/auth.rs web/src/lib/account.ts
git commit -m "$(cat <<'EOF'
feat(vault): block backup import and API tokens on guest accounts

Sample visitors can edit and delete in the browser; they cannot upload a backup or mint a vault-push token.
EOF
)"
```

---

### Task 8: Worker on `serve` and `reset-demo` refill

**Files:**
- Modify: `crates/vault/server/src/server.rs` (`serve` startup)
- Modify: `crates/vault/server/src/reset_demo.rs`
- Modify: `crates/vault/server/src/guest_pool.rs` if a `tick` helper is cleaner

**Interfaces:**
- Consumes: `refill_pool`, `sweep_expired_guests`, `drop_ready_guests`
- Produces: background `tokio` interval (60s) when `guest.enabled`; after a successful `reset-demo`, `drop_ready_guests` then `refill_pool` if env is enabled

On `serve` after `AppState` is built, if `state.guest.enabled`:

```rust
let worker_state = state.clone();
tokio::spawn(async move {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        interval.tick().await;
        let db = worker_state.cfg.paths.db.clone();
        let cfg = worker_state.cfg.clone();
        let guest = worker_state.guest;
        let demand = worker_state.guest_demand.lock().unwrap().count_last_15m();
        let clone_lock = worker_state.guest_clone_lock.clone();
        let data_dir = cfg.paths.data_dir.clone();
        let _ = tokio::task::spawn_blocking(move || -> anyhow::Result<u32> {
            let mut conn = schema::open_configured(&db)?;
            guest_pool::sweep_expired_guests(&conn, &data_dir)?;
            let _lock = clone_lock.lock().unwrap();
            guest_pool::refill_pool(&mut conn, &cfg, guest, demand)
        })
        .await;
    }
});
```

`reset-demo`: after the new template is live, if `GuestDemoSettings::from_env().enabled`, open the live DB and `drop_ready_guests` + `refill_pool`. Assigned guests stay.

- [ ] **Step 1: Write a failing reset-demo test**

Extend `reset_demo.rs` tests: insert a ready guest pointing at old data, run reset (or call the new hook `after_reset_refresh_guest_pool`), assert that ready guest id is gone and `count_ready` is `pool_min` (skip refill in the test if reset is too heavy — then only test `drop_ready_guests` + a stub refill).

Prefer testing `drop_ready_guests` in `guest_pool.rs` (already Task 5) and a thin `after_reset_refresh_guest_pool(cfg, settings)` in `reset_demo.rs` that the existing reset success path calls.

- [ ] **Step 2: Run the new test and confirm it fails**

Run: `cargo test -p message-vault-server after_reset_refresh_guest_pool -- --nocapture`

Expected: FAIL

- [ ] **Step 3: Implement the hook and the worker spawn**

- [ ] **Step 4: Run tests**

Run: `cargo test -p message-vault-server --lib reset_demo -- --nocapture`

Expected: PASS (existing reset tests still pass)

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/server.rs crates/vault/server/src/reset_demo.rs crates/vault/server/src/guest_pool.rs
git commit -m "$(cat <<'EOF'
feat(vault): keep the guest demo pool filled and drop stale copies after reset-demo

New visitors keep getting a ready inbox; a template refresh does not hand out clones of the old dataset.
EOF
)"
```

---

### Task 9: Website Try it button and hide desktop-only chrome

**Files:**
- Modify: `web/src/lib/authGuards.ts`
- Modify: `web/src/lib/authGuards.test.ts`
- Modify: `web/src/screens/LoginScreen.tsx`
- Modify: `web/src/components/LeftPanel.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/screens/settings/AccountSettingsPanel.tsx`
- Modify: `web/src/lib/auth.tsx` only if logout must call a new helper (existing `POST /v1/auth/logout` is enough once the server deletes the guest)

**Interfaces:**
- Consumes: `try_demo` on `/v1/auth/mode`, `is_guest` on profile
- Produces: Try it button; Import/Export hidden for `is_guest || !isTauri()`

- [ ] **Step 1: Write the failing authGuards test**

```ts
export function isTryDemoEnabled(value: unknown): boolean {
  return value === true;
}
```

```ts
it("reads try_demo only when true", () => {
  expect(isTryDemoEnabled(true)).toBe(true);
  expect(isTryDemoEnabled(false)).toBe(false);
  expect(isTryDemoEnabled(undefined)).toBe(false);
});
```

Add `canUseImportExport({ isTauri, isGuest }: { isTauri: boolean; isGuest: boolean }): boolean` in a small helper (e.g. `web/src/lib/desktopFeatures.ts`) so LeftPanel and App share one rule:

```ts
export function canUseImportExport(isTauriApp: boolean, isGuest: boolean): boolean {
  return isTauriApp && !isGuest;
}
```

Test: `(true, false) → true`; `(true, true) → false`; `(false, false) → false`.

- [ ] **Step 2: Run tests and confirm they fail**

Run: `cd web && npm test -- src/lib/authGuards.test.ts src/lib/desktopFeatures.test.ts`

Expected: FAIL (`desktopFeatures` missing)

- [ ] **Step 3: Implement helper + UI**

`LoginScreen`: extend `AuthModeResponse` with `try_demo?: boolean`. Above the username fields (local mode) and also on the vault-selection card when mode is already known, add:

```tsx
<Button
  variant="primary"
  onClick={() => {
    void run(async () => {
      const res = await apiClient.post<{
        token: string;
        account_id: string;
      }>("/v1/auth/try-demo", {});
      login(serverUrl.trim(), res.token, res.account_id);
    });
  }}
  disabled={busy}
>
  {busy ? "Opening sample…" : "Try it"}
</Button>
```

When `try_demo` is false, the same button still calls `/v1/auth/try-demo` (server logs in as `demo`). No username/password required for that click.

`LeftPanel`: change `{isTauri() && (` to `{canUseImportExport(isTauri(), profile?.is_guest === true) && (` — pass `is_guest` from `useAccountProfile()` (same hook Settings already uses). If LeftPanel should not fetch profile, pass `isGuest` as a prop from `AppLayout`.

`App.tsx`: wrap import/export routes so they `<Navigate to="/" />` when `!canUseImportExport(...)`.

`AccountSettingsPanel`: if `profile.is_guest`, hide password change and `<ApiTokensSection />`. Show: “This is a temporary sample account. It is removed after 24 hours or when you sign out.”

- [ ] **Step 4: Run web tests and lint**

Run: `cd web && npm test && npm run lint`

Expected: PASS (lint errors fail; warnings do not)

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/authGuards.ts web/src/lib/authGuards.test.ts web/src/lib/desktopFeatures.ts web/src/lib/desktopFeatures.test.ts web/src/screens/LoginScreen.tsx web/src/components/LeftPanel.tsx web/src/App.tsx web/src/screens/settings/AccountSettingsPanel.tsx
git commit -m "$(cat <<'EOF'
feat(web): add Try it and hide import in the browser and on guest sessions

Hosted visitors stay in the shared website; backup import and export remain desktop-only.
EOF
)"
```

---

### Task 10: Compose flag and docs

**Files:**
- Modify: `compose-release.yml` (comment + optional `GUEST_DEMO_POOL: ${GUEST_DEMO_POOL:-false}`)
- Modify: `docs/src/content/docs/get-started/try-the-vault.md`
- Modify: `docs/src/content/docs/set-up-the-server/try-the-demo.md`
- Modify: `docs/src/content/docs/introduction/quick-start.md`
- Modify: `docs/src/content/docs/reference/config-and-accounts.md` (guest + env table)

**Interfaces:**
- Consumes: Task 2 env names
- Produces: docs that split hosted Try it (browser button, no app download) from self-hosted `demo` / empty password

- [ ] **Step 1: Add env to compose-release and a comment on compose-dev**

```yaml
    environment:
      DEMO_DATA: ${DEMO_DATA:-true}
      VAULT_AUTH: ${VAULT_AUTH:-local}
      HANKO_API_URL: ${HANKO_API_URL:-}
      GUEST_DEMO_POOL: ${GUEST_DEMO_POOL:-false}
```

Default `false` so local compose behavior stays the shared `demo` user. Hosted ops sets `GUEST_DEMO_POOL=true`.

- [ ] **Step 2: Update the three user-facing demo pages**

Self-hosted paragraph stays: username `demo`, empty password.

Add a hosted paragraph: open the vault URL, click **Try it**. The site is enough. Do not send that reader to install the desktop app. Mention the copy lasts 24 hours and that Import/Export are not in the browser.

- [ ] **Step 3: Document env keys on `config-and-accounts.md`**

Copy the table from the spec (pool flag, min, max, session seconds).

- [ ] **Step 4: Commit**

```bash
git add compose-release.yml compose-dev.yml docs/src/content/docs/get-started/try-the-vault.md docs/src/content/docs/set-up-the-server/try-the-demo.md docs/src/content/docs/introduction/quick-start.md docs/src/content/docs/reference/config-and-accounts.md
git commit -m "$(cat <<'EOF'
docs: describe hosted Try it versus the shared self-hosted demo login

Operators can turn on the guest pool with GUEST_DEMO_POOL without changing local Docker defaults.
EOF
)"
```

---

## Self-review

**Spec coverage**

| Spec requirement | Task |
|---|---|
| Private copy per visitor | 4, 5, 6 |
| Fast click via ready pool that can grow to a ceiling | 5, 8 |
| Edit/delete allowed; no backup import/export | 7, 9 |
| Session-lived 24h; logout deletes | 3, 5, 6 |
| Hard-link assets; remap integer ids; skip `account_emails` | 4 |
| `POST /v1/auth/try-demo` + `try_demo` on mode | 6 |
| Hosted rejects password login as `demo` | 6 |
| Self-hosted Try it / `demo` login unchanged when flag off | 2, 6, 10 |
| Browser SPA; no “download the app” on hosted Try it | 9, 10 |
| Hide Import/Export when not Tauri and when guest | 9 |
| `reset-demo` drops ready guests and refills | 8 |
| Worker refill + sweep | 8 |
| Tests listed in the spec | 4, 5, 6, 7 |

**Type names used across tasks:** `GuestDemoSettings`, `clone_template_to_guest`, `assign_ready_guest`, `guest_status` / `is_guest_account`, `insert_account_session_token_with_ttl`, `canUseImportExport`.
