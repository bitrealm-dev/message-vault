# Import Promote Throughput Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hash and write content keys in batches inside the existing all-or-nothing promote, skip a second full rehash after import, and print phase lines so a large CLI or `--reset-demo` import does not sit on `filling content keys…`.

**Architecture:** Keep `promote_append` as one transaction. Extract a row hasher that Rayon can run off the Tokio worker, write `_content_keys` with multi-row `INSERT` chunks of at most `SQLITE_IN_CHUNK` (400) pairs, and switch `dedupe_cross_source` from `recompute_all_content_keys` to `fill_missing_content_keys`. Fingerprint formula stays `compute_content_key`. Same SQL on SQLite and Postgres.

**Tech Stack:** Rust 2024, `message-vault-server`, sqlx Any, Rayon 1.12, Tokio `spawn_blocking`, existing `engine::test_pool` tests.

**Spec:** `docs/superpowers/specs/2026-08-27-import-promote-throughput-design.md`

## Global Constraints

- One promote transaction. Do not commit mid-promote. A hash-task panic or write error rolls the whole promote back.
- Same SQL on SQLite and Postgres. No `COPY` or `UNNEST`.
- Content-key formula does not change (`compute_content_key` in `crates/vault/server/src/dedupe.rs`).
- Message promote windows stay `PROMOTE_MESSAGE_BATCH = 50_000`. Do not change `crates/vault/server/src/import/promote.rs` except if a comment is wrong.
- Content-key logs: `hashing content keys (N messages)…` after the SELECT; `writing content keys … running=X/N` about every 50,000 keys and on the last chunk. Use `println!` plus `io::stdout().flush()`, matching other promote/dedupe lines.
- `dedupe_cross_source` fills missing keys only. Do not delete `recompute_all_content_keys`.
- No Import UI, no `/v1/imports` progress payload, no staging rewrite, no `process-assets` changes.
- Do not commit `src-tauri/Cargo.lock` if it is dirty from an unrelated `sha2` bump.
- Never commit to `main`. Stay on `perf/parallel-content-key-fill` (or create it from the spec commit if you are on `main`).
- Product version files stay at the current lockstep value. Do not bump versions.
- Prefer a real fix over `allow`. Prefix unused bindings with `_`.
- `cargo fmt` the server crate after Rust edits.

## File map

| File | Responsibility |
|---|---|
| `crates/vault/server/Cargo.toml` | Add `rayon = "1.12.0"` |
| `crates/vault/server/src/dedupe.rs` | Hash helpers, batched inserts, fill logs, fill-missing dedupe |
| `crates/vault/server/src/import/promote.rs` | Already 50k windows; leave the content-key call as `fill_missing_content_keys` |
| `CHANGELOG.md` | Unreleased Changed note dated 2026-08-27 |

Out of scope: `web/**`, `src-tauri/**`, `crates/vault/server/src/import/staging.rs`, `process-assets`.

Partial work may already sit uncommitted on `perf/parallel-content-key-fill` (`rayon`, `hash_content_keys`, `insert_content_key_rows`, `spawn_blocking`). If a step’s test already passes, do not rewrite it. Move to the next failing step.

---

### Task 0: Branch and record the plan

**Files:**
- Create: this plan at `docs/superpowers/plans/2026-08-27-import-promote-throughput.md`
- Existing: `docs/superpowers/specs/2026-08-27-import-promote-throughput-design.md`

**Interfaces:**
- Consumes: locked spec on disk
- Produces: branch `perf/parallel-content-key-fill` with spec + plan committed

- [ ] **Step 1: Confirm the branch**

```bash
cd /home/mbeisser/repo/message-vault
git branch --show-current
```

Expected: `perf/parallel-content-key-fill`. If you are on `main`, create the branch from the spec commit:

```bash
git checkout -b perf/parallel-content-key-fill
```

- [ ] **Step 2: Commit this plan** (skip if `git status` already shows it committed)

```bash
git add docs/superpowers/plans/2026-08-27-import-promote-throughput.md
git commit -m "$(cat <<'EOF'
docs: add import promote throughput plan

EOF
)"
```

---

### Task 1: Parallel content-key hasher

**Files:**
- Modify: `crates/vault/server/Cargo.toml` (add `rayon`)
- Modify: `crates/vault/server/src/dedupe.rs`
- Test: `crates/vault/server/src/dedupe.rs` (`mod tests`)

