# Import Session Phase 2 — The Session Record Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The vault knows there is an import in progress, refuses to start a second one, and remembers where its staging folder is — so entering Import after a reload, a logout, or a crash reopens the session instead of a blank form.

**Architecture:** `vault_imports` gains five columns (`stage`, `staging_dir`, `device_id`, `form_json`, `source_fingerprint`) and a partial unique index that makes "one active session per account" a storage constraint rather than application logic. Three endpoints expose the record: read the active session, advance its stage, discard it. The desktop app writes the record as it goes and, on entering Import, asks the vault for the active session first — the form is what appears when the answer is nothing, not the default.

**Tech Stack:** Rust (Axum server, sqlx over SQLite and Postgres, Tauri commands), React 19 + TypeScript (Vitest + Testing Library), Biome.

**Spec:** `docs/superpowers/specs/2026-08-29-import-session-design.md` — this phase implements sequencing step 2 and decisions 1–5, 35, 37, and the reachable rows of 36. Phase 1 (decisions 21, 22, 40, 41) is merged as `56b0bb56`.

**Branch:** `claude/import-session-phase-2`, cut from `main` at `56b0bb56`.

## What this phase can and cannot deliver

Gates do not exist until Phase 3 and prepare is not restructured until Phase 4, so only three of the six stages are reachable here: `parse`, `write`, `pushing`. The `ImportStage` enum still carries all six — it is the spec's vocabulary and Phase 3 sets the rest — but nothing in this phase writes `awaiting_gate_1`, `transcode`, or `awaiting_gate_2`. A task that appears to under-use the enum is correct.

What ships:

