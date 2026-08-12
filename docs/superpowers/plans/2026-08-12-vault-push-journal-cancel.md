# Vault-push journal durability and cancel cleanup

> **For agentic workers:** Execute task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix three correctness bugs in `vault-push`: journal compact wiping other vault targets, concurrent journal append tearing large lines, and cancel exiting without joining in-flight import / writing a report.

**Architecture:** Keep the single `.vault-import-state.jsonl` file. Compact becomes read-merge-write so other `(url, username)` events survive. All disk appends go through one process-wide mutex and write a full line in one `write_all`. Cancel in the main pipeline sets `aborted` and breaks into the existing abort finish path instead of returning `Err` out of `thread::scope`.

**Tech Stack:** Rust (`vault-push` crate), `tempfile` unit tests, existing `httpmock` suite.

## Global Constraints

- Behavior for a single vault target must stay the same (resume, force, replace).
- Do not change journal event schema (`event` tag names / fields).
- Do not split `run.rs` in this plan (maintainability follow-up).
- Include the small `unguided:{ts}` collision fix while touching `project.rs`.

## File map

| File | Role |
|------|------|
| `crates/cli/vault-push/src/journal.rs` | Compact merge; atomic line append; write mutex |
| `crates/cli/vault-push/src/run.rs` | Cancel → abort path; call sites keep using `journal::append` |
| `crates/cli/vault-push/src/project.rs` | Disambiguate empty-guid journal keys with message index |

---

### Task 1: Compact preserves other vault targets

**Files:**
- Modify: `crates/cli/vault-push/src/journal.rs`
- Test: same file `#[cfg(test)]`

**Interfaces:**
- Consumes: existing `JournalEvent`, `JournalState`, `load`
- Produces: `compact(path, url, username, state)` that rewrites the file with (1) all events whose `(url, username)` ≠ current pair, then (2) compacted events for the current pair from `state`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn compact_preserves_other_vault_target_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(JOURNAL_NAME);
    fs::write(
        &path,
        concat!(
            r#"{"event":"asset_ok","url":"http://a","username":"alice","source":"sms","sha256":"aaa"}"#,
            "\n",
            r#"{"event":"file_ok","url":"http://b","username":"bob","source":"sms","file":"chat.jsonl"}"#,
            "\n",
        ),
    )
    .unwrap();

    let mut state = JournalState::default();
    state.assets.insert("bbb".into());
    compact(&path, "http://b", "bob", &state).unwrap();

    let a = load(&path, "http://a", "alice").unwrap();
    assert!(a.assets.contains("aaa"));
    let b = load(&path, "http://b", "bob").unwrap();
    assert!(b.assets.contains("bbb"));
    assert!(!b.files.contains("chat.jsonl")); // replaced by compacted state
}
```

- [ ] **Step 2: Run test — expect FAIL** (compact drops `http://a`)

Run: `cargo test -p vault-push compact_preserves_other_vault_target_events -- --nocapture`

- [ ] **Step 3: Implement**

In `compact`:
1. If journal path exists, read all lines; for each valid `JournalEvent`, if `(url, username)` ≠ current pair, keep the event.
2. Append compacted AssetOk / MessageBatchOk / FileOk for the current pair from `state` (same as today).
3. Write via temp file + rename (unchanged). Drop corrupt lines (same as load).

- [ ] **Step 4: Run test — expect PASS**

---

### Task 2: Serialize journal appends

**Files:**
- Modify: `crates/cli/vault-push/src/journal.rs`

**Interfaces:**
- Consumes: unchanged `append(path, event)` signature
- Produces: process-wide `Mutex<()>` guarding append and compact rewrite; each event serialized to `Vec<u8>` then one `write_all` of `bytes + b'\n'`

- [ ] **Step 1: Write stress test**

```rust
#[test]
fn append_writes_complete_lines_under_contention() {
    use std::sync::Arc;
    use std::thread;

    let dir = tempfile::tempdir().unwrap();
    let path = Arc::new(dir.path().join(JOURNAL_NAME));
    let mut handles = Vec::new();
    for i in 0..8 {
        let path = Arc::clone(&path);
        handles.push(thread::spawn(move || {
            for j in 0..50 {
                let guid = format!("g-{i}-{j}");
                let messages: Vec<_> = (0..200)
                    .map(|k| JournalMessage {
                        file: format!("f{i}.jsonl"),
                        guid: format!("{guid}-{k}"),
                    })
                    .collect();
                append(
                    &path,
                    &JournalEvent::MessageBatchOk {
                        url: "http://vault".into(),
                        username: "alice".into(),
                        source: "sms".into(),
                        messages,
                    },
                )
                .unwrap();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let text = fs::read_to_string(&*path).unwrap();
    let mut lines = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        serde_json::from_str::<JournalEvent>(line).expect("torn line");
        lines += 1;
    }
    assert_eq!(lines, 8 * 50);
}
```

