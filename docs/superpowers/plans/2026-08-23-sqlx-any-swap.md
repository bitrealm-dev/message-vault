# sqlx Any Swap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace rusqlite with sqlx's `Any` driver in `crates/vault/server` so one binary serves SQLite for self-hosted installs and Postgres for hosted deployments, with full-text search parity between the engines.

**Architecture:** One `sqlx::AnyPool` per server process replaces the current `Arc<StdMutex<rusqlite::Connection>>`. The engine is chosen at startup from a connection URL (`sqlite://…` default derived from `paths.db`, `postgres://…` for hosting). SQLite behavior stays byte-identical (same DDL, same FTS5, same pragma set via connect options). Postgres gets an engine-branched DDL variant (IDENTITY keys, `lower()` unique indexes), a `search_tsv` tsvector + GIN index maintained by trigger twins of the SQLite FTS triggers, and an engine branch in the query compiler translating the FTS AST to `to_tsquery`/`phraseto_tsquery`. A committed search-parity corpus asserts identical result id sets on both engines.

**Tech Stack:** Rust 2024, sqlx 0.8 (`runtime-tokio`, `any`, `sqlite`, `postgres`, `tls-rustls`), tokio (already a dependency), Axum 0.8 (unchanged), SQLite FTS5 + Postgres `tsvector`/`simple` config.

**Spec:** https://github.com/bitrealm-io/message-vault/issues/148 (the ticket; its "Full-text search across engines" section is normative for Task 8 and Task 9).

## Global Constraints

- Work only inside `crates/vault/server/` (plus `schema/sql/`, `tests/fixtures/search/`, `.github/workflows/ci.yml`, and `AGENTS.md`/`CLAUDE.md` doc lines). Do not touch exporters, `src-tauri/`, `web/`, or `web-next/`.
- Never bump the product version (`0.7.3`) and never create or push tags.
- Never commit to `main`; work on a branch. Intermediate sweep commits (Tasks 2–6) are checkpoints and may not compile — that is expected and stated per task; the branch is only PR-ready after Task 10.
- CI gates: rustfmt, workspace build + test, Biome, Vitest. Clippy is not gated; run `./scripts/lint-all.sh` locally in Task 10.
- Tests use committed fixtures under `tests/fixtures/`; never commit real message data. All new db tests use the `test_pool()` helper from Task 1.
- `rusqlite` remains in `imessage-ir-exporter` (vendor backup reader) — only the server crate drops it.
- `vendor/sqlx-sqlite/` is a byte-identical copy of upstream sqlx-sqlite 0.8.6 with one manifest line changed (libsqlite3-sys `0.30.1` → `0.38.0`) so the whole workspace unifies on one native SQLite bindings version (cargo's `links` rule allows only one per graph; rusqlite 0.40/crabapple 0.4.7 need 0.38, released sqlx caps below it). Wired via `[patch.crates-io]` in the workspace root. Never edit fork source beyond that manifest line; re-vendor per `VENDORING.md` on sqlx upgrades. Upstream license: MIT OR Apache-2.0.
- Timestamps stay `TEXT` columns read as `String`; do not introduce chrono decoding types.

---

### Task 1: Foundation — vendored sqlx-sqlite fork, sqlx dependency, engine detection, pool opening, test helper

**Files:**
- Create: `vendor/sqlx-sqlite/` (all source files copied verbatim from the sqlx-sqlite 0.8.6 crates.io release; keep `LICENSE-MIT`, `LICENSE-APACHE`)
- Modify: `vendor/sqlx-sqlite/Cargo.toml` (materialize workspace-inherited keys + the one-line libsqlite3-sys bump)
- Create: `VENDORING.md`
- Modify: `Cargo.toml` (workspace root — `[patch.crates-io]` entry)
- Modify: `crates/vault/server/Cargo.toml`
- Create: `crates/vault/server/src/db/engine.rs`
- Modify: `crates/vault/server/src/db/mod.rs` (add `pub mod engine;`)

**Interfaces:**
- Produces (used by Tasks 2–9):
  - `DbEngine` — `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum DbEngine { Sqlite, Postgres }`
  - `pub fn detect_engine(url: &str) -> anyhow::Result<DbEngine>`
  - `pub async fn open_pool_for_path(path: &Path) -> anyhow::Result<sqlx::AnyPool>`
  - `pub async fn open_pool_from_url(url: &str) -> anyhow::Result<sqlx::AnyPool>`
  - `#[cfg(test)] pub(crate) async fn test_pool() -> (sqlx::AnyPool, tempfile::TempDir)` — file-backed SQLite pool; the `TempDir` must be held by the test for the pool's lifetime.

- [ ] **Step 1: Vendor the sqlx-sqlite fork**

1. Download `https://static.crates.io/crates/sqlx-sqlite/sqlx-sqlite-0.8.6.crate`, unpack, and copy its contents into `vendor/sqlx-sqlite/` (keep `LICENSE-MIT`, `LICENSE-APACHE`, `.cargo_vcs_info.json`).
2. Edit `vendor/sqlx-sqlite/Cargo.toml` so the diff against the upstream manifest is exactly this (no other changes, no source edits):

```diff
5,9c5,9
< version.workspace = true
< license.workspace = true
< edition.workspace = true
< authors.workspace = true
< repository.workspace = true
---
> version = "0.8.6"
> license = "MIT OR Apache-2.0"
> edition = "2021"
> # authors removed in vendored copy
> # repository removed in vendored copy
39,41c39,41
< chrono = { workspace = true, optional = true }
< time = { workspace = true, optional = true }
< uuid = { workspace = true, optional = true }
---
> chrono = { version = "0.4", optional = true }
> time = { version = "0.3", optional = true }
> uuid = { version = "1", optional = true }
59c59
< version = "0.30.1"
---
> version = "0.38.0"
68c68
< workspace = true
---
> version = "0.8.6"
71,74c71
< sqlx = { workspace = true, default-features = false, features = ["macros", "runtime-tokio", "tls-none", "sqlite"] }
<
< [lints]
< workspace = true
---
> sqlx = { version = "0.8", default-features = false, features = ["macros", "runtime-tokio", "tls-none", "sqlite"] }
```

   (Line 59 is the `[dependencies.libsqlite3-sys]` entry — the one substantive change. Line 68 is `sqlx-core`'s version.)
3. Create `VENDORING.md` at the repo root:

```markdown
# Vendored sqlx-sqlite fork

`vendor/sqlx-sqlite/` is a byte-identical copy of the `sqlx-sqlite` 0.8.6
source from crates.io, with one change: `libsqlite3-sys` is bumped from
`0.30.1` to `0.38.0` so the workspace unifies on a single native SQLite
bindings version (rusqlite 0.40 / crabapple 0.4.7 use 0.38). Cargo's
`links` rule permits only one libsqlite3-sys per dependency graph; without
this bump, sqlx 0.8 (0.30) and rusqlite 0.40 (0.38) cannot coexist.
Released sqlx 0.9 does not help either (its range caps below 0.38).

## Re-vendoring on sqlx upgrades

1. Note the new `sqlx-sqlite` version from the lockfile after the sqlx
   bump, then download that version's `.crate` tarball from crates.io.
2. Unpack it over `vendor/sqlx-sqlite/`, then re-apply the manifest
   changes: materialize the workspace-inherited keys (version, license,
   edition), set `libsqlite3-sys` to the rusqlite-matched release
   (`0.38.0` unless rusqlite moved), pin `sqlx-core` to the matching
   version, and drop the `[lints] workspace = true` block.
3. Run the full verification suite — the dual-engine tests are the gate
   for this combination (upstream tests sqlx-sqlite only against its own
   libsqlite3-sys pin).
4. Never edit fork source beyond the manifest. Upstream license:
   MIT OR Apache-2.0 (both license files stay in the vendor dir).
```

4. Add to the workspace root `Cargo.toml` (under `[workspace]` members):

```toml
[patch.crates-io]
sqlx-sqlite = { path = "vendor/sqlx-sqlite" }
```

5. Run `cargo check -p message-vault-server` — expected: resolves on one libsqlite3-sys (0.38) with rusqlite 0.40 still present; no `links` error. Also run `cargo test -p imessage-ir-exporter` — expected: 8/8 pass (baseline unchanged by the patch).

- [ ] **Step 2: Add the sqlx dependency**

In `crates/vault/server/Cargo.toml`, add to `[dependencies]` (keep `rusqlite` for now — Tasks 2–6 remove it):

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "any", "sqlite", "postgres", "tls-rustls"] }
```

Run `cargo check -p message-vault-server` — expected: builds (new dep resolves, nothing uses it yet).

- [ ] **Step 3: Write failing tests for engine detection**

Create `src/db/engine.rs` with just the module doc and empty test module, add the module to `db/mod.rs`, then write these tests (they fail to compile/run until Step 5):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_engine_from_scheme() {
        assert_eq!(detect_engine("sqlite://data/vault.db").unwrap(), DbEngine::Sqlite);
        assert_eq!(detect_engine("sqlite:///abs/path.db").unwrap(), DbEngine::Sqlite);
        assert_eq!(detect_engine("postgres://u:p@h/db").unwrap(), DbEngine::Postgres);
        assert_eq!(detect_engine("postgresql://h/db").unwrap(), DbEngine::Postgres);
        assert!(detect_engine("mysql://h/db").is_err());
        assert!(detect_engine("not-a-url").is_err());
    }

    #[tokio::test]
    async fn opens_sqlite_pool_and_applies_pragmas() {
        let (pool, _dir) = test_pool().await;
        // foreign_keys must be ON (was PRAGMA foreign_keys = ON today)
        let on: i64 = sqlx::query_scalar("SELECT foreign_keys FROM pragma_foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(on, 1);
        // The pool is usable for real work.
        sqlx::query("CREATE TABLE t1 (id INTEGER PRIMARY KEY, v TEXT)")
            .execute(&pool)
            .await
            .unwrap();
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p message-vault-server db::engine`
Expected: FAIL (missing functions).