- A second import cannot start while one is live, enforced by the database.
- `staging_dir` survives a page reload, so Import reopens on the session rather than the form. This is the user-visible point of the phase.
- Resume during `pushing` and after `write` completes: the staging folder is intact, so the fix is to re-push it. Dedupe and `preflight_existing_assets` absorb the overlap (decision 4).
- A session belonging to another install is explained rather than silently opened (decision 36's `device_id` row).
- An explicit discard. No timeout ever reclaims a session (decision 37).

Decisions 38 and 39 are stored-but-unused here. The `source_fingerprint` column is written, but nothing acts on a mismatch: decision 36's own table says a changed source is "fatal for `write`; irrelevant at either gate and during `pushing`", and `write` is not separately resumable until Phase 4. Decision 39's recomputed summary belongs to the gates. Writing the column now means Phase 4 inherits data rather than starting blind.

## Global Constraints

- **Schema changes rebuild the database. This was decided deliberately and is not to be softened.** `SCHEMA_VERSION` goes 3 → 4; `migrate_vault_schema` then drops every user table and recreates it empty, and the user re-imports. The alternative — an additive `ALTER TABLE` migration — was considered and rejected in favour of the rule already written at `crates/vault/server/src/db/schema.rs:64`: *"The only kind of migration is a full rebuild: schema changes require a fresh reload of data, never in-place column patches."* Do not add an `ALTER TABLE ... ADD COLUMN` anywhere in this plan.
- **Postgres needs new code to honour that same rule.** `apply_postgres_vault_ddl` returns early whenever its marker exists, and its DDL is `CREATE TABLE IF NOT EXISTS`, which cannot add a column to an existing table. Left alone, an installed Postgres vault would keep the marker, never receive the columns, and fail at runtime on every query naming them. Task 1 gives Postgres a rebuild path matching SQLite's.
- Stage strings, exactly: `parse`, `write`, `awaiting_gate_1`, `transcode`, `awaiting_gate_2`, `pushing`.
- Status strings, exactly: `running`, `completed`, `completed_with_issues`, `failed`, `cancelled`. Only `running` is non-terminal; the partial unique index keys on it. (`complete_import` writes `cancelled` nowhere today — a discard in this phase introduces it.)
- **`status` says how a session ended; `stage` says where it is.** Both are needed and neither replaces the other (decision 2).
- The database is authoritative and the filesystem holds work products (decision 1). Progress *within* a stage is recomputed from the folder, never stored (decision 4).
- The word "transcode" never appears in user-facing copy (decision 18). It is a stage name only.
- **Product copy states what the product can do; it does not warn, alarm, or hedge about consequences.**
- Version lockstep files are not touched by this plan: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `web/package.json`, `crates/vault/server/Cargo.toml`. No version bump.
- `docs/src/assets/openapi.json` has a committed-dump gate (`committed_openapi_matches_dump`). Any change to a `utoipa::ToSchema` type or a routed handler must regenerate it in the same commit: `cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json`.
- `cargo fmt --all -- --check` must pass. Biome must pass (`cd web && npm run lint`); imports sorted, unused bindings prefixed `_`, real fixes over `biome-ignore`.
- Tests use committed fixtures in `tests/fixtures/`; never real message data.
- Never commit to `main`. Do not push, tag, or open a PR unless asked. Do not merge.
- Literal code below was written against `main` at `56b0bb56`. Where a snippet and the compiler disagree, the compiler is authoritative — keep the intent, fix the syntax.
- Commit after every task.

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `schema/sql/accounts.sql` | SQLite DDL: five columns, partial unique index | 1 |
| `schema/sql/pg_accounts.sql` | Postgres twin of the same | 1 |
| `crates/vault/server/src/db/schema.rs` | `SCHEMA_VERSION` bump; Postgres rebuild path | 1 |
| `crates/vault/server/src/db/vault_imports.rs` | `ImportStage`, row fields, active/stage/discard queries | 2 |
| `crates/vault/server/src/import/mod.rs` | Request/response bodies and the three handlers | 3 |
| `crates/vault/server/src/openapi.rs` | Route registration for the new handlers | 3 |
| `src-tauri/src/commands/paths.rs` | `PathStat` gains size and mtime, for the fingerprint | 4 |
| `web/src/lib/deviceId.ts` | **new** — stable per-install id | 4 |
| `web/src/lib/importSession.ts` | **new** — session API client, fingerprint builder, form snapshot | 4 |
| `web/src/screens/import/useImportJob.ts` | Writes the session record as the run advances | 5 |
| `web/src/screens/import/ResumeImportPanel.tsx` | **new** — the screen shown when a session is found | 6 |
| `web/src/screens/ImportScreen.tsx` | Asks for the active session before rendering the form | 6 |

New files stay small and single-purpose. `useImportJob.ts` and `ImportScreen.tsx` are both already large — work inside their existing patterns and do not restructure them.

---

### Task 1: Five columns, the one-session index, and a version bump both engines honour

**Files:**
- Modify: `schema/sql/accounts.sql` (the `vault_imports` table, ends around line 144; the index block that follows)
- Modify: `schema/sql/pg_accounts.sql` (same table, ends around line 156)
- Modify: `crates/vault/server/src/db/schema.rs` (`SCHEMA_VERSION` around line 58; `VAULT_SCHEMA_META_KEY` around line 233; `apply_postgres_vault_ddl` around line 161)
- Test: `crates/vault/server/src/db/schema.rs` tests module

**Interfaces:**
- Consumes: nothing.
- Produces: columns `stage`, `staging_dir`, `device_id`, `form_json`, `source_fingerprint` on `vault_imports`; the index `ux_vault_imports_active_account`; `SCHEMA_VERSION == 4`; `VAULT_SCHEMA_META_KEY == "vault_schema_v2"`. Task 2 reads and writes those columns.

- [ ] **Step 1: Write the failing test**

In the `tests` module of `crates/vault/server/src/db/schema.rs`, add the two tests below.

`crate::db::engine::test_pool()` returns `(AnyPool, TempDir)` directly — **not** a `Result`, so it takes no `.unwrap()`. The `accounts` table needs both `id` and `username`; `crates/vault/server/src/db/vault_imports.rs:670` shows the idiom.

```rust
    #[tokio::test]
    async fn one_running_import_per_account() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        ensure_vault_schema(&mut conn).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ('acct', 'alice')")
            .execute(&mut *conn)
            .await
            .unwrap();

        let insert = r#"
            INSERT INTO vault_imports (
                account_id, source, mode, status, started_at,
                message_count, attachment_count, bytes_uploaded
            ) VALUES ('acct', 'imessage', 'append', $1, '2026-08-30T00:00:00Z', 0, 0, 0)
        "#;

        sqlx::query(insert)
            .bind("running")
            .execute(&mut *conn)
            .await
            .expect("first running session inserts");

        let second = sqlx::query(insert)
            .bind("running")
            .execute(&mut *conn)
            .await;
        assert!(second.is_err(), "a second running session must be rejected");

        // A finished session does not occupy the slot.
        sqlx::query(insert)
            .bind("completed")
            .execute(&mut *conn)
            .await
            .expect("a completed session is not covered by the partial index");
    }

    #[tokio::test]
    async fn vault_imports_carries_the_session_columns() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        ensure_vault_schema(&mut conn).await.unwrap();
        sqlx::query(
            "SELECT stage, staging_dir, device_id, form_json, source_fingerprint
             FROM vault_imports WHERE 1 = 0",
        )
        .fetch_optional(&mut *conn)
        .await
        .expect("session columns exist");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p message-vault-server one_running_import_per_account vault_imports_carries`
Expected: FAIL — no such column `stage`; the second insert succeeds.

- [ ] **Step 3: Add the columns and the index to the SQLite DDL**

In `schema/sql/accounts.sql`, change the last field of `vault_imports` and the lines that follow it:

```sql
    -- JSON blob with a human-readable run summary for Import History.
    summary_json TEXT,
    -- Where a live session is: parse, write, awaiting_gate_1, transcode,
    -- awaiting_gate_2, or pushing. NULL once the run is over, and on rows
    -- written before sessions existed. `status` says how a run ended;
    -- `stage` says where it is.
    stage TEXT,
    -- Absolute path to this session's staging folder on the client. The
    -- database holds the pointer so resuming means asking the vault where
    -- to go, rather than guessing from a directory listing.
    staging_dir TEXT,
    -- Which install created the session, so another machine can say where
    -- it belongs instead of failing to open a path that was never local.
    device_id TEXT,
    -- Import form snapshot: restores the screen, and restarts the run with
    -- the same settings.
    form_json TEXT,
    -- Source path, size, mtime, and message count. A backup that grew
    -- between attempts has different conversation boundaries.
    source_fingerprint TEXT
);

CREATE INDEX IF NOT EXISTS ix_vault_imports_account_started
    ON vault_imports(account_id, started_at DESC);

-- At most one live import session per account. A partial unique index
-- rather than application logic, so it holds against a racing client.
CREATE UNIQUE INDEX IF NOT EXISTS ux_vault_imports_active_account
    ON vault_imports(account_id) WHERE status = 'running';
```

- [ ] **Step 4: Mirror it into the Postgres DDL**

Apply the identical column block and index to `schema/sql/pg_accounts.sql`. The partial-index syntax is the same on both engines; keep the comments identical so the two files stay diffable.

- [ ] **Step 5: Bump the SQLite schema version**

In `crates/vault/server/src/db/schema.rs`:

```rust
pub const SCHEMA_VERSION: i64 = 4;
```

Nothing else is needed for SQLite: `migrate_vault_schema` already rebuilds on a version mismatch.

- [ ] **Step 6: Give Postgres the same rebuild semantics**

This is the new code the rebuild rule forces. Bumping the marker alone is not enough — the DDL is `CREATE TABLE IF NOT EXISTS`, so an existing table would keep its old shape while the marker claimed otherwise.

First, the marker:

```rust
/// Marker that the one-time Postgres vault DDL install has completed.
/// Bumped with the schema: a vault holding an older marker is rebuilt
/// empty, matching SQLite's `user_version` behaviour.
pub const VAULT_SCHEMA_META_KEY: &str = "vault_schema_v2";
```

Then, inside `apply_postgres_vault_ddl`'s locked block, drop stale user tables before applying the DDL. Add this helper beside it:

```rust
/// Drop every user table in the current schema. Postgres twin of
/// [`rebuild_vault_schema`]: a vault stamped with an older marker is
/// rebuilt empty rather than patched in place.
///
/// `CASCADE` takes the FTS triggers and foreign keys down with their
/// tables; the sync functions are recreated with `CREATE OR REPLACE`.
async fn drop_pg_user_tables(conn: &mut AnyConnection) -> Result<()> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname = current_schema()",
    )
    .fetch_all(&mut *conn)
    .await?;
    for table in &tables {
        sqlx::query(&format!("DROP TABLE IF EXISTS \"{table}\" CASCADE"))
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}
```

and call it in the locked block, immediately before `execute_batch(&mut tx, PG_ACCOUNTS_DDL)`:

```rust
        // A vault carrying an older marker (or none, with tables present)
        // is rebuilt empty — the same contract SQLite's user_version
        // gives. Re-importing is the migration.
        if table_exists(&mut tx, "vault_imports").await? {
            eprintln!(
                "warning: vault schema predates {VAULT_SCHEMA_META_KEY}; rebuilding empty (re-import your data)"
            );
            drop_pg_user_tables(&mut tx).await?;
        }
```

`table_exists` already exists in this file (around line 595) and takes any executor. If it does not accept `&mut tx` as written, take a connection-shaped argument the way the neighbouring calls do — the compiler is authoritative.

- [ ] **Step 7: Run the tests**

Run: `cargo test -p message-vault-server`
Expected: PASS. Any test asserting `SCHEMA_VERSION == 3` updates to 4.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all -- --check
git add schema/sql crates/vault/server/src/db/schema.rs
git commit -m "feat(vault): the session record's columns and the one-session index

Schema version 4. An out-of-date vault is rebuilt empty on both engines
and re-imported, per the rule in schema.rs. Postgres gains the rebuild
path it was missing: its marker gate skipped the DDL entirely, and
CREATE TABLE IF NOT EXISTS cannot add a column to a table that exists.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: Reading and writing the session record

**Files:**
- Modify: `crates/vault/server/src/db/vault_imports.rs` — `VaultImportRow` (around line 17), `VAULT_IMPORT_COLUMNS` (around line 232), `vault_import_from_row` (around line 236), `start_import` (around line 205)
- Test: same file's `tests` module

**Interfaces:**
- Consumes: Task 1's columns and index.
- Produces, all from `crate::db::vault_imports`:
  - `pub enum ImportStage { Parse, Write, AwaitingGate1, Transcode, AwaitingGate2, Pushing }` with `pub fn as_str(self) -> &'static str` and `pub fn parse(s: &str) -> Option<Self>`
  - `pub struct StartImportArgs<'a> { pub account_id: &'a str, pub source: &'a str, pub mode: &'a str, pub tool: Option<&'a str>, pub stage: ImportStage, pub staging_dir: Option<&'a str>, pub device_id: Option<&'a str>, pub form_json: Option<&'a str>, pub source_fingerprint: Option<&'a str> }`
  - `pub async fn start_import(conn, args: &StartImportArgs<'_>) -> Result<i64, StartImportError>` where `pub enum StartImportError { AlreadyActive, Db(anyhow::Error) }`
  - `pub async fn get_active_import(conn, account_id: &str) -> Result<Option<VaultImportRow>>`
  - `pub async fn set_import_stage(conn, account_id: &str, import_id: i64, stage: ImportStage) -> Result<(), ImportLookupError>`
  - `pub async fn discard_import(conn, account_id: &str, import_id: i64) -> Result<(), ImportLookupError>`
  - `VaultImportRow` gains `pub stage: Option<String>`, `pub staging_dir: Option<String>`, `pub device_id: Option<String>`, `pub form_json: Option<String>`, `pub source_fingerprint: Option<String>`

Task 3 calls all of these.

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `crates/vault/server/src/db/vault_imports.rs`, using the helper that module already has (see the note after the code).

```rust
    #[tokio::test]
    async fn active_session_round_trips_and_blocks_a_second() {
        let (pool, _dir) = setup_accounts_only().await;
        let mut conn = pool.acquire().await.unwrap();
        let account = ACCOUNT_ID;

        assert!(
            get_active_import(&mut conn, account).await.unwrap().is_none(),
            "no session before one starts"
        );

        let args = StartImportArgs {
            account_id: account,
            source: "imessage",
            mode: "append",
            tool: Some("message-vault-io"),
            stage: ImportStage::Parse,
            staging_dir: Some("/home/u/message-vault/staging-iphone-260830"),
            device_id: Some("device-a"),
            form_json: Some(r#"{"source":"imessage-ios"}"#),
            source_fingerprint: Some(r#"{"path":"/b","size_bytes":10}"#),
        };
        let id = start_import(&mut conn, &args).await.unwrap();

        let active = get_active_import(&mut conn, account)
            .await
            .unwrap()
            .expect("the session is active");
        assert_eq!(active.id, id);
        assert_eq!(active.stage.as_deref(), Some("parse"));
        assert_eq!(
            active.staging_dir.as_deref(),
            Some("/home/u/message-vault/staging-iphone-260830")
        );
        assert_eq!(active.device_id.as_deref(), Some("device-a"));
        assert_eq!(active.form_json.as_deref(), Some(r#"{"source":"imessage-ios"}"#));

        assert!(
            matches!(
                start_import(&mut conn, &args).await,
                Err(StartImportError::AlreadyActive)
            ),
            "a second session is refused by the index, not by a race-prone check"
        );
    }

    #[tokio::test]
    async fn stage_advances_and_discard_frees_the_slot() {
        let (pool, _dir) = setup_accounts_only().await;
        let mut conn = pool.acquire().await.unwrap();
        let account = ACCOUNT_ID;
        let args = StartImportArgs {
            account_id: account,
            source: "imessage",
            mode: "append",
            tool: None,
            stage: ImportStage::Parse,
            staging_dir: None,
            device_id: None,
            form_json: None,
            source_fingerprint: None,
        };
        let id = start_import(&mut conn, &args).await.unwrap();

        set_import_stage(&mut conn, account, id, ImportStage::Pushing)
            .await
            .unwrap();
        let active = get_active_import(&mut conn, account).await.unwrap().unwrap();
        assert_eq!(active.stage.as_deref(), Some("pushing"));

        discard_import(&mut conn, account, id).await.unwrap();
        assert!(
            get_active_import(&mut conn, account).await.unwrap().is_none(),
            "a discarded session is no longer active"
        );
        let row = get_owned_import(&mut conn, account, id).await.unwrap();
        assert_eq!(row.status, "cancelled");
        assert!(row.finished_at.is_some(), "a discard closes the run");

        // The slot is genuinely free.
        start_import(&mut conn, &args).await.expect("a new session can start");
    }

    #[tokio::test]
    async fn completing_a_session_frees_the_slot_too() {
        let (pool, _dir) = setup_accounts_only().await;
        let mut conn = pool.acquire().await.unwrap();
        let account = ACCOUNT_ID;
        let args = StartImportArgs {
            account_id: account,
            source: "imessage",
            mode: "append",
            tool: None,
            stage: ImportStage::Parse,
            staging_dir: None,
            device_id: None,
            form_json: None,
            source_fingerprint: None,
        };
        let id = start_import(&mut conn, &args).await.unwrap();
        complete_import(
            &mut conn,
            account,
            id,
            &CompleteImportArgs::succeeded(10, 2),
        )
        .await
        .unwrap();
        assert!(get_active_import(&mut conn, account).await.unwrap().is_none());
    }

    #[test]
    fn every_stage_round_trips_through_its_string() {
        for stage in [
            ImportStage::Parse,
            ImportStage::Write,
            ImportStage::AwaitingGate1,
            ImportStage::Transcode,
            ImportStage::AwaitingGate2,
            ImportStage::Pushing,
        ] {
            assert_eq!(ImportStage::parse(stage.as_str()), Some(stage));
        }
        assert_eq!(ImportStage::parse("gate_1"), None);
    }
```

This module already has the setup helper these tests use: `setup_accounts_only() -> (sqlx::AnyPool, tempfile::TempDir)` at `crates/vault/server/src/db/vault_imports.rs:670`, together with the `ACCOUNT_ID` constant above it. Do not add a new one.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p message-vault-server -- vault_imports`
Expected: compile errors — `ImportStage`, `StartImportArgs`, `get_active_import` do not exist.

- [ ] **Step 3: Add the stage enum**

Near the top of `crates/vault/server/src/db/vault_imports.rs`:

```rust
/// Where a live import session is in its lifecycle.
///
/// `status` records how a run ended; this records where it is. Both are
/// needed: a session can sit at `Write` while running, and at `Write`
/// having failed.
///
/// All six stages exist because they are the design's vocabulary, but the
/// gates and the media pass are not built yet — only `Parse`, `Write`, and
/// `Pushing` are reachable today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStage {
    /// Reading the backup. Nothing durable exists yet.
    Parse,
    /// Writing conversation files and staging attachments.
    Write,
    /// Waiting for the user to approve spending time on the media step.
    AwaitingGate1,
    /// Converting or compressing staged media.
    Transcode,
    /// Waiting for the user to approve what lands in the vault.
    AwaitingGate2,
    /// Uploading to the vault.
    Pushing,
}

