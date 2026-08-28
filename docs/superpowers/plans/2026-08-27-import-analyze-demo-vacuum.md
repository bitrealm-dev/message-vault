# Import Analyze and Demo Vacuum Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (inline; user asked to implement). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run `ANALYZE` on `messages`, `attachments`, and `tapbacks` before every promote transaction, and run one `VACUUM` only after `--reset-demo` finishes all three sources, dedupe, and media.

**Architecture:** Dialect helpers return the SQL. `promote_append` calls analyze before `BEGIN` and treats errors as warnings. `reset-demo` (URL path and SQLite prepared path) calls vacuum after `process-assets` and treats errors as warnings. The lookup join and content-key plan stay unchanged.

**Tech Stack:** Rust 2024, `message-vault-server`, sqlx Any, existing `engine::test_pool` and `MV_TEST_POSTGRES_URL` tests.

## Global Constraints

- One promote transaction. Do not commit mid-promote. Do not analyze after `COMMIT`.
- Analyze failure and vacuum failure must not fail import / `reset-demo`.
- CLI and HTTP import do not vacuum.
- Autovacuum stays on. No `VACUUM FULL`. Do not change `_promote_msg_map`.
- Same analyze statements on SQLite and Postgres. Vacuum: named tables on Postgres, whole-file `VACUUM` on SQLite.
- Never commit to `main`. Do not commit dirty `src-tauri/Cargo.lock`.
- Product version files stay at the current lockstep value.
- `cargo fmt` the server crate after Rust edits.

## File map

| File | Responsibility |
|---|---|
| `crates/vault/server/src/db/dialect.rs` | Analyze/vacuum SQL; `analyze_import_tables` / `vacuum_import_tables` (warn, log seconds) |
| `crates/vault/server/src/import/promote.rs` | Call analyze before `BEGIN`; tests |
| `crates/vault/server/src/reset_demo.rs` | Vacuum after `process-assets` on both reset paths |
| `CHANGELOG.md` | Unreleased Changed note dated 2026-08-27 |

---

### Task 1: Dialect SQL and helpers

**Files:**
- Modify: `crates/vault/server/src/db/dialect.rs`

**Interfaces:**
- Produces:
  - `pub fn analyze_import_tables_sql() -> &'static [&'static str]` → `ANALYZE messages`, `ANALYZE attachments`, `ANALYZE tapbacks`
  - `pub fn vacuum_after_demo_sql(engine: DbEngine) -> &'static [&'static str]` → Postgres three table vacuums; SQLite `VACUUM`
  - `pub async fn analyze_import_tables(conn: &mut sqlx::AnyConnection)`
  - `pub async fn vacuum_import_tables(conn: &mut sqlx::AnyConnection)`

- [x] **Step 1–4:** Tests for SQL fragments, then helpers that warn on error and print `sql: analyze … (X.Xs)` / `sql: vacuum … (X.Xs)`
- [x] **Step 5:** Commit with the promote/reset-demo callers in later tasks if preferred as one logical change

---

### Task 2: Analyze before promote

**Files:**
- Modify: `crates/vault/server/src/import/promote.rs`

- [x] Failing test: after `promote_append`, SQLite `sqlite_stat1` has rows for the import tables
- [x] Postgres-gated: `last_analyze` on `messages` is set after the first promote
- [x] Call `analyze_import_tables` before `BEGIN`
- [x] Second promote still inserts

---

### Task 3: Vacuum after reset-demo

**Files:**
- Modify: `crates/vault/server/src/reset_demo.rs`

- [x] After `process-assets` on the URL path, open the URL pool and `vacuum_import_tables`
- [x] After `process-assets` on the SQLite prepared path, open `prepared_db` and vacuum before `install_reset_state`
- [x] Pool/connection errors are warnings, not `Err`

---

### Task 4: Changelog

**Files:**
- Modify: `CHANGELOG.md`

- [x] Changed bullet dated 2026-08-27
