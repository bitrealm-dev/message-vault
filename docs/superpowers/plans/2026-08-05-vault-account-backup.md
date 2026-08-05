# Vault Account Backup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fast, resumable full-account backup from Message Vault via the desktop app — parallel attachment downloads (8 workers), resume journal, buffered serialization, and a dedicated one-click GUI flow.

**Architecture:** Mirror vault-push's parallelism and reliability patterns in vault-pull. Add a `PullJournal` (append-only JSONL, same pattern as push's journal) that tracks downloaded assets. Parallelize attachment downloads with work-stealing workers. Buffer message serialization during page fetch. Add a standalone "Backup Account" GUI screen separate from the import/export guided workflow.

**Tech Stack:** Rust (edition 2024), `reqwest` blocking, `serde`/`serde_json`, Slint 1.17, same dependencies as existing vault-pull and message-vault-io-gui crates.

## Global Constraints

- No server-side API changes (same `/v1/export/messages`, `/v1/assets/{sha256}`, `/v1/export/messages/count`)
- Backward compatible: existing Vault Export screen and vault-pull CLI continue to work unchanged
- Journal trust model matches push: journal is authoritative, no re-hashing on resume unless `force` is set
- GUI follows existing Slint patterns (global adapters, `FormRow` widgets, log panel)

---

### Task 1: vault-pull journal module

**Files:**
- Create: `crates/vault-pull/src/journal.rs`
- Modify: `crates/vault-pull/src/lib.rs`

**Interfaces:**
- Consumes: nothing (no task dependencies)
- Produces:
  - `pub const PULL_JOURNAL_NAME: &str = ".vault-pull-state.jsonl"`
  - `pub enum PullJournalEvent { AssetOk { url, username, sha256, path, size_bytes }, BackupComplete { url, username, conversations, messages, assets } }`
  - `pub struct PullJournalState { pub assets: HashSet<String>, pub backup_complete: bool }`
  - `pub fn load(path: &Path, url: &str, username: &str) -> Result<PullJournalState>`
  - `pub fn append(path: &Path, event: &PullJournalEvent) -> Result<()>`
  - `pub fn compact(path: &Path, url: &str, username: &str, state: &PullJournalState) -> Result<()>`
  - `pub fn journal_path(out_dir: &Path) -> PathBuf`

- [ ] **Step 1: Write the journal module**

```rust
// crates/vault-pull/src/journal.rs
//! Append-only resume journal for vault-pull (mirrors vault-push/src/journal.rs).

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const PULL_JOURNAL_NAME: &str = ".vault-pull-state.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PullJournalEvent {
    AssetOk {
        url: String,
        username: String,
        sha256: String,
        path: String,
        size_bytes: u64,
    },
    BackupComplete {
        url: String,
        username: String,
        conversations: u64,
        messages: u64,
        assets: u64,
    },
}

#[derive(Debug, Default)]
pub struct PullJournalState {
    /// SHA-256 digests of assets already downloaded.
    pub assets: HashSet<String>,
    /// True if the last run completed cleanly (a `backup_complete` event was written).
    pub backup_complete: bool,
}

pub fn journal_path(out_dir: &Path) -> PathBuf {
    out_dir.join(PULL_JOURNAL_NAME)
}

pub fn load(path: &Path, url: &str, username: &str) -> Result<PullJournalState> {
    let mut state = PullJournalState::default();
    if !path.is_file() {
        return Ok(state);
    }
    let file = File::open(path).with_context(|| format!("open pull journal {}", path.display()))?;
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("read pull journal line {}", i + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: PullJournalEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue, // skip unparseable lines (forward compat)
        };
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

pub fn append(path: &Path, event: &PullJournalEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open pull journal for append {}", path.display()))?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

pub fn compact(path: &Path, url: &str, username: &str, state: &PullJournalState) -> Result<()> {
    let mut events: Vec<PullJournalEvent> = Vec::new();
    let mut assets: Vec<_> = state.assets.iter().collect();
    assets.sort_unstable();
    for sha in assets {
        events.push(PullJournalEvent::AssetOk {
            url: url.to_string(),
            username: username.to_string(),
            sha256: sha.clone(),
            path: String::new(),   // path not needed for resume (uses attachments/{sha})
            size_bytes: 0,         // size not needed for resume
        });
    }
    if state.backup_complete {
        // backup_complete is rewritten during compact — conversations/messages/assets set to 0
        // since the compacted form only needs to signal "was complete"
        events.push(PullJournalEvent::BackupComplete {
            url: url.to_string(),
            username: username.to_string(),
            conversations: 0,
            messages: 0,
            assets: 0,
        });
    }
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut out = File::create(&tmp)?;
        for event in &events {
            serde_json::to_writer(&mut out, event)?;
            out.write_all(b"\n")?;
        }
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_asset_and_backup_complete_events() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PULL_JOURNAL_NAME);
        fs::write(
            &path,
            concat!(
                "{\"event\":\"asset_ok\",\"url\":\"http://vault\",\"username\":\"alice\",",
                "\"sha256\":\"aaabbbccc\",\"path\":\"attachments/aaabbbccc\",\"size_bytes\":12345}\n",
                "{\"event\":\"asset_ok\",\"url\":\"http://vault\",\"username\":\"alice\",",
                "\"sha256\":\"dddeeefff\",\"path\":\"attachments/dddeeefff\",\"size_bytes\":67890}\n",
                "{\"event\":\"backup_complete\",\"url\":\"http://vault\",\"username\":\"alice\",",
                "\"conversations\":2,\"messages\":100,\"assets\":2}\n",
            ),
        )
        .unwrap();

        let state = load(&path, "http://vault", "alice").unwrap();

        assert!(state.assets.contains("aaabbbccc"));
        assert!(state.assets.contains("dddeeefff"));
        assert!(state.backup_complete);
    }

    #[test]
    fn filters_by_url_and_username() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PULL_JOURNAL_NAME);
        fs::write(
            &path,
            concat!(
                "{\"event\":\"asset_ok\",\"url\":\"http://vault-a\",\"username\":\"alice\",",
                "\"sha256\":\"aaa\",\"path\":\"attachments/aaa\",\"size_bytes\":1}\n",
                "{\"event\":\"asset_ok\",\"url\":\"http://vault-b\",\"username\":\"bob\",",
                "\"sha256\":\"bbb\",\"path\":\"attachments/bbb\",\"size_bytes\":2}\n",
            ),
        )
        .unwrap();

        let state = load(&path, "http://vault-a", "alice").unwrap();
        assert!(state.assets.contains("aaa"));
        assert!(!state.assets.contains("bbb"));
    }

    #[test]
    fn compact_sorts_assets_and_rewrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PULL_JOURNAL_NAME);
        let mut state = PullJournalState::default();
        state.assets.insert("ccc".into());
        state.assets.insert("aaa".into());
        state.assets.insert("bbb".into());
        state.backup_complete = true;

        compact(&path, "http://vault", "alice", &state).unwrap();

        let reloaded = load(&path, "http://vault", "alice").unwrap();
        assert_eq!(reloaded.assets.len(), 3);
        assert!(reloaded.assets.contains("aaa"));
        assert!(reloaded.assets.contains("bbb"));
        assert!(reloaded.assets.contains("ccc"));
        assert!(reloaded.backup_complete);
    }
}
```