impl ImportStage {
    /// Stored spelling of this stage.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::Write => "write",
            Self::AwaitingGate1 => "awaiting_gate_1",
            Self::Transcode => "transcode",
            Self::AwaitingGate2 => "awaiting_gate_2",
            Self::Pushing => "pushing",
        }
    }

    /// Parse a stored spelling, or `None` when it is not one of the six.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "parse" => Some(Self::Parse),
            "write" => Some(Self::Write),
            "awaiting_gate_1" => Some(Self::AwaitingGate1),
            "transcode" => Some(Self::Transcode),
            "awaiting_gate_2" => Some(Self::AwaitingGate2),
            "pushing" => Some(Self::Pushing),
            _ => None,
        }
    }
}
```

- [ ] **Step 4: Widen the row and the column list**

Add to `VaultImportRow`, after `summary_json`:

```rust
    /// Lifecycle stage while the session is live; `None` once it is over.
    pub stage: Option<String>,
    /// Absolute path to the staging folder on the client that owns it.
    pub staging_dir: Option<String>,
    /// Which install created the session.
    pub device_id: Option<String>,
    /// Import form snapshot, for restoring the screen.
    pub form_json: Option<String>,
    /// Source path, size, mtime, and message count.
    pub source_fingerprint: Option<String>,
```

Extend the column list and the row mapper. `VAULT_IMPORT_COLUMNS` becomes:

```rust
const VAULT_IMPORT_COLUMNS: &str = "id, account_id, source, tool, mode, status, started_at, \
     finished_at, message_count, attachment_count, bytes_uploaded, duration_ms, parse_ms, \
     attachments_ms, prepare_ms, upload_ms, summary_json, stage, staging_dir, device_id, \
     form_json, source_fingerprint";
```

and `vault_import_from_row` gains, after `summary_json: row.try_get(16)?`:

```rust
        stage: row.try_get(17)?,
        staging_dir: row.try_get(18)?,
        device_id: row.try_get(19)?,
        form_json: row.try_get(20)?,
        source_fingerprint: row.try_get(21)?,
```

- [ ] **Step 5: Rewrite `start_import` to carry the session and surface the conflict**

Replace the existing `start_import` with the argument struct, the typed error, and a unique-violation mapping. The index is the enforcement; the error type is how the API learns about it.

```rust
/// Everything recorded when a session begins.
pub struct StartImportArgs<'a> {
    /// Owning vault account.
    pub account_id: &'a str,
    /// IR source family (`imessage`, `whatsapp`, …), not a method id.
    pub source: &'a str,
    /// Import mode recorded by the importer.
    pub mode: &'a str,
    /// Client/tool name, when the caller names one.
    pub tool: Option<&'a str>,
    /// Stage the session opens at.
    pub stage: ImportStage,
    /// Absolute staging path on the client.
    pub staging_dir: Option<&'a str>,
    /// Which install is creating this session.
    pub device_id: Option<&'a str>,
    /// Import form snapshot as JSON.
    pub form_json: Option<&'a str>,
    /// Source fingerprint as JSON.
    pub source_fingerprint: Option<&'a str>,
}

/// Why a session could not be started.
#[derive(Debug)]
pub enum StartImportError {
    /// This account already has a live session. The partial unique index
    /// rejected the insert, so this holds even against a racing client.
    AlreadyActive,
    /// Anything else.
    Db(anyhow::Error),
}

impl std::fmt::Display for StartImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyActive => write!(f, "this account already has an active import session"),
            Self::Db(e) => write!(f, "{e}"),
        }
    }
}

