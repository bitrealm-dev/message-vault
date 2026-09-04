# Tests and Fixtures Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One test fixture backs every test in `message-vault-server`, the ten
hand-rolled `setup()` functions are gone, one `serve()` helper starts the app
for every HTTP helper, and four defects the PR 2 review recorded are fixed with
route-level tests that prove it.

**Architecture:** `crates/vault/server/src/test_support.rs` becomes the one
fixture. It already has `test_vault()` and the JSON helpers; this adds
`serve()` (one bind and spawn, kept alive until the response body has been
read), account creation, and a small conversation seeder. The ten `setup()`
functions scattered through `db/` and the `*_api.rs` modules are deleted in
favour of it. Then the four carried-over items get fixed, each with a test
through that fixture: the oversize-body helpers answer 413 instead of 400,
multipart rejections keep Axum's status, the fast `Content-Length` 413 carries
CORS headers, and the three unrun smoke scripts are replaced by route-level
tests and deleted.

**Tech Stack:** Rust, axum 0.8, tower-http, sqlx (`AnyPool` over SQLite),
reqwest, tokio, tempfile.

**Spec:** `docs/superpowers/specs/2026-09-03-http-interface-repair-design.md`,
section "Tests and fixtures". The roadmap
`docs/superpowers/plans/2026-09-03-http-interface-repair-roadmap.md`, section
"PR 7", carries four extra items from the PR 2 review and wins over the spec's
wording where the two disagree.

## Global Constraints

- ADR-0005: every route answers in the one shape — a success body or
  `{"error": "<sentence>"}` — and **the status carries the meaning**, so a
  status Axum or a helper already picked must not be flattened to 400.
  Regenerate `docs/src/assets/openapi.json` and `web/src/lib/vaultApi.types.ts`
  after any server change that alters a route's shape or status set.
- ADR-0002: one way to fetch data in `web/`. This pull request touches no
  `web/` source; if it somehow needs to, TanStack Query over `vaultApi.ts` is
  the only path.
- Export is the download button, never the path a screen reads by.
- No migration and no data preservation: `SCHEMA_VERSION` may be bumped freely
  and every vault is rebuilt empty. This plan changes no schema file, so no
  bump is expected.
- Simplification outranks compatibility and tests: a test may be deleted or
  rewritten whenever that produces a better design. Preserving a test's
  current shape is never a reason to keep a worse fixture.
- The spec says "eleven `setup()` functions"; there are ten today. The count
  in the spec is stale; the grep is the contract.
- `./scripts/check-pr.sh` must exit 0 on the head commit. Clippy is not gated
  in CI, so run `./scripts/lint-all.sh` before the final commit.
- Commit after every task with a conventional-commit subject in plain English.

## File Structure

| File | Responsibility after this plan |
| --- | --- |
| `crates/vault/server/src/test_support.rs` | The one fixture. `TestVault`, `serve()`, account creation, the conversation seeder, and every HTTP helper. |
| `crates/vault/server/src/server.rs` | `read_body_limited`, `discard_body`, `stream_body_to_file` answer 413; the CORS layer moves outside the body-limit layer. |
| `crates/vault/server/src/import/mod.rs` | The multipart rejection keeps Axum's status. Its `setup()`-free tests stay. |
| `crates/vault/server/src/assets.rs` | Gains route-level tests for `PUT` and `GET /v1/assets/{sha256}`. |
| `crates/vault/server/src/export_api.rs` | Gains route-level tests; loses its `setup()`. |
| `crates/vault/server/src/contacts_api.rs` | Gains the `offset` over `MAX_LIST_OFFSET` test; loses its `setup()`. |
| `crates/vault/server/src/db/{api_tokens,trash,schema,account_profile,saved_searches}.rs`, `named_membership.rs`, `profile.rs`, `conversations_api.rs` | Lose their `setup()`. |
| `scripts/test/` | Deleted. |

---

## Task 1: One `serve()` helper, and the server outlives the body read

**Files:**
- Modify: `crates/vault/server/src/test_support.rs:50-73` (`request`),
  `:234-303` (`post_raw`, `get_raw`, `delete_raw`)
- Test: `crates/vault/server/src/test_support.rs` (new `mod tests`)

**Interfaces:**
- Consumes: `crate::server::{AppState, http_app}` (already imported).
- Produces:
  - `pub struct TestServer` with `pub fn base(&self) -> &str` returning
    `http://127.0.0.1:<port>`, aborting its task on drop.
  - `pub async fn serve(state: &AppState) -> TestServer`.
  - Every existing helper keeps its current signature. Tasks 8 and 5 call
    `serve()` directly to set headers the helpers do not take.

**Why this is not test-driven.** There is no way to write a deterministic
failing test for the current code: the bug it removes is that `get_raw`,
`post_raw`, and `delete_raw` call `server.abort()` **before** awaiting
`response.text()`, which truncates a body only when the response has not
already been buffered — a race, not a behaviour. The guard for this task is
that the whole suite stays green, plus one new test that reads a body far
larger than a single TCP segment so the ordering requirement is written down
somewhere a future edit will trip over. Do not claim a red phase you did not
see.

- [ ] **Step 1: Add `TestServer` and `serve()`**

In `crates/vault/server/src/test_support.rs`, below the `RegisteredAccount`
struct:

```rust
/// A running instance of the real axum app on an ephemeral port.
///
/// The task is aborted when this value drops, so it must stay alive until the
/// response body has been read. Reading a body after the server task is gone
/// truncates it whenever the response was not already buffered.
pub struct TestServer {
    base: String,
    handle: tokio::task::JoinHandle<()>,
}

impl TestServer {
    /// `http://127.0.0.1:<port>`, to prefix a path with.
    pub fn base(&self) -> &str {
        &self.base
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Start the real axum app for `state` on an ephemeral port.
///
/// This is the one place in the test suite that binds a listener; every HTTP
/// helper below goes through it.
pub async fn serve(state: &AppState) -> TestServer {
    let app = http_app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    TestServer {
        base: format!("http://{address}"),
        handle,
    }
}
```

- [ ] **Step 2: Rewrite `request` to return status and text**

Replace the whole of the existing private `request` function
(`test_support.rs:50-73`) with:

```rust
/// Issue one request against a freshly started app and read the whole
/// response. `body` is a content type and the bytes to send with it.
async fn request(
    state: &AppState,
    method: reqwest::Method,
    path: &str,
    token: Option<&str>,
    body: Option<(&str, reqwest::Body)>,
) -> (StatusCode, String) {
    let server = serve(state).await;
    let mut req = reqwest::Client::new().request(method, format!("{}{path}", server.base()));
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    if let Some((content_type, body)) = body {
        req = req
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body);
    }
    let response = req.send().await.unwrap();
    let status = response.status();
    // Read the body before `server` drops and aborts the task.
    let text = response.text().await.unwrap();
    (status, text)
}

/// The JSON body every typed helper sends.
fn json_body(value: serde_json::Value) -> (&'static str, reqwest::Body) {
    (
        "application/json",
        reqwest::Body::from(serde_json::to_vec(&value).expect("test JSON always serializes")),
    )
}