- [ ] **Step 2: Register the module in lib.rs**

In `crates/vault-pull/src/lib.rs`, add `mod journal;` and re-export the public types:

```rust
mod http;
mod project;
mod run;
pub mod journal; // new

pub use http::ExportMessage;
pub use journal::{PullJournalState, PullJournalEvent, PULL_JOURNAL_NAME, journal_path};
pub use run::{
    DEFAULT_PAGE_LIMIT, ProgressEvent, ProgressFn, PullReport, QueryStats, VaultPullConfig,
    compose_query, query_stats, run,
};
pub use vault_push::{AuthError, AuthInfo, authenticate};
```

- [ ] **Step 3: Build and run tests**

```bash
cargo test -p vault-pull -- journal
```

Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vault-pull/src/journal.rs crates/vault-pull/src/lib.rs
git commit -m "feat(vault-pull): add PullJournal for resume-safe asset tracking

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: vault-pull config additions

**Files:**
- Modify: `crates/vault-pull/src/run.rs`

**Interfaces:**
- Consumes: PullJournalState from Task 1
- Produces: `VaultPullConfig` gets new fields: `asset_download_workers: usize`, `force: bool`, `journal_path: Option<PathBuf>`

- [ ] **Step 1: Add new fields to VaultPullConfig**

In `crates/vault-pull/src/run.rs`, add to the `VaultPullConfig` struct (after the `cancel` field):

```rust
#[derive(Debug, Clone)]
pub struct VaultPullConfig {
    pub out_dir: PathBuf,
    pub base_url: String,
    pub username: String,
    pub key: String,
    pub query: String,
    pub after: Option<String>,
    pub before: Option<String>,
    pub source: Option<String>,
    pub skip_attachments: bool,
    pub page_limit: usize,
    pub expected_messages: Option<u64>,
    pub cancel: Option<CancelFlag>,
    // --- new fields ---
    /// Number of parallel asset download workers (default 8).
    pub asset_download_workers: usize,
    /// Ignore the journal and re-download everything.
    pub force: bool,
    /// Path to the pull journal file. Defaults to out_dir/.vault-pull-state.jsonl.
    pub journal_path: Option<PathBuf>,
}
```

- [ ] **Step 2: Add default constants at the top of run.rs**

After the `DEFAULT_PAGE_LIMIT` constant:

```rust
pub const DEFAULT_PAGE_LIMIT: usize = 100;
/// Default number of parallel asset download workers.
pub const DEFAULT_ASSET_DOWNLOAD_WORKERS: usize = 8;
```

- [ ] **Step 3: Build to verify compilation**

```bash
cargo build -p vault-pull 2>&1 | head -20
```

Expected: either compiles clean or errors are only in code that constructs `VaultPullConfig` without the new fields (fix those next).

- [ ] **Step 4: Fix call sites that construct VaultPullConfig**

The CLI (`src/bin/vault_pull.rs`) and GUI (`start.rs`) construct `VaultPullConfig`. Add the new fields with defaults:

In `crates/vault-pull/src/bin/vault_pull.rs`, add to the struct literal:
```rust
asset_download_workers: DEFAULT_ASSET_DOWNLOAD_WORKERS,
force: false,
journal_path: None,
```

In `crates/message-vault-io-gui/src/start.rs`, add to both the `query_stats` and `start_vault_export` struct literals:
```rust
asset_download_workers: vault_pull::DEFAULT_ASSET_DOWNLOAD_WORKERS,
force: false,
journal_path: None,
```

- [ ] **Step 5: Build entire workspace**

```bash
cargo build --workspace 2>&1 | tail -5
```

Expected: clean build.

- [ ] **Step 6: Commit**

```bash
git add crates/vault-pull/src/run.rs crates/vault-pull/src/bin/vault_pull.rs crates/message-vault-io-gui/src/start.rs
git commit -m "feat(vault-pull): add asset_download_workers, force, journal_path to VaultPullConfig

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: vault-pull HTTP connection pool bump

**Files:**
- Modify: `crates/vault-pull/src/http.rs`

**Interfaces:**
- Consumes: nothing
- Produces: connection pool raised from 8 to 16

- [ ] **Step 1: Bump connection pool size**

In `crates/vault-pull/src/http.rs`, line 129, change:

```rust
// Before:
.pool_max_idle_per_host(8)

// After:
.pool_max_idle_per_host(16)
```

- [ ] **Step 2: Commit**

```bash
git add crates/vault-pull/src/http.rs
git commit -m "perf(vault-pull): bump HTTP connection pool to 16 (match vault-push)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: vault-pull parallel asset downloads

**Files:**
- Modify: `crates/vault-pull/src/run.rs`

**Interfaces:**
- Consumes: VaultPullConfig from Task 2 (uses `asset_download_workers`, `skip_attachments`)
- Produces: `download_assets_parallel()` function, replaces sequential download loop in `run()`

This is the core performance change. The current sequential download loop in `run()` (lines 399-423) is replaced with a work-stealing parallel download function that mirrors push's `upload_assets` pattern.

- [ ] **Step 1: Write `download_assets_parallel` function**

Add after the existing `run()` function in `crates/vault-pull/src/run.rs`:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

struct AssetDownloadJob {
    sha256: String,
    source: String,
    dest: PathBuf,
}

#[derive(Default)]
struct AssetDownloadStats {
    bytes: u64,
    downloaded: u64,
    skipped: u64,
}

/// Download unique assets in parallel using work-stealing workers.
///
/// Mirrors vault-push's `upload_assets` pattern: jobs are collected, then
/// `asset_download_workers` threads pull from a shared `AtomicUsize` counter.
/// Assets already on disk are skipped (counted as `skipped`).
fn download_assets_parallel(
    session: &crate::http::HttpSession,
    base_url: &str,
    key: &str,
    account: &str,
    assets: &HashMap<String, (String, String)>, // sha256 -> (source, rel_path)
    out_dir: &Path,
    workers: usize,
    cancel: Option<&CancelFlag>,
) -> Result<AssetDownloadStats> {
    let mut jobs: Vec<AssetDownloadJob> = Vec::with_capacity(assets.len());
    let mut stats = AssetDownloadStats::default();

    for (sha256, (source, rel)) in assets {
        let dest = out_dir.join(rel);
        if dest.is_file() {
            let meta = fs::metadata(&dest)
                .with_context(|| format!("stat {}", dest.display()))?;
            stats.bytes = stats.bytes.saturating_add(meta.len());
            stats.skipped += 1;
            continue;
        }
        jobs.push(AssetDownloadJob {
            sha256: sha256.clone(),
            source: source.clone(),
            dest,
        });
    }

    if jobs.is_empty() {
        return Ok(stats);
    }

    let worker_count = workers.max(1).min(jobs.len());
    let next_job = AtomicUsize::new(0);
    let results = Mutex::new(
        std::iter::repeat_with(|| None)
            .take(jobs.len())
            .collect::<Vec<Option<Result<u64, String>>>>(),
    );

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    check_cancel(cancel).ok(); // best-effort cancel check
                    let index = next_job.fetch_add(1, Ordering::Relaxed);
                    if index >= jobs.len() {
                        break;
                    }
                    let job = &jobs[index];
                    let result = (|| -> Result<u64> {
                        session.download_asset(
                            base_url,
                            key,
                            account,
                            &job.source,
                            &job.sha256,
                            &job.dest,
                        )?;
                        let meta = fs::metadata(&job.dest)
                            .with_context(|| format!("stat after download {}", job.dest.display()))?;
                        Ok(meta.len())
                    })()
                    .map_err(|e| e.to_string());
                    results.lock().expect("asset result mutex poisoned")[index] = Some(result);
                }
            });
        }
    });

    let mut results = results.into_inner().expect("asset result mutex poisoned");
    for result in results.drain(..) {
        match result.expect("every asset job has a result") {
            Ok(bytes) => {
                stats.bytes = stats.bytes.saturating_add(bytes);
                stats.downloaded += 1;
            }
            Err(error) => {
                bail!("asset download failed: {error}");
            }
        }
    }
    Ok(stats)
}
```

- [ ] **Step 2: Replace sequential download loop with parallel function**

In `crates/vault-pull/src/run.rs`, in the `run()` function, replace lines ~395-423 (the sequential download loop) with:

```rust
    let mut attachments_downloaded = 0u64;
    let mut attachments_skipped = 0u64;

    if !cfg.skip_attachments && !assets.is_empty() {
        emit(
            &mut on_progress,
            ProgressEvent::Log(format!(
                "Downloading {} unique asset(s) with {} worker(s)…",
                assets.len(),
                cfg.asset_download_workers
            )),
        );
        let dl_stats = download_assets_parallel(
            &session,
            &cfg.base_url,
            &cfg.key,
            &account,
            &assets,
            &cfg.out_dir,
            cfg.asset_download_workers,
            cfg.cancel.as_ref(),
        )?;
        attachments_downloaded = dl_stats.downloaded;
        attachments_skipped = dl_stats.skipped;
        emit(
            &mut on_progress,
            ProgressEvent::Log(format!(
                "Assets: {} downloaded, {} skipped ({} total bytes)",
                attachments_downloaded,
                attachments_skipped,
                format_bytes_human(dl_stats.bytes)
            )),
        );
    }