/// Open a new import session.
///
/// # Errors
///
/// [`StartImportError::AlreadyActive`] when a live session already exists
/// for this account; [`StartImportError::Db`] for any other failure.
pub async fn start_import(
    conn: &mut AnyConnection,
    args: &StartImportArgs<'_>,
) -> std::result::Result<i64, StartImportError> {
    let started_at = Utc::now().to_rfc3339();
    let inserted: std::result::Result<i64, sqlx::Error> = sqlx::query_scalar(
        r#"
        INSERT INTO vault_imports (
            account_id, source, tool, mode, status, started_at,
            message_count, attachment_count, bytes_uploaded,
            stage, staging_dir, device_id, form_json, source_fingerprint
        ) VALUES ($1, $2, $3, $4, 'running', $5, 0, 0, 0, $6, $7, $8, $9, $10)
        RETURNING id
        "#,
    )
    .bind(args.account_id)
    .bind(args.source)
    .bind(args.tool)
    .bind(args.mode)
    .bind(started_at)
    .bind(args.stage.as_str())
    .bind(args.staging_dir)
    .bind(args.device_id)
    .bind(args.form_json)
    .bind(args.source_fingerprint)
    .fetch_one(&mut *conn)
    .await;

    match inserted {
        Ok(id) => Ok(id),
        Err(err) if is_unique_violation(&err) => Err(StartImportError::AlreadyActive),
        Err(err) => Err(StartImportError::Db(err.into())),
    }
}

/// Whether this error is a unique-constraint violation on either engine.
///
/// SQLite reports `2067` / `1555`; Postgres reports SQLSTATE `23505`.
fn is_unique_violation(err: &sqlx::Error) -> bool {
    let Some(db_err) = err.as_database_error() else {
        return false;
    };
    if db_err.code().as_deref() == Some("23505") {
        return true;
    }
    matches!(db_err.code().as_deref(), Some("2067") | Some("1555"))
}
```

If `sqlx`'s `Any` driver does not expose SQLite's extended result code through `code()`, fall back to matching the message for `UNIQUE constraint failed: vault_imports.account_id` — but check `code()` first and say in your report which one actually fires. The test is the arbiter.

- [ ] **Step 6: Add the three session queries**

```rust
/// The account's live session, if it has one.
///
/// "Live" is `status = 'running'` — the same predicate the partial unique
/// index uses, so this can never return two rows.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn get_active_import(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Option<VaultImportRow>> {
    let row = sqlx::query(&format!(
        "SELECT {VAULT_IMPORT_COLUMNS}
         FROM vault_imports
         WHERE account_id = $1 AND status = 'running'"
    ))
    .bind(account_id)
    .fetch_optional(&mut *conn)
    .await?;
    match row {
        Some(data) => Ok(Some(vault_import_from_row(&data)?)),
        None => Ok(None),
    }
}

/// Move a live session to another stage.
///
/// # Errors
///
/// [`ImportLookupError::NotFound`] when the account owns no such import,
/// [`ImportLookupError::InvalidSession`] when it is no longer running.
pub async fn set_import_stage(
    conn: &mut AnyConnection,
    account_id: &str,
    import_id: i64,
    stage: ImportStage,
) -> std::result::Result<(), ImportLookupError> {
    let existing = get_owned_import(&mut *conn, account_id, import_id).await?;
    if existing.status != "running" {
        return Err(ImportLookupError::InvalidSession {
            message: format!(
                "import {import_id} is not running (status={})",
                existing.status
            ),
        });
    }
    sqlx::query("UPDATE vault_imports SET stage = $1 WHERE id = $2 AND account_id = $3")
        .bind(stage.as_str())
        .bind(import_id)
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Close a live session the user gave up on.
///
/// Records `cancelled` and clears `stage`, which frees the account's
/// single active slot. Nothing reclaims a session on a timer — a session
/// is broken by an explicit discard or not at all.
///
/// # Errors
///
/// [`ImportLookupError::NotFound`] when the account owns no such import,
/// [`ImportLookupError::InvalidSession`] when it is no longer running.
pub async fn discard_import(
    conn: &mut AnyConnection,
    account_id: &str,
    import_id: i64,
) -> std::result::Result<(), ImportLookupError> {
    let existing = get_owned_import(&mut *conn, account_id, import_id).await?;
    if existing.status != "running" {
        return Err(ImportLookupError::InvalidSession {
            message: format!(
                "import {import_id} is not running (status={})",
                existing.status
            ),
        });
    }
    sqlx::query(
        "UPDATE vault_imports
         SET status = 'cancelled', stage = NULL, finished_at = $1
         WHERE id = $2 AND account_id = $3",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(import_id)
    .bind(account_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
```

- [ ] **Step 7: Clear `stage` when a run completes**

`complete_import` already sets `status` and `finished_at`. Add `stage = NULL` to its `UPDATE` so a finished run carries no stage — otherwise `get_active_import` stays correct (it keys on status) but a completed row would still claim to be at `pushing`.

- [ ] **Step 8: Fix the existing `start_import` call site**

`crates/vault/server/src/import/mod.rs` around line 815 calls the old positional signature. Task 3 rewrites that handler properly; for now, make it compile with a `StartImportArgs` carrying `stage: ImportStage::Parse` and `None` for the session fields, mapping `StartImportError` to `ApiError::Internal`. Search for others:

```bash
grep -rn "start_import(" crates/ --include=*.rs
```

- [ ] **Step 9: Run the tests**

Run: `cargo test -p message-vault-server`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
cargo fmt --all -- --check
git add crates/vault/server/src
git commit -m "feat(vault): read and write the import session record"
```

---

### Task 3: The session endpoints

**Files:**
- Modify: `crates/vault/server/src/import/mod.rs` — `CreateImportBody` (around line 595), `imports_create_handler` (around line 793), plus three new handlers and their bodies
- Modify: `crates/vault/server/src/openapi.rs` (the `.routes(routes!(...))` block around line 114)
- Modify: `docs/src/assets/openapi.json` (regenerated)
- Test: `crates/vault/server/src/server.rs` tests module

**Interfaces:**
- Consumes: everything Task 2 produced.
- Produces:
  - `GET /v1/imports/active` → `{ ok: true, session: ActiveImportSession | null }` where `ActiveImportSession` carries `id`, `source`, `mode`, `status`, `started_at`, `stage`, `staging_dir`, `device_id`, `form`, `source_fingerprint`. `form` and `source_fingerprint` are parsed JSON values, not strings.
  - `POST /v1/imports/{id}/stage`, body `{ "stage": "<one of six>" }` → `{ ok: true, stage }`
  - `POST /v1/imports/{id}/discard` → `{ ok: true, id, status: "cancelled" }`
  - `CreateImportBody` gains `stage`, `staging_dir`, `device_id`, `form`, `source_fingerprint`, all optional; a conflicting create returns **409**.

Task 4 types these in the web client.

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `crates/vault/server/src/server.rs`, beside the existing `imports_*` tests. Import the new handlers alongside the ones already imported around line 790.

```rust
    #[tokio::test]
    async fn active_session_is_empty_then_reports_the_live_one() {
        let (_tmp, state, token, import_id) = test_state().await;

        let body = CreateImportBody {
            source: "imessage".into(),
            mode: "append".into(),
            tool: Some("message-vault-io".into()),
            account: None,
            stage: Some("write".into()),
            staging_dir: Some("/home/u/message-vault/staging-260830".into()),
            device_id: Some("device-a".into()),
            form: Some(serde_json::json!({ "source": "imessage-ios" })),
            source_fingerprint: Some(serde_json::json!({ "size_bytes": 42 })),
        };
        // `test_state` already opened a session; close it so this one can start.
        imports_discard_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
        )
        .await
        .unwrap();

        let created = imports_create_handler(State(state.clone()), auth_headers(&token), Json(body))
            .await
            .unwrap();

        let active = imports_active_handler(State(state.clone()), auth_headers(&token))
            .await
            .unwrap();
        let session = active.0.session.expect("a live session is reported");
        assert_eq!(session.id, created.0.id);
        assert_eq!(session.stage.as_deref(), Some("write"));
        assert_eq!(
            session.staging_dir.as_deref(),
            Some("/home/u/message-vault/staging-260830")
        );
        assert_eq!(session.device_id.as_deref(), Some("device-a"));
        assert_eq!(session.form["source"], "imessage-ios");
    }

    #[tokio::test]
    async fn a_second_session_is_refused_with_conflict() {
        let (_tmp, state, token, _import_id) = test_state().await;
        let body = CreateImportBody {
            source: "imessage".into(),
            mode: "append".into(),
            tool: None,
            account: None,
            stage: None,
            staging_dir: None,
            device_id: None,
            form: None,
            source_fingerprint: None,
        };
        let err = imports_create_handler(State(state.clone()), auth_headers(&token), Json(body))
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)));
    }

    #[tokio::test]
    async fn stage_endpoint_advances_and_rejects_an_unknown_stage() {
        let (_tmp, state, token, import_id) = test_state().await;

        imports_stage_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
            Json(SetImportStageBody { stage: "pushing".into() }),
        )
        .await
        .unwrap();
        let active = imports_active_handler(State(state.clone()), auth_headers(&token))
            .await
            .unwrap();
        assert_eq!(active.0.session.unwrap().stage.as_deref(), Some("pushing"));

        let err = imports_stage_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
            Json(SetImportStageBody { stage: "halfway".into() }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[tokio::test]
    async fn discard_frees_the_slot() {
        let (_tmp, state, token, import_id) = test_state().await;
        imports_discard_handler(
            State(state.clone()),
            auth_headers(&token),
            AxumPath(import_id),
        )
        .await
        .unwrap();
        let active = imports_active_handler(State(state.clone()), auth_headers(&token))
            .await
            .unwrap();
        assert!(active.0.session.is_none());
    }
```

`test_state()` currently returns a started import. If its helper calls `start_import` directly, update it for the new signature. If a test asserts a specific `import_id`, keep that behaviour.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p message-vault-server -- active_session a_second_session stage_endpoint discard_frees`
Expected: compile errors — the handlers and body fields do not exist.

- [ ] **Step 3: Widen `CreateImportBody` and return 409**

Add to `CreateImportBody`:

```rust
    /// Stage the session opens at. Defaults to `parse`.
    #[serde(default)]
    pub(crate) stage: Option<String>,
    /// Absolute staging path on the client that owns this session.
    #[serde(default)]
    pub(crate) staging_dir: Option<String>,
    /// Which install is creating the session.
    #[serde(default)]
    pub(crate) device_id: Option<String>,
    /// Import form snapshot, stored so the screen can be restored.
    #[serde(default)]
    pub(crate) form: Option<serde_json::Value>,
    /// Source path, size, mtime, and message count.
    #[serde(default)]
    pub(crate) source_fingerprint: Option<serde_json::Value>,
```

In `imports_create_handler`, validate the stage, serialize the two JSON values, and map the conflict:

```rust
    let stage = match body.stage.as_deref() {
        None => crate::db::vault_imports::ImportStage::Parse,
        Some(raw) => crate::db::vault_imports::ImportStage::parse(raw).ok_or_else(|| {
            ApiError::BadRequest(format!(
                "invalid import stage '{raw}'; expected one of parse, write, awaiting_gate_1, transcode, awaiting_gate_2, pushing"
            ))
        })?,
    };
    let form_json = optional_json_string(body.form.as_ref(), "form")?;
    let fingerprint_json = optional_json_string(body.source_fingerprint.as_ref(), "source_fingerprint")?;

    // Below, replacing the existing `start_import(...)` call that
    // follows `ensure_account_row`:
    let args = crate::db::vault_imports::StartImportArgs {
        account_id: account,
        source: &body.source,
        mode: &body.mode,
        tool: body.tool.as_deref(),
        stage,
        staging_dir: body.staging_dir.as_deref(),
        device_id: body.device_id.as_deref(),
        form_json: form_json.as_deref(),
        source_fingerprint: fingerprint_json.as_deref(),
    };
    let id = crate::db::vault_imports::start_import(&mut conn, &args)
        .await
        .map_err(|e| match e {
            crate::db::vault_imports::StartImportError::AlreadyActive => ApiError::Conflict(
                "this account already has an active import session".into(),
            ),
            crate::db::vault_imports::StartImportError::Db(err) => ApiError::Internal(err.to_string()),
        })?;
```

with this helper beside `validate_import_status`:

```rust
/// Serialize an optional JSON body field for storage as TEXT.
fn optional_json_string(
    value: Option<&serde_json::Value>,
    field: &str,
) -> Result<Option<String>, ApiError> {
    match value {
        None => Ok(None),
        Some(v) => serde_json::to_string(v)
            .map(Some)
            .map_err(|e| ApiError::Internal(format!("serialize {field}: {e}"))),
    }
}
```

- [ ] **Step 4: Add the three handlers**

```rust
/// One live import session, as the desktop app needs to resume it.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ActiveImportSession {
    pub(crate) id: i64,
    pub(crate) source: String,
    pub(crate) mode: String,
    pub(crate) status: String,
    pub(crate) started_at: String,
    pub(crate) stage: Option<String>,
    pub(crate) staging_dir: Option<String>,
    pub(crate) device_id: Option<String>,
    /// Import form snapshot, or null.
    pub(crate) form: serde_json::Value,
    /// Source path, size, mtime, and message count, or null.
    pub(crate) source_fingerprint: serde_json::Value,
}

/// The account's live session, or null when there is none.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ActiveImportResponse {
    ok: bool,
    pub(crate) session: Option<ActiveImportSession>,
}

/// The account's active import session, if it has one.
#[utoipa::path(
    get,
    path = "/v1/imports/active",
    tag = "Import",
    security(("bearer" = [])),
    responses(
        (status = 200, body = ActiveImportResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn imports_active_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ActiveImportResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;
    let account = resolve_import_account(&auth, None, &state.db).await?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let row = crate::db::vault_imports::get_active_import(&mut conn, account)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(ActiveImportResponse {
        ok: true,
        session: row.map(|row| ActiveImportSession {
            id: row.id,
            source: row.source,
            mode: row.mode,
            status: row.status,
            started_at: row.started_at,
            stage: row.stage,
            staging_dir: row.staging_dir,
            device_id: row.device_id,
            form: parse_summary_json(row.form_json),
            source_fingerprint: parse_summary_json(row.source_fingerprint),
        }),
    }))
}

/// New stage for a live session.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct SetImportStageBody {
    pub(crate) stage: String,
}

/// Confirmation that the stage moved.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct SetImportStageResponse {
    ok: bool,
    pub(crate) stage: String,
}