**Interfaces:**
- Consumes: existing `compute_content_key`, `chat_identity_for_content_key`
- Produces:
  - `type ContentKeyRow = (i64, i64, String, String, i64, Option<String>, String, Option<String>, Option<String>);`
    Fields in order: `id`, `conversation_id`, `chat_id`, `conversation_type`, `is_from_me`, `timestamp_utc`, `timestamp`, `body`, `sender_normalized`.
  - `fn content_key_for_row(row: &ContentKeyRow, group_handles: &HashMap<i64, Vec<String>>, shas_by_msg: &HashMap<i64, Vec<String>>) -> (i64, String)`
  - `fn hash_content_keys(rows: &[ContentKeyRow], group_handles: &HashMap<i64, Vec<String>>, shas_by_msg: &HashMap<i64, Vec<String>>) -> Vec<(i64, String)>`
    Uses `rows.par_iter()`. Rayon `collect` into `Vec` keeps original order.

- [ ] **Step 1: Add the failing test**

In `crates/vault/server/src/dedupe.rs`, `mod tests`, add `use std::collections::HashMap;` if it is not there. After `content_key_stable_across_whitespace_and_utc_forms`, add:

```rust
    #[test]
    fn parallel_content_keys_match_serial() {
        let rows = vec![
            (
                1,
                10,
                "+14075551212".into(),
                "individual".into(),
                1,
                Some("2015-03-12T18:04:22Z".into()),
                "x".into(),
                Some("hi".into()),
                None,
            ),
            (
                2,
                11,
                "chat-group".into(),
                "group".into(),
                0,
                Some("2015-03-12T18:04:23Z".into()),
                "x".into(),
                Some("yo".into()),
                Some("+15555550001".into()),
            ),
        ];
        let mut groups = HashMap::new();
        groups.insert(11, vec!["+15555550001".into(), "+15555550002".into()]);
        let mut shas = HashMap::new();
        shas.insert(2, vec!["abc".into()]);
        let parallel = hash_content_keys(&rows, &groups, &shas);
        let serial: Vec<_> = rows
            .iter()
            .map(|row| content_key_for_row(row, &groups, &shas))
            .collect();
        assert_eq!(parallel, serial);
    }
```

- [ ] **Step 2: Run the test and confirm it fails** (skip if the helpers already exist)

```bash
cargo test -p message-vault-server --lib dedupe::tests::parallel_content_keys_match_serial -- --nocapture
```

Expected if helpers are missing: compile error, `hash_content_keys` not found.

- [ ] **Step 3: Add Rayon and the helpers**

In `crates/vault/server/Cargo.toml`, under `[dependencies]`, next to `rand`:

```toml
rayon = "1.12.0"
```

In `dedupe.rs`, add:

```rust
use rayon::prelude::*;
```

Above `normalize_body`, add the `ContentKeyRow` type alias from Interfaces.

After `compute_content_key`, add:

```rust
fn content_key_for_row(
    row: &ContentKeyRow,
    group_handles: &HashMap<i64, Vec<String>>,
    shas_by_msg: &HashMap<i64, Vec<String>>,
) -> (i64, String) {
    let (
        id,
        conversation_id,
        chat_id,
        conversation_type,
        is_from_me,
        ts_utc,
        ts,
        body,
        sender_norm,
    ) = row;
    let empty: &[String] = &[];
    let shas = shas_by_msg.get(id).map(Vec::as_slice).unwrap_or(empty);
    let group_identity = if conversation_type == "group" {
        Some(chat_identity_for_content_key(
            chat_id,
            group_handles.get(conversation_id).map(Vec::as_slice),
        ))
    } else {
        None
    };
    let identity = group_identity.as_deref().unwrap_or(chat_id);
    let key = compute_content_key(
        identity,
        *is_from_me != 0,
        sender_norm.as_deref(),
        ts_utc.as_deref(),
        ts,
        body.as_deref(),
        shas,
    );
    (*id, key)
}

fn hash_content_keys(
    rows: &[ContentKeyRow],
    group_handles: &HashMap<i64, Vec<String>>,
    shas_by_msg: &HashMap<i64, Vec<String>>,
) -> Vec<(i64, String)> {
    rows.par_iter()
        .map(|row| content_key_for_row(row, group_handles, shas_by_msg))
        .collect()
}
```

- [ ] **Step 4: Run the test and confirm it passes**

```bash
cargo fmt -p message-vault-server
cargo test -p message-vault-server --lib dedupe::tests::parallel_content_keys_match_serial -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/Cargo.toml crates/vault/server/src/dedupe.rs Cargo.lock
git commit -m "$(cat <<'EOF'
perf(server): hash content keys on a rayon pool

EOF
)"
```

Do not add `src-tauri/Cargo.lock`.

---

### Task 2: Batch `_content_keys` inserts and hash off the Tokio worker

**Files:**
- Modify: `crates/vault/server/src/dedupe.rs` (`recompute_content_keys`, new `insert_content_key_rows`)