```

Add the `format_bytes_human` helper (same as in `start.rs`):

```rust
fn format_bytes_human(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}
```

- [ ] **Step 3: Build and test**

```bash
cargo build -p vault-pull --lib
cargo test -p vault-pull
```

Expected: existing tests pass, library compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/vault-pull/src/run.rs
git commit -m "feat(vault-pull): parallel asset downloads with work-stealing workers

Mirrors vault-push's upload_assets pattern. Assets already on disk are skipped.
Default 8 workers, configurable via VaultPullConfig.asset_download_workers.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: vault-pull serialization during fetch + resume integration

**Files:**
- Modify: `crates/vault-pull/src/run.rs`

**Interfaces:**
- Consumes: `PullJournalState`/`PullJournalEvent` from Task 1, `VaultPullConfig` from Task 2, `download_assets_parallel` from Task 4
- Produces: `run()` buffers messages during fetch, writes after, checks journal for resume

This combines serialization overlap and resume logic since they touch the same loop.

- [ ] **Step 1: Rewrite the `run()` function**

Replace the current `run()` function body with the new flow. The key changes:

1. Load journal at start (skip if `force`)
2. Page loop: serialize each `ExportMessage` → `IrMessage` into per-conversation buffers as pages arrive (not at the end)
3. Asset collection happens during paging (unchanged)
4. After paging: download assets in parallel (Task 4)
5. Write conversation files from buffers
6. Append `asset_ok` events to journal after each successful download
7. Append `backup_complete` after a clean run
8. Compact journal after clean run

```rust
pub fn run(cfg: &VaultPullConfig, mut on_progress: Option<&mut ProgressFn<'_>>) -> Result<PullReport> {
    let emit = |on_progress: &mut Option<&mut ProgressFn<'_>>, event: ProgressEvent| {
        if let Some(cb) = on_progress.as_mut() {
            cb(event);
        }
    };

    if cfg.key.trim().is_empty() {
        bail!("vault key is required");
    }
    if cfg.out_dir.as_os_str().is_empty() {
        bail!("output directory is required");
    }

    let auth = authenticate(&cfg.base_url, &cfg.key, &cfg.username)
        .map_err(|e| anyhow::anyhow!("{}", e.detail()))?;
    let account = auth.account_id.clone();
    let username = auth
        .username
        .clone()
        .unwrap_or_else(|| account.clone());
    emit(
        &mut on_progress,
        ProgressEvent::Auth {
            account_id: account.clone(),
            username: username.clone(),
        },
    );
    emit(
        &mut on_progress,
        ProgressEvent::Log(format!("Authenticated as {username} ({account})")),
    );

    let q = compose_query(
        &cfg.query,
        cfg.after.as_deref(),
        cfg.before.as_deref(),
    );
    emit(
        &mut on_progress,
        ProgressEvent::Log(if q.is_empty() {
            "Backup query: (all messages)".into()
        } else {
            format!("Backup query: {q}")
        }),
    );

    fs::create_dir_all(&cfg.out_dir)
        .with_context(|| format!("create {}", cfg.out_dir.display()))?;
    let attachments_dir = cfg.out_dir.join("attachments");
    if !cfg.skip_attachments {
        fs::create_dir_all(&attachments_dir)?;
    }

    // --- resume journal ---
    let journal_path = cfg
        .journal_path
        .clone()
        .unwrap_or_else(|| crate::journal::journal_path(&cfg.out_dir));
    let journal_state = if cfg.force {
        crate::journal::PullJournalState::default()
    } else {
        crate::journal::load(&journal_path, &cfg.base_url, &username)?
    };

    if journal_state.backup_complete && !cfg.force {
        emit(
            &mut on_progress,
            ProgressEvent::Log(
                "Previous backup completed successfully. Running to check for new messages…".into(),
            ),
        );
    }

    let session = HttpSession::new()?;
    let mut cursor: Option<String> = None;
    let mut total_messages = 0u64;

    // Per-conversation buffers: built during paging, written after.
    // BTreeMap<source::chat_identifier, (seed ExportMessage, Vec<IrMessage>)>
    let mut by_conv: BTreeMap<String, (ExportMessage, Vec<message_ir::IrMessage>)> =
        BTreeMap::new();
    // sha256 -> (source, relative path under out_dir)
    let mut assets: HashMap<String, (String, String)> = HashMap::new();

    // --- page loop with in-loop serialization ---
    loop {
        check_cancel(cfg.cancel.as_ref()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let page = session.export_messages(
            &cfg.base_url,
            &cfg.key,
            &q,
            cfg.page_limit.max(1),
            cursor.as_deref(),
            &account,
            cfg.source.as_deref(),
        )?;
        total_messages += page.messages.len() as u64;
        emit(
            &mut on_progress,
            ProgressEvent::Page {
                messages: page.messages.len(),
                total_so_far: total_messages,
            },
        );
        let page_log = match cfg.expected_messages {
            Some(n) => format!(
                "Fetched {} message(s) ({} of {})",
                page.messages.len(),
                total_messages,
                n
            ),
            None => format!(
                "Fetched {} message(s) ({} total)",
                page.messages.len(),
                total_messages
            ),
        };
        emit(&mut on_progress, ProgressEvent::Log(page_log));

        for msg in page.messages {
            if !cfg.skip_attachments {
                for att in &msg.attachments {
                    if let Some(sha) = att
                        .sha256
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        let rel = att
                            .path
                            .as_deref()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(|p| p.trim_start_matches('/').to_string())
                            .unwrap_or_else(|| format!("attachments/{sha}"));
                        assets
                            .entry(sha.to_string())
                            .or_insert_with(|| (msg.source.clone(), rel));
                    }
                }
            }
            // Serialize during fetch (was previously done after all pages)
            let key = conversation_key(&msg);
            let ir = to_ir_message(&msg, cfg.skip_attachments)?;
            let entry = by_conv.entry(key).or_insert_with(|| (msg.clone(), Vec::new()));
            entry.1.push(ir);
        }

        match page.next_cursor {
            Some(next) if !next.is_empty() => cursor = Some(next),
            _ => break,
        }
    }

    // --- parallel asset downloads (skip assets already in journal + on disk) ---
    let mut attachments_downloaded = 0u64;
    let mut attachments_skipped = 0u64;

    if !cfg.skip_attachments {
        // Filter out assets already in the journal that exist on disk.
        let total_assets = assets.len() as u64;
        let to_download: HashMap<String, (String, String)> = assets
            .into_iter()
            .filter(|(sha, (_source, rel))| {
                if journal_state.assets.contains(sha) {
                    let dest = cfg.out_dir.join(rel);
                    if dest.is_file() {
                        return false; // skip: journaled + on disk
                    }
                }
                true
            })
            .collect();
        let skipped_by_journal = total_assets - to_download.len() as u64;
        let assets = to_download;

        if !assets.is_empty() {
            emit(
                &mut on_progress,
                ProgressEvent::Log(format!(
                    "Downloading {} unique asset(s) with {} worker(s) ({} skipped from journal)…",
                    assets.len(),
                    cfg.asset_download_workers,
                    skipped_by_journal
                )),
            );
            let dl_stats = download_assets_parallel(
                &session,
                &cfg.base_url,
                &cfg.key,
                &account,
                &assets,
                &cfg.out_dir,
                cfg.asset_download_workers,
                cfg.cancel.as_ref(),
            )?;
            attachments_downloaded = dl_stats.downloaded;
            attachments_skipped = dl_stats.skipped + skipped_by_journal;

            // Journal each successful asset download
            for (sha, (_source, _rel)) in &assets {
                if !journal_state.assets.contains(sha) {
                    let event = crate::journal::PullJournalEvent::AssetOk {
                        url: cfg.base_url.clone(),
                        username: username.clone(),
                        sha256: sha.clone(),
                        path: String::new(),
                        size_bytes: 0,
                    };
                    let _ = crate::journal::append(&journal_path, &event);
                }
            }

            emit(
                &mut on_progress,
                ProgressEvent::Log(format!(
                    "Assets: {} downloaded, {} skipped",
                    attachments_downloaded,
                    attachments_skipped
                )),
            );
        } else {
            attachments_skipped = skipped_by_journal;
        }
    }

    // --- write conversation files from buffers ---
    let mut conversations = 0u64;
    for (_key, (seed, messages)) in by_conv {
        let source = seed.source.clone();
        let doc = build_document(&source, &seed, messages);
        write_conversation_jsonl(&cfg.out_dir, &doc)?;
        conversations += 1;
    }

    // --- journal completion ---
    let event = crate::journal::PullJournalEvent::BackupComplete {
        url: cfg.base_url.clone(),
        username: username.clone(),
        conversations,
        messages: total_messages,
        assets: attachments_downloaded + attachments_skipped,
    };
    crate::journal::append(&journal_path, &event)?;
    // Compact after clean run
    let final_state = crate::journal::PullJournalState {
        assets: {
            let mut s = journal_state.assets.clone();
            s.extend(assets.keys().cloned());
            s
        },
        backup_complete: true,
    };
    let _ = crate::journal::compact(&journal_path, &cfg.base_url, &username, &final_state);

    let report = PullReport {
        ok: true,
        account,
        query: q,
        conversations,
        messages: total_messages,
        attachments_downloaded,
        attachments_skipped,
        out_dir: cfg.out_dir.display().to_string(),
    };
    emit(
        &mut on_progress,
        ProgressEvent::Log(format!(
            "Wrote {} conversation(s), {} message(s) → {}",
            report.conversations, report.messages, report.out_dir
        )),
    );
    emit(&mut on_progress, ProgressEvent::Done(report.clone()));
    Ok(report)
}
```

- [ ] **Step 2: Build and run existing tests**

```bash
cargo build -p vault-pull --lib
cargo test -p vault-pull
```

Expected: compilation, existing tests pass. Fix any compilation errors (imports, ownership).

- [ ] **Step 3: Build workspace**

```bash
cargo build --workspace 2>&1 | tail -5
```

Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add crates/vault-pull/src/run.rs
git commit -m "feat(vault-pull): buffer serialization during fetch, journal resume

- Serialize ExportMessage->IrMessage during page fetch loop (not after)
- Filter already-downloaded assets via journal on resume
- Append asset_ok + backup_complete journal events
- Compact journal after clean run

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6: GUI Backup Account slint page

**Files:**
- Create: `crates/message-vault-io-gui/ui/pages/backup-account.slint`

**Interfaces:**
- Consumes: Theme from `theme.slint`
- Produces: `export global BackupAccountAdapter { ... }`

- [ ] **Step 1: Create the Backup Account page**

```slint
// crates/message-vault-io-gui/ui/pages/backup-account.slint
import { Theme } from "../theme.slint";
import { FormRow, PanelTitle } from "../widgets.slint";