- [ ] **Step 2: Implement**

```rust
use std::sync::Mutex;

static JOURNAL_WRITE_LOCK: Mutex<()> = Mutex::new(());

pub fn append(path: &Path, event: &JournalEvent) -> Result<()> {
    let _guard = JOURNAL_WRITE_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // create_dir_all, open append…
    let mut buf = serde_json::to_vec(event)?;
    buf.push(b'\n');
    file.write_all(&buf)?;
    file.flush()?;
    Ok(())
}
```

Hold the same lock for compact's rewrite+rename so appends cannot interleave with replace.

- [ ] **Step 3: Run stress test — PASS**

---

### Task 3: Cancel uses abort finish path

**Files:**
- Modify: `crates/cli/vault-push/src/run.rs`

**Approach:**
1. Main loop: cancel → `aborted = true; stop_submitting = true; break` (no `?`).
2. Post-loop: if `aborted`, always `join_inflight_import` (existing else branch). If `!aborted`, check cancel once more before final flush; on cancel set `aborted` and join instead of flush.
3. `flush_import_pipeline`: on cancel before spawning, return `Ok(false)` without taking `pending` (do not start a new import). Callers that already set `aborted` from the main loop are fine; for mid-flush cancel during `wait:false` overlap, `Ok(false)` with `continue_on_error` must still abort — set `aborted = true` whenever flush returns false **and** cancel is set. Cleaner: flush returns `Ok(false)` on cancel after clearing nothing, and main loop always does:

```rust
if check_cancel(cfg.cancel.as_ref()).is_err() {
    aborted = true;
    stop_submitting = true;
    // do not call flush for new work
}
```

before each flush_imports in the loop when we care. Simplest correct change set:

- Replace main-loop `check_cancel…?` with aborted+break.
- In `flush_import_pipeline`, replace cancel `?` with `return Ok(false)` (leave pending).
- After the `while` loop, unify:

```rust
if check_cancel(cfg.cancel.as_ref()).is_err() {
    aborted = true;
}
if !aborted {
    let request_ok = flush_imports!(wait: true)?;
    if !request_ok && !cfg.continue_on_error {
        aborted = true;
    }
}
if aborted {
    let mut guard = shared_journal.lock()…;
    let _ = join_inflight_import(…);
}
```

Note: when `!aborted` and flush fails with `continue_on_error`, do not join twice. Current code only joins in the else branch. Preserve that: only join when aborted; when !aborted && flush returned false with continue_on_error, pending may have been cleared by flush on hard failure — leave as today.

When cancel returns Ok(false) from flush without aborted yet (overlap flush in loop): change loop sites from `if !request_ok && !cfg.continue_on_error` to also abort on cancel:

```rust
let request_ok = flush_imports!(wait: false)?;
if !request_ok {
    if check_cancel(cfg.cancel.as_ref()).is_err() || !cfg.continue_on_error {
        aborted = true;
        stop_submitting = true;
    }
}
```

- [ ] **Step 1: Apply cancel → abort changes in `run.rs`**
- [ ] **Step 2: `cargo test -p vault-push`**
- [ ] **Step 3: Confirm `finish_run` still runs after cancel (report `ok: false`)**

---

### Task 4: Disambiguate unguided journal keys

**Files:**
- Modify: `crates/cli/vault-push/src/project.rs`
- Modify: `crates/cli/vault-push/src/run.rs` (pass message index)

**Interfaces:**
- `message_line(msg, projections, index: usize)`
- `message_line_without_attachments(msg, index: usize)`
- Empty guid → `unguided:{timestamp_unix_ms}:{index}`

- [ ] **Step 1: Test**

```rust
#[test]
fn unguided_keys_include_message_index() {
    let msg = IrMessage {
        guid: "  ".into(),
        timestamp_unix_ms: 42,
        // …defaults…
    };
    let (_, g0) = message_line(&msg, &[], 0).unwrap();
    let (_, g1) = message_line(&msg, &[], 1).unwrap();
    assert_eq!(g0, "unguided:42:0");
    assert_eq!(g1, "unguided:42:1");
}
```

- [ ] **Step 2: Implement + update `run.rs` call sites** (`enumerate` already has `i`)
- [ ] **Step 3: `cargo test -p vault-push`**

---

### Task 5: Verify

- [ ] `cargo test -p vault-push`
- [ ] `cargo fmt` for touched files

## Out of scope

- Splitting `run.rs`
- Structured `is_transient_error`
- Import body clone on retry
- `assets_bytes` accounting rename
