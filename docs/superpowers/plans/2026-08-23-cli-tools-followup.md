# CLI tools follow-up implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all 12 CLI-tools findings from the product Rust audit — extract the duplicated JSONL journal and vault HTTP client into two new lib crates (`journal`, `vault-http`), replace substring retry classification with a typed classifier (all 4xx permanent), wire demo-seed's dead config fields, and document the CLI pub surfaces with honest help text — with byte-identical journal files, demo dataset, error text, and CLI help except the one sanctioned `--mode` rewrite.

**Architecture:** `crates/libs/journal` owns the generic JSONL mechanics (`append`, `load_events`, `compact_with` under one process-wide write lock); `crates/libs/vault-http` owns `build_client`, `truncate`, `AuthError`/`AuthInfo` (moved down from vault-push), and the typed retry machinery (`RetryKind`, `VaultHttpError`, `classify_retry`, `with_retries`). vault-push and vault-pull become thin consumers; every existing public path (including the desktop app and GUI) keeps working through re-exports.

**Tech Stack:** Rust workspace, anyhow, reqwest (blocking), serde/serde_json, clap (help text only), httpmock (dev).

**Spec:** `docs/superpowers/specs/2026-08-23-cli-tools-followup-design.md` — the plan argues from the spec, so the spec travels with it; executors read both.

## Global Constraints

From `docs/superpowers/specs/2026-08-23-cli-tools-followup-design.md` — every task's requirements implicitly include this section:

- **Behavior-preserving** except the sanctioned deltas (spec's Behavior deltas catalog, plus the three plan refinements below): byte-identical journal files (formats, filenames, event schemas), byte-identical error text at every call site, byte-identical demo dataset with the shipped `demo_seed.toml`, byte-identical CLI help except `vault-push --mode` (Task 8) and `demo-seed --about` (Task 7).
- **Green after every task.** `cargo fmt --all -- --check`, `cargo build --workspace && cargo test --workspace` (all targets, 0 failed), `cargo clippy --workspace --all-targets -- -D warnings` all clean after every task commit.
- **Docs gates.** `cargo doc --no-deps -p journal` (from Task 1), `-p vault-http` (from Task 4), `-p dump-cli-docs` (from Task 8) emit zero warnings. No `#[allow(missing_docs)]`.
- **Generated artifacts.** `openapi.json` untouched. CLI reference pages byte-identical unless a task's own clap-visible change alters rendered help — only `cli/vault-push.md` changes, regenerated in Task 8 with `cargo run -p dump-cli-docs -- --output-dir docs/src/content/docs/vault/developer/reference`; `cargo test -p dump-cli-docs committed_cli_pages_match_dump` must be green at the end of Task 8 and ever after.
- **Doc style.** `docs/src/content/docs/vault/developer/rustdoc-style.md` governs all doc text.
- **No dependency version bumps.** New deps only where the moved code needs them: `journal` = anyhow + serde + serde_json (+ tempfile dev); `vault-http` = anyhow + reqwest (+ httpmock dev). New crates must be listed in the root workspace `members`.
- **src-tauri is not a workspace member.** No src-tauri edits in this group; keep `cargo check --manifest-path src-tauri/Cargo.toml` green via the re-export shims (Task 5).
- **Line anchors.** Audit line numbers are context only; find items by name — the compiler and `cargo test` are authoritative.

## Plan refinements to the spec

Three mechanical refinements, each preserving the spec's stated behavior:

1. **`VaultHttpError` instead of the `PayloadTooLarge` marker.** The spec's marker-context idea cannot carry `head_asset`'s plain-string 401/403 bails, and `anyhow` contexts change `error.to_string()`. Instead `vault-http` gets a small error type `VaultHttpError { status, message }` whose `Display` prints only `message` (byte-identical error text) and whose `status` the classifier reads. Every status-derived bail in the retried paths tags itself with it, including the 413 sites.
2. **`journal` exposes `compact_with`, not `rewrite`.** Today push's `compact` holds the write lock across read + rewrite, so a concurrent append cannot be lost. A public `rewrite` would let callers compose read and write under two separate lock acquisitions and open that race. `compact_with(label, path, rebuild)` reads, calls `rebuild(Vec<E>) -> Vec<E>`, and rewrites under one acquisition; `rewrite` stays private. Push and pull `compact` both become `compact_with` closures.
3. **Error-context labels are a parameter.** The shared fns take a `label: &str` ("journal" from push, "pull journal" from pull) so pull's existing `open pull journal …` / `read pull journal line …` error text stays byte-identical.

---

### Task 1: `journal` crate — generic JSONL journal mechanics

Findings 1. Create the shared crate with the exact mechanics both CLI crates use today, plus tests ported from vault-push's journal.

**Files:**
- Create: `crates/libs/journal/Cargo.toml`, `crates/libs/journal/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Produces: `journal::append(label: &str, path: &Path, event: &E) -> anyhow::Result<()>` where `E: Serialize`; `journal::load_events<E: DeserializeOwned>(label: &str, path: &Path, on_corrupt: &mut dyn FnMut(usize, &serde_json::Error)) -> anyhow::Result<Vec<E>>`; `journal::compact_with<E: Serialize + DeserializeOwned, F: FnOnce(Vec<E>) -> Vec<E>>(label: &str, path: &Path, rebuild: F) -> anyhow::Result<()>`. Dep key everywhere: `jsonl_journal = { package = "journal", … }` (the local module is also named `journal`, so callers use the `jsonl_journal::` alias).
- Consumes: nothing.

- [ ] **Step 1: Create the crate manifest**

`crates/libs/journal/Cargo.toml`:

```toml
[package]
name = "journal"
version = "0.1.0"
edition = "2024"
description = "Generic JSON Lines state journal: append-only events with sorted rewrite compaction"
license = "LicenseRef-FCL-1.0-ALv2"

[dependencies]
anyhow = "1.0.103"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"

[dev-dependencies]
tempfile = "3.27.0"
```

Add the workspace member in the root `Cargo.toml` `members` list, right after `"crates/libs/go-sms-mms",`:

```toml
    "crates/libs/journal",
```

- [ ] **Step 2: Write the crate**

`crates/libs/journal/src/lib.rs`:

```rust
//! JSON Lines state journals: append-only logs rewritten by sorted compaction.
//!
//! A journal is one JSON object per line. Readers rebuild skip-sets from the
//! events; writers append rows and periodically rewrite the file compacted.
//! Events are opaque to this crate — callers bring their own serde type.
//!
//! All writes run under one process-wide lock, so a rewrite can never mix
//! bytes with a concurrent append.

#![warn(missing_docs)]

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// One lock for append and rewrite so two threads cannot mix bytes on a line
/// or rewrite the file while another thread is appending.
static JOURNAL_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Append one event as a single JSON Lines row and flush it to disk.
///
/// The event is serialized to a buffer first so a serialization failure cannot
/// tear a half-written row.
///
/// # Errors
///
/// Returns an error when the parent folder cannot be created, the file cannot
/// be opened, the event cannot be serialized, or the write fails.
pub fn append<E: Serialize>(label: &str, path: &Path, event: &E) -> Result<()> {
    let _guard = JOURNAL_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open {label} for append {}", path.display()))?;
    let mut buf = serde_json::to_vec(event).context("serialize journal event")?;
    buf.push(b'\n');
    file.write_all(&buf)?;
    file.flush()?;
    Ok(())
}

/// Parse every event from a journal file.
///
/// A missing file is treated as an empty journal. Each line that cannot be
/// parsed is reported to `on_corrupt(line_number, parse_error)` and skipped —
/// the caller decides whether to warn or stay silent.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or a line cannot be read.
pub fn load_events<E: DeserializeOwned>(
    label: &str,
    path: &Path,
    on_corrupt: &mut dyn FnMut(usize, &serde_json::Error),
) -> Result<Vec<E>> {
    let mut events = Vec::new();
    if !path.is_file() {
        return Ok(events);
    }
    let file = File::open(path).with_context(|| format!("open {label} {}", path.display()))?;
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("read {label} line {}", i + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(&line) {
            Ok(event) => events.push(event),
            Err(error) => on_corrupt(i + 1, &error),
        }
    }
    Ok(events)
}

/// Read the journal, transform the surviving events with `rebuild`, and
/// rewrite the file — all under one write-lock acquisition, so a concurrent
/// [`append`] either lands before the read or after the rewrite, never between
/// them.
///
/// Corrupt lines are skipped silently during the read, matching both CLI
/// crates' compaction behavior.
///
/// # Errors
///
/// Returns an error when the file cannot be read, the temporary file cannot
/// be written, or the rename fails.
pub fn compact_with<E, F>(label: &str, path: &Path, rebuild: F) -> Result<()>
where
    E: Serialize + DeserializeOwned,
    F: FnOnce(Vec<E>) -> Vec<E>,
{
    let _guard = JOURNAL_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let events = read_unlocked::<E>(label, path)?;
    let events = rebuild(events);
    write_unlocked(path, &events)
}

/// Read events without the lock (callers either do not write, or hold it).
fn read_unlocked<E: DeserializeOwned>(label: &str, path: &Path) -> Result<Vec<E>> {
    let mut events = Vec::new();
    if !path.is_file() {
        return Ok(events);
    }
    let file = File::open(path).with_context(|| format!("open {label} {}", path.display()))?;
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("read {label} line {}", i + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str(&line) {
            events.push(event);
        }
    }
    Ok(events)
}

/// Write `events` to a temp file and rename over the journal (lock held).
fn write_unlocked<E: Serialize>(path: &Path, events: &[E]) -> Result<()> {
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut out = File::create(&tmp)?;
        for event in events {
            let mut buf = serde_json::to_vec(event).context("serialize journal event")?;
            buf.push(b'\n');
            out.write_all(&buf)?;
        }
        out.flush()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestEvent {
        url: String,
        user: String,
        key: String,
    }

    #[test]
    fn missing_file_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.jsonl");
        let events: Vec<TestEvent> = load_events("journal", &path, &mut |_, _| {}).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn corrupt_lines_are_reported_and_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"url\":\"http://vault\",\"user\":\"alice\",\"key\":\"a\"}\n",
                "{not json}\n",
                "{\"url\":\"http://vault\",\"user\":\"alice\",\"key\":\"b\"}\n",
            ),
        )
        .unwrap();
        let mut reported = Vec::new();
        let events: Vec<TestEvent> =
            load_events("journal", &path, &mut |line, error| reported.push((line, error.to_string())))
                .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].key, "b");
        assert_eq!(reported.len(), 1);
        assert_eq!(reported[0].0, 2);
    }

    #[test]
    fn append_writes_complete_lines_under_contention() {
        let dir = tempfile::tempdir().unwrap();
        let path = std::sync::Arc::new(dir.path().join("state.jsonl"));
        let mut handles = Vec::new();
        for i in 0..8 {
            let path = std::sync::Arc::clone(&path);
            handles.push(std::thread::spawn(move || {
                for j in 0..50 {
                    append(
                        "journal",
                        &path,
                        &TestEvent {
                            url: "http://vault".into(),
                            user: "alice".into(),
                            key: format!("g-{i}-{j}"),
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
            serde_json::from_str::<TestEvent>(line).expect("torn line");
            lines += 1;
        }
        assert_eq!(lines, 8 * 50);
    }

    #[test]
    fn compact_with_rebuilds_under_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.jsonl");
        append(
            "journal",
            &path,
            &TestEvent {
                url: "http://a".into(),
                user: "alice".into(),
                key: "old".into(),
            },
        )
        .unwrap();
        compact_with::<TestEvent, _>("journal", &path, |mut events| {
            events.push(TestEvent {
                url: "http://a".into(),
                user: "alice".into(),
                key: "new".into(),
            });
            events
        })
        .unwrap();
        let loaded: Vec<TestEvent> = load_events("journal", &path, &mut |_, _| {}).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[1].key, "new");
        assert!(!path.with_extension("jsonl.tmp").exists());
    }
}
```

- [ ] **Step 3: Run the crate tests**

Run: `cargo test -p journal`
Expected: 4 passed, 0 failed.

- [ ] **Step 4: Format, lint, doc**

Run: `cargo fmt --all -- --check && cargo clippy -p journal --all-targets -- -D warnings && cargo doc --no-deps -p journal`
Expected: all clean, zero doc warnings.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/libs/journal
git commit -m "refactor(cli-tools): add generic jsonl journal crate"
```

(Cargo.lock changed because the workspace gained a member; commit the delta.)

---

### Task 2: vault-push journal through the shared crate

Findings 1. Replace the push journal's file mechanics with the crate; keep every const, event type, state type, semantic fold, and error string.

**Files:**
- Modify: `crates/cli/vault-push/src/journal.rs`, `crates/cli/vault-push/Cargo.toml`

**Interfaces:**
- Consumes: `jsonl_journal::{append, load_events, compact_with}` (Task 1).
- Produces: unchanged `vault_push::journal` internals; lib.rs exports untouched.

- [ ] **Step 1: Add the dependency**

In `crates/cli/vault-push/Cargo.toml`, add after `hex = "0.4.3"`:

```toml
jsonl_journal = { package = "journal", path = "../../libs/journal" }
```

- [ ] **Step 2: Replace the mechanics, keep the semantics**

In `crates/cli/vault-push/src/journal.rs`:

- Keep lines 1–120 unchanged (intro, consts, `JournalEvent`, `JournalEvent::target`, `JournalState`, `JournalState::message_key`, `journal_path`).
- Delete `JOURNAL_WRITE_LOCK` (lines ~23-25), `write_event_line` (lines ~110-120), and the bodies of `load`, `append`, `compact` (lines ~122-301).
- Delete the now-unused imports (`Mutex`, `OpenOptions`, `Write`, `BufReader`, `BufRead`) — the file keeps `std::collections::HashSet`, `std::fs`, `std::path::{Path, PathBuf}`, `anyhow::{Context, Result}`, `serde::{Deserialize, Serialize}`.
- Replace `load`, `append`, `compact` with:

```rust
/// Read the journal and keep events that match this vault URL and username.
///
/// A missing file is treated as an empty journal. A corrupt line is skipped
/// after a warning; those entries will be uploaded again. The server ignores
/// true duplicates.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or a line cannot be read.
pub fn load(path: &Path, url: &str, username: &str) -> Result<JournalState> {
    let mut state = JournalState::default();
    let events: Vec<JournalEvent> = jsonl_journal::load_events("journal", path, &mut |i, e| {
        eprintln!(
            "warning: journal {} line {} is corrupt ({}). \
             The affected entries will be re-submitted (server dedup is safe).",
            path.display(),
            i,
            e
        );
    })?;
    for event in events {
        match event {
            JournalEvent::AssetOk {
                url: u,
                username: a,
                sha256,
                ..
            } if u == url && a == username => {
                state.assets.insert(sha256);
            }
            JournalEvent::MessageOk {
                url: u,
                username: a,
                file,
                guid,
                ..
            } if u == url && a == username => {
                state
                    .messages
                    .insert(JournalState::message_key(&file, &guid));
            }
            JournalEvent::MessageBatchOk {
                url: u,
                username: a,
                messages,
                ..
            } if u == url && a == username => {
                for message in messages {
                    state
                        .messages
                        .insert(JournalState::message_key(&message.file, &message.guid));
                }
            }
            JournalEvent::FileOk {
                url: u,
                username: a,
                file,
                ..
            } if u == url && a == username => {
                state.files.insert(file);
            }
            _ => {}
        }
    }
    Ok(state)
}

/// Append one event as a JSON Lines row and flush it to disk.
///
/// # Errors
///
/// Returns an error when the parent folder cannot be created, the file cannot
/// be opened, or the write fails.
pub fn append(path: &Path, event: &JournalEvent) -> Result<()> {
    jsonl_journal::append("journal", path, event)
}

/// Rewrite the journal from in-memory `state` for one vault URL and username.
///
/// Events for other URL and username pairs are kept, so one export folder can
/// resume against more than one server.
///
/// # Errors
///
/// Returns an error when the existing file cannot be read, the temporary file
/// cannot be written, or the rename fails.
pub fn compact(path: &Path, url: &str, username: &str, state: &JournalState) -> Result<()> {
    jsonl_journal::compact_with::<JournalEvent, _>("journal", path, |mut events| {
        // Preserve other vault targets so one export folder can resume against
        // multiple servers without wiping their skip state.
        events.retain(|event| {
            let (u, a) = event.target();
            u != url || a != username
        });
        let mut assets: Vec<_> = state.assets.iter().collect();
        assets.sort_unstable();
        for sha in assets {
            events.push(JournalEvent::AssetOk {
                url: url.to_string(),
                username: username.to_string(),
                source: String::new(),
                sha256: sha.clone(),
            });
        }
        let messages = messages_from_state_keys(state);
        for batch in messages.chunks(1_000) {
            events.push(JournalEvent::MessageBatchOk {
                url: url.to_string(),
                username: username.to_string(),
                source: String::new(),
                messages: batch.to_vec(),
            });
        }
        let mut files: Vec<_> = state.files.iter().collect();
        files.sort_unstable();
        for file in files {
            events.push(JournalEvent::FileOk {
                url: url.to_string(),
                username: username.to_string(),
                source: String::new(),
                file: file.clone(),
            });
        }
        events
    })
}
```

- Keep `messages_from_state_keys` and the tests module unchanged — the existing tests exercise the wrappers and must pass untouched.

- [ ] **Step 3: Run the tests**

Run: `cargo test -p vault-push`
Expected: all vault-push tests pass, including `journal::tests::{loads_legacy_and_batch_message_success_events, compact_preserves_other_vault_target_events, append_writes_complete_lines_under_contention}`.

- [ ] **Step 4: Format, lint, workspace check**

Run: `cargo fmt --all -- --check && cargo clippy -p vault-push --all-targets -- -D warnings && cargo check --workspace`
Expected: all clean.

- [ ] **Step 5: Commit**

```bash
git add crates/cli/vault-push
git commit -m "refactor(vault-push): journal through the shared jsonl journal crate"
```

---

### Task 3: vault-pull journal through the shared crate

Findings 1. Same treatment for pull. Pull gains the shared write lock (single-threaded today — no observable change, spec delta #4); its `open pull journal …` error strings stay byte-identical via the label.

**Files:**
- Modify: `crates/cli/vault-pull/src/journal.rs`, `crates/cli/vault-pull/Cargo.toml`

**Interfaces:**
- Consumes: `jsonl_journal::{append, load_events, compact_with}` (Task 1).
- Produces: unchanged `vault_pull::journal` public surface (`PULL_JOURNAL_NAME`, `PullJournalEvent`, `PullJournalState`, `journal_path` stay exactly as today).

- [ ] **Step 1: Add the dependency**

In `crates/cli/vault-pull/Cargo.toml`, add after `hex = "0.4"`:

```toml
jsonl_journal = { package = "journal", path = "../../libs/journal" }
```

- [ ] **Step 2: Replace the mechanics**

In `crates/cli/vault-pull/src/journal.rs`:

- Keep lines 1–49 unchanged (intro, const, `PullJournalEvent`, `PullJournalState`, `journal_path`).
- Delete the bodies of `load`, `append`, `compact` and replace with:

```rust
/// Read the journal and keep events that match this vault URL and username.
///
/// A missing file is treated as an empty journal. A line that cannot be parsed
/// is skipped so a newer event type does not break an older client.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or a line cannot be read.
pub fn load(path: &Path, url: &str, username: &str) -> Result<PullJournalState> {
    let mut state = PullJournalState::default();
    let events: Vec<PullJournalEvent> =
        jsonl_journal::load_events("pull journal", path, &mut |_, _| {})?;
    for event in events {
        match event {
            PullJournalEvent::AssetOk {
                url: u,
                username: a,
                sha256,
                ..
            } if u == url && a == username => {
                state.assets.insert(sha256);
            }
            PullJournalEvent::BackupComplete {
                url: u,
                username: a,
                ..
            } if u == url && a == username => {
                state.backup_complete = true;
            }
            _ => {}
        }
    }
    Ok(state)
}

/// Append one event as a JSON Lines row and flush it to disk.
///
/// # Errors
///
/// Returns an error when the parent folder cannot be created, the file cannot
/// be opened, or the write fails.
pub fn append(path: &Path, event: &PullJournalEvent) -> Result<()> {
    jsonl_journal::append("pull journal", path, event)
}

/// Rewrite the journal from in-memory `state` for one vault URL and username.
///
/// # Errors
///
/// Returns an error when the temporary file cannot be written or the rename fails.
pub fn compact(path: &Path, url: &str, username: &str, state: &PullJournalState) -> Result<()> {
    jsonl_journal::compact_with::<PullJournalEvent, _>("pull journal", path, |_events| {
        let mut events: Vec<PullJournalEvent> = Vec::new();
        let mut assets: Vec<_> = state.assets.iter().collect();
        assets.sort_unstable();
        for sha in assets {
            events.push(PullJournalEvent::AssetOk {
                url: url.to_string(),
                username: username.to_string(),
                sha256: sha.clone(),
                path: String::new(), // resume looks up attachments/{sha}, not this path
                size_bytes: 0,       // size is unused when skipping already-downloaded files
            });
        }
        if state.backup_complete {
            // Counts are unused on resume; a `backup_complete` row only means the last run finished.
            events.push(PullJournalEvent::BackupComplete {
                url: url.to_string(),
                username: username.to_string(),
                conversations: 0,
                messages: 0,
                assets: 0,
            });
        }
        events
    })
}
```

- Delete the now-unused imports (`File`, `OpenOptions`, `Write`, `BufReader`, `BufRead`) — the file keeps `std::collections::HashSet`, `std::fs`, `std::path::{Path, PathBuf}`, `anyhow::Result`, `serde::{Deserialize, Serialize}`.
- Keep the tests module unchanged.

- [ ] **Step 3: Run the tests**

Run: `cargo test -p vault-pull`
Expected: all vault-pull tests pass, including `journal::tests::{loads_asset_and_backup_complete_events, filters_by_url_and_username, compact_sorts_assets_and_rewrites}`.

- [ ] **Step 4: Format, lint, workspace check**

Run: `cargo fmt --all -- --check && cargo clippy -p vault-pull --all-targets -- -D warnings && cargo check --workspace`
Expected: all clean.

- [ ] **Step 5: Commit**

```bash
git add crates/cli/vault-pull
git commit -m "refactor(vault-pull): journal through the shared jsonl journal crate"
```

---

### Task 4: `vault-http` crate — client builder, truncate, auth types

Findings 2, 3 (shared home), 10 (truncate fix). Create the second crate and move `AuthError`/`AuthInfo` into it verbatim. No consumers change in this task.

**Files:**
- Create: `crates/libs/vault-http/Cargo.toml`, `crates/libs/vault-http/src/lib.rs`, `crates/libs/vault-http/src/auth_error.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Produces: `vault_http::{build_client, truncate, AuthError, AuthInfo}`.
- Consumes: nothing.

- [ ] **Step 1: Create the crate manifest**

`crates/libs/vault-http/Cargo.toml`:

```toml
[package]
name = "vault-http"
version = "0.1.0"
edition = "2024"
description = "Blocking HTTP client helpers and typed retry classification for the vault CLI crates"
license = "LicenseRef-FCL-1.0-ALv2"

[dependencies]
anyhow = "1.0.103"
reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls"] }

[dev-dependencies]
httpmock = "0.7"
```

Add the workspace member in the root `Cargo.toml` `members` list, right after `"crates/libs/journal",`:

```toml
    "crates/libs/vault-http",
```

- [ ] **Step 2: Move `AuthError` verbatim**

Copy `crates/cli/vault-push/src/auth_error.rs` to `crates/libs/vault-http/src/auth_error.rs` with no edits (it imports only `std::fmt`), then verify character-identical:

```bash
diff crates/cli/vault-push/src/auth_error.rs crates/libs/vault-http/src/auth_error.rs
```

Expected: no diff output. Do NOT delete the vault-push copy in this task.

- [ ] **Step 3: Write the crate root**

`crates/libs/vault-http/src/lib.rs`:

```rust
//! Blocking HTTP client helpers and retry classification for the vault CLI
//! crates.
//!
//! `vault-push` and `vault-pull` both build their session client through
//! [`build_client`], share [`truncate`] for error snippets, and classify
//! retryable failures through [`classify_retry`] / [`with_retries`].
//! [`AuthError`] and [`AuthInfo`] live here so both crates — and the desktop
//! app through their re-exports — share one auth surface.

#![warn(missing_docs)]

mod auth_error;

pub use auth_error::AuthError;

use anyhow::{Context, Result};

/// Account id and username returned by a successful `GET /v1/auth/check`.
#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub account_id: String,
    pub username: Option<String>,
}

/// Build the shared blocking reqwest client.
///
/// One client per `HttpSession`; the connection pool keeps 16 idle
/// connections per host for the worker threads.
///
/// # Errors
///
/// Returns an error when the reqwest client cannot be built.
pub fn build_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .pool_max_idle_per_host(16)
        .build()
        .context("build HTTP client")
}

/// Copy `s`, cutting it to at most `max` bytes and adding an ellipsis when
/// longer.
///
/// Cuts on a char boundary, so multi-byte characters are never split.
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..s.floor_char_boundary(max)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_leaves_short_and_exact_strings_alone() {
        assert_eq!(truncate("short", 200), "short");
        assert_eq!(truncate("exact", 5), "exact");
    }

    #[test]
    fn truncate_adds_ellipsis_and_cuts_on_a_char_boundary() {
        assert_eq!(truncate("123456", 5), "12345…");
        // 'h' is 1 byte, 'é' is 2: max=2 would split 'é' under the old code.
        assert_eq!(truncate("héllo", 2), "h…");
        assert_eq!(truncate("héllo", 3), "hé…");
    }

    #[test]
    fn truncate_survives_max_zero() {
        assert_eq!(truncate("héllo", 0), "…");
    }
}
```

- [ ] **Step 4: Run the crate tests**

Run: `cargo test -p vault-http`
Expected: 3 truncate tests plus all moved AuthError tests pass, 0 failed.

- [ ] **Step 5: Format, lint, doc**

Run: `cargo fmt --all -- --check && cargo clippy -p vault-http --all-targets -- -D warnings && cargo doc --no-deps -p vault-http`
Expected: all clean, zero doc warnings.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/libs/vault-http
git commit -m "refactor(cli-tools): add vault-http crate with client, truncate, and auth types"
```

---

### Task 5: vault-push and vault-pull adopt vault-http

Findings 2, 3. Rewire both CLI crates onto the shared crate and delete the now-duplicated code. Every existing public path (`vault_push::AuthError`, `vault_pull::AuthError`, `vault_pull::authenticate`) keeps working through re-exports — Tauri, the legacy GUI, and dump-cli-docs compile unchanged.

**Files:**
- Modify: `crates/cli/vault-push/src/lib.rs`, `crates/cli/vault-push/src/http.rs`, `crates/cli/vault-push/Cargo.toml`
- Delete: `crates/cli/vault-push/src/auth_error.rs`
- Modify: `crates/cli/vault-pull/src/lib.rs`, `crates/cli/vault-pull/src/http.rs`, `crates/cli/vault-pull/Cargo.toml`

**Interfaces:**
- Consumes: `vault_http::{AuthError, AuthInfo, build_client, truncate}` (Task 4).
- Produces: `vault_push::{AuthError, AuthInfo}` now via re-export; `vault_pull::{AuthError, AuthInfo}` via re-export, `vault_pull::authenticate` unchanged from vault-push.

- [ ] **Step 1: Add the dependency to both crates**

In `crates/cli/vault-push/Cargo.toml`, after the `jsonl_journal` line:

```toml
vault-http = { path = "../../libs/vault-http" }
```

In `crates/cli/vault-pull/Cargo.toml`, after the `jsonl_journal` line:

```toml
vault-http = { path = "../../libs/vault-http" }
```

- [ ] **Step 2: vault-push lib.rs and auth_error.rs**

In `crates/cli/vault-push/src/lib.rs`:

- Delete `mod auth_error;`
- Replace `pub use auth_error::AuthError;` with `pub use vault_http::AuthError;`
- Replace `pub use http::AuthInfo;` with `pub use vault_http::AuthInfo;`

Delete `crates/cli/vault-push/src/auth_error.rs`:

```bash
git rm crates/cli/vault-push/src/auth_error.rs
```

`crate::AuthError` keeps resolving inside vault-push because the lib.rs re-export puts the name at the crate root — http.rs's `use crate::AuthError;` needs no edit.

- [ ] **Step 3: vault-push http.rs — client, truncate, AuthInfo**

In `crates/cli/vault-push/src/http.rs`:

- Delete the `AuthInfo` struct (lines ~16-21) — the name stays visible as `crate::AuthInfo` via the lib.rs re-export.
- Replace the `HttpSession::new` body (lines ~130-143) with:

```rust
impl HttpSession {
    /// Blocking HTTP client with a connection pool for worker threads.
    ///
    /// # Errors
    ///
    /// Returns an error when the reqwest client cannot be built.
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: vault_http::build_client()?,
        })
    }
}
```

- Delete the local `truncate` fn (lines ~173-180). Add to the top `use` block:

```rust
use vault_http::truncate;
```

The three existing bare call sites (`truncate(&text, 200)` in `auth_check`, `truncate(&start_text, 200)` in `put_asset_multipart`, and any others the compiler reports) keep working unchanged.

- [ ] **Step 4: vault-pull lib.rs and http.rs**

In `crates/cli/vault-pull/src/lib.rs`, replace:

```rust
pub use vault_push::{AuthError, AuthInfo, authenticate};
```

with:

```rust
pub use vault_http::{AuthError, AuthInfo};
pub use vault_push::authenticate;
```

In `crates/cli/vault-pull/src/http.rs`:

- Replace the `HttpSession::new` body (lines ~192-202) the same way as Step 3 (blocking doc text identical).
- Delete the local `truncate` fn (lines ~394-401). Add to the top `use` block:

```rust
use vault_http::truncate;
```

The existing call sites (`truncate(&body, 300)` ×2, `truncate(&body, 300)` in the download bail) keep working unchanged.

- [ ] **Step 5: Build and test the whole workspace**

Run: `cargo build --workspace && cargo test --workspace && cargo check --manifest-path src-tauri/Cargo.toml`
Expected: build clean; all tests pass (auth_error tests now run under `-p vault-http`); src-tauri still compiles through the re-exports.

- [ ] **Step 6: Format, lint**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/cli/vault-push crates/cli/vault-pull Cargo.lock
git commit -m "refactor(cli-tools): vault-push and vault-pull adopt vault-http"
```

---

### Task 6: typed retry classification via vault-http

Findings 11 (and the 401/403/413 string bails it feeds on). Add the retry machinery to vault-http, tag every status-derived bail in push's retried paths with `VaultHttpError`, and delete the string-matching classifier. Adjudicated semantics: all 4xx permanent.

**Files:**
- Create: `crates/libs/vault-http/src/retry.rs`, `crates/libs/vault-http/tests/classify_reqwest.rs`
- Modify: `crates/libs/vault-http/src/lib.rs`, `crates/cli/vault-push/src/http.rs`, `crates/cli/vault-push/src/run.rs`

**Interfaces:**
- Produces: `vault_http::{RetryKind, VaultHttpError, classify_retry, with_retries}`.
- Consumes: Task 4 crate; Task 5 push wiring.

- [ ] **Step 1: Write the failing classifier tests**

In `crates/libs/vault-http/src/lib.rs`, add `mod retry;` directly after the existing `mod auth_error;` line, and extend the existing `pub use auth_error::AuthError;` so the declaration block reads:

```rust
mod auth_error;
mod retry;

pub use auth_error::AuthError;
pub use retry::{RetryKind, VaultHttpError, classify_retry, with_retries};
```

Create `crates/libs/vault-http/src/retry.rs` with the tests first and a stub implementation (compiles, tests fail):

```rust
//! Typed retry classification for the vault HTTP paths.

use std::io;
use std::thread;
use std::time::Duration;

use anyhow::Result;

use crate::AuthError;

/// Whether a failure is likely to succeed on retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryKind {
    /// Worth retrying: network, timeout, 5xx, or anything unrecognized.
    Transient,
    /// Will fail the same way again: auth, 4xx, missing local files.
    Permanent,
}

