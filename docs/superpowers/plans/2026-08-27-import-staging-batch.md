# Import Staging Batch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (inline; user asked to implement). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Write staging messages, attachments, and tapbacks in multi-row chunks, and remember sender handle ids for the rest of an import, so CLI, HTTP, and `--reset-demo` stop doing one database call per row.

**Architecture:** A bind-limit helper sizes chunks (column count × rows ≤ 999). `upsert_handle_row_cached` wraps the existing upsert with a map owned by the import. `import_conversation_to_staging` flushes messages in chunks, matches `RETURNING id, guid`, then flushes attachments and tapbacks. Promote, analyze, and vacuum stay unchanged.

**Tech Stack:** Rust 2024, `message-vault-server`, sqlx Any, `engine::test_pool` / `import_jsonl_files` tests, `MV_TEST_POSTGRES_URL` when set.

## Global Constraints

- Same `INSERT … VALUES` on SQLite and Postgres. No `COPY` / `UNNEST`.
- Every import path uses the new write (no demo-only branch).
- Duplicate guid still skips that message and its children (`ON CONFLICT DO NOTHING`).
- A failed chunk fails the import. No half-chunk retry.
- Conversations and participants stay one row at a time.
- Do not change promote, `ANALYZE`, `VACUUM`, JSONL parse, or file copy.
- Progress lines stay `[N/total] … (Xs)`. No Import-screen fields.
- Never commit to `main`. Do not commit dirty `src-tauri/Cargo.lock`.
- Product version files stay at the current lockstep value.
- `cargo fmt` the server crate after Rust edits.

## File map

| File | Responsibility |
|---|---|
| `crates/vault/server/src/db/sql.rs` | `max_rows_for_bind_limit`, `values_tuples` |
| `crates/vault/server/src/db/handles.rs` | `HandleIdCache`, `upsert_handle_row_cached` |
| `crates/vault/server/src/import/contact_name.rs` | Pass the cache when resolving a sender |
| `crates/vault/server/src/import/staging.rs` | Chunked inserts; match `RETURNING` ids |
| `CHANGELOG.md` | Unreleased Changed note dated 2026-08-27 |

---

### Task 1: Bind-limit helpers

**Files:**
- Modify: `crates/vault/server/src/db/sql.rs`

**Interfaces:**
- Produces:
  - `pub const SQLITE_MAX_VARIABLES: usize = 999`
  - `pub fn max_rows_for_bind_limit(columns: usize) -> usize` — `999 / columns`, or `0` if `columns == 0`
  - `pub fn values_tuples(row_count: usize, col_count: usize) -> String` — `($1,$2,…,$C),($C+1,…)`

- [x] **Step 1:** Add unit tests for `max_rows_for_bind_limit(18) == 55`, `(10) == 99`, `(6) == 166`, `(0) == 0`; `values_tuples(2, 3) == "($1,$2,$3),($4,$5,$6)"`
- [x] **Step 2:** Implement the two functions and constant
- [x] **Step 3:** `cargo test -p message-vault-server --lib -- max_rows_for_bind_limit values_tuples`
- [x] **Step 4:** Commit with later tasks if preferred as one logical change

---

### Task 2: Handle id cache

**Files:**
- Modify: `crates/vault/server/src/db/handles.rs`
- Modify: `crates/vault/server/src/import/contact_name.rs`

**Interfaces:**
- Produces:
  - `pub type HandleIdCache = HashMap<(String, String, String, String), i64>` — key is `(account_id, normalized, handle_type, service)`
  - `pub async fn upsert_handle_row_cached(conn, cache, account_id, raw, handle_type, service) -> Result<(i64, bool)>` — cache hit skips SQL; miss calls `upsert_handle_row` and stores the id
- Leave `upsert_handle_row` signature unchanged (`account_profile`, `contacts_api`)

- [x] **Step 1:** Test: two cached upserts of the same incoming phone create one `handles` row
- [x] **Step 2:** Implement cache + wrapper; `resolve_incoming_sender_handle` takes `&mut HandleIdCache`
- [x] **Step 3:** `cargo test -p message-vault-server --lib -- upsert_handle`

---

### Task 3: Chunked staging inserts

**Files:**
- Modify: `crates/vault/server/src/import/staging.rs`
- Test: `crates/vault/server/src/import/mod.rs` (existing `import_jsonl_files` helpers)

**Interfaces:**
- Consumes: `max_rows_for_bind_limit`, `values_tuples`, `HandleIdCache`, `upsert_handle_row_cached`
- Message chunk size: `max_rows_for_bind_limit(18)` (55)
- Attachment chunk size: `max_rows_for_bind_limit(10)`
- Tapback chunk size: `max_rows_for_bind_limit(6)`
- Flush SQL: `INSERT INTO staging_messages (…) VALUES … ON CONFLICT DO NOTHING RETURNING id, guid`
- Match returned rows by `account_id + source + guid`
- Skipped rows increment `messages_deduped` and get no children

- [x] **Step 1:** Failing test: one JSONL conversation with 56 incoming messages, attachment on first and last, tapback on the second; all 56 land; attachments and tapback sit on the right production rows after promote
- [x] **Step 2:** Failing test: import the same guid twice in one file (or two files, replace then the unique index on staging within one import); second is skipped; first keeps its attachment
- [x] **Step 3:** Implement per-conversation chunk flush in `import_conversation_to_staging`; participants/chat handle use the same cache
- [x] **Step 4:** `cargo test -p message-vault-server --lib -- import::`
- [x] **Step 5:** Existing `append_skips_existing_guids_and_keeps_id_map` still passes

---

### Task 4: Changelog

**Files:**
- Modify: `CHANGELOG.md`

- [x] **Step 1:** Changed bullet dated 2026-08-27: import writes staging messages, attachments, and tapbacks in multi-row chunks and reuses sender handle ids for the rest of the import
- [x] **Step 2:** `cargo fmt --all`; `cargo test -p message-vault-server --lib -- import:: max_rows_for_bind_limit values_tuples upsert_handle`
- [x] **Step 3:** Commit