/// Decode a response the caller expects to be `200 OK` with a JSON body.
fn expect_ok<T: DeserializeOwned>(what: &str, status: StatusCode, text: &str) -> T {
    assert_eq!(status, StatusCode::OK, "{what} must succeed, got: {text}");
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{what} returned non-JSON ({e}): {text}"))
}
```

- [ ] **Step 3: Route every helper through it**

Rewrite each public helper's body. The signatures do not change, so no call
site outside this file moves. Worked examples for one of each shape — apply
the same transformation to `get_status`, `get_json`, `get_raw`, `post_json`,
`post_status`, `post_raw`, `put_json`, `put_status`, `patch_json`,
`patch_status`, `delete_json`, `delete_status`, `delete_raw`,
`register_via_api`, and `login_status`:

```rust
/// GET a path with a Bearer token, returning only the status.
pub async fn get_status(state: &AppState, path: &str, token: &str) -> StatusCode {
    request(state, reqwest::Method::GET, path, Some(token), None)
        .await
        .0
}

/// GET a path with a Bearer token and decode the JSON body.
pub async fn get_json<T: DeserializeOwned>(state: &AppState, path: &str, token: &str) -> T {
    let (status, text) = request(state, reqwest::Method::GET, path, Some(token), None).await;
    expect_ok(&format!("GET {path}"), status, &text)
}

/// GET a path with a Bearer token, returning the status and the raw response
/// text. For asserting on a non-JSON or malformed body, such as the error
/// fallbacks' JSON that a plain `get_json` would panic decoding on failure.
pub async fn get_raw(state: &AppState, path: &str, token: &str) -> (StatusCode, String) {
    request(state, reqwest::Method::GET, path, Some(token), None).await
}

/// POST a JSON body with a Bearer token and decode the JSON response.
pub async fn post_json<T: DeserializeOwned>(
    state: &AppState,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> T {
    let (status, text) = request(
        state,
        reqwest::Method::POST,
        path,
        Some(token),
        Some(json_body(body)),
    )
    .await;
    expect_ok(&format!("POST {path}"), status, &text)
}

/// POST a body that is not JSON (JSONL, plain text, an empty body) with a
/// Bearer token and an explicit Content-Type, returning the status and the
/// response text. For routes whose contract is the raw body, such as
/// `POST /v1/import`.
pub async fn post_raw(
    state: &AppState,
    path: &str,
    token: &str,
    content_type: &str,
    body: impl Into<reqwest::Body>,
) -> (StatusCode, String) {
    request(
        state,
        reqwest::Method::POST,
        path,
        Some(token),
        Some((content_type, body.into())),
    )
    .await
}
```

`register_via_api` keeps its assertion and its field reads; only the transport
line changes:

```rust
pub async fn register_via_api(
    state: &AppState,
    username: &str,
    password: &str,
) -> RegisteredAccount {
    let (status, text) = request(
        state,
        reqwest::Method::POST,
        "/v1/auth/register",
        None,
        Some(json_body(
            serde_json::json!({ "username": username, "password": password }),
        )),
    )
    .await;
    let body: serde_json::Value = expect_ok("register", status, &text);
    RegisteredAccount {
        account_id: body["account_id"].as_str().unwrap().to_string(),
        username: body["username"].as_str().unwrap().to_string(),
        token: body["token"].as_str().unwrap().to_string(),
    }
}
```

- [ ] **Step 4: Update the module doc comment**

The header says each call "starts the real axum app on an ephemeral port,
issues one request, and shuts it down". That is still true, but say where:

```rust
//! Shared HTTP helpers for the server's own tests. [`serve`] is the one place
//! that binds a listener and spawns the app; every helper below issues one
//! request through it, reads the whole response, and lets the server drop.
```

- [ ] **Step 5: Write the large-body test**

Append to `test_support.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A body far larger than one TCP segment must come back whole. This
    /// pins the ordering in `request`: the response is read before the
    /// `TestServer` drops and aborts the task serving it.
    #[tokio::test]
    async fn a_large_response_body_is_read_before_the_server_stops() {
        let vault = test_vault().await;
        let user = register_via_api(&vault.state, "alice", "hunter2hunter2").await;
        for i in 0..300 {
            seed_conversation(
                &vault.state,
                &SeedConversation {
                    account_id: &user.account_id,
                    handle: &format!("+1555000{i:04}"),
                    conversation_type: "individual",
                    group_title: None,
                    source_file: "seed.jsonl",
                    messages: &[SeedMessage {
                        source: "imessage",
                        timestamp: "2020-01-01T00:00:00Z",
                        is_from_me: true,
                        body: "hello, this is a message long enough to add up",
                    }],
                },
            )
            .await;
        }

        let (status, text) = get_raw(&vault.state, "/v1/conversations?limit=300", &user.token).await;
        assert_eq!(status, StatusCode::OK, "{text}");
        assert!(
            text.len() > 64 * 1024,
            "the fixture must produce a body bigger than one segment, got {} bytes",
            text.len()
        );
        let page: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("truncated body ({e}): {} bytes", text.len()));
        assert_eq!(page["items"].as_array().unwrap().len(), 300);
    }
}
```

This test depends on `seed_conversation`, `SeedConversation`, and
`SeedMessage` from Task 3. Write it now but leave it commented out with a
`// Enabled in Task 3:` note, and uncomment it as Task 3's last step. Do not
invent a different seeder here.

- [ ] **Step 6: Run the whole suite**

Run: `cargo test -p message-vault-server`
Expected: PASS, with the same test count as before this task (the new test is
still commented out).

- [ ] **Step 7: Verify one bind survives**

Run: `grep -c 'TcpListener::bind' crates/vault/server/src/test_support.rs`
Expected: `1`

- [ ] **Step 8: Commit**

```bash
git add crates/vault/server/src/test_support.rs
git commit -m "test: one helper starts the test server, and the body is read before it stops"
```

---

## Task 2: The fixture makes accounts, and seven `setup()` functions go

**Files:**
- Modify: `crates/vault/server/src/test_support.rs` (add `conn`,
  `account_with_id`, `account`)
- Modify: `crates/vault/server/src/db/api_tokens.rs:398`,
  `crates/vault/server/src/named_membership.rs:722`,
  `crates/vault/server/src/db/trash.rs:150`,
  `crates/vault/server/src/profile.rs:443`,
  `crates/vault/server/src/db/account_profile.rs:528`,
  `crates/vault/server/src/contacts_api.rs:1290`,
  `crates/vault/server/src/db/saved_searches.rs:316`

**Interfaces:**
- Consumes: `TestVault` from Task 1's file.
- Produces:
  - `TestVault::conn(&self) -> sqlx::pool::PoolConnection<sqlx::Any>`
  - `TestVault::account_with_id(&self, id: &str, username: &str) -> String`
  - `TestVault::account(&self, username: &str) -> String`

These seven `setup()` functions all do the same three things: build a pool,
apply a schema, and insert one or two `accounts` rows. `test_vault()` already
does the first two.

- [ ] **Step 1: Write the failing test**

Append inside `test_support.rs`'s `mod tests`:

```rust
#[tokio::test]
async fn the_fixture_makes_an_account_with_the_id_a_test_asks_for() {
    let vault = test_vault().await;
    let id = vault
        .account_with_id("00000000-0000-4000-8000-00000000000f", "alice")
        .await;
    assert_eq!(id, "00000000-0000-4000-8000-00000000000f");

    let mut conn = vault.conn().await;
    let username: String = sqlx::query_scalar("SELECT username FROM accounts WHERE id = $1")
        .bind(&id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(username, "alice");

    let other = vault.account("bob").await;
    assert_ne!(other, id, "each account must get its own id");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p message-vault-server the_fixture_makes_an_account -- --nocapture`
Expected: FAIL to compile — no method `account_with_id` on `TestVault`.

- [ ] **Step 3: Add the three methods**

In `test_support.rs`, after the `TestVault` struct:

```rust
impl TestVault {
    /// A connection from this vault's pool, for a test that seeds or asserts
    /// with SQL directly.
    pub async fn conn(&self) -> sqlx::pool::PoolConnection<sqlx::Any> {
        self.state.db.acquire().await.unwrap()
    }

    /// Insert an `accounts` row with a chosen id, for a test that asserts on
    /// the id itself. Returns the id it was given, so a caller can bind the
    /// result rather than repeat the literal.
    pub async fn account_with_id(&self, id: &str, username: &str) -> String {
        let mut conn = self.conn().await;
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, $2)")
            .bind(id)
            .bind(username)
            .execute(&mut *conn)
            .await
            .unwrap();
        id.to_string()
    }

    /// Insert an `accounts` row under a generated id, for a test that only
    /// needs an account to exist.
    pub async fn account(&self, username: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.account_with_id(&id, username).await
    }
}
```

`uuid` is already a dependency of this crate with the `v4` feature
(`crates/vault/server/Cargo.toml:34`), so `Uuid::new_v4()` needs nothing
added.

- [ ] **Step 4: Run the test**

Run: `cargo test -p message-vault-server the_fixture_makes_an_account`
Expected: PASS

- [ ] **Step 5: Replace the seven `setup()` functions**

For each file below, delete its `async fn setup(...)` and rewrite each caller.
The mechanical shape, using `db/saved_searches.rs` as the worked example — its
old callers read `let (pool, _dir, account) = setup().await;` and then
`pool.acquire()`:

```rust
// Before
let (pool, _dir, account) = setup().await;
let mut conn = pool.acquire().await.unwrap();

// After
let vault = crate::test_support::test_vault().await;
let account = vault
    .account_with_id("00000000-0000-4000-8000-0000000000e1", "alice")
    .await;
let mut conn = vault.conn().await;
```

Keep each module's existing account-id constant and literal; the ids appear in
assertions and in `ORDER BY` expectations. The files and their ids:

| File | Old `setup()` behaviour to reproduce at the call site |
| --- | --- |
| `db/api_tokens.rs:398` | one account `aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa`, username `alice` |
| `named_membership.rs:722` | one account `00000000-0000-4000-8000-0000000000d9`, username `alice` |
| `db/trash.rs:150` | two accounts, `ACCOUNT_A` and `ACCOUNT_B`, each with its own id as its username |
| `profile.rs:443` | one account `00000000-0000-4000-8000-000000000001`, username `alice` |
| `db/account_profile.rs:528` | one account `ACCOUNT_ID`, username `Alice` |
| `contacts_api.rs:1290` | one account `00000000-0000-4000-8000-0000000000c1`, username `alice` |
| `db/saved_searches.rs:316` | one account `00000000-0000-4000-8000-0000000000e1`, username `alice` |

Two things to watch. `db/api_tokens.rs`'s `setup()` applies **only** the
accounts schema, while `test_vault()` applies the vault schema too — that is a
superset, so nothing breaks, but if a test there asserts a table is absent,
say so in the task report rather than working around it. And the `TempDir` the
old tuples carried is now owned by `TestVault`, so a test must hold `vault`
alive for as long as it holds a connection; binding it to `_vault` is wrong
because `_`-prefixed bindings still drop at end of scope but read as
throwaway — bind it to `vault`.

- [ ] **Step 6: Run the suite**

Run: `cargo test -p message-vault-server`
Expected: PASS, same test count as before.

- [ ] **Step 7: Check the grep**

Run: `grep -rn 'fn setup(' crates/vault/server/src`
Expected: only `db/schema.rs`, `export_api.rs`, and `conversations_api.rs`
remain. Those three seed conversations and are Task 3's work.

- [ ] **Step 8: Commit**

```bash
git add crates/vault/server/src
git commit -m "test: the fixture makes accounts, and seven setup functions go"
```

---

## Task 3: The fixture seeds conversations, and the last three `setup()` functions go

**Files:**
- Modify: `crates/vault/server/src/test_support.rs:305-342` (`seed_one_message`)
- Modify: `crates/vault/server/src/db/schema.rs:888`,
  `crates/vault/server/src/export_api.rs:345`,
  `crates/vault/server/src/conversations_api.rs:853`

**Interfaces:**
- Consumes: `TestVault`, `TestVault::conn` from Task 2.
- Produces:
  - `pub struct SeedMessage<'a> { pub source: &'a str, pub timestamp: &'a str, pub is_from_me: bool, pub body: &'a str }`
  - `pub struct SeedConversation<'a> { pub account_id: &'a str, pub handle: &'a str, pub conversation_type: &'a str, pub group_title: Option<&'a str>, pub source_file: &'a str, pub messages: &'a [SeedMessage<'a>] }`
  - `pub async fn seed_conversation(state: &AppState, c: &SeedConversation<'_>) -> i64` — returns the new `conversations.id`.
  - `seed_one_message` keeps its signature and becomes a call to it.

- [ ] **Step 1: Write the failing test**

Append inside `test_support.rs`'s `mod tests`:

```rust
#[tokio::test]
async fn the_seeder_returns_the_conversation_id_it_made() {
    let vault = test_vault().await;
    let account = vault.account("alice").await;
    let id = seed_conversation(
        &vault.state,
        &SeedConversation {
            account_id: &account,
            handle: "+15555550100",
            conversation_type: "group",
            group_title: Some("Book Club"),
            source_file: "backup-a.jsonl",
            messages: &[
                SeedMessage {
                    source: "imessage",
                    timestamp: "2020-01-01T00:00:00Z",
                    is_from_me: true,
                    body: "first",
                },
                SeedMessage {
                    source: "imessage",
                    timestamp: "2020-01-02T00:00:00Z",
                    is_from_me: false,
                    body: "second",
                },
            ],
        },
    )
    .await;

    let mut conn = vault.conn().await;
    let title: String = sqlx::query_scalar("SELECT group_title FROM conversations WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(title, "Book Club");

    let bodies: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM messages WHERE conversation_id = $1 ORDER BY sort_order",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .unwrap();
    assert_eq!(bodies, vec!["first".to_string(), "second".to_string()]);
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p message-vault-server the_seeder_returns_the_conversation_id`
Expected: FAIL to compile — `seed_conversation` not found.

- [ ] **Step 3: Write the seeder**

Replace `seed_one_message` (`test_support.rs:305-342`) with:

```rust
/// One message to seed into a conversation.
pub struct SeedMessage<'a> {
    /// The `messages.source` slug, such as `imessage`.
    pub source: &'a str,
    /// RFC 3339 timestamp, stored as text the way the importer writes it.
    pub timestamp: &'a str,
    /// Whether the account sent it.
    pub is_from_me: bool,
    /// The message text.
    pub body: &'a str,
}

/// A conversation to seed, with its messages in order.
pub struct SeedConversation<'a> {
    /// The account that owns it.
    pub account_id: &'a str,
    /// The peer handle, created as a `handles` row. Must be unique per
    /// account: `handles` is keyed on the normalized value.
    pub handle: &'a str,
    /// `individual` or `group`.
    pub conversation_type: &'a str,
    /// The group's title, for a group conversation.
    pub group_title: Option<&'a str>,
    /// The `conversations.source_file` value.
    pub source_file: &'a str,
    /// Messages, seeded with `sort_order` following this order.
    pub messages: &'a [SeedMessage<'a>],
}

/// Seed one conversation and its messages, returning the new
/// `conversations.id`.
///
/// `messages.conversation_id` and `conversations.chat_handle_id` are integer
/// foreign keys, so this first creates a `handles` row the way every real
/// importer does rather than binding a string straight into `chat_handle_id`.
pub async fn seed_conversation(state: &AppState, c: &SeedConversation<'_>) -> i64 {
    let mut conn = state.db.acquire().await.unwrap();
    let handle_id: i64 = sqlx::query_scalar(
        "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
         VALUES ($1, $2, $2, 'phone', 'phone') RETURNING id",
    )
    .bind(c.account_id)
    .bind(c.handle)
    .fetch_one(&mut *conn)
    .await
    .unwrap();

    let conversation_id: i64 = sqlx::query_scalar(
        "INSERT INTO conversations (
            account_id, chat_handle_id, conversation_type, group_title, source_file
         ) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(c.account_id)
    .bind(handle_id)
    .bind(c.conversation_type)
    .bind(c.group_title)
    .bind(c.source_file)
    .fetch_one(&mut *conn)
    .await
    .unwrap();

    for (index, message) in c.messages.iter().enumerate() {
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(conversation_id)
        .bind(c.account_id)
        .bind(message.source)
        .bind(message.timestamp)
        .bind(i64::from(message.is_from_me))
        .bind(index as i64)
        .bind(message.body)
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    conversation_id
}

/// Give an account one conversation holding one message, so counts are
/// non-zero.
pub async fn seed_one_message(state: &AppState, account_id: &str) {
    seed_conversation(
        state,
        &SeedConversation {
            account_id,
            handle: &format!("+1555{account_id}"),
            conversation_type: "individual",
            group_title: None,
            source_file: "seed.jsonl",
            messages: &[SeedMessage {
                source: "imessage",
                timestamp: "2020-01-01T00:00:00Z",
                is_from_me: true,
                body: "hello",
            }],
        },
    )
    .await;
}
```

Note the `handles` insert no longer uses `last_insert_rowid()`. `RETURNING id`
works on both SQLite and Postgres; `last_insert_rowid()` is SQLite-only and
`export_api.rs`'s old `setup()` used it, which is one reason that fixture could
not have been shared as it stood.

- [ ] **Step 4: Run the test**

Run: `cargo test -p message-vault-server the_seeder_returns_the_conversation_id`
Expected: PASS

- [ ] **Step 5: Replace the last three `setup()` functions**

Delete each and rewrite its callers with `test_vault()` plus
`account_with_id` plus `seed_conversation`. What each old fixture built:

- `db/schema.rs:888` — two accounts (`A1`/`alice`, `A2`/`bob`), each with one
  `handles` row `+15555550100` and one `individual` conversation from
  `t.json`. Read the rest of the old body before deleting it: it continues
  past the excerpt in this plan, and every row it inserts must be reproduced.
- `export_api.rs:345` — account `a1`/`alice`, then two conversations with
  explicit ids `1` and `2` on handles `+1555` and `+1666`. Tests there assert
  on those ids, so bind the ids the seeder returns to local variables and use
  the variables rather than re-hardcoding `1` and `2`. If any test compares a
  literal id, change the test to use the variable.
- `conversations_api.rs:853` — account `00000000-0000-4000-8000-0000000000c2`,
  and a conversation whose peer handle is created through
  `account_profile::link_account_handle`, not a raw insert. That call is
  load-bearing: it links the handle to the account profile, which the naming
  query reads. Keep it — call `link_account_handle` at the call site and then
  insert the conversation with `seed_conversation` only if the handle it makes
  matches; if the two cannot be reconciled, leave that one seeding step as an
  explicit SQL insert at the call site and say so in the task report. Do not
  silently drop `link_account_handle`.

- [ ] **Step 6: Uncomment Task 1's large-body test**

Remove the `// Enabled in Task 3:` comment and the comment markers from
`a_large_response_body_is_read_before_the_server_stops`.

- [ ] **Step 7: Run the suite and the grep**

Run: `cargo test -p message-vault-server`
Expected: PASS

Run: `grep -rn 'fn setup(' crates/vault/server/src`
Expected: no output.

- [ ] **Step 8: Commit**

```bash
git add crates/vault/server/src
git commit -m "test: one seeder builds conversations, and the last setup functions go"
```

---

## Task 4: Route-level tests for Export and the contacts offset ceiling

**Files:**
- Modify: `crates/vault/server/src/export_api.rs` (tests module)
- Modify: `crates/vault/server/src/contacts_api.rs` (tests module)

**Interfaces:**
- Consumes: `test_vault`, `register_via_api`, `seed_conversation`,
  `SeedConversation`, `SeedMessage`, `get_json`, `get_raw`, `get_status`.
- Produces: nothing other tasks consume.

The two conversation read routes already have route-level tests, added in
PR 4 (`conversations_api.rs:2205`, `:2225`, `:2247`, `:2666`, `:2824`,
`:2840`). Export has exactly one (`the_export_route_answers_a_page_and_refuses_a_bad_limit`);
everything else in `export_api.rs` calls the query builder directly. The
contacts list has no `offset` ceiling test, though the conversations list does
(`conversations_api.rs:2187`).

- [ ] **Step 1: Write the failing tests**

In `contacts_api.rs`'s tests module:

```rust
/// The conversations list refuses an offset past `MAX_LIST_OFFSET`
/// (conversations_api.rs). The contacts list shares `page_params` and must
/// answer the same way over HTTP.
#[tokio::test]
async fn the_contacts_route_refuses_an_offset_past_the_ceiling() {
    let vault = crate::test_support::test_vault().await;
    let user = crate::test_support::register_via_api(&vault.state, "alice", "hunter2hunter2").await;

    let (status, text) = crate::test_support::get_raw(
        &vault.state,
        "/v1/contacts?offset=50001",
        &user.token,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(body["error"].is_string(), "{body}");

    let ok = crate::test_support::get_status(
        &vault.state,
        "/v1/contacts?offset=50000",
        &user.token,
    )
    .await;
    assert_eq!(ok, axum::http::StatusCode::OK, "the ceiling itself is allowed");
}
```