/// An HTTP-status failure with its human-readable message.
///
/// `Display` prints only the message, so error text stays exactly what the
/// call site wrote; the status travels typed for [`classify_retry`].
#[derive(Debug)]
pub struct VaultHttpError {
    status: u16,
    message: String,
}

impl VaultHttpError {
    /// Build a status-tagged error that displays `message` verbatim.
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for VaultHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for VaultHttpError {}

/// Classify an error for [`with_retries`].
///
/// Checks, in order: [`VaultHttpError`] (4xx permanent), [`AuthError`] (auth
/// and 4xx permanent, transport transient), `reqwest::Error` status (4xx
/// permanent), `std::io::Error` kind (`NotFound` permanent). Anything
/// unrecognized is transient, matching the historical default.
pub fn classify_retry(_error: &anyhow::Error) -> RetryKind {
    RetryKind::Transient // stub — replaced in Step 3
}

/// Run `op` again on transient failures, with backoff, up to `max_retries`
/// extra tries.
///
/// # Errors
///
/// Returns the last error from `op` when retries are exhausted or the error is
/// permanent.
pub fn with_retries<T, F>(max_retries: u32, mut op: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match op() {
            Ok(v) => return Ok(v),
            Err(e) => {
                if attempt > max_retries || classify_retry(&e) == RetryKind::Permanent {
                    return Err(e);
                }
                // Exponential backoff with jitter.
                let base_ms = 500u64 * 2u64.saturating_pow(attempt.saturating_sub(1));
                let jitter_ms = (base_ms / 4).min(5000);
                let wait_ms = base_ms + (jitter_ms / 2) + (jitter_ms as f64 * rand_factor()) as u64;
                thread::sleep(Duration::from_millis(wait_ms.min(30_000)));
            }
        }
    }
}

