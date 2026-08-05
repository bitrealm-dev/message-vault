# Vault Account Backup — Design Spec

A fast, resumable full-account download from Message Vault into a local JSONL export folder, with a dedicated one-click GUI flow. Mirrors the parallelism and reliability patterns that vault-push already uses for upload.

## Motivation

Today vault-pull is single-threaded, serial, and has no resume support. A full account export means: page 100 messages at a time in a blocking loop, then download every attachment one at a time, then write files. If the process crashes or the network drops, everything is lost. The GUI also buries this behind a search form with a Query-then-Export two-step flow.

Push already solved these problems: parallel attachment workers, a prepare-ahead pipeline, a resume journal, and batched progress reporting. Pull should mirror that architecture since it moves the same data over the same network, just in the opposite direction.

## Goals

1. Pull throughput comparable to push throughput for the same dataset
2. Resume-safe: crash or cancel mid-pull, restart picks up where it left off
3. Dedicated one-click "Backup Account" GUI flow, separate from search/export

## Non-goals

- Changing the server-side API endpoints (same `/v1/export/messages`, `/v1/assets/{sha256}`, `/v1/export/messages/count`)
- Streaming or incremental sync (this is full-account backup, pull everything)
- Web-based export (this stays in the desktop app)

---

## 1. Pull journal and resume

### Journal file

`.vault-pull-state.jsonl` — append-only JSONL, written next to the output directory. Same format conventions as push's `.vault-import-state.jsonl`.

### Events

```jsonl
{"event":"asset_ok","url":"https://...","username":"alice","sha256":"abc...","path":"attachments/abc","size_bytes":12345}
{"event":"backup_complete","url":"https://...","username":"alice","conversations":89,"messages":12340,"assets":1203}
```

Two event types. `asset_ok` is written after each successful attachment download. `backup_complete` is written once at the end of a clean run. Together they tell the next run what can be skipped.

### Journal scoping

Journal entries are keyed by `url` + `username`. When loading a journal, entries for other URLs or accounts are ignored. A journal file sits next to the output directory.

### Resume logic

On resume, message pages are **always re-fetched in full**. Message JSON is small (a page of 100 messages is ~50-100 KB) and the HTTP cost of re-fetching is negligible compared to re-downloading attachments. This keeps the resume path simple: no temp files, no incremental writes, no partial state to reconcile.

**Assets** — the only thing tracked for resume. An asset is skipped if its SHA-256 is in the journal AND the file exists on disk at the expected path with matching size. No re-hashing (same trust-model as push: the journal is authoritative unless `force` is set). This is where resume actually saves time — attachment bytes are orders of magnitude larger than message JSON.

**Backup complete** — if the journal ends with a `backup_complete` event and the output directory still exists with all the conversation files, a subsequent run with no new server data will: re-fetch all pages (fast, no new messages), find zero assets to download (all in journal + on disk), and report "0 new messages, 0 new attachments — backup is current."

### `force` flag

When `force` is true, the journal is ignored entirely. All message pages are re-fetched, all assets re-downloaded, all conversation files re-written. Same semantics as push's force flag.

### Journal compact

After a clean run (0 failures, not cancelled), the journal is compacted: sorted events rewritten through a temp file + atomic rename. After a failed or cancelled run, the raw append-only journal is preserved so the next run can resume accurately. Same approach as push's `journal::compact`.

---

## 2. Parallel downloads

### Config additions to `VaultPullConfig`

```rust
pub asset_download_workers: usize,  // default 8
pub force: bool,                     // default false
pub journal_path: Option<PathBuf>,   // default out_dir/.vault-pull-state.jsonl
```

### Overlap paging with writes

Current flow is three sequential phases:

```
[page 1] → [page 2] → ... → [page N] → [download all assets] → [write all files]
```

New flow overlaps phase 1 with phase 3:

```
[page 1 → buffer + serialize] → [page 2 → buffer + serialize] → ...
    → [page N → buffer + serialize] → [download assets in parallel] → [write files]
```

As each page arrives, its messages are serialized to JSON lines and appended into per-conversation buffers in memory. A later page may add more messages to a conversation that already has messages from an earlier page — each conversation's buffer grows across the fetch loop. The serialization work (turning `ExportMessage` into `IrMessage` JSON) is spread across the loop instead of happening all at once after all pages are fetched.

After all pages are fetched, per-conversation JSONL files are written to the output directory (one `ConversationHeader` line + buffered message lines per file). Attachment downloads run in parallel during this finalization phase.

Message fetching stays sequential — each page depends on the previous page's `next_cursor`. But serialization overlaps with network wait time, and attachment downloads are parallelized.

### Parallel attachment downloads

Mirrors push's asset upload workers (`vault-push/src/run.rs:1853-1907`):