- [ ] **Step 5: Implement engine detection and pool opening**

Use this module verbatim — it is the validated implementation (2/2 tests pass; the `AnyConnectOptions::from(SqliteConnectOptions)` conversion used by earlier drafts does not exist in any sqlx release, which is why the pragma set rides an `after_connect` hook and SQLite options become a URL via `to_url_lossy`):

```rust
//! Database engine detection and pool construction.

use std::path::Path;
use std::str::FromStr;

use anyhow::{Result, bail};
use sqlx::any::{AnyConnectOptions, AnyPoolOptions};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{AnyPool, ConnectOptions};

/// Which database engine a connection URL selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbEngine {
    Sqlite,
    Postgres,
}

/// Resolve the engine from a connection URL scheme.
pub fn detect_engine(url: &str) -> Result<DbEngine> {
    let scheme = url.split("://").next().unwrap_or("");
    match scheme {
        "sqlite" | "sqlite-file" => Ok(DbEngine::Sqlite),
        "postgres" | "postgresql" => Ok(DbEngine::Postgres),
        _ => bail!("unsupported database URL scheme {scheme:?} (want sqlite:// or postgres://)"),
    }
}

/// The vault's historical pragma set, applied to each new connection:
/// busy timeout first (overlapping auth and UI writes wait), foreign keys on,
/// synchronous NORMAL, temp_store MEMORY, cache_size -200000.
fn with_vault_pragmas(pool: AnyPoolOptions) -> AnyPoolOptions {
    pool.after_connect(|conn, _meta| {
        Box::pin(async move {
            sqlx::query("PRAGMA busy_timeout = 15000").execute(&mut *conn).await?;
            sqlx::query("PRAGMA foreign_keys = ON").execute(&mut *conn).await?;
            sqlx::query("PRAGMA synchronous = NORMAL").execute(&mut *conn).await?;
            sqlx::query("PRAGMA temp_store = MEMORY").execute(&mut *conn).await?;
            sqlx::query("PRAGMA cache_size = -200000").execute(&mut *conn).await?;
            Ok(())
        })
    })
}

fn sqlite_url_from_path(path: &Path) -> String {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .to_url_lossy()
        .to_string()
}

fn sqlite_pool_options() -> AnyPoolOptions {
    with_vault_pragmas(AnyPoolOptions::new().max_connections(4))
}

/// Open the configured pool for a SQLite file. WAL is best-effort, exactly
/// like today's `configure_connection`: a hot rollback journal or another
/// process holding the database can make it fail, and callers still get a
/// usable pool.
pub async fn open_pool_for_path(path: &Path) -> Result<AnyPool> {
    let pool = sqlite_pool_options()
        .connect_with(AnyConnectOptions::from_str(&sqlite_url_from_path(path))?)
        .await?;
    match sqlx::query("PRAGMA journal_mode = WAL").execute(&pool).await {
        Ok(_) => {}
        Err(err) => {
            eprintln!("warning: could not enable write-ahead logging ({err}); continuing without it");
        }
    }
    Ok(pool)
}

/// Open a pool from a user-supplied connection URL (`sqlite://…` or
/// `postgres://…`). The scheme selects the engine.
pub async fn open_pool_from_url(url: &str) -> Result<AnyPool> {
    let engine = detect_engine(url)?;
    if engine == DbEngine::Sqlite {
        return sqlite_pool_options()
            .connect_with(AnyConnectOptions::from_str(url)?)
            .await
            .map_err(Into::into);
    }
    Ok(AnyPoolOptions::new().max_connections(4).connect(url).await?)
}

/// Shared test pool: file-backed SQLite in a fresh temp dir.
#[cfg(test)]
pub(crate) async fn test_pool() -> (AnyPool, tempfile::TempDir) {
    sqlx::any::install_default_drivers();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.db");
    let pool = sqlite_pool_options()
        .connect_with(AnyConnectOptions::from_str(&sqlite_url_from_path(&path)).unwrap())
        .await
        .unwrap();
    (pool, dir)
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p message-vault-server db::engine`
Expected: PASS (2 tests).

- [ ] **Step 7: Commit**

```bash
git add vendor/ VENDORING.md Cargo.toml Cargo.lock crates/vault/server/Cargo.toml crates/vault/server/src/db/engine.rs crates/vault/server/src/db/mod.rs
git commit -m "feat: vendor sqlx-sqlite fork and add engine/pool plumbing (#148)"
```

---

### Task 2: DDL application — split_ddl, engine-branched ensure, introspection, Postgres DDL variants

This task ports `db/schema.rs` and `db/sql.rs`. It is the first task of the compile cascade: after this task, `cargo check` will fail across the crate — the *only* remaining rusqlite errors must be in the files listed in Tasks 3–6 (use `cargo check -p message-vault-server 2>&1 | grep -oP 'src/[a-z_/]+\.rs' | sort -u` and compare against the per-task file lists; fix nothing outside them).

**Files:**
- Modify: `crates/vault/server/src/db/schema.rs` (full port)
- Modify: `crates/vault/server/src/db/sql.rs` (full port)
- Create: `crates/vault/server/src/db/dialect.rs`
- Modify: `crates/vault/server/src/db/mod.rs` (add `pub mod dialect;`)
- Create: `schema/sql/pg_messages.sql`, `schema/sql/pg_accounts.sql`, `schema/sql/pg_contacts.sql`, `schema/sql/pg_staging.sql` (Postgres DDL variants; see Step 4)

**Interfaces (exact signatures; used by all later tasks):**
- `pub fn split_ddl(batch: &str) -> Vec<String>`
- `pub async fn ensure_vault_schema(conn: &mut AnyConnection) -> Result<()>`
- `pub async fn ensure_accounts_schema(conn: &mut AnyConnection) -> Result<()>`
- `pub(crate) async fn drop_messages_fts_triggers(conn: &mut AnyConnection) -> Result<()>`
- `pub(crate) async fn install_messages_fts_triggers(conn: &mut AnyConnection) -> Result<()>`
- `pub(crate) async fn drop_messages_secondary_indexes(conn: &mut AnyConnection) -> Result<()>`
- `pub(crate) async fn create_messages_secondary_indexes(conn: &mut AnyConnection) -> Result<()>`
- `pub(crate) async fn index_messages_fts_from_promote_map(conn: &mut AnyConnection, min_new_message_id: i64) -> Result<u64>`
- `pub fn delete_messages_for_source(conn: &mut AnyConnection, account_id: &str, source: &str) -> Result<u64>` — becomes async (Task 2 keeps the signature above but all db fns are `async`)
- `pub async fn reset_staging_for_account(conn: &mut AnyConnection, account_id: &str) -> Result<()>`
- `pub fn table_exists(conn: &mut AnyConnection, name: &str) -> Result<bool>` — async
- `pub fn like_ci(engine: DbEngine) -> &'static str` (fragment form emitting `?` — ONLY for Task 5's renumber-pass fragments), `pub fn like_ci_numbered(engine: DbEngine, n: usize) -> String` (literal form emitting `$N` — for hand-numbered SQL in Tasks 3/4), and `pub fn now_utc_sql(engine: DbEngine) -> &'static str` in `dialect.rs`

**Conversion recipes (normative for Tasks 2–6).** Every db-layer function changes shape in exactly this way:

| rusqlite today | sqlx Any replacement |
|---|---|
| `fn f(conn: &Connection, …) -> Result<T>` | `async fn f(conn: &mut AnyConnection, …) -> Result<T>` |
| `conn.execute(sql, params![a, b])?` (usize) | `sqlx::query(sql).bind(a).bind(b).execute(&mut *conn).await?.rows_affected()` (u64) |
| `conn.query_row(sql, params![…], \|row\| row.get(0))?` | `sqlx::query_scalar::<_, T>(sql).bind(a).bind(b).fetch_one(&mut *conn).await?` |
| `…query_row(…).optional()?` | `sqlx::query_scalar::<_, T>(sql).bind(a).fetch_optional(&mut *conn).await?` (returns `Option<T>`) |
| `stmt.query_map(params![…], \|row\| Ok((row.get(0)?, row.get(1)?)))` | `sqlx::query_as::<_, (i64, String, …)>(sql).bind(a).fetch_all(&mut *conn).await?` (tuples up to 16 elements implement `FromRow`) |
| `conn.last_insert_rowid()` | `INSERT … RETURNING id` + `sqlx::query_scalar::<_, i64>(…).fetch_one(&mut *conn).await?` (both engines support RETURNING) |
| `INSERT OR IGNORE INTO …` | `INSERT INTO … ON CONFLICT DO NOTHING` |
| `INSERT OR REPLACE INTO …` | `INSERT INTO … ON CONFLICT(col) DO UPDATE SET …` (target column required) |
| `IFNULL(a, b)` / `MAX` guards | `COALESCE(a, b)` (portable; rewrite all 8 sites) |
| `LIKE ? COLLATE NOCASE` | `dialect::like_ci_numbered(engine, n)` in hand-numbered SQL (engine from `AppState.db_engine` in API layers / `conn.backend_name()` in db modules); `dialect::like_ci(engine)` fragment form only inside Task 5's renumber-pass fragments |
| `datetime('now')` | Rust: `chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()` (matches SQLite's format) or `dialect::now_utc_sql(engine)` inside SQL |
| `conn.transaction()?` | `let mut tx = conn.begin().await?;` … `tx.commit().await?;` — pass `&mut *tx` where a fn takes `&mut AnyConnection` (`Transaction` derefs to `AnyConnection`) |
| `conn.execute_batch(batch)` | `for stmt in split_ddl(batch) { sqlx::query(stmt).execute(&mut *conn).await?; }` |
| `prepare()` + loop `ins.execute(params![…])` | `sqlx::query(sql).bind(…).execute(&mut *conn).await?` inside the loop (no prepared-statement caching) |
| `params_from_iter` / dynamic bind lists | hand-number `$N` placeholders (counter in the loop) + chain `.bind()` calls in order; see the placeholder-discipline note below |

**Placeholder discipline (verified against sqlx-core 0.8.6 source — normative for Tasks 2–6):** sqlx's Any driver performs no placeholder rewriting and `?` is invalid on Postgres (it is the JSONB operator); sqlx-sqlite 0.8.6 parses `$NNN`, so **`$N` is the only portable placeholder**. Fixed-shape SQL literals use hand-numbered `$1, $2, …`. `sqlx::QueryBuilder::<Any>` is unusable (its `push_bind` emits `?` via the Arguments trait default with no Any override). Dynamic fragment builders (Task 5's export compiler) instead keep `?` inside fragments and run one shared `renumber_placeholders()` pass over the final joined SQL before execution, binding heterogeneous values through a small `SqlParam` enum (`Text(String) | Int(i64) | Bool(bool) | Null`) chained onto the query in order — `sqlx::any::AnyValue` is not user-constructible, so this enum is the dynamic-bind carrier. `in_placeholders`/`pair_placeholders` in `db/sql.rs` become `$N`-aware (emit numbered ranges from a starting index) or are replaced by loop counters at call sites.

- [ ] **Step 1: Write failing tests for `split_ddl`**

Add to `db/schema.rs` a `#[cfg(test)]` module test (uses only `split_ddl`, no db):

```rust
#[test]
fn split_ddl_keeps_trigger_bodies_intact() {
    let create = include_str!("../../../../../schema/sql/fts_triggers_create.sql");
    let drop = include_str!("../../../../../schema/sql/fts_triggers_drop.sql");
    let fts = include_str!("../../../../../schema/sql/fts_virtual.sql");
    assert_eq!(split_ddl(create).len(), 6, "six sync triggers");
    assert_eq!(split_ddl(drop).len(), 6);
    assert_eq!(split_ddl(fts).len(), 1);
    for stmt in split_ddl(create) {
        assert!(stmt.starts_with("CREATE TRIGGER"), "unexpected split: {stmt}");
    }
    // A statement is never empty and never ends mid-line.
    for stmt in split_ddl(include_str!("../../../../../schema/sql/messages.sql")) {
        assert!(stmt.ends_with(';'), "statement must end with ;: {stmt}");
        assert!(stmt.starts_with("CREATE"), "unexpected split: {stmt}");
    }
}

#[test]
fn split_ddl_skips_comments_and_blanks() {
    let out = split_ddl("-- header\nCREATE TABLE a (x INTEGER);\n\nCREATE TABLE b (y INTEGER);\n");
    assert_eq!(out, vec!["CREATE TABLE a (x INTEGER);", "CREATE TABLE b (y INTEGER);"]);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p message-vault-server db::schema::tests::split_ddl`
Expected: FAIL (function missing).

- [ ] **Step 3: Implement `split_ddl` and `dialect.rs`**

`split_ddl` (put it at the bottom of `db/schema.rs` before the tests):

```rust
/// Split a multi-statement DDL batch into individual statements.
///
/// The schema files follow a fixed format: comments are whole `--` lines,
/// ordinary statements end with `;` at end of line, and trigger bodies are
/// the only multi-line statements (each ends with a line ending in `END;`).
pub fn split_ddl(batch: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_trigger = false;
    for line in batch.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        if trimmed.starts_with("CREATE TRIGGER") {
            in_trigger = true;
        }
        current.push_str(line);
        current.push('\n');
        if in_trigger {
            if trimmed.ends_with("END;") {
                statements.push(current.trim_end().to_string());
                current.clear();
                in_trigger = false;
            }
        } else if trimmed.ends_with(';') {
            statements.push(current.trim_end().to_string());
            current.clear();
        }
    }
    debug_assert!(
        current.trim().is_empty(),
        "unterminated DDL statement: {current}"
    );
    statements
}

/// Run every statement in a DDL batch against one connection.
async fn execute_batch(conn: &mut AnyConnection, batch: &str) -> Result<()> {
    for stmt in split_ddl(batch) {
        sqlx::query(&stmt).execute(&mut *conn).await?;
    }
    Ok(())
}
```

`db/dialect.rs`:

```rust
//! SQL dialect helpers for queries that cannot be written portably.

use crate::db::engine::DbEngine;

/// Case-insensitive substring match (`%term%` patterns).
pub fn like_ci(engine: DbEngine) -> &'static str {
    match engine {
        DbEngine::Sqlite => "LIKE ? COLLATE NOCASE",
        DbEngine::Postgres => "ILIKE ?",
    }
}

/// Current timestamp in the format SQLite's `datetime('now')` produces
/// (`YYYY-MM-DD HH:MM:SS`, UTC), so both engines write identical values.
pub fn now_utc_sql(engine: DbEngine) -> &'static str {
    match engine {
        DbEngine::Sqlite => "datetime('now')",
        DbEngine::Postgres => "to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')",
    }
}

/// Engine for a live connection, for db-module code that has no `DbEngine` in scope.
pub fn engine_of(conn: &sqlx::AnyConnection) -> DbEngine {
    if conn.backend_name() == "PostgreSQL" {
        DbEngine::Postgres
    } else {
        DbEngine::Sqlite
    }
}
```

- [ ] **Step 4: Write the Postgres DDL variants**

Create the four `schema/sql/pg_*.sql` files. Rules, applied to the corresponding `schema/sql/*.sql` file (line numbers refer to the current files):

1. Every `id INTEGER PRIMARY KEY` becomes `id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY` (`BY DEFAULT` because the import path inserts explicit ids). Sites: `messages.sql:4,25,43,108,143,163`; `staging.sql:4,23,38,87,122`; `contacts.sql:4,18,56`; `accounts.sql:105,144`.
2. `username TEXT NOT NULL UNIQUE COLLATE NOCASE` (`accounts.sql:6`) becomes `username TEXT NOT NULL`, plus `CREATE UNIQUE INDEX IF NOT EXISTS ix_accounts_username_ci ON accounts (lower(username));`. Same treatment for `email` (`accounts.sql:24`) → `CREATE UNIQUE INDEX IF NOT EXISTS ix_account_emails_email_ci ON account_emails (lower(email));`.
3. `last_modified TEXT NOT NULL DEFAULT (datetime('now'))` (`contacts.sql:9`) becomes `last_modified TEXT NOT NULL DEFAULT (to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS'))`.
4. Everything else (columns, FKs, `UNIQUE(…)` table constraints, partial indexes with `WHERE`, `TEXT`/`INTEGER` types, `DEFAULT '0'`) copies verbatim.
5. Skip every `CREATE VIRTUAL TABLE` / FTS statement — the Postgres FTS twin is Task 8, not here.

- [ ] **Step 5: Port `db/schema.rs`**

Convert every function to the Interfaces list above, using the recipe table. Specifics:

- `ensure_vault_schema`: `execute_batch` per DDL constant; the DDL constant used depends on engine — `let pg = dialect::engine_of(conn) == DbEngine::Postgres;` then `execute_batch(conn, if pg { PG_MESSAGES_DDL } else { MESSAGE_TABLES_DDL }).await?` etc., where the PG constants are the `pg_*.sql` includes (`const PG_MESSAGES_DDL: &str = include_str!("../../../../../schema/sql/pg_messages.sql");`). The initial `PRAGMA foreign_keys = ON;` line disappears (it is a connect option now). `migrate_contact_labels_to_groups` and `ensure_messages_fts`'s SQLite path run only when `!pg`; `ensure_messages_fts` on Postgres is a no-op for now (Task 8 fills it).
- `migrate_contact_labels_to_groups`: SQLite-only (returns early on Postgres). Replace the two `PRAGMA foreign_keys` lines with nothing (foreign keys stay on; the rename sequence is atomic per statement, which is fine — it was already run statement-by-statement).
- `ensure_column`: uses `PRAGMA table_info` — replace with a portable helper:

```rust
/// True when `table` has `column` on this engine.
async fn column_exists(conn: &mut AnyConnection, table: &str, column: &str) -> Result<bool> {
    let pg = dialect::engine_of(conn) == DbEngine::Postgres;
    let found: i64 = if pg {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns
             WHERE table_name = ?1 AND column_name = ?2",
        )
        .bind(table)
        .bind(column)
        .fetch_one(&mut *conn)
        .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2")
            .bind(table)
            .bind(column)
            .fetch_one(&mut *conn)
            .await?
    };
    Ok(found > 0)
}
```

- `table_exists`: SQLite branch keeps `sqlite_master`; Postgres branch uses `SELECT COUNT(*) FROM pg_catalog.pg_tables WHERE tablename = ?1`. Add `index_exists`/`trigger_exists`/`table_columns` helpers with the same branch shape (needed by the schema-contract test in Step 6).
- `drop_messages_fts_triggers`/`install_messages_fts_triggers`: keep the SQLite statements byte-identical (Postgres branch is Task 8). Note `install_messages_fts_triggers`'s `INSERT OR REPLACE INTO schema_meta` becomes `INSERT INTO schema_meta (key, value) VALUES (?1, '1') ON CONFLICT(key) DO UPDATE SET value = excluded.value`.
- `index_messages_fts_from_promote_map`: SQLite branch keeps the `group_concat` SQL byte-identical (Postgres branch is Task 8).
- `delete_messages_for_source`: the `format!`-composed `IN (SELECT …)` subqueries are already portable; only the API conversion applies.

- [ ] **Step 6: Port the `db/schema.rs` tests**

Convert `mod tests` to `#[tokio::test]` + `test_pool()`:
- `fn setup()` becomes `async fn setup() -> (AnyPool, TempDir)` calling `test_pool()`, then `ensure_vault_schema(&mut conn).await` with `let mut conn = pool.acquire().await.unwrap();`.
- Every `conn.execute(params![…])` → `sqlx::query(…).bind(…).execute(&mut *conn).await.unwrap();`; `last_insert_rowid()` → `RETURNING id` fetch.
- The schema-contract test switches its introspection to the new `table_exists`/`index_exists`/`trigger_exists`/`table_columns` helpers; keep `tests/fixtures/schema/current-schema.json` unchanged.
- `messages_fts_stays_in_sync` keeps `SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH ?1` with `sqlx::query_scalar::<_, i64>(…).bind(term).fetch_one(…)`.

- [ ] **Step 7: Port `db/sql.rs`**

Keep `in_placeholders`/`pair_placeholders` (string helpers, still used). Replace `fold_in_id_chunks` with this shape (call sites are converted in Task 4/5 — for now port the function and let call sites break loudly):

```rust
/// Run `query_chunk` on successive slices of `ids` and group results by id.
/// Each chunk keeps binds under the engine bind limit; `SQLITE_IN_CHUNK` (400)
/// stays as the chunk size for both engines.
pub async fn fold_in_id_chunks<T, E>(
    conn: &mut AnyConnection,
    ids: &[i64],
    mut query_chunk: impl FnMut(&mut AnyConnection, &[i64]) -> Result<Vec<(i64, T)>, E>,
) -> Result<HashMap<i64, Vec<T>>, E> {
    let mut map = HashMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    for chunk in ids.chunks(SQLITE_IN_CHUNK) {
        for (id, row) in query_chunk(conn, chunk)? {
            map.entry(id).or_default().push(row);
        }
    }
    Ok(map)
}
```

- [ ] **Step 8: Commit (checkpoint — crate does not compile)**

```bash
git add crates/vault/server/src/db/ schema/sql/pg_*.sql
git commit -m "refactor(server): port schema DDL application to sqlx with Postgres variants (#148)"
```

Then run `cargo check -p message-vault-server 2>&1 | grep -oP 'src/[a-z_/]+\.rs' | sort -u` and confirm every error file belongs to the Task 3–6 lists (API modules, import, server, auth, guest, reset_demo). Fix nothing else.

---

### Task 3: Domain db modules — contacts, handles, account_profile, api_tokens, session_tokens, vault_imports

**Files:**
- Modify: `crates/vault/server/src/db/contacts.rs`, `db/handles.rs`, `db/account_profile.rs`, `db/api_tokens.rs`, `db/session_tokens.rs`, `db/vault_imports.rs`
- Modify: `crates/vault/server/src/contacts.rs` (the address-book loader in `src/`, not `src/db/`) — it has 8 rusqlite references and is ported with the same recipes

**Interfaces:**
- Consumes: `test_pool`, `ensure_vault_schema`, `ensure_accounts_schema`, `like_ci`, `engine_of`, `split_ddl`/`execute_batch` (Task 2).
- Produces: nothing new — every public fn keeps its name and return type, changing only `&Connection` → `&mut AnyConnection` + `async`. Callers (Tasks 4–6) rely on these names unchanged.

- [ ] **Step 1: Port each module with the recipe table (Task 2), file by file**

Worked examples from `db/contacts.rs` (before → after):

```rust
// BEFORE
pub fn touch_contact(conn: &Connection, account_id: &str, contact_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE contacts SET last_modified = datetime('now')
         WHERE id = ?1 AND account_id = ?2",
        params![contact_id, account_id],
    )?;
    Ok(())
}
```

```rust
// AFTER
pub async fn touch_contact(conn: &mut AnyConnection, account_id: &str, contact_id: i64) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query("UPDATE contacts SET last_modified = $1 WHERE id = $2 AND account_id = $3")
        .bind(now)
        .bind(contact_id)
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}
```

```rust
// BEFORE
let found: Option<i64> = conn.query_row(sql, params![account_id, handle_id], |row| row.get(0)).optional()?;
```

```rust
// AFTER ($N placeholders — hand-numbered per the placeholder discipline)
let found: Option<i64> = sqlx::query_scalar::<_, i64>(sql)
    .bind(account_id)
    .bind(handle_id)
    .fetch_optional(&mut *conn)
    .await?;
```

```rust
// BEFORE (in insert_contact_drafts: tx.execute + tx.last_insert_rowid)
tx.execute("INSERT INTO contacts (account_id, preferred_name) VALUES (?1, ?2)", params![account_id, preferred_name])?;
let contact_id = tx.last_insert_rowid();
```

```rust
// AFTER
let contact_id: i64 = sqlx::query_scalar(
    "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
)
.bind(account_id)
.bind(preferred_name)
.fetch_one(&mut *tx)
.await?;
```

Per-file gotchas (from a full grep of the source):
- `db/contacts.rs`: `INSERT OR IGNORE` ×3 (`handles`, `contact_handles`, `contact_group_members` in `insert_contact_drafts`/`ensure_group`) → `ON CONFLICT DO NOTHING`; `datetime('now')` in `touch_contact`; `transaction()` in `insert_contact_drafts`; the `loads_*_into_sqlite` tests port with `test_pool()` and `&mut conn`.
- `contacts.rs` (address-book loader): `INSERT OR IGNORE` ×2, `last_insert_rowid` ×2, `transaction()`, `query_map` ×1; `snapshot_email_handles`/`restore_email_handles` port to `query_as::<_, (i64, i64, String, String, String)>`.
- `db/handles.rs`: one rusqlite reference — convert and check its callers.
- `db/account_profile.rs`, `db/api_tokens.rs`, `db/session_tokens.rs`: `query_row`/`optional` sites; `session_tokens` uses Unix-seconds TEXT columns — read as `String` (unchanged behavior).
- `db/vault_imports.rs`: 4 references including a `last_insert_rowid` site → `RETURNING id`.

- [ ] **Step 2: Port each module's `#[cfg(test)]` tests**

Every test becomes `#[tokio::test]`, builds state through `test_pool()` + `ensure_vault_schema`, and asserts through `query_scalar`/`query_as`. Keep assertion values identical to today's — behavior must not change.

- [ ] **Step 3: Commit (checkpoint — crate does not compile)**

```bash
git add crates/vault/server/src/db/ crates/vault/server/src/contacts.rs
git commit -m "refactor(server): port db domain modules to sqlx Any (#148)"
```

---

### Task 4: API modules sweep — conversations, contacts, groups, tags, membership, profile, assets

**Files:**
- Modify: `crates/vault/server/src/conversations_api.rs` (5 refs), `contacts_api.rs` (8), `contact_groups_api.rs` (2), `thread_tags_api.rs` (5), `named_membership.rs` (4), `profile.rs` (3), `process_assets.rs` (1)

**Interfaces:**
- Consumes: Task 2/3 signatures; `like_ci(engine)` for the 23 `COLLATE NOCASE` sites across these files.
- Produces: no signature changes in handlers (Axum `State` handling is Task 6 — for now, replace `lock_conn`/`with_locked_conn` uses with `let mut conn = state.db.acquire().await?;` where `state.db` will exist after Task 6; mark these call sites with a `// TODO(#148): pool acquire` so Task 6 finds them).

- [ ] **Step 1: Replace the lock helpers with pool acquires**

Every `let conn = lock_conn(&state.db)?;` / `lock_import_conn` / `with_locked_conn(state.db.clone(), …)` call site becomes:

```rust
let mut conn = state.db.acquire().await?;
```

Handlers that ran sync closures under `spawn_blocking` now await the db fns directly; delete the `spawn_blocking` wrapper at each site. The `JoinBlocking` trait and lock helpers themselves are removed in Task 6 — leaving them temporarily unused (with `#[allow(dead_code)]` if needed) is fine.