/// Deterministic pseudo-random factor in [0.0, 1.0) for retry jitter.
fn rand_factor() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1000) as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    fn classified(kind: RetryKind) -> bool {
        kind == RetryKind::Permanent
    }

    #[test]
    fn http_status_errors_are_permanent_for_4xx() {
        let e = anyhow::Error::from(VaultHttpError::new(404, "asset HEAD failed (HTTP 404)"));
        assert!(classified(classify_retry(&e)));
        let e = anyhow::Error::from(VaultHttpError::new(413, "import rejected: HTTP 413"));
        assert!(classified(classify_retry(&e)));
        let e = anyhow::Error::from(VaultHttpError::new(401, "invalid vault key"));
        assert!(classified(classify_retry(&e)));
    }

    #[test]
    fn http_status_errors_are_transient_for_5xx() {
        let e = anyhow::Error::from(VaultHttpError::new(503, "asset part 1 failed (HTTP 503)"));
        assert!(!classified(classify_retry(&e)));
    }

    #[test]
    fn auth_failures_are_permanent() {
        assert!(classified(classify_retry(&anyhow::Error::from(
            AuthError::InvalidKey
        ))));
        assert!(classified(classify_retry(&anyhow::Error::from(
            AuthError::Forbidden {
                status: 403,
                body: "username does not match vault key".into(),
            }
        ))));
        assert!(classified(classify_retry(&anyhow::Error::from(
            AuthError::RateLimited {
                status: 429,
                body: "slow down".into(),
            }
        ))));
        assert!(classified(classify_retry(&anyhow::Error::from(
            AuthError::HttpStatus {
                status: 418,
                body: "teapot".into(),
            }
        ))));
    }

    #[test]
    fn auth_transport_failures_are_transient() {
        assert!(!classified(classify_retry(&anyhow::Error::from(
            AuthError::Network {
                url: "https://v".into(),
                detail: "dns".into(),
            }
        ))));
        assert!(!classified(classify_retry(&anyhow::Error::from(
            AuthError::ServerError {
                status: 503,
                body: "busy".into(),
            }
        ))));
    }

    #[test]
    fn io_not_found_is_permanent_other_io_is_transient() {
        let e = anyhow::Error::from(io::Error::new(io::ErrorKind::NotFound, "no such file"));
        assert!(classified(classify_retry(&e)));
        let e = anyhow::Error::from(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        assert!(!classified(classify_retry(&e)));
    }

    #[test]
    fn unrecognized_errors_are_transient() {
        assert!(!classified(classify_retry(&anyhow!("something odd"))));
    }

    #[test]
    fn with_retries_gives_up_on_permanent_immediately() {
        let mut calls = 0;
        let result = with_retries(3, || -> Result<u32> {
            calls += 1;
            Err(anyhow::Error::from(VaultHttpError::new(404, "gone")))
        });
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }
}
```

Create `crates/libs/vault-http/tests/classify_reqwest.rs`:

```rust
//! reqwest-status classification needs a real response: build an error via
//! `error_for_status()` against a local mock server.