/// Move a live import session to another stage.
#[utoipa::path(
    post,
    path = "/v1/imports/{id}/stage",
    tag = "Import",
    security(("bearer" = [])),
    request_body = SetImportStageBody,
    responses(
        (status = 200, body = SetImportStageResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn imports_stage_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(import_id): AxumPath<i64>,
    Json(body): Json<SetImportStageBody>,
) -> Result<Json<SetImportStageResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;
    let account = resolve_import_account(&auth, None, &state.db).await?;
    let stage = crate::db::vault_imports::ImportStage::parse(&body.stage).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "invalid import stage '{}'; expected one of parse, write, awaiting_gate_1, transcode, awaiting_gate_2, pushing",
            body.stage
        ))
    })?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    crate::db::vault_imports::set_import_stage(&mut conn, account, import_id, stage).await?;
    Ok(Json(SetImportStageResponse {
        ok: true,
        stage: stage.as_str().to_string(),
    }))
}

/// Confirmation that a session was discarded.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct DiscardImportResponse {
    ok: bool,
    pub(crate) id: i64,
    pub(crate) status: String,
}

/// Discard a live import session, freeing the account's single slot.
#[utoipa::path(
    post,
    path = "/v1/imports/{id}/discard",
    tag = "Import",
    security(("bearer" = [])),
    responses(
        (status = 200, body = DiscardImportResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn imports_discard_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(import_id): AxumPath<i64>,
) -> Result<Json<DiscardImportResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;
    let account = resolve_import_account(&auth, None, &state.db).await?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    crate::db::vault_imports::discard_import(&mut conn, account, import_id).await?;
    Ok(Json(DiscardImportResponse {
        ok: true,
        id: import_id,
        status: "cancelled".into(),
    }))
}
```

`parse_summary_json` already exists in this file (around line 900) and returns `Value::Null` for `None` — exactly the shape wanted here.

`ImportLookupError` already converts into `ApiError` (`server.rs:258`), so `?` on the two db calls works.

- [ ] **Step 5: Register the routes**

In `crates/vault/server/src/openapi.rs`, beside the existing import routes:

```rust
        .routes(routes!(crate::import::imports_active_handler))
        .routes(routes!(crate::import::imports_stage_handler))
        .routes(routes!(crate::import::imports_discard_handler))
