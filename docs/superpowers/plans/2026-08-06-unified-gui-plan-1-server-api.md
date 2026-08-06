# Unified GUI — Plan 1: Server API

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add new REST endpoints to message-vault-rs needed by the unified GUI: auth mode discovery, offset-based message pagination, and import history listing.

**Architecture:** Three new endpoints on the existing axum server. Auth mode is read from a new `AUTH_MODE` environment variable (defaults to `"local"`). Offset pagination is added to `ExportPageOpts` alongside the existing cursor. Import history reads from the existing `vault_imports` table.

**Tech Stack:** Rust, axum, rusqlite (existing message-vault-rs stack)

## Global Constraints

- Rust edition 2024 (match existing workspace)
- All new endpoints go through the existing `server.rs` router and `ApiError` type
- Auth mode is server-side only — the client discovers it via the endpoint
- Offset pagination is additive — existing cursor-based pagination continues to work
- Import history is read-only (GET), uses existing `resolve_auth` for Bearer token auth

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/server.rs` | Register new routes, wire to handlers |
| `src/config.rs` | Add `AuthMode` enum and parse from env |
| `src/export_api.rs` | Add `offset` to `ExportPageOpts`, support in query |
| `src/db/vault_imports.rs` | Add `list_imports()` query function |

---

### Task 1: Auth mode endpoint

**Files:**
- Modify: `src/config.rs` — add `AuthMode` enum, env parsing
- Modify: `src/server.rs` — register `GET /v1/auth/mode` route, add handler

**Interfaces:**
- Produces: `GET /v1/auth/mode` → `200 {"mode": "hanko" | "local"}`
- Consumes: `AUTH_MODE` environment variable (default `"local"`)

- [ ] **Step 1: Add AuthMode to config.rs**

In `src/config.rs`, add after the `Config` struct:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Hanko,
    Local,
}

impl AuthMode {
    pub fn from_env() -> Self {
        match std::env::var("AUTH_MODE").unwrap_or_default().to_lowercase().as_str() {
            "hanko" => AuthMode::Hanko,
            _ => AuthMode::Local,
        }
    }
}
```

Add `use serde::Serialize;` to the imports if not already present.

- [ ] **Step 2: Add auth_mode handler in server.rs**

Add after the `health` handler (around line 253):

```rust
/// Returns the server's configured authentication mode so clients
/// can render the correct login form before authenticating.
async fn auth_mode_handler() -> Json<serde_json::Value> {
    let mode = crate::config::AuthMode::from_env();
    Json(serde_json::json!({
        "mode": match mode {
            crate::config::AuthMode::Hanko => "hanko",
            crate::config::AuthMode::Local => "local",
        }
    }))
}
```

- [ ] **Step 3: Register the route**

In `src/server.rs`, add the route near the other GET routes (after `/health`):

```rust
.route("/v1/auth/mode", get(auth_mode_handler))
```

Add a startup log line near the existing `eprintln!` block:

```rust
eprintln!("  GET  /v1/auth/mode     (unauthenticated — returns hanko or local)");
```

- [ ] **Step 4: Build and verify**

```bash
cargo build -p message-vault-rs
```

Expected: compiles cleanly.

- [ ] **Step 5: Manual test**

Start the server and curl the endpoint:

```bash
# With default (local)
curl http://localhost:5556/v1/auth/mode
# Expected: {"mode":"local"}

# With Hanko
AUTH_MODE=hanko cargo run -- serve
curl http://localhost:5556/v1/auth/mode
# Expected: {"mode":"hanko"}
```

- [ ] **Step 6: Commit**

```bash
git add src/config.rs src/server.rs
git commit -m "feat(api): add GET /v1/auth/mode endpoint

Returns {'mode':'hanko'} or {'mode':'local'} based on AUTH_MODE env var.
Unauthenticated — clients call this before rendering the login form.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Offset-based message pagination

**Files:**
- Modify: `src/export_api.rs` — add `offset` field to `ExportPageOpts`, pass through to SQL
- Modify: `src/server.rs` — add `offset` query parameter to export handler

**Interfaces:**
- Produces: `GET /v1/export/messages?q=&offset=0&limit=50` returns the same `ExportMessagesResponse` shape
- Consumes: existing `export_messages()` function signature gains `offset: Option<usize>`

- [ ] **Step 1: Add offset to ExportPageOpts**

In `src/export_api.rs`, modify the `ExportPageOpts` struct:

```rust
#[derive(Debug, Clone)]
pub struct ExportPageOpts<'a> {
    pub account_id: &'a str,
    pub query: &'a str,
    pub limit: usize,
    pub offset: Option<usize>,
    pub cursor: Option<&'a str>,
    pub source_override: Option<&'a str>,
}
```

- [ ] **Step 2: Thread offset through export_messages**

Find the SQL query builder in `export_messages()`. The function already constructs a dynamic SQL query from the search query. Add an `OFFSET` clause when `opts.offset` is `Some` and no cursor is provided:

```rust
// After the existing WHERE clause assembly, before ORDER BY / LIMIT:
if let Some(offset) = opts.offset {
    if opts.cursor.is_none() {
        query.push_str(&format!(" OFFSET {}", offset));
    }
}
```

Offset and cursor are mutually exclusive — if a cursor is provided, cursor-based pagination takes precedence.

- [ ] **Step 3: Add offset query param to the handler**

In `src/server.rs`, find the `ExportMessagesQuery` struct (around line 730) and add:

```rust
offset: Option<usize>,
```

In `export_messages_handler`, thread it through:

```rust
let offset = query.offset;
// ...
let opts = ExportPageOpts {
    account_id: &account_id,
    query: &query.q,
    limit,
    offset,
    cursor: cursor.as_deref(),
    source_override: query.account.as_deref(),
};
```

- [ ] **Step 4: Build and verify**

```bash
cargo build -p message-vault-rs
```

Expected: compiles cleanly.

- [ ] **Step 5: Manual test**

```bash
# Page 1 (messages 0-49)
curl -H "Authorization: Bearer <token>" \
  "http://localhost:5556/v1/export/messages?q=&offset=0&limit=50"