use httpmock::prelude::*;
use vault_http::{RetryKind, classify_retry};

#[test]
fn reqwest_4xx_status_is_permanent_5xx_is_transient() {
    let server = MockServer::start();
    let mock_404 = server.mock(|when, then| {
        when.method(GET).path("/missing");
        then.status(404);
    });
    let mock_500 = server.mock(|when, then| {
        when.method(GET).path("/broken");
        then.status(500);
    });

    let client = reqwest::blocking::Client::new();
    let e404 = client
        .get(server.url("/missing"))
        .send()
        .and_then(|r| r.error_for_status())
        .unwrap_err();
    assert_eq!(classify_retry(&anyhow::Error::new(e404)), RetryKind::Permanent);

    let e500 = client
        .get(server.url("/broken"))
        .send()
        .and_then(|r| r.error_for_status())
        .unwrap_err();
    assert_eq!(classify_retry(&anyhow::Error::new(e500)), RetryKind::Transient);

    mock_404.assert();
    mock_500.assert();
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p vault-http`
Expected: the `retry::tests` classification tests FAIL (stub returns Transient for everything) while the stub compiles. The httpmock integration test also fails.

- [ ] **Step 3: Implement the classifier**

Replace the stub `classify_retry` in `crates/libs/vault-http/src/retry.rs`:

```rust
pub fn classify_retry(error: &anyhow::Error) -> RetryKind {
    if let Some(http) = error.downcast_ref::<VaultHttpError>() {
        return if (400..500).contains(&http.status) {
            RetryKind::Permanent
        } else {
            RetryKind::Transient
        };
    }
    if let Some(auth) = error.downcast_ref::<AuthError>() {
        return match auth {
            AuthError::InvalidKey
            | AuthError::Forbidden { .. }
            | AuthError::ApiNotFound { .. }
            | AuthError::RateLimited { .. }
            | AuthError::Rejected { .. } => RetryKind::Permanent,
            AuthError::HttpStatus { status, .. } if (400..500).contains(status) => {
                RetryKind::Permanent
            }
            _ => RetryKind::Transient,
        };
    }
    if let Some(reqwest) = error.downcast_ref::<reqwest::Error>() {
        return match reqwest.status() {
            Some(status) if (400..500).contains(&status.as_u16()) => RetryKind::Permanent,
            _ => RetryKind::Transient,
        };
    }
    if let Some(io) = error.downcast_ref::<io::Error>() {
        return if io.kind() == io::ErrorKind::NotFound {
            RetryKind::Permanent
        } else {
            RetryKind::Transient
        };
    }
    RetryKind::Transient
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p vault-http`
Expected: all classification tests and the httpmock integration test pass.

- [ ] **Step 5: Tag the vault-push bail sites**

In `crates/cli/vault-push/src/http.rs`, add the import (top `use` block):

```rust
use vault_http::{HttpStatusError, truncate};
```

(Replace the `use vault_http::truncate;` line from Task 5 with this combined line.)

Replace the status-derived bails in the retried paths (`head_asset`, `put_asset`, `put_asset_multipart`, `post_import`) so each error carries its status. Every message string stays byte-identical:

`head_asset` (lines ~306-307):

```rust
            401 => return Err(HttpStatusError::new(401, "invalid vault key").into()),
            403 => return Err(HttpStatusError::new(403, "username does not match vault key").into()),
```

`head_asset` generic failure (lines ~310-313) — replace the `bail!` with:

```rust
        if !status.is_success() {
            let text = response.text().unwrap_or_default();
            return Err(HttpStatusError::new(
                status.as_u16(),
                format!("asset HEAD failed (HTTP {status}): {text}"),
            )
            .into());
        }
```

`put_asset` 413 site (lines ~373-378):

```rust
        if looks_like_payload_too_large(status, &text) {
            return Err(HttpStatusError::new(
                413,
                payload_too_large_message("asset upload", Some(file_len as usize)),
            )
            .into());
        }
```

`put_asset` generic failure (lines ~384-391):

```rust
        if !status.is_success() || !parsed.ok {
            return Err(HttpStatusError::new(
                status.as_u16(),
                parsed
                    .error
                    .unwrap_or_else(|| format!("HTTP {status}: {text}")),
            )
            .into());
        }
```

`put_asset_multipart` start 413 (lines ~428-430):

```rust
        if looks_like_payload_too_large(start_status, &start_text) {
            return Err(HttpStatusError::new(
                413,
                payload_too_large_message("asset upload start", None),
            )
            .into());
        }
```

`put_asset_multipart` start generic (lines ~438-444):

```rust
        if !start_status.is_success() || !started.ok {
            return Err(HttpStatusError::new(
                start_status.as_u16(),
                started
                    .error
                    .unwrap_or_else(|| format!("HTTP {start_status}: {start_text}")),
            )
            .into());
        }
```

`put_asset_multipart` part 413 (lines ~502-508):

```rust
            if looks_like_payload_too_large(status, &text) {
                abort(self, &upload_id);
                return Err(HttpStatusError::new(
                    413,
                    payload_too_large_message("asset upload part", Some(this_len)),
                )
                .into());
            }
```

`put_asset_multipart` part generic (lines ~509-512):

```rust
            if !status.is_success() {
                abort(self, &upload_id);
                return Err(HttpStatusError::new(
                    status.as_u16(),
                    format!("asset part {part} failed (HTTP {status}): {text}"),
                )
                .into());
            }
```

`put_asset_multipart` complete generic (lines ~536-544):

```rust
        if !status.is_success() || !parsed.ok {
            abort(self, &upload_id);
            return Err(HttpStatusError::new(
                status.as_u16(),
                parsed
                    .error
                    .unwrap_or_else(|| format!("HTTP {status}: {text}")),
            )
            .into());
        }
```

`post_import` local size pre-check (lines ~568-570):

```rust
        if body_len > crate::run::MAX_PROXY_BODY_BYTES {
            return Err(HttpStatusError::new(
                413,
                payload_too_large_message("import", Some(body_len)),
            )
            .into());
        }
```

`post_import` response 413 (lines ~593-595):

```rust
        if looks_like_payload_too_large(status, &text) {
            return Err(HttpStatusError::new(
                413,
                payload_too_large_message("import", Some(body_len)),
            )
            .into());
        }
```

`post_import` generic failure (lines ~607-613):

```rust
        if !status.is_success() || !parsed.ok {
            return Err(HttpStatusError::new(
                status.as_u16(),
                parsed
                    .error
                    .unwrap_or_else(|| format!("HTTP {status}: {text}")),
            )
            .into());
        }
```

Leave `start_import` and `complete_import` bails untouched — they are not inside retried closures.

- [ ] **Step 6: Delete the old classifier and swap the call sites**

In `crates/cli/vault-push/src/http.rs`, delete `is_transient_error`, `with_retries`, and `rand_factor` (lines ~793-850), and the now-unused `use std::thread;` and `use std::time::Duration;` if the compiler says they are unused (Duration stays — timeouts use it; thread goes away).

In `crates/cli/vault-push/src/run.rs`, replace the two `http::with_retries(` call expressions with `vault_http::with_retries(` (no new `use` needed — the path resolves through the extern prelude). The closures and their `.map_err(|error| error.to_string())` wrappers stay untouched.

- [ ] **Step 7: Build, test, lint**

Run: `cargo build --workspace && cargo test --workspace && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all green. `grep -rn "is_transient_error" crates/` returns nothing.

- [ ] **Step 8: Commit**

```bash
git add crates/libs/vault-http crates/cli/vault-push Cargo.lock
git commit -m "refactor(vault-push): typed retry classification via vault-http"
```

---

### Task 7: demo-seed config wiring and honest about line

Findings 8, 9 (adjudicated: wire, don't delete). The shipped `demo_seed.toml` values equal today's hard-coded ones, so the generated dataset stays byte-identical — verified by a before/after generation diff in this task.

**Files:**
- Modify: `crates/vault/demo-seed/src/config.rs`, `crates/vault/demo-seed/src/personas.rs`, `crates/vault/demo-seed/src/main.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: unchanged `SeedConfig` deserialization surface (the fields already exist); new load-time validation for `labels.names`.

- [ ] **Step 1: Generate the baseline dataset**

Before any edit, generate the dataset from the unmodified tree into a scratch dir:

```bash
cargo run -p demo-seed -- --out "$CLAUDE_JOB_DIR/tmp/demo-before" --seed 42
```

Expected: exits 0; `demo-before/staging/{imessage,sms-backup-restore,whatsapp}` written.

- [ ] **Step 2: Write the failing validation test**

In `crates/vault/demo-seed/src/config.rs` tests module (after `rejects_inverted_large_band`):

```rust
    #[test]
    fn rejects_labels_names_without_four_entries() {
        let mut cfg = SeedConfig::load(&SeedConfig::default_path()).expect("load");
        cfg.labels.names = vec!["Family".into(), "Work".into()];
        assert!(cfg.validate().is_err());
    }
```

In `crates/vault/demo-seed/src/personas.rs` tests module (after `roster_guarantees_large_groups`):

```rust
    #[test]
    fn sample_name_shape_respects_configured_shares() {
        let mut cfg = SeedConfig::load(&SeedConfig::default_path()).expect("load demo_seed.toml");
        let names = NameBank::load_default().expect("names");
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        cfg.contacts.first_only = 1.0;
        cfg.contacts.first_middle_last = 0.0;
        cfg.contacts.first_last = 0.0;
        for _ in 0..50 {
            let (first, middle, last) = sample_name_shape(&cfg, &names, &mut rng);
            assert!(!first.is_empty());
            assert!(middle.is_empty());
            assert!(last.is_empty());
        }
        cfg.contacts.first_only = 0.0;
        cfg.contacts.first_middle_last = 1.0;
        cfg.contacts.first_last = 0.0;
        for _ in 0..50 {
            let (first, middle, last) = sample_name_shape(&cfg, &names, &mut rng);
            assert!(!first.is_empty());
            assert!(!middle.is_empty());
            assert!(!last.is_empty());
        }
        cfg.contacts.first_middle_last = 0.0;
        cfg.contacts.first_last = 1.0;
        for _ in 0..50 {
            let (first, middle, last) = sample_name_shape(&cfg, &names, &mut rng);
            assert!(!first.is_empty());
            assert!(middle.is_empty());
            assert!(!last.is_empty());
        }
    }

    #[test]
    fn group_labels_come_from_config_names() {
        let cfg = SeedConfig::load(&SeedConfig::default_path()).expect("load demo_seed.toml");
        let names = NameBank::load_default().expect("names");
        let mut rng = ChaCha8Rng::seed_from_u64(cfg.seed);
        let roster = build_roster(&cfg, &names, &mut rng).expect("roster");
        for contact in &roster.contacts {
            for group in &contact.groups {
                assert!(
                    cfg.labels.names.iter().any(|n| n == group),
                    "group label not from config: {group}"
                );
            }
        }
    }
```

Run: `cargo test -p demo-seed`
Expected: the two new tests FAIL to compile or fail at runtime (labels still hard-coded; first_last ignored).

- [ ] **Step 3: Wire the config into the generator**

In `crates/vault/demo-seed/src/config.rs`:

- Delete `#[allow(dead_code)]` above `first_last` and the stale comment above it (lines ~51-53).
- Delete `#[allow(dead_code)]` above `LabelsConfig.names`.
- At the TOP of `validate()` (before the `let g = &self.groups;` line and its early return — the existing early return would skip any check placed below it), insert:

```rust
        if self.labels.names.len() != 4 {
            anyhow::bail!(
                "labels.names must have exactly 4 entries (family, work, college, inactive), found {}",
                self.labels.names.len()
            );
        }
```

In `crates/vault/demo-seed/src/personas.rs`, replace the label block in `build_contact` (lines ~154-166):

```rust
    let mut groups = Vec::new();
    if inactive {
        groups.push(cfg.labels.names[3].clone());
    } else {
        if rng.random_bool(cfg.labels.family) {
            groups.push(cfg.labels.names[0].clone());
        }
        if rng.random_bool(cfg.labels.work) {
            groups.push(cfg.labels.names[1].clone());
        }
        if rng.random_bool(cfg.labels.college) {
            groups.push(cfg.labels.names[2].clone());
        }
    }
```

Replace the third branch condition in `sample_name_shape` (line ~210):

```rust
    } else if roll < cfg.contacts.first_only + cfg.contacts.first_middle_last + cfg.contacts.first_last {
```

(The final `else` stays — defensive fallthrough; with the shipped values summing to 1.0 the third branch covers `[0.04, 1.0)` exactly like today's `else`.)

In `crates/vault/demo-seed/src/main.rs`, replace the `about`:

```rust
#[command(about = "Generate the demo message dataset (iMessage, SMS Backup & Restore, WhatsApp) for Message Vault")]
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p demo-seed`
Expected: all tests pass, including the three new ones.

- [ ] **Step 5: Verify dataset identity**

Run:

```bash
cargo run -p demo-seed -- --out "$CLAUDE_JOB_DIR/tmp/demo-after" --seed 42
diff -r "$CLAUDE_JOB_DIR/tmp/demo-before" "$CLAUDE_JOB_DIR/tmp/demo-after" && echo IDENTICAL
```

Expected: `IDENTICAL` (byte-for-byte equal staging trees).

- [ ] **Step 6: Format, lint**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean (the removed `#[allow(dead_code)]`s must not resurface as warnings).

- [ ] **Step 7: Commit**

```bash
git add crates/vault/demo-seed
git commit -m "feat(demo-seed): wire first_last and label names into the generator"
```

---

### Task 8: docs, honest help text, sha2 removal, page regeneration

Findings 4–7, 12. Document the dump-cli-docs surface and gate it; the one-line doc gaps; the `--mode` rewrite; drop the unused sha2; regenerate the one changed CLI page and pin it.

**Files:**
- Modify: `crates/cli/dump-cli-docs/src/lib.rs`, `crates/cli/vault-push/src/cli.rs`, `crates/cli/vault-pull/src/cli.rs`, `crates/cli/vault-pull/src/run.rs`, `crates/cli/vault-pull/Cargo.toml`, `CHANGELOG.md`
- Regenerate: `docs/src/content/docs/vault/developer/reference/cli/vault-push.md`
- Likely: `Cargo.lock` (sha2 0.10 entry drops)

**Interfaces:**
- Consumes: nothing new.
- Produces: documented `dump_cli_docs` surface (same items); corrected clap help for vault-push `--mode`.

- [ ] **Step 1: dump-cli-docs crate intro, gate, and item docs**

In `crates/cli/dump-cli-docs/src/lib.rs`, add before `use clap::Command;`:

```rust
//! Generates the CLI reference pages on the docs site from each command's
//! clap definition.
//!
//! The pages live in `docs/src/content/docs/vault/developer/reference/cli/`
//! and are regenerated with
//! `cargo run -p dump-cli-docs -- --output-dir docs/src/content/docs/vault/developer/reference`.

#![warn(missing_docs)]

```

Replace the `PageSpec` struct with the documented version:

```rust
/// One generated CLI reference page.
pub struct PageSpec {
    /// Stable id used by [`command_for`] and [`page_markdown`] to look the page up.
    pub id: &'static str,
    /// Page title (Starlight frontmatter `title`).
    pub title: &'static str,
    /// Page description (Starlight frontmatter `description`).
    pub description: &'static str,
    /// Output path relative to the docs reference directory.
    pub rel_path: &'static str,
}

/// Every CLI reference page, in site order.
pub const PAGE_SPECS: &[PageSpec] = &[
```

(Keep the 11 `PageSpec` entries and the closing `];` unchanged.)

Add doc lines to the four functions, keeping every body unchanged:

```rust
/// Render one page: Starlight frontmatter plus the clap-generated markdown body.
pub fn render_page(spec: &PageSpec, command: &Command) -> String {
```

```rust
/// The clap `Command` for a page id.
///
/// # Errors
///
/// Returns an error when `id` is not one of the commands known here.
pub fn command_for(id: &str) -> anyhow::Result<clap::Command> {
```

```rust
/// The full markdown source of one page.
///
/// # Errors
///
/// Returns an error when `id` is not in [`PAGE_SPECS`] or the command cannot
/// be built.
pub fn page_markdown(id: &str) -> anyhow::Result<String> {
```

```rust
/// Write every page in [`PAGE_SPECS`] under `output_dir`.
///
/// # Errors
///
/// Returns an error when a page cannot be rendered or written.
pub fn write_pages(output_dir: &std::path::Path) -> anyhow::Result<()> {
```

- [ ] **Step 2: The one-line doc gaps**

In `crates/cli/vault-push/src/cli.rs` and `crates/cli/vault-pull/src/cli.rs`, above each `pub fn clap_command()`:

```rust
/// The clap `Command` for embedding --help output into the docs pages and GUI.
```

In `crates/cli/vault-pull/src/run.rs`, replace the bare `pub const DEFAULT_PAGE_LIMIT: usize = 100;` line with:

```rust
/// Default page size for GET /v1/export/messages.
pub const DEFAULT_PAGE_LIMIT: usize = 100;
```

- [ ] **Step 3: The honest `--mode` help**

In `crates/cli/vault-push/src/cli.rs`, replace the `mode` doc line (line ~34):

```rust
    /// Import mode: append: add to existing data (safe to re-run); replace: delete existing messages for this source, then import
```

- [ ] **Step 4: Drop the unused sha2**

Delete the `sha2 = "0.10"` line from `crates/cli/vault-pull/Cargo.toml`.

- [ ] **Step 5: Regenerate the CLI pages and pin them**

Run:

```bash
cargo run -p dump-cli-docs -- --output-dir docs/src/content/docs/vault/developer/reference
git status --short docs/src/content/docs/vault/developer/reference
```

Expected: exactly one modified file, `cli/vault-push.md` (its `--mode` help text changed). Then:

```bash
cargo test -p dump-cli-docs committed_cli_pages_match_dump
```

Expected: pass.

- [ ] **Step 6: Docs gate and tests**

Run: `cargo doc --no-deps -p dump-cli-docs` — expected zero warnings. Run `cargo build --workspace && cargo test --workspace` — expected all green (the sha2 removal updates `Cargo.lock`; commit the delta).

- [ ] **Step 7: Changelog**

In `CHANGELOG.md`, under `## [Unreleased]` → `### Changed`, after the Exporters bullet, add:

```markdown
- **CLI tools:** extract the duplicated JSONL journal and vault HTTP client
  into two shared lib crates, replace substring retry classification with a
  typed classifier (all 4xx failures are permanent), wire demo-seed's
  name-shape and label-name config into the generator, and document the
  dump-cli-docs surface. Retry and truncation edge cases fixed; journal
  files, error text, and the demo dataset unchanged.
```

- [ ] **Step 8: Format, lint**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add crates/cli CHANGELOG.md docs/src/content/docs/vault/developer/reference/cli/vault-push.md Cargo.lock
git commit -m "docs(cli-tools): document CLI surfaces and make --mode help honest"
```

---

### Task 9: final verification sweep

Whole-branch pass and leftover sweeps, then the PR.

**Files:**
- Modify: whatever the sweep finds (expected: none).

**Interfaces:**
- Consumes: all prior tasks.

- [ ] **Step 1: Full pre-PR check**

Run: `./scripts/check-pr.sh`
Expected: ends with `All pre-PR checks passed.` (format, license, cargo-deny, workspace build+test, src-tauri check, web lint/test/audit/build, docs check/build/audit).

- [ ] **Step 2: Docs gates**

Run: `cargo doc --no-deps -p journal -p vault-http -p dump-cli-docs`
Expected: zero warnings.

- [ ] **Step 3: Leftover sweeps**

Run:

```bash
grep -rn "fn truncate" crates/cli | grep -v vault-http || true
grep -rn "is_transient_error\|JOURNAL_WRITE_LOCK" crates/ || true
grep -rn "resume-safe" crates/ docs/ || true
grep -rn "sha2" crates/cli/vault-pull/Cargo.toml || true
```

Expected: `fn truncate` only in vault-http; `is_transient_error` and `JOURNAL_WRITE_LOCK` nowhere; `resume-safe` nowhere; no sha2 in vault-pull.

- [ ] **Step 4: Commit any sweep fixes**

Only if Step 3 or the checks surfaced drift:

```bash
git add -A
git commit -m "chore(cli-tools): final verification sweep"
```

- [ ] **Step 5: Push and open the PR**

```bash
git push -u origin worktree-cli-tools-followup
gh pr create --title "refactor(cli-tools): shared journal and vault client, typed retries" \
  --body "Implementation PR for the CLI tools follow-up (group 4 of 5 from the product Rust audit). \
Spec: docs/superpowers/specs/2026-08-23-cli-tools-followup-design.md. \
Executes this plan task-by-task with subagent-driven development; behavior pinned by the existing \
journal, retry, demo-seed, and committed CLI-page tests."
```

Then verify with `gh pr view` and `gh pr checks`.