In `export_api.rs`'s tests module:

```rust
/// The export route runs the search language, not a metadata subset. This
/// goes over HTTP rather than through the query builder, so a change to the
/// route's wiring is caught as well as a change to the compiler.
#[tokio::test]
async fn the_export_route_runs_the_search_language() {
    let vault = crate::test_support::test_vault().await;
    let user = crate::test_support::register_via_api(&vault.state, "alice", "hunter2hunter2").await;
    crate::test_support::seed_conversation(
        &vault.state,
        &crate::test_support::SeedConversation {
            account_id: &user.account_id,
            handle: "+15555550100",
            conversation_type: "individual",
            group_title: None,
            source_file: "backup-a.jsonl",
            messages: &[
                crate::test_support::SeedMessage {
                    source: "imessage",
                    timestamp: "2020-01-01T00:00:00Z",
                    is_from_me: true,
                    body: "pizza tonight",
                },
                crate::test_support::SeedMessage {
                    source: "imessage",
                    timestamp: "2020-01-02T00:00:00Z",
                    is_from_me: false,
                    body: "salad tomorrow",
                },
            ],
        },
    )
    .await;

    let page: serde_json::Value = crate::test_support::get_json(
        &vault.state,
        "/v1/export/messages?q=pizza&limit=10",
        &user.token,
    )
    .await;
    assert_eq!(page["total"], 1, "free text must match one message: {page}");
    assert_eq!(page["items"][0]["body"], "pizza tonight");

    let negated: serde_json::Value = crate::test_support::get_json(
        &vault.state,
        "/v1/export/messages?q=NOT%20pizza&limit=10",
        &user.token,
    )
    .await;
    assert_eq!(negated["total"], 1, "NOT must be honoured: {negated}");
    assert_eq!(negated["items"][0]["body"], "salad tomorrow");
}

/// An unknown field is a 400 with a sentence, not an empty page.
#[tokio::test]
async fn the_export_route_refuses_a_word_the_language_does_not_have() {
    let vault = crate::test_support::test_vault().await;
    let user = crate::test_support::register_via_api(&vault.state, "alice", "hunter2hunter2").await;

    let (status, text) = crate::test_support::get_raw(
        &vault.state,
        "/v1/export/messages?q=wibble:yes",
        &user.token,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(body["error"].is_string(), "{body}");
}

/// Export must never reach another account's messages, whatever the query.
#[tokio::test]
async fn the_export_route_does_not_leak_another_account() {
    let vault = crate::test_support::test_vault().await;
    let alice = crate::test_support::register_via_api(&vault.state, "alice", "hunter2hunter2").await;
    let bob = crate::test_support::register_via_api(&vault.state, "bob", "hunter2hunter2").await;
    crate::test_support::seed_one_message(&vault.state, &alice.account_id).await;

    let page: serde_json::Value =
        crate::test_support::get_json(&vault.state, "/v1/export/messages?q=&limit=50", &bob.token)
            .await;
    assert_eq!(page["total"], 0, "bob must see nothing of alice's: {page}");
    assert_eq!(page["items"].as_array().unwrap().len(), 0);
}
```

- [ ] **Step 2: Run them and watch which fail**

Run: `cargo test -p message-vault-server the_contacts_route_refuses_an_offset the_export_route_ -- --nocapture`
Expected: the two export behaviour tests and the leak test may already pass —
they assert behaviour that should already hold. The offset test is the one
that has never been asserted at the route.

If any of these fail, that is a real defect: stop and report it rather than
adjusting the assertion to match. The one to watch is the `NOT pizza` case —
if the compiler treats a bare `NOT` differently over HTTP than in the builder
tests, say so.

- [ ] **Step 3: Make them pass**

If all pass as written, this task adds coverage and changes no source. If the
offset test fails, `contacts_api.rs:1105` already passes
`Some(MAX_LIST_OFFSET)` to `page_params`, so the failure is in how the handler
reports the error — fix it there, not in the test.

- [ ] **Step 4: Run the suite**

Run: `cargo test -p message-vault-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src
git commit -m "test: export and the contacts offset ceiling are tested at the route"
```

---

## Task 5: Route-level tests for the asset PUT and GET

**Files:**
- Modify: `crates/vault/server/src/assets.rs` (tests module)

**Interfaces:**
- Consumes: `test_vault`, `register_via_api`, `post_raw`, `serve`.
- Produces: the coverage Task 9 needs before deleting
  `scripts/test/smoke-export-api.sh`.

`assets.rs` has fifteen tests and none of them goes over HTTP. The only thing
exercising `PUT /v1/assets/{sha256}` followed by `GET /v1/assets/{sha256}` is
`scripts/test/smoke-export-api.sh`, which nothing runs. Task 9 deletes that
script, so the coverage has to exist first.

- [ ] **Step 1: Write the failing test**

`test_support` has no PUT-with-raw-body helper, so add one next to `post_raw`
in `test_support.rs`:

```rust
/// PUT a body that is not JSON with a Bearer token and an explicit
/// Content-Type, returning the status and the response text. For routes whose
/// contract is the raw body, such as `PUT /v1/assets/{sha256}`.
pub async fn put_raw(
    state: &AppState,
    path: &str,
    token: &str,
    content_type: &str,
    body: impl Into<reqwest::Body>,
) -> (StatusCode, String) {
    request(
        state,
        reqwest::Method::PUT,
        path,
        Some(token),
        Some((content_type, body.into())),
    )
    .await
}
```

Then in `assets.rs`'s tests module:

```rust
#[tokio::test]
async fn an_asset_put_then_get_returns_the_same_bytes() {
    use sha2::{Digest, Sha256};

    let vault = crate::test_support::test_vault().await;
    let user = crate::test_support::register_via_api(&vault.state, "alice", "hunter2hunter2").await;

    let bytes = b"a small attachment".to_vec();
    let sha = format!("{:x}", Sha256::digest(&bytes));
    let path = format!("/v1/assets/{sha}?source=sms-backup-restore&account={}", user.username);

    let (status, text) = crate::test_support::put_raw(
        &vault.state,
        &path,
        &user.token,
        "application/octet-stream",
        bytes.clone(),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{text}");

    let server = crate::test_support::serve(&vault.state).await;
    let response = reqwest::Client::new()
        .get(format!("{}{path}", server.base()))
        .bearer_auth(&user.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let got = response.bytes().await.unwrap();
    assert_eq!(got.as_ref(), bytes.as_slice(), "the bytes must round-trip");
}

#[tokio::test]
async fn an_asset_get_for_an_unknown_sha_is_a_json_404() {
    let vault = crate::test_support::test_vault().await;
    let user = crate::test_support::register_via_api(&vault.state, "alice", "hunter2hunter2").await;

    let unknown = "0".repeat(64);
    let (status, text) = crate::test_support::get_raw(
        &vault.state,
        &format!("/v1/assets/{unknown}?source=sms-backup-restore&account={}", user.username),
        &user.token,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND, "{text}");
    let body: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|_| panic!("non-JSON body: {text}"));
    assert!(body["error"].is_string(), "{body}");
}
```