```

Watch the ordering: `/v1/imports/active` must not be captured by `/v1/imports/{id}`. Axum's router prefers the literal segment, but assert it — the `active_session_is_empty_then_reports_the_live_one` test goes through the handler directly, so add one route-level check if the existing test module has a helper for hitting paths (`crate::test_support::get_status`, used around `server.rs:1311`).

- [ ] **Step 6: Run the tests and regenerate the spec**

```bash
cargo test -p message-vault-server
cargo run -p message-vault-server -- dump-openapi --output docs/src/assets/openapi.json
cargo test -p message-vault-server committed_openapi_matches_dump
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all -- --check
git add crates/vault/server/src docs/src/assets/openapi.json
git commit -m "feat(server): read, advance, and discard the active import session"
```

---

### Task 4: The client's side of the record — device id, fingerprint, API

**Files:**
- Modify: `src-tauri/src/commands/paths.rs` (`PathStat`, `path_stat_inner`)
- Modify: `web/src/lib/tauri.ts` (the `PathStat` interface around line 158)
- Create: `web/src/lib/deviceId.ts`
- Create: `web/src/lib/importSession.ts`
- Test: `web/src/lib/deviceId.test.ts`, `web/src/lib/importSession.test.ts` (create both)

**Interfaces:**
- Consumes: Task 3's endpoints.
- Produces:
  - `getDeviceId(): string` from `deviceId.ts` — a stable id in `localStorage` under `mv-device-id`, generated on first read.
  - From `importSession.ts`: `type ActiveImportSession` mirroring the server shape; `getActiveImportSession(): Promise<ActiveImportSession | null>`; `setImportStage(id: number, stage: ImportStage): Promise<void>`; `discardImportSession(id: number): Promise<void>`; `type ImportStage`; `buildSourceFingerprint(path: string, stat: PathStat): SourceFingerprint`.
  - `PathStat` gains `sizeBytes: number` and `modifiedUnixMs: number | null`.

Tasks 5 and 6 use all of it.

- [ ] **Step 1: Write the failing tests**

`web/src/lib/deviceId.test.ts`:

```ts
import { beforeEach, describe, expect, it } from "vitest";
import { DEVICE_ID_KEY, getDeviceId } from "./deviceId";

beforeEach(() => {
  localStorage.clear();
});

describe("getDeviceId", () => {
  it("generates an id on first read and keeps it", () => {
    const first = getDeviceId();
    expect(first).toMatch(/^[0-9a-f-]{36}$/);
    expect(getDeviceId()).toBe(first);
    expect(localStorage.getItem(DEVICE_ID_KEY)).toBe(first);
  });

  it("reuses an id already stored", () => {
    localStorage.setItem(DEVICE_ID_KEY, "11111111-2222-3333-4444-555555555555");
    expect(getDeviceId()).toBe("11111111-2222-3333-4444-555555555555");
  });
});
```

`web/src/lib/importSession.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { buildSourceFingerprint } from "./importSession";