**Interfaces:**
- Consumes: `ContentKeyRow`, `hash_content_keys`, `crate::db::sql::SQLITE_IN_CHUNK` (`400`)
- Produces:
  - `async fn insert_content_key_rows(conn: &mut AnyConnection, keys: &[(i64, String)]) -> Result<()>`
  - `recompute_content_keys` hashes via `tokio::task::spawn_blocking(move || hash_content_keys(...)).await.context("content-key hash task panicked")?`
  - After a non-empty SELECT, prints `  sql:      hashing content keys ({n} messages)…` and flushes.
  - While inserting, prints `  sql:      writing content keys … running={written}/{total}` when `written == total` or `written` is a positive multiple of `50_000`.

- [ ] **Step 1: Add the second-fill test (will fail until fill still works; it should compile)**

In `mod tests`, after `setup_db` / `insert_msg`, add:

```rust
    #[tokio::test]
    async fn fill_missing_content_keys_skips_rows_that_already_have_keys() {
        let (pool, _dir) = engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        setup_db(&mut conn).await;
        insert_msg(
            &mut conn,
            InsertMsgArgs {
                source: "go-sms-pro",
                guid: "g-fill",
                utc: "2015-03-12T18:04:22Z",
                local: "2015-03-12T14:04:22-04:00",
                from_me: 1,
                body: "Need a key",
                sort_order: 0,
            },
        )
        .await;
        let first = fill_missing_content_keys(&mut conn, TEST_ACCOUNT_ID)
            .await
            .unwrap();
        assert_eq!(first, 1);
        let second = fill_missing_content_keys(&mut conn, TEST_ACCOUNT_ID)
            .await
            .unwrap();
        assert_eq!(second, 0);
        let key: Option<String> =
            sqlx::query_scalar("SELECT content_key FROM messages WHERE guid = 'g-fill'")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert!(key.as_deref().is_some_and(|k| !k.is_empty()));
    }
```

This test should pass after Step 3 even before Task 3, as long as `fill_missing_content_keys` still writes keys. Add it now so the batched path is covered.

- [ ] **Step 2: Run the new test**

```bash
cargo test -p message-vault-server --lib dedupe::tests::fill_missing_content_keys_skips_rows_that_already_have_keys -- --nocapture
```

Expected before Step 3: PASS if the old one-row insert path still works, or FAIL/compile-error if `recompute_content_keys` is mid-edit. After Step 3 it must PASS.

- [ ] **Step 3: Replace the serial hash loop and one-row inserts**

Add `use crate::db::sql::SQLITE_IN_CHUNK;` and `use anyhow::Context` if missing.

Add this constant next to the helpers:

```rust
const CONTENT_KEY_WRITE_LOG_EVERY: usize = 50_000;
```

Add:

```rust
async fn insert_content_key_rows(conn: &mut AnyConnection, keys: &[(i64, String)]) -> Result<()> {
    let total = keys.len();
    let mut written = 0usize;
    for chunk in keys.chunks(SQLITE_IN_CHUNK) {
        let mut sql = String::from("INSERT INTO _content_keys (id, content_key) VALUES ");
        for (i, _) in chunk.iter().enumerate() {
            if i > 0 {
                sql.push(',');
            }
            sql.push_str(&format!("(${}, ${})", i * 2 + 1, i * 2 + 2));
        }
        let mut q = sqlx::query(&sql);
        for (id, key) in chunk {
            q = q.bind(*id).bind(key);
        }
        q.execute(&mut *conn).await?;
        written += chunk.len();
        if written == total || (written > 0 && written % CONTENT_KEY_WRITE_LOG_EVERY == 0) {
            println!("  sql:      writing content keys … running={written}/{total}");
            let _ = io::stdout().flush();
        }
    }
    Ok(())
}
```

In `recompute_content_keys`, after `fetch_all`, if `rows` is empty return `Ok(0)` (already there). If not empty, print:

```rust
    println!(
        "  sql:      hashing content keys ({} messages)…",
        rows.len()
    );
    let _ = io::stdout().flush();
```

Replace the serial `for` that builds `keys` with:

```rust
    let keys =
        tokio::task::spawn_blocking(move || hash_content_keys(&rows, &group_handles, &shas_by_msg))
            .await
            .context("content-key hash task panicked")?;
```

Delete the per-row `INSERT INTO _content_keys (id, content_key) VALUES ($1, $2)` loop. After `CREATE TEMP TABLE` / `DELETE`, call `insert_content_key_rows(conn, &keys).await?;`. Keep the existing `UPDATE messages AS m SET content_key = k.content_key FROM _content_keys AS k WHERE m.id = k.id` and the `DROP TABLE`.

Change the inner `query_as` type from a local `ExactDedupeRow` alias to `ContentKeyRow` if that alias is still in the function.

- [ ] **Step 4: Run fill + hasher + existing exact dedupe tests**

```bash
cargo fmt -p message-vault-server
cargo test -p message-vault-server --lib dedupe:: -- --test-threads=1
```