Before writing these, read `assets.rs:688-860` for the real query parameters
and required headers of both routes and correct the paths above to match. The
`?source=…&account=…` shape is copied from `scripts/test/smoke-export-api.sh`
and may be stale. `sha2` is already a dependency (`crates/vault/server/Cargo.toml:27`), so
`Sha256::digest` needs nothing added.

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p message-vault-server an_asset_ -- --nocapture`
Expected: FAIL — `put_raw` does not exist yet, and once it does, whichever of
the two routes is wired differently from the smoke script's assumption.

- [ ] **Step 3: Make them pass**

Add `put_raw`. Correct the test paths against the real route definitions. Do
not change `assets.rs` source unless a test finds a genuine defect; if it
does, fix it and say so in the report.

- [ ] **Step 4: Run the suite**

Run: `cargo test -p message-vault-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src
git commit -m "test: an asset round-trips through PUT and GET at the route"
```

---

## Task 6: A body over the limit is 413, not 400

**Files:**
- Modify: `crates/vault/server/src/server.rs:809-870`
  (`read_body_limited`, `discard_body`, `stream_body_to_file`)
- Test: `crates/vault/server/src/assets.rs` (tests module)

**Interfaces:**
- Consumes: `ApiError::Status` (`server.rs:294`), which exists precisely so a
  status the caller already picked is not flattened to 400.
- Produces: nothing other tasks consume.

All three helpers answer their own oversize check with
`ApiError::BadRequest("request body too large")`. ADR-0005 says the status
carries the meaning, and 413 is the status that means this. Axum's own `Json`
extractor already answers 413 for the same condition
(`extract.rs:70`, tested at `extract.rs:179`), so today the same failure
answers two different statuses depending on which route read the body.

**Which of the three is reachable, and why it matters for the test.** Read
this before writing anything. `discard_body` and `stream_body_to_file` are
both called with `state.max_body_bytes` (`assets.rs:832`, `:850`,
`import/mod.rs:1444`) — the *same* value `RequestBodyLimitLayer` is built with
in `http_app`. The layer wraps the router from outside and enforces on the
streamed body as well as on `Content-Length`, so at an identical threshold it
always answers first: those two inner checks cannot be reached over HTTP at
all. `read_body_limited` is different. Its one caller
(`assets.rs:1009`, the chunked-upload part route) passes
`state.upload_limits.part_size`, which defaults to 1 MiB against the layer's
512 MiB. That is the one reachable path, and it is what the test drives.

Fix all three anyway. Two are defensive today, the limits can diverge
tomorrow, and leaving two of three answering the wrong status is how the
inconsistency came about in the first place. Say plainly in the task report
that only one is covered by a test and why — do not imply the test proves all
three.

- [ ] **Step 1: Write the failing test**

In `assets.rs`'s tests module, beside Task 5's tests, which is where the
working `?source=…&account=…` shape for these routes has been established:

```rust
/// A part body past `upload_limits.part_size` is a 413. This is the one
/// oversize check reachable over HTTP: the layer limit is `max_body_bytes`
/// (512 MiB by default) and the part limit is far smaller, so the handler's
/// own check is what answers. ADR-0005: the status carries the meaning.
#[tokio::test]
async fn an_upload_part_over_the_part_size_is_a_json_413() {
    let vault = crate::test_support::test_vault().await;
    let mut state = vault.state.clone();
    // `UploadLimits` is `Copy` and `part_size` is public, so a test can lower
    // it without rebuilding the config.
    state.upload_limits.part_size = 16;
    let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;

    let sha = "0".repeat(64);
    let (status, text) = crate::test_support::put_raw(
        &state,
        &format!(
            "/v1/assets/{sha}/uploads/upload-1/parts/1?source=sms-backup-restore&account={}",
            user.username
        ),
        &user.token,
        "application/octet-stream",
        vec![b'x'; 4096],
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::PAYLOAD_TOO_LARGE,
        "a part over part_size must be 413, got: {text}"
    );
    let body: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|_| panic!("non-JSON body: {text}"));
    assert_eq!(
        body["error"], "request body too large",
        "the sentence must be the handler's own, proving the layer did not answer: {body}"
    );
}
```

Two things to check as you write it. `resolve_asset_lookup` runs *before*
`read_body_limited`, so the `source` and `account` parameters have to be ones
that resolve — copy exactly what Task 5's passing test uses, and if that test
needed a registered source or a prior `PUT`, do the same here. And the
`upload-1` upload id need not exist: the size check happens before
`asset_uploads::put_part` is ever called. If the route rejects the unknown
upload id first, create a session through
`POST /v1/assets/{sha}/uploads` and use the id it returns.

The sentence assertion is load-bearing. `RequestBodyLimitLayer`'s own 413 is
rewritten by `json_body_limit_response` into `"the request body is too
large"`; the handler's is `"request body too large"`. Asserting the exact
sentence is what proves this test exercises the code the task changes.

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p message-vault-server an_upload_part_over_the_part_size -- --nocapture`
Expected: FAIL with `400 Bad Request` where `413 Payload Too Large` was
expected.

- [ ] **Step 3: Change the three helpers**

In `server.rs`, replace each of the three oversize returns:

```rust
// read_body_limited, ~line 818
if out.len().saturating_add(chunk.len()) > max_bytes {
    return Err(ApiError::Status(
        StatusCode::PAYLOAD_TOO_LARGE,
        "request body too large".into(),
    ));
}
```

```rust
// discard_body, ~line 836
if seen > max_body_bytes {
    return Err(ApiError::Status(
        StatusCode::PAYLOAD_TOO_LARGE,
        "request body too large".into(),
    ));
}
```

```rust
// stream_body_to_file, ~line 865
if written > max_body_bytes as u64 {
    return Err(ApiError::Status(
        StatusCode::PAYLOAD_TOO_LARGE,
        "request body too large".into(),
    ));
}
```

Leave the `failed to read body: {e}` mappings as `BadRequest` — a broken
connection is not an oversize body.

- [ ] **Step 4: Run the test**

Run: `cargo test -p message-vault-server an_upload_part_over_the_part_size`
Expected: PASS

- [ ] **Step 5: Regenerate the OpenAPI document**

Any route whose documented responses list 400 for an oversize body now also
answers 413. The upload-part route (`assets.rs:975-995`) definitely does; add
`(status = 413, body = crate::server::ErrorBody)` to its `responses(...)`.
Check the other asset routes (`assets.rs:688`, `:723`, `:802`, `:918`,
`:1029`) and `import_handler` (`import/mod.rs:1395-1410`) and add it wherever
the route can answer 413 — including the ones where only the layer can
produce it, since a client sees the same status either way.

Run: `./scripts/generate-openapi.sh` if it exists; otherwise find how
`docs/src/assets/openapi.json` is produced —
`grep -rn 'openapi.json' scripts/ .github/workflows/` — and run that. Then
regenerate `web/src/lib/vaultApi.types.ts` the same way.

- [ ] **Step 6: Run the suite**

Run: `cargo test -p message-vault-server`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/vault/server/src docs/src/assets/openapi.json web/src/lib/vaultApi.types.ts
git commit -m "fix: a request body over the limit answers 413, not 400"
```