# Page 2 (messages 50-99)
curl -H "Authorization: Bearer <token>" \
  "http://localhost:5556/v1/export/messages?q=&offset=50&limit=50"
```

- [ ] **Step 6: Commit**

```bash
git add src/export_api.rs src/server.rs
git commit -m "feat(api): add offset-based pagination to export messages

Offset and cursor are mutually exclusive — cursor takes precedence.
Enables Fastmail-style 'messages 1-50 of 1,423' navigation.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Import history endpoint

**Files:**
- Modify: `src/db/vault_imports.rs` — add `list_imports()` and `ImportSummary` struct
- Modify: `src/server.rs` — register `GET /v1/imports`, add handler

**Interfaces:**
- Produces: `GET /v1/imports` → `200 { "imports": [ImportSummary] }`
- Consumes: existing `vault_imports` SQLite table via `resolve_auth`

- [ ] **Step 1: Define ImportSummary and list_imports**

In `src/db/vault_imports.rs`, add:

```rust
#[derive(Debug, Serialize)]
pub struct ImportSummary {
    pub id: String,
    pub source: String,
    pub tool: Option<String>,
    pub mode: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub message_count: u64,
    pub conversation_count: u64,
    pub duplicate_count: u64,
    pub attachment_count: u64,
    pub total_bytes: u64,
}

pub fn list_imports(conn: &rusqlite::Connection, account_id: &str) -> anyhow::Result<Vec<ImportSummary>> {
    let mut stmt = conn.prepare(
        "SELECT i.id, i.source, i.tool, i.mode, i.created_at, i.completed_at,
                COUNT(m.id) as message_count,
                COUNT(DISTINCT m.conversation_id) as conversation_count,
                COUNT(DISTINCT CASE WHEN m.duplicate_of IS NOT NULL THEN m.id END) as duplicate_count,
                COUNT(DISTINCT a.sha256) as attachment_count,
                COALESCE(SUM(a.size_bytes), 0) as total_bytes
         FROM vault_imports i
         LEFT JOIN messages m ON m.import_id = i.id
         LEFT JOIN message_attachments ma ON ma.message_id = m.id
         LEFT JOIN assets a ON a.sha256 = ma.sha256
         WHERE i.account_id = ?
         GROUP BY i.id
         ORDER BY i.created_at DESC
         LIMIT 100"
    )?;
    let rows = stmt.query_map([account_id], |row| {
        Ok(ImportSummary {
            id: row.get(0)?,
            source: row.get(1)?,
            tool: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            mode: row.get(3)?,
            created_at: row.get(4)?,
            completed_at: row.get::<_, Option<String>>(5)?,
            message_count: row.get(6)?,
            conversation_count: row.get(7)?,
            duplicate_count: row.get(8)?,
            attachment_count: row.get(9)?,
            total_bytes: row.get(10)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
```

> **Note:** The actual SQL column names in `vault_imports` may differ from the spec above. Read the existing schema in `src/db/schema.rs` and adjust the column names and JOIN logic to match. The intent is: list recent import sessions with aggregate stats from linked messages and attachments.

- [ ] **Step 2: Add handler in server.rs**

```rust
#[derive(Deserialize)]
struct ListImportsQuery {
    account: Option<String>,
}

async fn imports_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListImportsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let account = resolve_import_account(&auth, query.account.as_deref(), &state.cfg.paths.db).await?;

    let db = Arc::clone(&state.db);
    let imports = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database mutex poisoned"))?;
        crate::db::vault_imports::list_imports(&conn, &account)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("list imports task: {e}")))?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "imports": imports })))
}
```

- [ ] **Step 3: Register the route**

In `src/server.rs`, add near the other `/v1/imports` routes:

```rust
.route("/v1/imports", get(imports_list_handler))
```

> **Important:** There is already a `POST /v1/imports` route. axum differentiates by method, so `GET /v1/imports` and `POST /v1/imports` coexist on the same path without conflict.

Add startup log line:

```rust
eprintln!("  GET  /v1/imports       (list past import sessions with stats)");
```

- [ ] **Step 4: Build and verify**

```bash
cargo build -p message-vault-rs
```

Expected: compiles cleanly. Fix any column name mismatches against the actual schema.

- [ ] **Step 5: Manual test**

```bash
curl -H "Authorization: Bearer <token>" \
  "http://localhost:5556/v1/imports"
# Expected: {"imports": [...]} with past import sessions
```

- [ ] **Step 6: Commit**

```bash
git add src/db/vault_imports.rs src/server.rs
git commit -m "feat(api): add GET /v1/imports endpoint for import history

Returns chronological list of past import sessions with aggregate stats:
message count, conversation count, duplicates, attachments, data size.

Co-Authored-By: Claude <noreply@anthropic.com>"
```