- All unique attachment SHA-256s are collected across all pages
- One `AssetDownloadJob { sha256, source, dest_path }` per unique SHA-256
- `asset_download_workers` threads (default 8) pull jobs from a shared `AtomicUsize` counter
- Each worker: check if `dest_path` already exists on disk → skip; otherwise `GET /v1/assets/{sha256}` → write to `attachments/` path
- Journal appends `asset_ok` after each successful download
- `AssetDownloadStats { bytes, downloaded, skipped }` aggregated and reported

### HTTP connection pool

Bump `pool_max_idle_per_host` from 8 to 16 in vault-pull's `HttpSession::new()` to match vault-push. More idle connections means less TCP handshake overhead during parallel downloads.

### What stays the same

- Cursor pagination at 100 messages per page (server-side default)
- Conversation grouping (`conversation_key`, `build_document`, `to_ir_message`)
- API endpoints (`/v1/export/messages`, `/v1/export/messages/count`, `/v1/assets/{sha256}`)
- Output format (JSONL with `ConversationHeader` + `IrMessage` lines)

---

## 3. Dedicated "Backup Account" GUI

### Entry point

A new card on the Home screen: **"Backup Account"**. Sits alongside the existing "Vault Import & Export" card. This is a separate, simpler flow — no credentials screen, no search form, no format selector, no Query-then-Export two-step.

### Screen: Backup Account

```
Backup Account
─────────────────────────────────────────
Vault URL    [________________________]
API Key      [________________________]
Output Dir   [________________________] [Browse]

[Backup Account]  [ ] Force full backup

── Log ─────────────────────────────────
Authenticated as alice (acct_abc123)
Fetching messages... page 42 (4,200 total)
Downloading attachments... 156/1,203 (8 workers)
files 10/89 - conversations=10 messages=1,420...
...

==== Summary ====
Backup complete
Conversations: 89
Messages: 12,340
Attachments: 1,203 downloaded, 45 skipped
Elapsed: 4m32s
Output: /home/user/vault-backups/
```

### Behavior

- **Backup Account button**: authenticates (`GET /v1/auth/check`), runs a quick count (`GET /v1/export/messages/count`), then starts the full pull with the new parallel + journal code
- **Force full backup checkbox**: sets `force: true`, ignores any existing journal
- **Cancel**: same `CancelFlag` as every other job. Journal is consistent up to the last completed page
- **Rerun**: pressing Backup Account again resumes from the journal. If no new messages exist on the server, the second run completes near-instantly
- **Progress**: batched log lines matching push's format (`files N/M - conversations=X messages=Y transfer size=Z.MB download time=... total time=...`)

### State persistence

URL and key are persisted to `export.ini` under the existing `[vault]` section (same credentials as import/export). Output directory is saved to `export.ini` under a new `[backup]` section. Second time the user opens this screen, everything is pre-filled.

### Existing Export screen

Unchanged. It remains for targeted exports with search queries and date filters. Backup Account is the "give me everything" path. Two separate use cases, two separate entry points.

---

## 4. Files changed

| File | Change |
|------|--------|
| `crates/vault-pull/src/run.rs` | Overlap paging with writes, parallel asset downloads, journal integration, force flag |
| `crates/vault-pull/src/http.rs` | Bump connection pool to 16 |
| `crates/vault-pull/src/journal.rs` | New file — append-only journal, load, compact (mirrors `vault-push/src/journal.rs`) |
| `crates/vault-pull/src/lib.rs` | Export new config fields |
| `crates/vault-push/src/journal.rs` | Minor: extract shared journal types if practical (both crates need `JournalEvent`, append/load/compact) |
| `crates/message-vault-io-gui/ui/pages/backup-account.slint` | New file — Backup Account screen |
| `crates/message-vault-io-gui/ui/pages/home.slint` | Add "Backup Account" card |
| `crates/message-vault-io-gui/ui/app-window.slint` | Add backup-account page, wire workflow-screen |
| `crates/message-vault-io-gui/src/start.rs` | Add `start_account_backup` job spawner |
| `crates/message-vault-io-gui/src/wire.rs` | Wire Backup Account callbacks |
| `crates/message-vault-io-gui/src/sync.rs` | Add backup adapter sync |
| `crates/message-vault-io-gui/src/state.rs` | Add backup config state (or reuse vault section) |
| `crates/message-vault-io-core/src/export_ini.rs` | Optional: `[backup]` section for output dir persistence |

## 5. Testing

- Unit tests for journal load/resume/compact (mirror `vault-push` journal tests)
- Unit tests for should-skip logic (page already journaled, asset already on disk, conversation already written)
- Integration test with `httpmock` for the parallel download path (verify concurrent GET requests to `/v1/assets/`, verify journal events written)
- Manual test: start a backup, cancel mid-way, resume — confirm messages/assets already pulled are skipped

## 6. Rollout

Backward compatible. No API changes. No format changes. The existing Vault Export screen and `vault-pull` CLI continue to work. The journal is additive — if no journal file exists, behavior is identical to today (full download, no resume). Users opt into the new Backup Account flow from the Home screen.