---

## Task 7: A multipart rejection keeps its status

**Files:**
- Modify: `crates/vault/server/src/import/mod.rs:1435-1437`
- Test: `crates/vault/server/src/assets.rs` (tests module)

**Interfaces:**
- Consumes: `ApiError::Status`.
- Produces: nothing other tasks consume.

`import_handler` maps every multipart rejection to
`ApiError::BadRequest(format!("invalid multipart body: {e}"))`. The body shape
is already right — it is `{error}` — but the status is flattened, so a
multipart body over the limit answers 400 where `extract::Json` answers 413
for the identical condition.

No new extractor belongs in `extract.rs` for this. The three wrappers there
exist because handlers name them in their argument lists; `import_handler`
takes `request: Request` and only converts to `Multipart` after inspecting the
Content-Type, so there is nothing for an extractor to hook. Fix it where it is
and match `extract.rs`'s mapping exactly.

- [ ] **Step 1: Write the failing test**

In `import/mod.rs`'s tests module:

```rust
/// A malformed multipart body keeps the status Axum picked, the way
/// `extract::Json` does. ADR-0005: the status carries the meaning.
#[tokio::test]
async fn a_malformed_multipart_body_answers_axums_status_as_json() {
    let vault = crate::test_support::test_vault().await;
    let user = crate::test_support::register_via_api(&vault.state, "alice", "hunter2hunter2").await;

    // A multipart Content-Type with no boundary parameter: Axum's
    // `Multipart` extractor rejects it before reading a byte.
    let (status, text) = crate::test_support::post_raw(
        &vault.state,
        "/v1/import?source=imessage&mode=append",
        &user.token,
        "multipart/form-data",
        "not really multipart",
    )
    .await;
    let body: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|_| panic!("non-JSON body: {text}"));
    assert!(body["error"].is_string(), "{body}");
    assert!(
        !body["error"]
            .as_str()
            .unwrap()
            .starts_with("invalid multipart body:"),
        "the handler must pass Axum's own sentence through, not wrap it: {body}"
    );
    assert_eq!(
        status,
        axum::http::StatusCode::BAD_REQUEST,
        "a missing boundary is Axum's 400: {text}"
    );
}
```

The status this particular input produces is Axum's choice, not ours — run the
test first and read what Axum actually answers, then assert that. If it is not
400, correct the assertion to the observed status and say so in the report:
the point of the test is that the handler passes Axum's status through, and
the sentence assertion is what proves it.

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p message-vault-server a_malformed_multipart_body -- --nocapture`
Expected: FAIL on the sentence assertion — the error starts with
`invalid multipart body:`.

- [ ] **Step 3: Pass the rejection through**

In `import/mod.rs`, replace lines 1435-1437:

```rust
let multipart = Multipart::from_request(request, &state)
    .await
    // Axum already picked the right status (413 over the body limit, 400 for
    // a missing boundary); keep it rather than flattening everything to 400,
    // exactly as `extract::Json` does.
    .map_err(|e| ApiError::Status(e.status(), e.body_text()))?;
```

If `status()` or `body_text()` is not found on `MultipartRejection`, the
methods live behind `axum::extract::rejection`; import what the compiler names
rather than reaching for a different mapping.

- [ ] **Step 4: Run the test**

Run: `cargo test -p message-vault-server a_malformed_multipart_body`
Expected: PASS

- [ ] **Step 5: Add 413 to the import route's documented responses**

If Task 6 has not already added it, `import_handler`'s `responses(...)` needs
`(status = 413, body = crate::server::ErrorBody)`. Regenerate
`docs/src/assets/openapi.json` and `web/src/lib/vaultApi.types.ts`.

- [ ] **Step 6: Run the suite**

Run: `cargo test -p message-vault-server`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/vault/server/src docs/src/assets/openapi.json web/src/lib/vaultApi.types.ts
git commit -m "fix: a multipart rejection keeps the status axum picked"
```

---

## Task 8: The fast 413 carries CORS headers

**Files:**
- Modify: `crates/vault/server/src/server.rs:504-506`
- Test: `crates/vault/server/src/server.rs` (tests module)

**Interfaces:**
- Consumes: `serve` and `TestServer::base` from Task 1, because this test sets
  an `Origin` header no helper takes.
- Produces: nothing other tasks consume.

Today `http_app` layers, from innermost outward:

```rust
.layer(build_cors_layer(&cors_origins))          // innermost of the three
.layer(RequestBodyLimitLayer::new(state.max_body_bytes))
.layer(axum::middleware::map_response(json_body_limit_response));  // outermost
```

`.layer()` applied later wraps the outside, so the body-limit layer sits
**outside** CORS. When it answers its own 413 from a `Content-Length` header,
that response never passes through the CORS layer and carries no
`Access-Control-Allow-Origin`. A browser then shows a CORS failure instead of
the 413 the vault actually sent, which is the worst of both: the person sees
neither the real status nor the real sentence.

The fix is to reorder so CORS is outermost. Note the auth router's own
32 KiB `RequestBodyLimitLayer` (`limited_auth_router`) is merged into the
router before any of these, so it is already inside CORS — which is why
`extract.rs:179`'s existing 413 test passes today and does not catch this.

- [ ] **Step 1: Write the failing test**

In `server.rs`'s tests module:

```rust
/// `RequestBodyLimitLayer` answers its own 413 the moment a `Content-Length`
/// announces an oversize body, without running any handler. That response
/// must still pass through the CORS layer, or a browser reports a CORS
/// failure instead of showing the 413 the vault sent.
#[tokio::test]
async fn the_fast_413_carries_cors_headers() {
    let vault = crate::test_support::test_vault().await;
    let mut state = vault.state.clone();
    state.max_body_bytes = 1024;
    let user = crate::test_support::register_via_api(&state, "alice", "hunter2hunter2").await;

    let server = crate::test_support::serve(&state).await;
    let response = reqwest::Client::new()
        .post(format!("{}/v1/import?source=imessage&mode=append", server.base()))
        .bearer_auth(&user.token)
        .header(header::ORIGIN, "https://app.example")
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        // A sized body, so the limit layer answers from Content-Length alone.
        .body(vec![b'x'; 4096])
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(
        response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        "the fast 413 must carry CORS headers, got: {:?}",
        response.headers()
    );
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"].is_string(), "{body}");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p message-vault-server the_fast_413_carries_cors_headers -- --nocapture`
Expected: FAIL on the header assertion. If it passes, stop: the premise in
this task is wrong and the layer order already works. Report that rather than
reordering layers for no reason — the ordering claim came from a review, not
from a test, and a review can be wrong.

- [ ] **Step 3: Reorder the layers**

In `http_app`:

```rust
        .method_not_allowed_fallback(api_method_not_allowed)
        .fallback_service(ServeDir::new("static"))
        .layer(RequestBodyLimitLayer::new(state.max_body_bytes))
        // Rewrite the limit layer's plain-text 413 into `{error}` before CORS
        // sees it, so the response a browser gets is both JSON and CORS-clean.
        .layer(axum::middleware::map_response(json_body_limit_response))
        // Outermost: every response, including one the limit layer answered
        // itself, carries the CORS headers a browser needs to show it.
        .layer(build_cors_layer(&cors_origins));
```