export global BackupAccountAdapter {
    in property <string> url;
    in property <string> key;
    in property <string> output;
    in property <bool> force;
    in property <bool> enabled: true;
    callback browse(string);
    callback run();
}

export component BackupAccountPage inherits VerticalLayout {
    padding: 24px;
    spacing: 0;
    vertical-stretch: 1;
    alignment: start;

    PanelTitle {
        title: "Backup Account";
        subtitle: "Download your entire message history from Message Vault.";
    }

    Rectangle { height: 16px; }

    FormRow {
        label: "Vault URL";
        field-id: "backup.url";
    }
    FormRow {
        label: "API Key";
        field-id: "backup.key";
        password: true;
    }
    FormRow {
        label: "Output Directory";
        field-id: "backup.output";
    }

    Rectangle { height: 8px; }

    HorizontalLayout {
        spacing: 12px;
        alignment: center;
        vertical-stretch: 0;

        CheckBox {
            text: "Force full backup (ignore previous state)";
            checked <=> BackupAccountAdapter.force;
            enabled: BackupAccountAdapter.enabled;
        }
    }

    Rectangle { height: 16px; }

    HorizontalLayout {
        spacing: 12px;
        alignment: start;
        vertical-stretch: 0;

        Button {
            text: "Backup Account";
            enabled: BackupAccountAdapter.enabled;
            clicked => { BackupAccountAdapter.run(); }
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/message-vault-io-gui/ui/pages/backup-account.slint
git commit -m "feat(gui): add Backup Account slint page

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 7: GUI Home card + app-window wiring

**Files:**
- Modify: `crates/message-vault-io-gui/ui/pages/home.slint`
- Modify: `crates/message-vault-io-gui/ui/app-window.slint`

**Interfaces:**
- Consumes: BackupAccountAdapter from Task 6
- Produces: Home card clickable, app-window routes to screen 4 (BACKUP)

- [ ] **Step 1: Add callback to HomeAdapter**

In `crates/message-vault-io-gui/ui/pages/home.slint`, add a new callback to `HomeAdapter`:

```slint
export global HomeAdapter {
    in property <bool> enabled: true;
    callback vault-import();
    callback convert-messages();
    callback backup-account();   // new
}
```

Add a new `HomeCard` after the Vault Import card:

```slint
HomeCard {
    title: "Backup Account";
    subtitle: "Download your entire message history from Message Vault.";
    glyph: "↓";
    enabled: HomeAdapter.enabled;
    clicked => { HomeAdapter.backup-account(); }
}
```

- [ ] **Step 2: Add backup-account screen to app-window**

In `crates/message-vault-io-gui/ui/app-window.slint`:

Add import at top:
```slint
import { BackupAccountPage, BackupAccountAdapter } from "pages/backup-account.slint";
```

Add export:
```slint
export { BackupAccountAdapter } from "pages/backup-account.slint";
```

Add screen routing (after the Export screen, as screen 4):
```slint
if root.workflow-screen == 4: BackupAccountPage { }
```

Update the `back-label` property to include screen 4:
```slint
property <string> back-label:
    root.workflow-screen == 2 || root.workflow-screen == 3 ? "Vault Credentials"
    : root.workflow-screen == 4 ? "Home"
    : root.workflow-screen == 1 ? "Home"
    : "";
```

- [ ] **Step 3: Build GUI**

```bash
cargo build -p message-vault-io-gui 2>&1 | tail -10
```

Expected: Slint compilation errors about missing callbacks in wire.rs (fix in Task 8), but the `.slint` files should compile.

- [ ] **Step 4: Commit**

```bash
git add crates/message-vault-io-gui/ui/pages/home.slint crates/message-vault-io-gui/ui/app-window.slint
git commit -m "feat(gui): add Backup Account card to Home, route to screen 4

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 8: GUI state, sync, wire, start

**Files:**
- Modify: `crates/message-vault-io-gui/src/state.rs`
- Modify: `crates/message-vault-io-gui/src/sync.rs`
- Modify: `crates/message-vault-io-gui/src/wire.rs`
- Modify: `crates/message-vault-io-gui/src/start.rs`

**Interfaces:**
- Consumes: BackupAccountAdapter from Task 6/7, VaultPullConfig from Task 2
- Produces: Full GUI wiring for Backup Account flow

- [ ] **Step 1: Add screen constant and state fields**

In `crates/message-vault-io-gui/src/state.rs`, add to `screen` module:

```rust
pub const BACKUP: i32 = 4;
```

Add to `AppState`:
```rust
/// Output directory for account backup (persisted).
pub backup_output: String,
```

In `AppState::load()`, initialize:
```rust
backup_output: String::new(),
```

- [ ] **Step 2: Add sync functions**

In `crates/message-vault-io-gui/src/sync.rs`, add:

```rust
use crate::BackupAccountAdapter;

pub fn pull_backup_account(ui: &AppWindow, state: &mut AppState) {
    let adapter = ui.global::<BackupAccountAdapter>();
    state.export_ini.vault.url = adapter.get_url().trim().to_string();
    state.export_ini.vault.key = adapter.get_key().trim().to_string();
    state.backup_output = adapter.get_output().trim().to_string();
}

pub fn push_backup_account(ui: &AppWindow, state: &AppState) {
    let adapter = ui.global::<BackupAccountAdapter>();
    adapter.set_url(state.export_ini.vault.url.clone().into());
    adapter.set_key(state.export_ini.vault.key.clone().into());
    adapter.set_output(state.backup_output.clone().into());
}
```

- [ ] **Step 3: Add wire callbacks**

In `crates/message-vault-io-gui/src/wire.rs`:

Add import:
```rust
use crate::BackupAccountAdapter;
```

In `wire_all()`, add:
```rust
wire_backup_account(ui, Arc::clone(&state));
```

Add the wiring function:
```rust
fn wire_backup_account(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();

    ui.global::<BackupAccountAdapter>().on_browse({
        let ui_weak = ui_weak.clone();
        move |field_id| {
            let kind = browse::browse_kind_for_field(&field_id);
            browse::pick_path(ui_weak.clone(), field_id.to_string(), kind);
        }
    });

    ui.global::<BackupAccountAdapter>().on_run({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_account_backup(&ui_weak, &state)
    });
}
```

In `wire_home()`, add the backup-account callback:
```rust
ui.global::<HomeAdapter>().on_backup_account({
    let ui_weak = ui_weak.clone();
    move || {
        if let Some(ui) = ui_weak.upgrade() {
            let st = state.lock().expect("state lock");
            sync::push_backup_account(&ui, &st);
            ui.set_workflow_screen(state::screen::BACKUP);
            sync::push_chrome(&ui, &st);
        }
    }
});
```

In `wire_navigate_back()`, add screen 4 routing:
```rust
let previous = match current {
    x if x == state::screen::IMPORT || x == state::screen::EXPORT => {
        state::screen::CREDENTIALS
    }
    x if x == state::screen::BACKUP => state::screen::HOME,
    _ => current - 1,
};
```

- [ ] **Step 4: Add start_account_backup job spawner**

In `crates/message-vault-io-gui/src/start.rs`:

Add a new public function after `start_vault_export`:

```rust
pub(crate) fn start_account_backup(
    ui_weak: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_backup_account(&ui, &mut st);
        let adapter = ui.global::<BackupAccountAdapter>();
        let url = st.export_ini.vault.url.trim().to_string();
        let key = st.export_ini.vault.key.trim().to_string();
        let output_raw = adapter.get_output().trim().to_string();
        let force = adapter.get_force();

        let mut errors = Vec::new();
        if url.is_empty() {
            errors.push("Vault URL is required.".into());
        }
        if key.is_empty() {
            errors.push("API Key is required.".into());
        }
        if !errors.is_empty() {
            report_errors(&ui, &mut st, errors);
            return;
        }

        let out_dir = if output_raw.is_empty() {
            staging::export_dir_path(
                &staging::default_export_parent(),
                "vault-backup",
                Local::now(),
            )
        } else {
            PathBuf::from(&output_raw)
        };

        if output_raw.is_empty() {
            adapter.set_output(out_dir.display().to_string().into());
        }

        if let Err(error) = st.save_export_ini() {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }

        let label = "vault account backup (library)".to_string();
        let job: LibraryJob = Box::new(move |cancel, tx| {
            let _ = tx.send(ProcessEvent::Log(format!(
                "Backing up to {}",
                out_dir.display()
            )));
            let cfg = VaultPullConfig {
                out_dir,
                base_url: url,
                username: String::new(),
                key,
                query: String::new(), // empty = all messages
                after: None,
                before: None,
                source: None,
                skip_attachments: false,
                page_limit: vault_pull::DEFAULT_PAGE_LIMIT,
                expected_messages: None,
                cancel: Some(cancel),
                asset_download_workers: vault_pull::DEFAULT_ASSET_DOWNLOAD_WORKERS,
                force,
                journal_path: None, // default: out_dir/.vault-pull-state.jsonl
            };
            let expected_messages = Arc::new(AtomicU64::new(0));
            let mut on_progress = |event: VaultPullProgressEvent| match event {
                VaultPullProgressEvent::Log(line) => {
                    let _ = tx.send(ProcessEvent::Log(line));
                }
                VaultPullProgressEvent::Auth {
                    account_id,
                    username,
                } => {
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Authenticated as {username} ({account_id})"
                    )));
                }
                VaultPullProgressEvent::Page {
                    messages: _,
                    total_so_far,
                } => {
                    expected_messages.store(total_so_far, Ordering::Relaxed);
                }
                VaultPullProgressEvent::Done(report) => {
                    let summary = format_backup_summary(&report);
                    let _ = tx.send(ProcessEvent::Log(summary));
                }
            };
            match run_vault_pull(&cfg, Some(&mut on_progress)) {
                Ok(report) if report.ok => Ok(()),
                Ok(_) => Err(JobError::detail("Backup finished with errors.")),
                Err(e) => Err(JobError::detail(format!("{e:#}"))),
            }
        });
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job, OnSuccess::None);
    }
}
```

Add the import at the top of `start.rs`:
```rust
use std::sync::atomic::AtomicU64;
use crate::BackupAccountAdapter;
```

Add a helper in `vault-push/src/run.rs` for formatting pull reports (or inline it):
```rust
// In start.rs, add a format helper for pull reports:
fn format_backup_summary(report: &vault_pull::PullReport) -> String {
    format!(
        "==== Backup Complete ====\n\
         Conversations: {}\n\
         Messages: {}\n\
         Attachments: {} downloaded, {} skipped\n\
         Output: {}",
        report.conversations,
        report.messages,
        report.attachments_downloaded,
        report.attachments_skipped,
        report.out_dir
    )
}
```

- [ ] **Step 5: Build workspace**

```bash
cargo build --workspace 2>&1 | tail -10
```

Expected: clean build after fixing import/type issues.

- [ ] **Step 6: Commit**

```bash
git add crates/message-vault-io-gui/src/state.rs crates/message-vault-io-gui/src/sync.rs crates/message-vault-io-gui/src/wire.rs crates/message-vault-io-gui/src/start.rs
git commit -m "feat(gui): wire Backup Account screen with one-click full backup