- [ ] **Step 2: Port the SQL in each file with the Task 2 recipes plus file-specific constructs**

- `contacts_api.rs`: two `GROUP_CONCAT(val, char(31))` sites (~lines 690, 705) become engine-branched: SQLite keeps `GROUP_CONCAT(col, char(31))`; Postgres uses `string_agg(col, chr(31))`. Write a helper in `dialect.rs`:

```rust
/// Aggregate many values into one column with U+001F separators (the format
/// the export pipeline expects). SQLite uses GROUP_CONCAT, Postgres string_agg.
pub fn group_concat_unit_separator(engine: DbEngine, col: &str) -> String {
    match engine {
        DbEngine::Sqlite => format!("GROUP_CONCAT({col}, char(31))"),
        DbEngine::Postgres => format!("string_agg({col}, chr(31))"),
    }
}
```

  Add `#[cfg(test)]` coverage for the helper (both engines' outputs).
- `conversations_api.rs`: 5 refs; `like_ci` for `LIKE ? COLLATE NOCASE` search clauses.
- `thread_tags_api.rs` / `contact_groups_api.rs` / `named_membership.rs` / `profile.rs` / `process_assets.rs`: `last_insert_rowid` sites → `RETURNING id`; `optional()` → `fetch_optional`; `query_map` → `query_as` tuples.

- [ ] **Step 3: Commit (checkpoint — crate does not compile)**

```bash
git add crates/vault/server/src/
git commit -m "refactor(server): port API modules to sqlx Any (#148)"
```

---

### Task 5: export_api.rs — QueryBuilder params and the engine-branched FTS compiler

**Files:**
- Modify: `crates/vault/server/src/export_api.rs` (13 refs — the largest single file)

**Interfaces:**
- Consumes: `like_ci`, `group_concat_unit_separator`, `DbEngine`.
- Produces (used by Tasks 8–9): `fn compile_metadata_fts_expr(node: &FtsNode, engine: DbEngine, qb: &mut sqlx::QueryBuilder<'_, sqlx::Any>) -> Result<(), ExportQueryError>` — the engine branch lives here.

- [ ] **Step 1: Convert the params plumbing to fragment strings + `SqlParam` binds**

Current shape: `append_metadata_text_filters(parsed, &mut where_parts: Vec<String>, &mut params: Vec<rusqlite::types::Value>)` and `compile_metadata_fts_expr(node, &mut params) -> String`. New shape: the same functions but `params` becomes `Vec<SqlParam>`; fragments keep `?` placeholders exactly as today; the caller joins the fragments, renumbers placeholders to `$N`, and chains the binds. `QueryBuilder::<Any>` is NOT usable here (its `push_bind` emits `?`, invalid on Postgres; verified in sqlx-core source), and `sqlx::any::AnyValue` is not user-constructible, so this task defines three small helpers:

```rust
/// One bound parameter in a dynamic export query. sqlx's Any driver exposes
/// no user-constructible dynamic value, so heterogeneous binds ride this enum.
pub(crate) enum SqlParam {
    Text(String),
    Int(i64),
    Bool(bool),
    Null,
}

/// Chain all params onto a query in order. Placeholders in the SQL must
/// match this order after `renumber_placeholders`.
pub(crate) fn bind_all<'q>(
    mut q: sqlx::Query<'q, sqlx::Any>,
    params: &[SqlParam],
) -> sqlx::Query<'q, sqlx::Any> {
    for p in params {
        q = match p {
            SqlParam::Text(v) => q.bind(v.clone()),
            SqlParam::Int(v) => q.bind(*v),
            SqlParam::Bool(v) => q.bind(*v),
            SqlParam::Null => q.bind(Option::<String>::None),
        };
    }
    q
}

/// Rewrite `?` placeholders to `$1..$N` in order. The Any driver performs no
/// placeholder rewriting and `?` is invalid on Postgres; SQLite accepts `$N`.
/// Valid because no SQL fragment in this crate contains `?` inside a string
/// literal — keep it that way, and unit-test this against the committed
/// fragment set.
pub(crate) fn renumber_placeholders(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut n = 0usize;
    for ch in sql.chars() {
        if ch == '?' {
            n += 1;
            out.push('$');
            out.push_str(&n.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}
```

Execution at the call site: `let sql = renumber_placeholders(&joined); let q = bind_all(sqlx::query(&sql), &params);` then `fetch_all(&mut *conn)`.

- [ ] **Step 2: Rewrite the FTS compiler with the engine branch**

Full replacement for `compile_metadata_fts_expr` / `compile_metadata_fts_children` / `metadata_term_matches_sql` (current code at `export_api.rs:786-888`). The code below is written in `QueryBuilder` form; translate it mechanically to the Step 1 shape: every `qb.push(x)` becomes `sql.push_str(x)`, every `qb.push_bind(v)` becomes `params.push(SqlParam::Text(v))`, and the `qb: &mut sqlx::QueryBuilder<'_, sqlx::Any>` parameters become `sql: &mut String, params: &mut Vec<SqlParam>`. The compiler's logic (engine branch, phrase/prefix handling, chain structure) is unchanged:

```rust
fn compile_metadata_fts_expr(
    node: &FtsNode,
    engine: DbEngine,
    qb: &mut sqlx::QueryBuilder<'_, sqlx::Any>,
) -> Result<(), ExportQueryError> {
    match node {
        FtsNode::Term { value, prefix } => {
            push_metadata_like_chain(qb, engine, value);
            // Full-text match on the message body index, per engine.
            match engine {
                DbEngine::Sqlite => {
                    // Prefix: `"term"*` (star inside the quoted literal, matching
                    // the current export_api.rs behavior); plain term: `"term"`.
                    let fts_query = if *prefix == Some(true) {
                        format!("{}*", fts5_literal_query(value))
                    } else {
                        fts5_literal_query(value)
                    };
                    qb.push(" OR EXISTS (SELECT 1 FROM messages_fts fts WHERE fts.rowid = m.id AND messages_fts MATCH ");
                    qb.push_bind(fts_query);
                    qb.push(")");
                }
                DbEngine::Postgres => {
                    if *prefix == Some(true) {
                        qb.push(" OR EXISTS (SELECT 1 FROM messages m_fts WHERE m_fts.id = m.id AND m_fts.search_tsv @@ to_tsquery('simple', ");
                        qb.push_bind(pg_prefix_tsquery(value));
                    } else {
                        qb.push(" OR EXISTS (SELECT 1 FROM messages m_fts WHERE m_fts.id = m.id AND m_fts.search_tsv @@ plainto_tsquery('simple', ");
                        qb.push_bind(value.clone());
                    }
                    qb.push("))");
                }
            }
            qb.push(")");
            Ok(())
        }
        FtsNode::Phrase { value } => {
            push_metadata_like_chain(qb, engine, value);
            match engine {
                DbEngine::Sqlite => {
                    qb.push(" OR EXISTS (SELECT 1 FROM messages_fts fts WHERE fts.rowid = m.id AND messages_fts MATCH ");
                    qb.push_bind(fts5_literal_query(value));
                    qb.push(")");
                }
                DbEngine::Postgres => {
                    qb.push(" OR EXISTS (SELECT 1 FROM messages m_fts WHERE m_fts.id = m.id AND m_fts.search_tsv @@ phraseto_tsquery('simple', ");
                    qb.push_bind(value.clone());
                    qb.push("))");
                }
            }
            qb.push(")");
            Ok(())
        }
        FtsNode::And { children } => compile_metadata_fts_children("AND", engine, children, qb),
        FtsNode::Or { children } => compile_metadata_fts_children("OR", engine, children, qb),
        FtsNode::Not { child } => {
            qb.push("(NOT (");
            compile_metadata_fts_expr(child, engine, qb)?;
            qb.push("))");
            Ok(())
        }
    }
}

fn compile_metadata_fts_children(
    operator: &str,
    engine: DbEngine,
    children: &[FtsNode],
    qb: &mut sqlx::QueryBuilder<'_, sqlx::Any>,
) -> Result<(), ExportQueryError> {
    if children.is_empty() {
        return Err(ExportQueryError::bad(format!("{operator} search expression has no operands")));
    }
    qb.push("(");
    for (i, child) in children.iter().enumerate() {
        if i > 0 {
            qb.push(format!(" {operator} "));
        }
        compile_metadata_fts_expr(child, engine, qb)?;
    }
    qb.push(")");
    Ok(())
}

/// Quote a term for a Postgres prefix query under the 'simple' config:
/// `'term':*`. Single quotes are stripped (FTS5 treats them as literal text,
/// 'simple' cannot carry them either).
fn pg_prefix_tsquery(term: &str) -> String {
    format!("'{}':*", term.replace('\'', ""))
}
```

The LIKE-based metadata chain is shared by the Term and Phrase leaves (it matches the current `metadata_term_matches_sql` structure exactly — 8 LIKE binds, then the FTS branch adds the 9th):

```rust
/// The LIKE-based metadata chain shared by Term and Phrase leaves: handles,
/// participant aliases, contacts, and attachment names, all `%term%`
/// case-insensitive. Pushes 8 binds (one per LIKE clause); the caller then
/// pushes the full-text match and closes the outer parenthesis.
fn push_metadata_like_chain(
    qb: &mut sqlx::QueryBuilder<'_, sqlx::Any>,
    engine: DbEngine,
    term: &str,
) {
    let pattern = format!("%{term}%");
    qb.push("(coalesce(hs.raw, '') ");
    qb.push(like_ci(engine));
    qb.push(" OR EXISTS (SELECT 1 FROM participants p_md JOIN handles hp ON hp.id = p_md.handle_id WHERE p_md.conversation_id = c.id AND (hp.raw ");
    qb.push(like_ci(engine));
    qb.push(" OR coalesce(p_md.name_alias, '') ");
    qb.push(like_ci(engine));
    qb.push(")) OR EXISTS (SELECT 1 FROM contact_handles ch_md JOIN contacts ct_md ON ct_md.id = ch_md.contact_id JOIN handles hm ON hm.id = ch_md.handle_id WHERE ch_md.account_id = c.account_id AND (hm.raw ");
    qb.push(like_ci(engine));
    qb.push(" OR coalesce(ct_md.preferred_name, '') ");
    qb.push(like_ci(engine));
    qb.push(") AND ((c.conversation_type = 'individual' AND hm.id = c.chat_handle_id) OR EXISTS (SELECT 1 FROM participants p_md2 WHERE p_md2.conversation_id = c.id AND p_md2.handle_id = ch_md.handle_id))) OR EXISTS (SELECT 1 FROM attachments a_md WHERE a_md.message_id = m.id AND (coalesce(a_md.original_name, '') ");
    qb.push(like_ci(engine));
    qb.push(" OR coalesce(a_md.mime_type, '') ");
    qb.push(like_ci(engine));
    qb.push(" OR coalesce(a_md.derived_mime_type, '') ");
    qb.push(like_ci(engine));
    qb.push("))");
    for _ in 0..8 {
        qb.push_bind(pattern.clone());
    }
}
```

- [ ] **Step 3: Port the remaining export_api SQL**

`with_configured_db`/`with_configured_db_map` call sites become `state.db.acquire().await`; the export query assembly (the parts outside the FTS compiler) converts `params: Vec<Value>` → QueryBuilder pushes. `IFNULL` → `COALESCE`; `COLLATE NOCASE` → `like_ci(engine)`.

- [ ] **Step 4: Commit (checkpoint — crate does not compile)**

```bash
git add crates/vault/server/src/export_api.rs crates/vault/server/src/db/dialect.rs
git commit -m "refactor(server): port export query builder and FTS compiler to sqlx Any (#148)"
```

---

### Task 6: Import pipeline — import/*, dedupe.rs, import_cli.rs

**Files:**
- Modify: `crates/vault/server/src/import/mod.rs`, `import/staging.rs`, `import/contact_name.rs`, `import/promote.rs`, `dedupe.rs`, `import_cli.rs`

**Interfaces:**
- Consumes: `ensure_vault_schema`, `drop_messages_fts_triggers`, `install_messages_fts_triggers`, `index_messages_fts_from_promote_map`, `drop/create_messages_secondary_indexes`, `fold_in_id_chunks` (Task 2), Task 3 db modules.
- Produces: unchanged public entry points (`import::run`, `import_cli::run`), now async over `&mut AnyConnection`.

- [ ] **Step 1: Port `import/promote.rs`**

The promote transaction (`promote.rs:146-308`) converts with the recipe table: `tx.query_row` → `query_scalar` + `fetch_one(&mut *tx)`; `IFNULL(MAX(id), 0)` → `COALESCE(MAX(id), 0)`; `tx.execute` (bulk INSERT…SELECT) → `sqlx::query(…).execute(&mut *tx)`; `tx.commit()` → `tx.commit().await`. The FTS pause/resume calls (`drop_messages_fts_triggers(&tx)`, `index_messages_fts_from_promote_map(&tx, …)`, `install_messages_fts_triggers(&tx)`) take `&mut *tx` per Task 2 signatures. `CREATE TEMP TABLE _promote_msg_map` is portable SQL (works on Postgres). `fill_missing_content_keys` comes from `dedupe.rs` (Step 3).

- [ ] **Step 2: Port `import/mod.rs`, `import/staging.rs`, `import/contact_name.rs`**

`import/mod.rs` uses `lock_conn`/`lock_import_conn`/`with_locked_conn` — replace with `state.db.acquire().await` (or, for import CLI which has no state yet, open a pool via `engine::open_pool_for_path` — see Step 4). `staging.rs` and `contact_name.rs` have one `last_insert_rowid` each → `RETURNING id`.

- [ ] **Step 3: Port `dedupe.rs`**

`source_priority_from_db` (`query_map` over a joined SELECT) → `query_as::<_, (String,)>::…::fetch_all`. The `_content_keys` temp-table flow (`dedupe.rs:375-400`): `execute_batch` for `CREATE TEMP TABLE`/`DROP TABLE` → `split_ddl` + `execute_batch` helper; the prepared `INSERT INTO _content_keys` loop → per-row `sqlx::query(…).bind(id).bind(key).execute(&mut *conn)`; `apply_duplicate_flags`'s `execute_batch(format!(…))` DDL → `split_ddl`.

- [ ] **Step 4: Port `import_cli.rs`**

The import CLI currently opens SQLite directly through helpers with `--db` path overrides. Add `--db-url` to the `Import`/`DedupeCrossSource` subcommands (see Task 9's config work; flag lands here): when `--db-url` is given, open `engine::open_pool_from_url(&url)`; otherwise `engine::open_pool_for_path(&db_path)` where `db_path` is `--db` or `cfg.paths.db`. Run `ensure_vault_schema` through an acquired connection, then the import pipeline.

- [ ] **Step 5: Commit (checkpoint — crate does not compile)**

```bash
git add crates/vault/server/src/import/ crates/vault/server/src/dedupe.rs crates/vault/server/src/import_cli.rs
git commit -m "refactor(server): port import pipeline to sqlx Any (#148)"
```

---

### Task 7: Server plumbing, auth, guest/demo paths — remove rusqlite, crate compiles

**Files:**
- Modify: `crates/vault/server/src/server.rs`, `auth.rs`, `guest_pool.rs`, `guest_clone.rs`, `reset_demo.rs`, `config.rs`, `cli.rs`, `Cargo.toml`

**Interfaces:**
- Produces (used by Tasks 8–9):
  - `AppState` gains `pub db: sqlx::AnyPool` and `pub db_engine: DbEngine`, replacing `pub(crate) db: Arc<StdMutex<Connection>>`.
  - `Config` gains `pub database: DatabaseConfig` where `#[derive(Debug, Clone, Default, Deserialize)] pub struct DatabaseConfig { #[serde(default)] pub url: Option<String> }` (TOML key `[database] url = "postgres://…"`).
  - `resolve_auth` and every handler use `state.db` directly.

- [ ] **Step 1: Convert `AppState` and delete the lock helpers**

In `server.rs`: replace the `db` field; delete `JoinBlocking`, `lock_conn`, `lock_import_conn`, `lock_named`, `with_configured_db`, `with_configured_db_map`, `with_locked_conn` (call sites were converted in Tasks 4–6; any remainder shows up as compile errors here). `reject_if_guest(conn: &Connection, …)` becomes `async fn reject_if_guest(conn: &mut AnyConnection, account_id: &str) -> Result<(), ApiError>` with `query_scalar` + `fetch_optional`.

- [ ] **Step 2: Rewire `run()`**

```rust
pub async fn run(cfg: Config) -> anyhow::Result<()> {
    let server = cfg.require_server()?.clone();
    let bind = server.bind.clone();
    let db_url = cfg.database.url.clone();
    let engine = match &db_url {
        Some(url) => engine::detect_engine(url)?,
        None => DbEngine::Sqlite,
    };
    let lock_path = if engine == DbEngine::Sqlite {
        cfg.paths.db.clone()
    } else {
        cfg.paths.data_dir.join(".operation.lock")
    };
    let _operation_lock = crate::operation_lock::acquire_for_serve(&lock_path)?;
    // …
    let pool = match &db_url {
        Some(url) => engine::open_pool_from_url(url).await?,
        None => engine::open_pool_for_path(&cfg.paths.db).await?,
    };
    {
        let mut conn = pool.acquire().await?;
        let _: i64 = sqlx::query_scalar("SELECT 1").fetch_one(&mut *conn).await?; // warmup
        schema::ensure_vault_schema(&mut conn).await?;
    }
    let state = AppState { db: pool, db_engine: engine, /* …unchanged fields */ };
    // …
}
```

- [ ] **Step 3: Port `auth.rs`, `guest_pool.rs`, `guest_clone.rs`, `reset_demo.rs`**

- `auth.rs`: `with_configured_db(&state.cfg.paths.db, …)` (server.rs:654 `resolve_auth`) becomes `state.db.acquire().await` + async db fns.
- `guest_clone.rs`: the clone SQL is already portable (explicit id remapping, no `last_insert_rowid` in production paths). `TransactionBehavior::Immediate` becomes a plain `conn.begin().await?` (the operation lock already serializes imports; note this in the commit message). `clone_template_to_guest(conn: &mut Connection, …)` → `async fn …(conn: &mut AnyConnection, …)`; `clone_sql(&tx, …)` passes `&mut *tx`.
- `guest_pool.rs`: the worker (server.rs:521-536) keeps its structure but acquires from the pool instead of `schema::open_configured(&cfg.paths.db)`.
- `reset_demo.rs`: 6 refs — port with recipes; demo reset wipes tables + files identically on both engines.

- [ ] **Step 4: Remove rusqlite and make the crate compile**

Remove `rusqlite = { version = "0.40.0", features = ["bundled"] }` from `Cargo.toml`. Fix remaining compile errors until `cargo check -p message-vault-server` passes. Every remaining error must be inside `crates/vault/server/` — if any other crate breaks, you converted something outside scope; revert that change.

- [ ] **Step 5: Commit (crate compiles — first green checkpoint)**

```bash
git add crates/vault/server/
git commit -m "refactor(server): switch AppState to sqlx AnyPool and drop rusqlite (#148)"
```

---

### Task 8: Postgres FTS twin — search_tsv, triggers, GIN, promote branch

**Files:**
- Create: `schema/sql/fts_postgres.sql` (column, GIN index, functions, triggers)
- Create: `schema/sql/fts_postgres_drop.sql` (DROP TRIGGER statements)
- Modify: `crates/vault/server/src/db/schema.rs` (embed + engine-branch `ensure_messages_fts`, `drop/install_messages_fts_triggers`, `index_messages_fts_from_promote_map`)
- Modify: `crates/vault/server/src/import/promote.rs` (Postgres bulk-index branch)

**Interfaces:**
- Consumes: Task 7 `AppState.db_engine`, Task 5 `compile_metadata_fts_expr` (already references `m_fts.search_tsv @@ …` on Postgres).
- Produces: Postgres schema parity for FTS — the Task 5 compiler's Postgres branch becomes executable.

- [ ] **Step 1: Write the failing Postgres FTS sync test (SQLite twin first)**

In `db/schema.rs` tests, port `messages_fts_stays_in_sync` (Task 2 did the port) and add its assertion twin that runs against Postgres when available:

```rust
#[tokio::test]
async fn messages_fts_stays_in_sync_pg() {
    let Some(url) = crate::pg_test_url() else { return };
    let pool = sqlx::AnyPoolOptions::new().connect(&url).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    ensure_vault_schema(&mut conn).await.unwrap();
    // … same inserts/updates/deletes as the SQLite test, asserting via:
    // SELECT COUNT(*) FROM messages WHERE search_tsv @@ plainto_tsquery('simple', ?1)
    // for "vault" / "secret" / "goodbye" with the same expected counts.
}
```

where `crate::pg_test_url()` (added in this task, in `lib.rs`) reads `MV_TEST_POSTGRES_URL`:

```rust
/// Postgres test URL when the gated suite should run (CI sets this).
pub fn pg_test_url() -> Option<String> {
    std::env::var("MV_TEST_POSTGRES_URL").ok().filter(|u| !u.is_empty())
}
```

- [ ] **Step 2: Write `schema/sql/fts_postgres.sql`**

```sql
ALTER TABLE messages ADD COLUMN IF NOT EXISTS search_tsv tsvector;

CREATE INDEX IF NOT EXISTS ix_messages_search_tsv ON messages USING GIN (search_tsv);

CREATE OR REPLACE FUNCTION messages_fts_sync() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        UPDATE messages SET search_tsv = NULL WHERE id = OLD.id;
        RETURN OLD;
    END IF;
    UPDATE messages SET search_tsv = fts.vec
    FROM (
        SELECT m.id,
               to_tsvector('simple',
                   coalesce(m.body, '') || ' ' || coalesce(m.subject, '') || ' ' || coalesce(a.attachment_text, '')) AS vec
        FROM messages m
        LEFT JOIN (
            SELECT message_id,
                   string_agg(trim(coalesce(original_name, '') || ' ' || coalesce(transcription, '')), ' ') AS attachment_text
            FROM attachments
            GROUP BY message_id
        ) a ON a.message_id = m.id
        WHERE m.id = NEW.id
    ) fts
    WHERE messages.id = fts.id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER messages_fts_ai AFTER INSERT ON messages FOR EACH ROW EXECUTE FUNCTION messages_fts_sync();
CREATE TRIGGER messages_fts_au AFTER UPDATE OF body, subject ON messages FOR EACH ROW EXECUTE FUNCTION messages_fts_sync();
CREATE TRIGGER messages_fts_ad AFTER DELETE ON messages FOR EACH ROW EXECUTE FUNCTION messages_fts_sync();

CREATE OR REPLACE FUNCTION attachments_fts_sync() RETURNS trigger AS $$
DECLARE mid bigint;
BEGIN
    mid := COALESCE(NEW.message_id, OLD.message_id);
    UPDATE messages SET search_tsv = fts.vec
    FROM (
        SELECT m.id,
               to_tsvector('simple',
                   coalesce(m.body, '') || ' ' || coalesce(m.subject, '') || ' ' || coalesce(a.attachment_text, '')) AS vec
        FROM messages m
        LEFT JOIN (
            SELECT message_id,
                   string_agg(trim(coalesce(original_name, '') || ' ' || coalesce(transcription, '')), ' ') AS attachment_text
            FROM attachments
            GROUP BY message_id
        ) a ON a.message_id = m.id
        WHERE m.id = mid
    ) fts
    WHERE messages.id = fts.id;
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER attachments_fts_ai AFTER INSERT ON attachments FOR EACH ROW EXECUTE FUNCTION attachments_fts_sync();
CREATE TRIGGER attachments_fts_ad AFTER DELETE ON attachments FOR EACH ROW EXECUTE FUNCTION attachments_fts_sync();
CREATE TRIGGER attachments_fts_au AFTER UPDATE OF original_name, transcription ON attachments FOR EACH ROW EXECUTE FUNCTION attachments_fts_sync();
```

And `schema/sql/fts_postgres_drop.sql`:

```sql
DROP TRIGGER IF EXISTS messages_fts_ai ON messages;
DROP TRIGGER IF EXISTS messages_fts_au ON messages;
DROP TRIGGER IF EXISTS messages_fts_ad ON messages;
DROP TRIGGER IF EXISTS attachments_fts_ai ON attachments;
DROP TRIGGER IF EXISTS attachments_fts_ad ON attachments;
DROP TRIGGER IF EXISTS attachments_fts_au ON attachments;
```

- [ ] **Step 3: Engine-branch the schema functions**

In `db/schema.rs`: embed both files; `ensure_messages_fts` on Postgres runs `execute_batch(conn, FTS_POSTGRES_DDL)` then `install_messages_fts_triggers`; `install_messages_fts_triggers` on Postgres runs `fts_postgres_drop.sql` then `fts_postgres.sql`'s trigger statements (guarded by `IF NOT EXISTS`-style idempotence via `CREATE OR REPLACE FUNCTION` + `CREATE TRIGGER` — make the create trigger statements idempotent by dropping first, exactly as the SQLite path does); `drop_messages_fts_triggers` on Postgres runs `fts_postgres_drop.sql` plus `DELETE FROM schema_meta WHERE key = …` (same marker key). `index_messages_fts_from_promote_map` on Postgres:

```rust
if pg {
    let n = sqlx::query(
        r#"
        UPDATE messages SET search_tsv = fts.vec
        FROM (
            SELECT mm.prod_id,
                   to_tsvector('simple',
                       coalesce(m.body, '') || ' ' || coalesce(m.subject, '') || ' ' || coalesce(a.attachment_text, '')) AS vec
            FROM (SELECT DISTINCT prod_id FROM _promote_msg_map WHERE prod_id > ?1) mm
            JOIN messages m ON m.id = mm.prod_id
            LEFT JOIN (
                SELECT message_id,
                       string_agg(trim(coalesce(original_name, '') || ' ' || coalesce(transcription, '')), ' ') AS attachment_text
                FROM attachments
                GROUP BY message_id
            ) a ON a.message_id = mm.prod_id
        ) fts
        WHERE messages.id = fts.prod_id
        "#,
    )
    .bind(min_new_message_id)
    .execute(&mut *conn)
    .await?;
    return Ok(n.rows_affected());
}
```

- [ ] **Step 4: Promote branch**

In `import/promote.rs`, the "pausing FTS triggers" phase branches on engine: SQLite keeps `drop_messages_fts_triggers` (per-row trigger skip); Postgres instead runs `ALTER TABLE messages DISABLE TRIGGER ALL; ALTER TABLE attachments DISABLE TRIGGER ALL;` before the bulk inserts and `ENABLE TRIGGER ALL` on both after `index_messages_fts_from_promote_map` (which performs the bulk vector fill). Add these two statements as `split_ddl`-compatible helper fns in `db/schema.rs` (`pub(crate) async fn disable_fts_triggers_pg(conn) / enable_fts_triggers_pg(conn)`), each a single `sqlx::query` call.

- [ ] **Step 5: Run the FTS tests**

Run: `cargo test -p message-vault-server messages_fts` — expected PASS on SQLite (unchanged behavior); the `_pg` twin skips locally without `MV_TEST_POSTGRES_URL`. With a local Postgres (Task 9 compose file), run `MV_TEST_POSTGRES_URL=postgres://vault:vault@127.0.0.1:5432/vault cargo test -p message-vault-server messages_fts` — expected PASS.

- [ ] **Step 6: Commit**

```bash
git add schema/sql/fts_postgres.sql schema/sql/fts_postgres_drop.sql crates/vault/server/src/db/schema.rs crates/vault/server/src/import/promote.rs crates/vault/server/src/lib.rs
git commit -m "feat(server): Postgres FTS twin (search_tsv + GIN + triggers) (#148)"
```

---

### Task 9: Search parity suite, local Postgres, CI

**Files:**
- Create: `tests/fixtures/search/parity-messages.json` (committed message corpus)
- Create: `crates/vault/server/tests/search_parity.rs` (integration test — the crate has no `tests/` dir yet; create it)
- Create: `docker-compose.pg.yml` (repo root, dev-only Postgres)
- Modify: `.github/workflows/ci.yml` (Postgres service + second test matrix entry)

**Interfaces:**
- Consumes: `pg_test_url`, and a duplicate of Task 1's validated `test_pool` helper in the test file (duplicate it — it keeps `install_default_drivers()` + the URL-based pool opening; `test_pool` stays `#[cfg(test)]`-only), Task 5 compiler.

- [ ] **Step 1: Write the committed corpus**

`tests/fixtures/search/parity-messages.json` — an array of message objects the test inserts into a fresh vault on each engine. Cover the FTS shapes the ticket promises: plain term (`"vault"`), case-insensitive (`"HELLO"` matches `"hello"`), prefix (`"report*"` matches `"reporting"`), phrase (`"two words"`), boolean AND/OR/NOT, attachment transcription text, subject text. ~15 messages with distinct bodies; each has a stable integer key `k` used for the expected id sets below.

```json
[
  {"k": 1, "source": "sms", "guid": "p1", "body": "hello vault"},
  {"k": 2, "source": "sms", "guid": "p2", "body": "HELLO again"},
  {"k": 3, "source": "imessage", "guid": "p3", "body": "quarterly reporting deadline"},
  {"k": 4, "source": "imessage", "guid": "p4", "body": "exactly two words"},
  {"k": 5, "source": "sms", "guid": "p5", "body": "two words apart"},
  {"k": 6, "source": "sms", "guid": "p6", "body": "red apple"},
  {"k": 7, "source": "sms", "guid": "p7", "body": "green apple"},
  {"k": 8, "source": "sms", "guid": "p8", "body": "red car"},
  {"k": 9, "source": "whatsapp", "guid": "p9", "body": "cafe meeting"},
  {"k": 10, "source": "whatsapp", "guid": "p10", "body": "café meeting"},
  {"k": 11, "source": "sms", "guid": "p11", "body": null, "subject": "dinner plans"},
  {"k": 12, "source": "sms", "guid": "p12", "body": "listen to the attached note", "attachments": [{"original_name": "voice.m4a", "transcription": "secret phrase"}]},
  {"k": 13, "source": "sms", "guid": "p13", "body": "attachment filename only", "attachments": [{"original_name": "IMG_0001.jpg"}]},
  {"k": 14, "source": "sms", "guid": "p14", "body": "punctuation: dash-separated words"},
  {"k": 15, "source": "sms", "guid": "p15", "body": "alpha and beta"}
]
```

- [ ] **Step 2: Write the parity test**

`crates/vault/server/tests/search_parity.rs`: a `run_against(pool)` function that (a) creates a fresh vault schema + one account + conversation, (b) inserts the corpus messages with `k`-tracked ids, (c) runs the committed query list through the same export/search entry point the API uses (call `export_api`'s public search fn with a parsed query from `search_query::parse_search_query`), (d) returns the result id sets. The test then asserts the committed expected sets, and runs the whole thing against SQLite (`test_pool` duplicate) and, when `pg_test_url()` is set, against Postgres, asserting identical results:

```rust
// Committed queries and expected id sets (message keys from the fixture).
// Format: (query string, expected keys). These are the parity contract.
const CASES: &[(&str, &[i64])] = &[
    ("vault", &[1]),
    ("hello", &[1, 2]),                      // case-insensitive on both engines
    ("report*", &[3]),                       // prefix
    ("\"two words\"", &[4]),                 // phrase (exact adjacency)
    ("red AND apple", &[6]),
    ("red apple", &[6]),                     // implicit AND
    ("red OR green", &[6, 7, 8]),
    ("apple NOT red", &[7]),
    ("secret", &[12]),                       // attachment transcription
    ("IMG_0001", &[13]),                     // attachment filename
    ("dinner", &[11]),                       // subject
    ("dash-separated", &[14]),               // punctuation tokenization
    ("alpha beta", &[15]),
];
// Diacritics: FTS5 strips them, Postgres 'simple' does not — the documented
// exception. "cafe" matches k=9 and k=10 on SQLite, only k=9 on Postgres.
```

For the insert helper, reuse the shape of the fixture schema test setup: insert account → conversation (`chat_handle_id` requires a handle row) → messages (bind `k` as the message id — the PG DDL uses `BY DEFAULT AS IDENTITY` so explicit ids work).

- [ ] **Step 3: Local Postgres for development**

`docker-compose.pg.yml`:

```yaml
services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: vault
      POSTGRES_PASSWORD: vault
      POSTGRES_DB: vault
    ports: ["5432:5432"]
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U vault"]
      interval: 2s
      timeout: 2s
      retries: 10
```

Document the two commands in the task commit message and in `AGENTS.md` (Task 10): `docker compose -f docker-compose.pg.yml up -d` and `MV_TEST_POSTGRES_URL=postgres://vault:vault@127.0.0.1:5432/vault cargo test -p message-vault-server`.

- [ ] **Step 4: Wire CI**

In `.github/workflows/ci.yml`, add a `postgres` service to the Rust test job and a second matrix entry (or second job) running `MV_TEST_POSTGRES_URL=postgres://vault:vault@postgres:5432/vault cargo test -p message-vault-server` so the gated tests actually run. Keep the default SQLite entry unchanged.

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/search/parity-messages.json crates/vault/server/tests/search_parity.rs docker-compose.pg.yml .github/workflows/ci.yml
git commit -m "test(server): search parity corpus across SQLite and Postgres (#148)"
```

---

### Task 10: Verification, docs, and finishing the branch

**Files:**
- Modify: `CLAUDE.md` (data-flow line: "Axum HTTP API (`/v1/*`) over SQLite at `data/vault.db`" → mention engine choice), `AGENTS.md` (dev-run note for `[database] url` and the Postgres compose commands)
- No other file changes; fix whatever verification surfaces.

- [ ] **Step 1: Format and lint**

Run: `cargo fmt --all -- --check` (fix with `cargo fmt --all`), `./scripts/lint-all.sh`, and from the repo root `cargo clippy -p message-vault-server --all-targets`. Fix every finding; no `#[allow]` silences.

- [ ] **Step 2: Full test suite, both engines**

Run: `cargo test -p message-vault-server` (SQLite default) and `cargo test --workspace` (no other crate regressions). Then start the compose Postgres and run `MV_TEST_POSTGRES_URL=postgres://vault:vault@127.0.0.1:5432/vault cargo test -p message-vault-server`. Expected: everything PASS, including `search_parity` (the diacritic exception asserted explicitly) and the `_pg` schema/FTS tests.

- [ ] **Step 3: Smoke runs**

Run `./scripts/run-vault-dev.sh --reset-demo` and confirm: server boots, `/health` 200, demo sign-in works, a search for a seeded term returns results. Then `MV_TEST_POSTGRES_URL=… cargo run -p message-vault-server -- serve` against the compose Postgres and repeat. Use `127.0.0.1`, not `localhost` (CLAUDE.md).

- [ ] **Step 4: Docs**

Update the two doc lines from Files. Regenerate nothing else (no OpenAPI/CLI pages change — no public API changed). Confirm no `docs/` build is needed (`cd docs && npm run check`).

- [ ] **Step 5: Final verification pass**

Run: `./scripts/check-pr.sh`. This must pass end to end.

- [ ] **Step 6: Commit and push the branch**

```bash
git add CLAUDE.md AGENTS.md
git commit -m "docs: document dual-engine database configuration (#148)"
git push -u origin worktree-sqlx-any-swap-plan
```

Then open the PR against `main` with the ticket linked ("Closes #148") — do not merge.

## Self-Review Notes (checked while writing this plan)

- **Spec coverage:** every ticket section maps to a task — dependency + pool (1), DDL portability (2), FTS SQLite-unchanged + Postgres twin + 'simple' config (8), AST compiler branch (5), ILIKE dialect helper (2/4), bm25/ts_rank (ticket says scores differ by design; recon found no bm25/ts_rank in the code today, so `sort:relevance` needs no ranking work — behavior must simply not change; Task 5 keeps it that way), parity corpus (9), CI both engines (9), existing vault.db works (Task 2 keeps SQLite DDL byte-identical + Task 10 smoke).
- **Placeholder scan:** no TBD/TODO markers in code blocks; the one `// TODO(#148): pool acquire` marker is a deliberate Task 4 → 7 handoff tag, tracked by the compiler.
- **Type consistency:** `ensure_vault_schema(&mut AnyConnection)`, `test_pool()`, `like_ci`, `DbEngine`, and `compile_metadata_fts_expr` signatures are defined once (Tasks 1/2/5) and reused verbatim in later tasks.