- [ ] **Step 4: Run the test and the neighbours**

Run: `cargo test -p message-vault-server the_fast_413_carries_cors_headers a_json_body_over_the_auth_router_body_limit`
Expected: PASS, both. The second is the existing 413 test at
`extract.rs:179`; the reorder must not break it.

- [ ] **Step 5: Run the suite**

Run: `cargo test -p message-vault-server`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/vault/server/src/server.rs
git commit -m "fix: the body-limit 413 passes through the CORS layer"
```

---

## Task 9: The three unrun smoke scripts go

**Files:**
- Delete: `scripts/test/smoke-vault-push.sh`,
  `scripts/test/smoke-import-api.sh`, `scripts/test/smoke-export-api.sh`,
  `scripts/test/fixtures/smoke-sms-attachment.jsonl`,
  `scripts/test/fixtures/smoke-sms-text.jsonl` — the whole `scripts/test/`
  tree
- Modify: `docs/src/content/docs/vault/developer/reference/api.md`,
  `docs/adr/0001-no-command-line-except-the-vault-server.md` if either names a
  deleted script

**Interfaces:**
- Consumes: the route-level coverage from Tasks 4 and 5.
- Produces: closes issue #273.

Issue #273 asks for a decision between renaming the script and wiring it into
CI, wiring it in unchanged, or deleting it. The answer is delete, and the same
answer covers its two neighbours, which are in exactly the same position.

The reasoning, for the commit message and the issue comment. All three scripts
build the server, start it, and drive the HTTP API with `curl`. Nothing runs
them: no workflow in `.github/workflows/`, not `scripts/check-pr.sh`, no other
script. `smoke-vault-push.sh` does not touch `vault-push` at all, and since
the command-line retirement there is no `vault-push` binary for it to touch —
so the thing its name promises cannot exist. What the three actually cover —
import over JSONL, export paging and filtering, and an asset round-trip —
is now covered by tests that run on every push: `import/mod.rs`'s tests,
Task 4's export tests, and Task 5's asset tests.

What is genuinely lost, and it is worth writing down rather than glossing:
the scripts start the real binary against a real config file, so they cover
`serve()`'s own wiring, config loading, and the `static/` directory. The
in-process tests drive `http_app` and cover none of that. That gap is real,
but it is a gap either way — a script nobody runs covers nothing. If it
should be closed, it should be closed by something CI runs, and that is not
this pull request.

- [ ] **Step 1: Confirm nothing runs them**

Run:

```bash
grep -rn 'scripts/test' .github/workflows/ scripts/ Makefile* 2>/dev/null
```

Expected: no output. If anything does reference them, stop — the premise is
wrong and this task needs rethinking rather than the reference deleting.

- [ ] **Step 2: Confirm the coverage exists**

Run: `cargo test -p message-vault-server import export asset`
Expected: PASS, and the run includes Task 4's and Task 5's tests by name.

- [ ] **Step 3: Delete the tree**

```bash
git rm -r scripts/test
```

- [ ] **Step 4: Fix the prose that names them**

Run: `grep -rn 'smoke-vault-push\|smoke-import-api\|smoke-export-api\|scripts/test' docs/ AGENTS.md CLAUDE.md`

For each hit, remove the reference or replace it with the test that now covers
that ground. Do not leave a docs page pointing at a deleted file. Plan
documents under `docs/superpowers/plans/` that record what a past pull request
did are history and stay as they are; the roadmap
(`2026-09-03-http-interface-repair-roadmap.md`) is updated separately after
this pull request merges, so leave it alone here too.

- [ ] **Step 5: Run the docs build**

Run: `cd docs && npm run check && npm run build`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "test: delete three smoke scripts nothing ran

Closes #273."
```

- [ ] **Step 7: Comment on the issue**

After the pull request merges, comment on #273 with the reasoning from this
task's preamble, including what the deletion gives up. The `Closes #273` in
the commit will close it; the comment is what makes the decision findable.

---

## Task 10: Whole-branch verification

**Files:** none.

- [ ] **Step 1: The grep the roadmap names**

Run: `grep -rn 'fn setup(' crates/vault/server/src`
Expected: no output.

Run: `grep -c 'TcpListener::bind' crates/vault/server/src/test_support.rs`
Expected: `1`

- [ ] **Step 2: Both database engines**

Run: `cargo test -p message-vault-server`
Expected: PASS

The Postgres job in CI runs the same suite against Postgres. The seeder's
`RETURNING id` works on both; `last_insert_rowid()` would not, which is why it
is gone. If any test still uses it, find it now:

Run: `grep -rn 'last_insert_rowid' crates/vault/server/src`

- [ ] **Step 3: The full gate**

Run: `./scripts/check-pr.sh`
Expected: exit 0

Run: `./scripts/lint-all.sh`
Expected: exit 0 (Clippy is not gated in CI; this catches what CI would not)

- [ ] **Step 4: Open the pull request**

Push the branch, open it against main, and wait for CI. The body says what
changed and why in plain English, names each of the four carried-over items
and how it was resolved, and records anything a task's report flagged that was
left undone.

---

## Self-Review

**Spec coverage.** The spec's section asks for `test_vault()` (exists),
`test_vault_http()` (Task 1's `serve()` — see the note below), the eleven
`setup()` functions replaced (Tasks 2 and 3; there are ten, not eleven), and
new or rewritten routes tested through HTTP (Tasks 4 and 5). The roadmap's
four carried-over items are Tasks 6, 7, 8, and 9. The roadmap's "contacts test
covers `offset` above `MAX_LIST_OFFSET`" is Task 4.

**One deliberate departure from the spec.** The spec names
`test_vault_http()` — "the router on top" — which reads as one long-lived
server per vault, with the helpers hanging off it. This plan instead extracts
`serve(state)`, which is what the roadmap's "Done when" actually requires
("one `serve(state)` helper backs every raw and JSON HTTP helper"), and keeps
every helper's `&AppState` signature. The reason is proportion: moving to a
per-vault server means rewriting roughly 250 call sites across thirteen files
to method syntax, which buys speed rather than correctness and is not what
either document asks for. If the reviewer disagrees, the change is mechanical
and can be its own pull request.

**A note on Task 4's likely outcome.** Three of its four tests assert
behaviour that should already hold, so they may well pass on the first run.
That is a coverage task, not a bug hunt, and the plan says so rather than
implying every task must turn something red. The offset test is the one with a
real chance of failing.

**Type consistency.** `SeedConversation`/`SeedMessage`/`seed_conversation` are
defined in Task 3 and used in Tasks 1 (after uncommenting), 3, and 4 with the
same field names. `TestServer::base()` is defined in Task 1 and used in Tasks
5 and 8. `put_raw` is defined in Task 5 and used only there.
`TestVault::conn`/`account`/`account_with_id` are defined in Task 2 and used
in Tasks 2, 3, and 4. `ApiError::Status` is existing code, used in Tasks 6
and 7.