- Home card routes to screen 4 (Backup Account)
- start_account_backup builds VaultPullConfig with empty query, parallel workers
- Persists URL/key from vault section, output dir from backup_output
- Force checkbox gates journal bypass

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 9: export.ini backup section

**Files:**
- Modify: `crates/message-vault-io-core/src/export_ini.rs`
- Modify: `crates/message-vault-io-gui/src/state.rs`

**Interfaces:**
- Consumes: AppState.backup_output from Task 8
- Produces: `[backup]` section persisted to `export.ini`

- [ ] **Step 1: Add BackupSection to export_ini**

In `crates/message-vault-io-core/src/export_ini.rs`, add after the `VaultSection`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackupSection {
    #[serde(default)]
    pub output: String,
}
```

Add to `ExportIniState`:
```rust
#[serde(default)]
pub backup: BackupSection,
```

- [ ] **Step 2: Load/save backup_output in AppState**

In `crates/message-vault-io-gui/src/state.rs`, update `AppState::load()`:

```rust
backup_output: export_ini.backup.output.clone(),
```

In `AppState::save_export_ini()`, add before the save call:
```rust
self.export_ini.backup.output = self.backup_output.clone();
```

- [ ] **Step 3: Build workspace**

```bash
cargo build --workspace 2>&1 | tail -5
```

Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add crates/message-vault-io-core/src/export_ini.rs crates/message-vault-io-gui/src/state.rs
git commit -m "feat(core): add [backup] section to export.ini for output dir persistence

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 10: Build, test, and verify

**Files:**
- None (verification only)

- [ ] **Step 1: Run full test suite**

```bash
cargo test --workspace
```

Expected: all existing tests pass.

- [ ] **Step 2: Run vault-pull specific tests**

```bash
cargo test -p vault-pull
```

Expected: journal tests (Task 1) pass, existing project tests pass.

- [ ] **Step 3: Build release**

```bash
cargo build --workspace --release 2>&1 | tail -5
```

Expected: clean release build.

- [ ] **Step 4: Manual smoke test checklist**

1. Launch the GUI: `cargo run --release -p message-vault-io-gui`
2. On Home screen, verify "Backup Account" card is visible
3. Click it → verify Backup Account screen loads with URL, Key, Output fields
4. Enter vault URL and API key, pick output directory
5. Click "Backup Account" → verify progress in log panel
6. Cancel mid-backup → verify clean cancellation
7. Click "Backup Account" again → verify resume (assets skipped from journal)
8. Check "Force full backup" → verify all assets re-downloaded
9. Verify output directory has `.jsonl` files and `attachments/` directory

- [ ] **Step 5: Commit any remaining changes**

```bash
git status
git add -A
git commit -m "chore: final build verification and docs

Co-Authored-By: Claude <noreply@anthropic.com>"
```