describe("buildSourceFingerprint", () => {
  it("records the path, size, and mtime of the backup", () => {
    expect(
      buildSourceFingerprint("/Users/u/Backup/abc", {
        exists: true,
        isFile: false,
        isDirectory: true,
        sizeBytes: 4096,
        modifiedUnixMs: 1_756_512_000_000,
      }),
    ).toEqual({
      path: "/Users/u/Backup/abc",
      size_bytes: 4096,
      modified_unix_ms: 1_756_512_000_000,
      message_count: null,
    });
  });

  it("leaves the message count null until parse has run", () => {
    const fp = buildSourceFingerprint("/b", {
      exists: true,
      isFile: true,
      isDirectory: false,
      sizeBytes: 1,
      modifiedUnixMs: null,
    });
    expect(fp.message_count).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify they fail**

Run from `web/`: `npx vitest run src/lib/deviceId.test.ts src/lib/importSession.test.ts`
Expected: FAIL — neither module exists.

- [ ] **Step 3: Give `PathStat` a size and an mtime**

The fingerprint needs them and the column is decoration without them. In `src-tauri/src/commands/paths.rs`, add to the struct:

```rust
    /// Size in bytes; `0` when the path does not exist.
    pub size_bytes: u64,
    /// Last modification time in milliseconds since the Unix epoch, or
    /// `None` when the platform does not report one.
    pub modified_unix_ms: Option<i64>,
```

and in `path_stat_inner`, replace the final `Ok(PathStat { ... })` with a version that reads the metadata once:

```rust
    let path = Path::new(trimmed);
    let meta = std::fs::metadata(path).ok();
    let modified_unix_ms = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok());
    Ok(PathStat {
        exists: path.exists(),
        is_file: path.is_file(),
        is_directory: path.is_dir(),
        size_bytes: meta.as_ref().map(std::fs::Metadata::len).unwrap_or(0),
        modified_unix_ms,
    })
```

The early return for an empty path gains `size_bytes: 0, modified_unix_ms: None`. The struct already carries `#[serde(rename_all = "camelCase")]`, so the wire names are `sizeBytes` and `modifiedUnixMs`.

Update the TypeScript twin in `web/src/lib/tauri.ts`:

```ts
export interface PathStat {
  exists: boolean;
  isFile: boolean;
  isDirectory: boolean;
  sizeBytes: number;
  modifiedUnixMs: number | null;
}
```

Then fix the fallout: `web/src/screens/ImportScreen.tsx` has a `mapPathStat` (around line 40) and an `emptyImessagePathStats` that construct `PathStat` literals. Find them all:

```bash
cd web && npx tsc --noEmit
```

- [ ] **Step 4: Write `deviceId.ts`**

```ts
/** localStorage key for this install's stable identifier. */
export const DEVICE_ID_KEY = "mv-device-id";

let cached: string | null = null;

/**
 * Stable id for this install.
 *
 * A session records which install created it, so a different machine can
 * say where the staged work lives instead of failing to open a path that
 * was never local to it.
 *
 * Generated on first read and kept in localStorage. When storage is
 * unavailable the id lives only in memory for this page, which degrades
 * to "this looks like a different install after a reload" — the resume
 * screen handles that case rather than breaking.
 */
export function getDeviceId(): string {
  if (cached) return cached;
  try {
    const stored = localStorage.getItem(DEVICE_ID_KEY)?.trim();
    if (stored) {
      cached = stored;
      return stored;
    }
  } catch {
    // Private browsing and full storage can throw.
  }
  const fresh = crypto.randomUUID();
  cached = fresh;
  try {
    localStorage.setItem(DEVICE_ID_KEY, fresh);
  } catch {
    // Keep the in-memory value.
  }
  return fresh;
}
```

- [ ] **Step 5: Write `importSession.ts`**

```ts
import { apiClient } from "./api";
import type { PathStat } from "./tauri";

/** Where a live import session is. Mirrors the vault's `ImportStage`. */
export type ImportStage =
  | "parse"
  | "write"
  | "awaiting_gate_1"
  | "transcode"
  | "awaiting_gate_2"
  | "pushing";

/** Identity of the backup a session was started from. */
export type SourceFingerprint = {
  path: string;
  size_bytes: number;
  modified_unix_ms: number | null;
  /** Filled in after parse; null until then. */
  message_count: number | null;
};

/** The account's live import session, as the vault reports it. */
export type ActiveImportSession = {
  id: number;
  source: string;
  mode: string;
  status: string;
  started_at: string;
  stage: ImportStage | null;
  staging_dir: string | null;
  device_id: string | null;
  form: unknown;
  source_fingerprint: SourceFingerprint | null;
};

/** The account's live session, or null when there is none. */
export async function getActiveImportSession(): Promise<ActiveImportSession | null> {
  const res = await apiClient.get<{ session: ActiveImportSession | null }>("/v1/imports/active");
  return res.session ?? null;
}

/** Move a live session to another stage. */
export async function setImportStage(id: number, stage: ImportStage): Promise<void> {
  await apiClient.post(`/v1/imports/${String(id)}/stage`, { stage });
}

/** Close a session the user gave up on, freeing the account's slot. */
export async function discardImportSession(id: number): Promise<void> {
  await apiClient.post(`/v1/imports/${String(id)}/discard`, {});
}

/**
 * Identity of the backup this session reads.
 *
 * The message count is unknown until parse finishes, so it starts null
 * and is filled in afterwards.
 */
export function buildSourceFingerprint(path: string, stat: PathStat): SourceFingerprint {
  return {
    path,
    size_bytes: stat.sizeBytes,
    modified_unix_ms: stat.modifiedUnixMs,
    message_count: null,
  };
}
```

Check `apiClient`'s actual method names and signatures in `web/src/lib/api.ts` before writing this — `get` and `post` are used elsewhere in the tree (`useStorageData.ts:15`, `useImportJob.ts`), so follow those call shapes exactly.

- [ ] **Step 6: Run the tests**

Run from `web/`: `npx vitest run src/lib/deviceId.test.ts src/lib/importSession.test.ts && npx tsc --noEmit && npm test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cd web && npm run lint && cd ..
git add src-tauri/src web/src
git commit -m "feat: device id, source fingerprint, and the session API client"
```

---

### Task 5: The import writes its own session record

**Files:**
- Modify: `web/src/screens/import/useImportJob.ts` — the `POST /v1/imports` call (around line 255) and the stage transitions through `startImport`
- Test: `web/src/screens/import/useImportJob.test.tsx` (exists from Phase 1; extend it)

**Interfaces:**
- Consumes: `getDeviceId`, `setImportStage`, `buildSourceFingerprint`, `ActiveImportSession` from Task 4; the widened `POST /v1/imports` body from Task 3.
- Produces: `useImportJob` returns `importSessionId: number | null` alongside its existing fields, and accepts an optional resume argument (Task 6 supplies it). The session row now carries `staging_dir` from the moment the folder is known.

The stage transitions this phase can make, and no others:

| When | Stage written |
|---|---|
| Session created, before extract | `parse` |
| Extract returns, before push | `pushing` |

`write` is not separately observable until Phase 4 splits prepare, so extract spans `parse` and the record stays at `parse` until push begins. Do not invent a transition the pipeline cannot signal.

- [ ] **Step 1: Write the failing test**

`web/src/screens/import/useImportJob.test.tsx` exists from Phase 1. Its harness gives you `runMock`, `postMock`, `resolveImportStagingDirMock`, a `failedReport()` builder, and a `baseForm`, behind `vi.mock` calls for `../../hooks/useTauriJob`, `../../lib/api`, `../../lib/auth`, `../../lib/system-settings`, and `../../lib/tauri-check`. Reuse all of it.

**One addition is mandatory before your test can run.** That file does *not* mock `../../lib/tauri` — Phase 1 only imported types from it, which erase at compile time. Task 5 makes `useImportJob` call `invokePathStat` at runtime, and importing the real module under jsdom pulls in `@tauri-apps/api/core`. Add this mock beside the others, above the dynamic `import("./useImportJob")`:

```ts
const invokePathStatMock = vi.fn();

vi.mock("../../lib/tauri", () => ({
  invokeExtract: vi.fn(),
  invokePush: vi.fn(),
  invokePathStat: (...args: unknown[]) => invokePathStatMock(...args),
}));
```

Check which symbols `useImportJob.ts` actually imports from `../../lib/tauri` and mock exactly those — an incomplete factory throws at import time rather than failing an assertion, which is a confusing way to discover the problem.

Then add the two tests, driving a run the same way Phase 1's tests do:

```ts
  it("records the staging folder and device on the session it creates", async () => {
    resolveImportStagingDirMock.mockResolvedValue("/home/u/message-vault/staging-260830");
    invokePathStatMock.mockResolvedValue({
      exists: true,
      isFile: false,
      isDirectory: true,
      sizeBytes: 4096,
      modifiedUnixMs: 1_756_512_000_000,
    });
    postMock.mockResolvedValue({ id: 42 });
    runMock.mockResolvedValue({ summary: "ok", report: failedReport() });

    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport(baseForm);
    });

    const createCall = postMock.mock.calls.find(([path]) => path === "/v1/imports");
    expect(createCall).toBeDefined();
    const body = createCall?.[1] as Record<string, unknown>;
    expect(body.stage).toBe("parse");
    expect(body.device_id).toEqual(expect.any(String));
    expect(body.staging_dir).toBe("/home/u/message-vault/staging-260830");
    expect(body.form).toMatchObject({ source: "imessage-ios" });
  });

  it("keeps the backup password out of the stored form snapshot", async () => {
    resolveImportStagingDirMock.mockResolvedValue("/tmp/staging");
    invokePathStatMock.mockResolvedValue(null);
    postMock.mockResolvedValue({ id: 43 });
    runMock.mockResolvedValue({ summary: "ok", report: failedReport() });

    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport({ ...baseForm, backupPassword: "hunter2" });
    });

    const body = postMock.mock.calls.find(([path]) => path === "/v1/imports")?.[1] as Record<
      string,
      unknown
    >;
    expect(JSON.stringify(body.form)).not.toContain("hunter2");
  });

  it("moves the session to pushing before the upload starts", async () => {
    resolveImportStagingDirMock.mockResolvedValue("/tmp/staging");
    invokePathStatMock.mockResolvedValue(null);
    postMock.mockResolvedValue({ id: 44 });
    runMock.mockResolvedValue({ summary: "ok", report: failedReport() });

    const { result } = renderHook(() => useImportJob());
    await act(async () => {
      await result.current.startImport(baseForm);
    });

    const stageCall = postMock.mock.calls.find(([path]) => String(path).endsWith("/stage"));
    expect(stageCall?.[1]).toEqual({ stage: "pushing" });
  });
```

`invokePathStatMock.mockResolvedValue(null)` exercises the branch where the backup cannot be stat'd — the create body then carries `source_fingerprint: null` rather than failing the run. Adjust the mock's shape if the code you write awaits it differently; the assertions are the contract.

- [ ] **Step 2: Run to verify it fails**

Run from `web/`: `npx vitest run src/screens/import/useImportJob.test.tsx`
Expected: FAIL — the create body has no `stage`/`device_id`/`staging_dir`, and no `/stage` call is made.

- [ ] **Step 3: Send the session fields when creating**

`resolveImportStagingDir` currently runs *after* the session is created. Swap the order so the staging path is known first, then include it. In `startImport`:

```ts
      outputDir = await resolveImportStagingDir(form.backupPath, form.source);
      setStagingDir(outputDir);

      const backupStat = await invokePathStat(form.backupPath).catch(() => null);
      const importSession = await apiClient.post<{ id: number }>("/v1/imports", {
        ...importSessionCreateBody(form.source),
        stage: "parse",
        staging_dir: outputDir,
        device_id: getDeviceId(),
        form,
        source_fingerprint: backupStat
          ? buildSourceFingerprint(form.backupPath, backupStat)
          : null,
      });
      importSessionId = importSession.id;
```

`form` is the `ImportJobFormValues` the caller passed. It carries `backupPassword` and `whatsappKey` — **strip both before storing**: the record exists to restore a screen, not to keep a credential in the vault database. Write a small local helper next to `startImport`:

```ts
/** Form snapshot for the session record, without the secrets. */
function formSnapshot(form: ImportJobFormValues): Record<string, unknown> {
  const { backupPassword: _backupPassword, whatsappKey: _whatsappKey, ...rest } = form;
  return rest;
}
```

and send `form: formSnapshot(form)`.

- [ ] **Step 4: Advance the stage before pushing**

Immediately before the `invokePush` call (around line 355, after the four steps are set and `activeStepRef.current = "upload"`):

```ts
      if (importSessionId != null) {
        // Best effort: a stale stage costs a slower resume, never a wrong
        // one — resume correctness is recomputed from the folder.
        await setImportStage(importSessionId, "pushing").catch(() => {});
      }
```

- [ ] **Step 5: Return the session id**

Add `importSessionId` to the hook's returned object, backed by a `useState` set when the session is created and cleared in `returnToForm`. Task 6 needs it to offer a discard.

- [ ] **Step 6: Run the tests**

Run from `web/`: `npm test && npx tsc --noEmit && npm run lint`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add web/src
git commit -m "feat(web): an import writes its own session record as it runs"
```

---

### Task 6: Entering Import asks the vault first

**Files:**
- Create: `web/src/screens/import/ResumeImportPanel.tsx`
- Create: `web/src/screens/import/resumeDecision.ts`
- Modify: `web/src/screens/ImportScreen.tsx` (the top of the component, around line 58; the `phase === "form"` branch, around line 335)
- Test: `web/src/screens/import/resumeDecision.test.ts`, `web/src/screens/import/ResumeImportPanel.test.tsx` (create both)

**Interfaces:**
- Consumes: `getActiveImportSession`, `discardImportSession`, `getDeviceId`, `ActiveImportSession` from Task 4; `invokePathStat` from `lib/tauri`.
- Produces: `resumeDecisionFor(args): ResumeDecision` — a pure function, and the reason this task is testable. `ResumeImportPanel` renders a decision and calls back on the user's choice.

**The decision table**, from spec decision 36 restricted to what Phase 2 can reach:

| Condition | `kind` | What the panel offers |
|---|---|---|
| No session | `none` | Nothing — the form renders |
| `device_id` differs from this install | `other_device` | Discard only, and say where it belongs |
| Staging folder missing at `staging_dir` | `folder_missing` | Discard only |
| Stage `pushing`, folder present | `resume_push` | Resume upload, or discard |
| Stage `parse` or `write`, folder present | `restart` | Start over with the same settings, or discard |
| Any other stage, folder present | `restart` | Same — gates do not exist yet |

- [ ] **Step 1: Write the failing test**

`web/src/screens/import/resumeDecision.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { ActiveImportSession } from "../../lib/importSession";
import { resumeDecisionFor } from "./resumeDecision";

function session(overrides: Partial<ActiveImportSession> = {}): ActiveImportSession {
  return {
    id: 7,
    source: "imessage",
    mode: "append",
    status: "running",
    started_at: "2026-08-30T00:00:00Z",
    stage: "pushing",
    staging_dir: "/home/u/message-vault/staging-260830",
    device_id: "this-device",
    form: { source: "imessage-ios" },
    source_fingerprint: null,
    ...overrides,
  };
}

describe("resumeDecisionFor", () => {
  it("has nothing to decide without a session", () => {
    expect(
      resumeDecisionFor({ session: null, deviceId: "this-device", folderExists: false }).kind,
    ).toBe("none");
  });

  it("says where a session belongs when another install owns it", () => {
    const decision = resumeDecisionFor({
      session: session({ device_id: "other-device" }),
      deviceId: "this-device",
      folderExists: true,
    });
    expect(decision.kind).toBe("other_device");
    expect(decision.canResume).toBe(false);
  });

  it("offers discard alone when the staging folder is gone", () => {
    const decision = resumeDecisionFor({
      session: session(),
      deviceId: "this-device",
      folderExists: false,
    });
    expect(decision.kind).toBe("folder_missing");
    expect(decision.canResume).toBe(false);
  });

  it("resumes the upload when a push was interrupted", () => {
    const decision = resumeDecisionFor({
      session: session({ stage: "pushing" }),
      deviceId: "this-device",
      folderExists: true,
    });
    expect(decision.kind).toBe("resume_push");
    expect(decision.canResume).toBe(true);
  });

  it("restarts when the run died before the folder was finished", () => {
    for (const stage of ["parse", "write"] as const) {
      expect(
        resumeDecisionFor({
          session: session({ stage }),
          deviceId: "this-device",
          folderExists: true,
        }).kind,
      ).toBe("restart");
    }
  });

  it("treats a missing device id as this install rather than locking the user out", () => {
    expect(
      resumeDecisionFor({
        session: session({ device_id: null }),
        deviceId: "this-device",
        folderExists: true,
      }).kind,
    ).toBe("resume_push");
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run from `web/`: `npx vitest run src/screens/import/resumeDecision.test.ts`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write the decision**

```ts
import type { ActiveImportSession } from "../../lib/importSession";

/** What entering Import should do about a session that already exists. */
export type ResumeDecision = {
  kind: "none" | "other_device" | "folder_missing" | "resume_push" | "restart";
  /** Whether staged work can be picked up rather than redone. */
  canResume: boolean;
  session: ActiveImportSession | null;
};

/**
 * Decide what to show when Import opens and the vault reports a session.
 *
 * Pure so the table can be read and tested on its own: the caller does the
 * network and filesystem work and hands the answers in.
 *
 * A session with no recorded device is treated as this install's. The
 * column is new, so an older session predates it, and locking someone out
 * of their own staged work over a missing field would be worse than the
 * rare case of two installs sharing a vault.
 */
export function resumeDecisionFor(args: {
  session: ActiveImportSession | null;
  deviceId: string;
  folderExists: boolean;
}): ResumeDecision {
  const { session, deviceId, folderExists } = args;
  if (!session) return { kind: "none", canResume: false, session: null };
  if (session.device_id && session.device_id !== deviceId) {
    return { kind: "other_device", canResume: false, session };
  }
  if (!session.staging_dir || !folderExists) {
    return { kind: "folder_missing", canResume: false, session };
  }
  if (session.stage === "pushing") {
    return { kind: "resume_push", canResume: true, session };
  }
  return { kind: "restart", canResume: false, session };
}
```

- [ ] **Step 4: Write the panel**

`ResumeImportPanel.tsx` renders one decision. Follow the styling of the existing Import screens — read `ImportProgressView.tsx` and `ImportFormUi.tsx` first and reuse their `Button` and layout primitives rather than inventing any.

Copy, exactly:

| `kind` | Heading | Body | Actions |
|---|---|---|---|
| `resume_push` | `Finish your last import` | `Your messages are staged and ready to upload. Picking up where you left off skips the extract.` | `Upload to vault` (primary), `Discard this import` |
| `restart` | `Pick up your last import` | `The extract did not finish. Starting again reuses your settings and reads the backup from the beginning.` | `Start over` (primary), `Discard this import` |
| `folder_missing` | `The staged files are gone` | `This import's folder is no longer at {staging_dir}. Discarding it lets you start a new one.` | `Discard this import` (primary) |
| `other_device` | `This import belongs to another computer` | `It was started on a different install and its files are staged there. Discarding it lets you start a new import here.` | `Discard this import` (primary) |

Every string states what the user can do. None of them warns.

- [ ] **Step 5: Reconcile on entry**

At the top of `ImportScreen`, before the form renders, resolve the decision once:

```ts
  const [resume, setResume] = useState<ResumeDecision | null>(null);
  const [resumeChecked, setResumeChecked] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const session = await getActiveImportSession();
        const folderExists = session?.staging_dir
          ? ((await probePath(session.staging_dir))?.exists ?? false)
          : false;
        if (!cancelled) {
          setResume(resumeDecisionFor({ session, deviceId: getDeviceId(), folderExists }));
        }
      } catch {
        // A vault that cannot answer is not a reason to block the form.
        if (!cancelled) setResume(null);
      } finally {
        if (!cancelled) setResumeChecked(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);
```

Render `ResumeImportPanel` in place of the form whenever `phase === "form"` and `resume` is not `none`. While `!resumeChecked`, render neither — a flash of the form before the panel replaces it is the bug this phase exists to fix.

Discarding calls `discardImportSession(session.id)` and then clears `resume`, which drops through to the form.

For `restart`, restore the form from `session.form` and start the import again. For `resume_push`, this phase re-runs the push against `staging_dir`; dedupe and asset HEAD-skip absorb the overlap (decision 4), so a re-push is cheap and correct.

- [ ] **Step 6: Run the suite**

Run from `web/`: `npm test && npx tsc --noEmit && npm run lint`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add web/src
git commit -m "feat(web): entering Import resumes the active session instead of reopening the form"
```

---

### Task 7: Say that a schema change rebuilds the vault

An operator upgrading past this release loses their vault and re-imports. Today that is announced only by a line on stderr. The docs must say it before they upgrade.

**Files:**
- Modify: `docs/src/content/docs/vault/user/how-to/update.md` — the update page, confirmed to be the only doc that covers upgrading

**Interfaces:** none.

- [ ] **Step 1: Read the page and match its voice**

`docs/src/content/docs/vault/user/how-to/update.md` is the only page covering upgrades — I checked. Read it first: the published docs must read like a human wrote them, and this repo rejects AI-flavoured and clipped-imperative prose.

- [ ] **Step 2: Write the note**

Add a short section in the docs' own voice — plain English, full sentences, no clipped imperatives, no alarm. Content: a vault built by an older server is rebuilt empty when the schema version changes, so an upgrade means re-importing from your backups; the source backups are what the vault is built from, so nothing is unrecoverable; this release changes the schema.

Do not phrase it as a warning label. State what happens and what the reader does about it.

- [ ] **Step 3: Build the docs**

```bash
cd docs && npm run check && npm run build
```

- [ ] **Step 4: Commit**

```bash
git add docs README.md
git commit -m "docs: a schema change rebuilds the vault and you re-import"
```

---

### Final verification

- [ ] `./scripts/check-pr.sh` — the whole gate in one pass.
- [ ] `./scripts/lint-all.sh` — Clippy is not CI-gated; run it locally.
- [ ] Manually confirm the phase's point: run `./scripts/run-vault-dev.sh`, start an import in the desktop app, reload the page mid-run, and confirm Import reopens on the session rather than the form.
- [ ] Confirm the openapi dump is current: `cargo test -p message-vault-server committed_openapi_matches_dump`.
- [ ] Confirm no `ALTER TABLE ... ADD COLUMN` was added anywhere: `grep -rn "ADD COLUMN" crates/ schema/`.