Expected: all `dedupe::tests` PASS, including `fill_missing_content_keys_skips_rows_that_already_have_keys` and `integration_exact_flags_cross_source`.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/dedupe.rs
git commit -m "$(cat <<'EOF'
perf(server): batch content-key writes and log chunks

EOF
)"
```

---

### Task 3: Dedupe after import fills missing keys only

**Files:**
- Modify: `crates/vault/server/src/dedupe.rs` (`dedupe_cross_source`)

**Interfaces:**
- Consumes: `fill_missing_content_keys(conn: &mut AnyConnection, account_id: &str) -> Result<u64>`
- Produces: `dedupe_cross_source` first transaction calls `fill_missing_content_keys`, not `recompute_all_content_keys`. Exact and near passes unchanged. `recompute_all_content_keys` remains `pub async` for later rebuilds.

- [ ] **Step 1: Run existing integration tests once (baseline)**

```bash
cargo test -p message-vault-server --lib dedupe::tests::integration_exact_flags_cross_source -- --nocapture
```

Expected: PASS. Those tests insert messages with no `content_key`, so fill-missing still hashes them.

- [ ] **Step 2: Switch the first dedupe transaction**

In `dedupe_cross_source`, replace:

```rust
        println!("  dedupe:   recomputing content keys…");
        let _ = io::stdout().flush();
        let mut tx = conn.begin().await?;
        stats.keys_filled = recompute_all_content_keys(&mut tx, account_id).await?;
```

with:

```rust
        println!("  dedupe:   filling missing content keys…");
        let _ = io::stdout().flush();
        let mut tx = conn.begin().await?;
        stats.keys_filled = fill_missing_content_keys(&mut tx, account_id).await?;
```

Leave the `duplicate_of = NULL` update, commit, and `keys filled=` print as they are.

- [ ] **Step 3: Add a test that a second `dedupe_cross_source` does not refill keys**

```rust
    #[tokio::test]
    async fn dedupe_cross_source_does_not_rehash_existing_keys() {
        let (pool, _dir) = engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        setup_db(&mut conn).await;
        insert_msg(
            &mut conn,
            InsertMsgArgs {
                source: "go-sms-pro",
                guid: "g-once",
                utc: "2015-03-12T18:04:22Z",
                local: "2015-03-12T14:04:22-04:00",
                from_me: 1,
                body: "Once",
                sort_order: 0,
            },
        )
        .await;
        let first = dedupe_cross_source(&mut conn, TEST_ACCOUNT_ID, None, 2)
            .await
            .unwrap();
        assert_eq!(first.keys_filled, 1);
        let second = dedupe_cross_source(&mut conn, TEST_ACCOUNT_ID, None, 2)
            .await
            .unwrap();
        assert_eq!(second.keys_filled, 0);
    }
```

- [ ] **Step 4: Run the new test and the exact/near/priority suite**

```bash
cargo fmt -p message-vault-server
cargo test -p message-vault-server --lib dedupe:: -- --test-threads=1
```

Expected: PASS, including `dedupe_cross_source_does_not_rehash_existing_keys`.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/dedupe.rs
git commit -m "$(cat <<'EOF'
perf(server): skip full content-key rebuild after import

EOF
)"
```

---

### Task 4: Changelog and crate-level check

**Files:**
- Modify: `CHANGELOG.md` under `[Unreleased]` → `### Changed`

**Interfaces:**
- Consumes: behavior from Tasks 1–3
- Produces: one dated Changed bullet

- [ ] **Step 1: Add the changelog bullet**

Under `### Changed`, first bullet (newest first):

```markdown
- 2026-08-27: Import promote hashes content keys on a thread pool and writes them in multi-row batches. The later dedupe pass only fills missing keys instead of hashing every message again. Server logs print hash and write progress during a long fill.
```

- [ ] **Step 2: Run the server lib tests that this work touches**

```bash
cargo test -p message-vault-server --lib dedupe:: -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md
git commit -m "$(cat <<'EOF'
docs: changelog for faster content-key fill

EOF
)"
```

---

## Self-review

| Spec decision | Task |
|---|---|
| One promote transaction / rollback on hash panic | Task 2 (`spawn_blocking` error); no mid-promote commit |
| Same SQL both engines | Task 2 multi-row `INSERT` + existing `UPDATE … FROM` |
| Parallel hashes, batched writes, same formula | Tasks 1–2 |
| 50k message windows | Already on branch; File map leaves `promote.rs` |
| Hash/write log lines every 50k | Task 2 |
| Dedupe fills missing only; keep `recompute_all` | Task 3 |
| Server logs only | No web/API files |
| Second fill writes zero keys | Task 2 test |
| Existing exact/near/priority tests | Tasks 2–3 |

No placeholders. Types match across tasks: `ContentKeyRow`, `hash_content_keys`, `insert_content_key_rows`, `fill_missing_content_keys`.
