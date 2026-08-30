# Import Session Phase 4 — Prepare Restructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the write phase into a queue of conversations drained by parallel writer threads, add the disk-headroom check, make `write` itself resumable ("re-parse, skip conversations already written"), and land the four remaining library fixes (decisions 42, 43, 46, plus the `persist_clone` temp-suffix fix of decision 34).

**Architecture:** Parse already finishes before anything is written (every exporter buffers documents and stages attachments in one flat pass), so the work is regrouping that flat pass into per-conversation units. A new `write_queue` module in `message-ir-format` owns the engine: a `ConversationUnit` (one document plus its attachment sources), a sequential drain for exporters whose loader cannot cross threads (encrypted iOS backups — `crabapple::Backup` holds a rusqlite `Connection`, which is not `Sync`), and a parallel drain (`std::thread::scope` over a `Mutex<VecDeque>`) for everyone else. Each unit writes its attachments first and its conversation file last (decision 25's invariant), skipping units whose conversation file already exists when resuming. Inline convert/compress leaves the JSONL path entirely: the engine stages originals and runs the existing `transcode_staged` pass afterwards (decisions 26/27), which also gives the CLI the resumable pass and progress logging. The desktop plumbs a `resume` flag through `extract`, and the web grows a `resume_write` decision kind gated on the phase-2 fingerprint.

**Tech Stack:** Rust (std threads, no new async), `fs2` for `available_space` (already used by the server crate), React 19 + TypeScript + Vitest for the web side.

**Spec:** `docs/superpowers/specs/2026-08-29-import-session-design.md` — decisions 23–34, 36 (parse/write rows), 38, 42, 43, 46. Phases 1–3 delivered decisions 1–22, 26–31 (desktop pass), 35–37, 39–41, 44, 45.

## Global Constraints

- **Writers do not transcode** (decision 26). The queue engine stages originals only; Convert/Compress becomes a post-pass via `transcode_staged`.
- **The queue unit is the conversation** (decision 25): a worker writes a conversation's attachments, then its conversation file last. A conversation file on disk means everything it references is on disk.
- **Progress within a stage is recomputed, never stored** (decision 4). Write-resume enumerates `*.jsonl` files; no progress record.
- **Stage names are internal** (decision 7); "transcode" and "Gate 1" never appear in user-facing copy. Product copy states what happens; it never warns or hedges.
- **Log-line contract:** the desktop scrapes progress from log lines (`src-tauri/src/commands/progress.rs`). The strings `Preparing {N} conversation file(s)...` and `  attachments {done}/{total} {bytes_done}/{bytes_total}` must keep their exact shapes. Do not interleave `  preparing {n}/{N}` count lines with attachment lines (the scraper would misfile them).
- **Version lockstep files untouched** (`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `web/package.json`, `crates/vault/server/Cargo.toml` stay at 0.8.3). No `v*` tags. No server/API changes in this phase — no OpenAPI regeneration needed.
- Tests use committed fixtures under `tests/fixtures/`; never real message data.
- `src-tauri` builds with `--manifest-path src-tauri/Cargo.toml` (not a workspace member).
- Biome: prefix unused bindings with `_`; prefer real fixes over `biome-ignore`.
- Workspace MSRV constraints: some exporter crates are MSRV 1.85 (`%` instead of `u64::is_multiple_of` — see existing comments); match the local style.
- `message-vault-io-core` avoids `anyhow` (String errors); `message-ir-format` uses `anyhow`.

## Sequencing note

Tasks 1–5 are library groundwork and are independent of each other except where Interfaces say otherwise. Tasks 6–8 migrate exporters onto the engine and depend on Tasks 2–5. Tasks 9–10 are the desktop/web resume plumbing and depend on Task 5's config flag. Task 11 closes with docs and a full verification pass.

---

### Task 1: File-list media pass and logged convert (decisions 46 + 43)

**Files:**
- Modify: `crates/libs/media/src/process.rs` (entry points ~lines 43–131, `collect_media_files` ~line 190)
- Modify: `crates/libs/media/src/lib.rs` (exports, lines 24–27)
- Modify: `crates/core/message-vault-io-core/src/attachment_jobs.rs` (`run_attachment_jobs` line 47, `apply_convert_or_compress` line 162)
- Modify (call sites of `run_attachment_jobs`, each gains one `log` argument): `crates/exporters/imessage-ir-exporter/src/emit.rs:290`, `crates/exporters/whatsapp-exporter/src/emit.rs:366`, `crates/libs/ir-format/src/read_sbr.rs:196`, `crates/exporters/go-sms-pro-exporter/src/attachments_emit.rs:110`, `crates/exporters/sms-backup-plus-exporter/src/attachments_emit.rs:115`, `crates/exporters/imazing-exporter/src/attachments.rs:266`, `crates/libs/reexport/src/lib.rs` (its `run_attachment_jobs` / convert call sites), plus any test callers the compiler finds
- Test: existing `#[cfg(test)]` modules in `process.rs` and `attachment_jobs.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `media::collect_media_files(attachments_dir: &Path) -> Result<Vec<PathBuf>>` (now `pub`)
  - `media::process_attachment_files(output_dir: &Path, files: &[PathBuf], mode: MediaMode, compress: &CompressOptions, log: Option<&mut dyn FnMut(&str)>) -> Result<(MediaReport, HashMap<String, String>)>` — replaces `process_attachments_dir` and `process_attachments_dir_with_log`, which are deleted.
  - `message_vault_io_core::run_attachment_jobs(jobs, attachments_dir, mode, compress, load, on_progress, log: Option<&LogSink>, cancel)` — new `log` parameter, second-to-last.

**Steps:**

- [ ] **Step 1: Write the failing tests**

In `crates/libs/media/src/process.rs` tests, adapt the existing entry-point tests to the new name and add a scoping test:

```rust
#[test]
fn process_attachment_files_touches_only_the_listed_files() {
    // Build a temp output dir with attachments/a.png and attachments/b.png
    // (valid fixture bytes, as existing convert tests do), then call
    // process_attachment_files with only a.png in the list under
    // MediaMode::Convert. Assert the remap contains a key for a.png and no
    // key for b.png, and that b.png is still on disk unmodified.
}
```

Use the same fixture/ffmpeg gating the existing convert tests in this file use (follow `clone_with_log_emits_nothing` at process.rs:1199 and the convert tests near process.rs:958 for setup). If those tests skip when ffmpeg is absent, this one skips the same way.

In `crates/core/message-vault-io-core/src/attachment_jobs.rs` tests:

```rust
#[test]
fn convert_mode_emits_progress_through_the_log_sink() {
    // Run run_attachment_jobs with MediaMode::Clone and a LogSink that
    // pushes lines into a Mutex<Vec<String>>; assert no lines are emitted
    // (clone has no media pass). This pins that the new parameter is wired
    // without requiring ffmpeg in core's tests.
    let lines = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
    let sink_lines = std::sync::Arc::clone(&lines);
    let sink = LogSink::new(move |l: &str| sink_lines.lock().unwrap().push(l.to_string()));
    // ... existing clone_writes_file_and_fills_hash setup, passing Some(&sink) ...
    assert!(lines.lock().unwrap().is_empty());
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p media` and `cargo test -p message-vault-io-core`
Expected: compile failures (new names/parameters do not exist yet).

- [ ] **Step 3: Implement the media crate change**

In `process.rs`:
- Make `collect_media_files` `pub` with a doc comment: it lists the files a media pass would touch — every non-temp file `classify` recognizes, sorted, recursive.
- Replace both entry points with one:

```rust
/// Convert or compress the given attachment files in place.
///
/// The caller builds `files` (usually via [`collect_media_files`]), so a
/// resumed or scoped pass can name exactly the files it means instead of
/// sweeping the whole directory (spec decision 46). Paths must live under
/// `output_dir`'s `attachments/` directory.
///
/// # Errors
///
/// Returns an error when ffmpeg/ffprobe are missing or fail, an input path
/// escapes the output directory, or IO fails.
pub fn process_attachment_files(
    output_dir: &Path,
    files: &[PathBuf],
    mode: MediaMode,
    compress: &CompressOptions,
    mut log: Option<&mut dyn FnMut(&str)>,
) -> Result<(MediaReport, HashMap<String, String>)> {
    if matches!(mode, MediaMode::Clone | MediaMode::Disabled) {
        return Ok((MediaReport::default(), HashMap::new()));
    }
    require_ffmpeg()?;

    let attachments = output_dir.join("attachments");
    if !attachments.is_dir() {
        return Ok((MediaReport::default(), HashMap::new()));
    }

    // Leftovers from a previous failed ffmpeg run.
    remove_msgmedia_temps(&attachments)?;

    let mut report = MediaReport::default();
    let mut remap = HashMap::new();
    let total = files.len();
    if total == 0 {
        return Ok((report, remap));
    }
    // ... body identical to the old process_attachments_dir_with_log from
    // `report.bytes_before = ...` on, iterating `files` instead of a fresh
    // collect_media_files result ...
}
```

The loop iterates `for path in files` (borrowing, `process_one(output_dir, path, ...)`). Everything else (byte totals, progress every 100, final sweep, summary line) stays verbatim.
- Update `lib.rs` exports: export `process_attachment_files` and `collect_media_files`; drop the two old names.
- Fix the in-crate tests that called the old names: they now call `collect_media_files(&attachments)` then `process_attachment_files(...)`.

- [ ] **Step 4: Implement the core change**

In `attachment_jobs.rs`:
- Add `log: Option<&LogSink>` to `run_attachment_jobs` between `on_progress` and `cancel`, and pass it through to `apply_convert_or_compress`.
- Rewrite `apply_convert_or_compress`:

```rust
fn apply_convert_or_compress(
    jobs: &mut [AttachmentJob<'_>],
    attachments_dir: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
    log: Option<&LogSink>,
) -> Result<(), String> {
    let Some(output_dir) = attachments_dir.parent() else {
        return Err("attachments directory has no parent".into());
    };
    let files = media::collect_media_files(attachments_dir).map_err(|e| e.to_string())?;
    let mut emit = |line: &str| crate::emit_log(log, line);
    let (report, remap) =
        media::process_attachment_files(output_dir, &files, mode, compress, Some(&mut emit))
            .map_err(|e| e.to_string())?;
    apply_remap_to_jobs(jobs, &remap, output_dir);
    for err in &report.errors {
        mark_convert_error(jobs, err);
    }
    Ok(())
}
```

(Adjust the `emit_log` path to however the module already imports it; `LogSink`/`emit_log` live in `crate::process`.)
- Update every `run_attachment_jobs` call site to pass a log argument. Each site already has one in scope: iMessage passes `session.options.log.as_ref()` (`MailOptions.log` at `options.rs:64`); whatsapp/go-sms-pro/sms-backup-plus/imazing pass their `log` parameter; `read_sbr.rs` passes `options.log`; reexport passes whatever `LogSink` its convert path holds (follow the compiler). Update the in-crate tests (pass `None`).

- [ ] **Step 5: Run the tests and the workspace build**

Run: `cargo test -p media && cargo test -p message-vault-io-core && cargo build --workspace`
Expected: PASS, no remaining references to the old names.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(media): media pass takes an explicit file list and logs progress through the shared sink"
```

---

### Task 2: `persist_clone` unique temp suffix (decision 34)

**Files:**
- Modify: `crates/core/message-vault-io-core/src/attachment_jobs.rs:142-160`
- Test: same file's test module

**Interfaces:**
- Consumes/produces: none outside the file. This unblocks Task 3's parallel workers: today two workers staging identical bytes would both write `{name}.tmp` and race the rename.

**Steps:**

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn clone_temp_paths_are_unique_per_call() {
    // Two sequential persist_clone calls for identical bytes must not
    // reuse the same temp path. Pin it via the counter: call
    // next_clone_temp_name("x.jpg") twice and assert inequality.
    let a = next_clone_temp_name("x.jpg");
    let b = next_clone_temp_name("x.jpg");
    assert_ne!(a, b);
    assert!(a.starts_with("x.jpg."));
    assert!(a.ends_with(".tmp"));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p message-vault-io-core clone_temp_paths_are_unique_per_call`
Expected: FAIL — `next_clone_temp_name` does not exist.

- [ ] **Step 3: Implement**

```rust
use std::sync::atomic::AtomicU64;

/// Monotonic counter distinguishing concurrent temp files (decision 34).
/// The final name is content-addressed, so two workers staging identical
/// bytes produce the same `dest` — that is fine (the second rename is a
/// no-op overwrite of identical bytes) — but they must not share a temp
/// path mid-write.
static CLONE_TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn next_clone_temp_name(name: &str) -> String {
    let seq = CLONE_TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{name}.{seq}.tmp")
}
```

In `persist_clone`, replace `let tmp = attachments_dir.join(format!("{name}.tmp"));` with `let tmp = attachments_dir.join(next_clone_temp_name(&name));`. (The `.tmp` final extension keeps the file invisible to `media::classify` and to `clean_previous_ir_output`'s artifact patterns; stray temps under `attachments/` are unreferenced by any conversation file and are removed with the folder.)

- [ ] **Step 4: Run the crate tests**

Run: `cargo test -p message-vault-io-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "fix(core): give each staged-attachment temp file a unique name"
```

---

### Task 3: Write-queue engine, sequential drain (decisions 24, 25)

**Files:**
- Create: `crates/libs/ir-format/src/write_queue.rs`
- Modify: `crates/libs/ir-format/src/lib.rs` (module + re-exports)
- Test: `write_queue.rs` test module

**Interfaces:**
- Consumes: `message_vault_io_core::{run_attachment_jobs, AttachmentJob, LogSink, emit_log, CancelFlag}`, `crate::write::write_format` (pub(crate)), `message_ir::ConversationDocument`, `media::{MediaMode, CompressOptions}`.
- Produces (Tasks 4–8 rely on these exact names):

```rust
pub enum AttachmentSource {
    /// Read this file at write time (worker-safe: plain `fs::read`).
    Path(PathBuf),
    /// Bytes already in memory (SBR blobs, handwriting SVG).
    Bytes(Vec<u8>),
    /// No source; the attachment becomes `file_missing` under copy modes.
    Missing,
}

pub struct UnitAttachment {
    pub message_index: usize,
    pub attachment_index: usize,
    pub source: AttachmentSource,
    pub timestamp_unix_ms: i64,
    pub size_hint: Option<u64>,
}

pub struct ConversationUnit {
    pub doc: ConversationDocument,
    pub attachments: Vec<UnitAttachment>,
}

impl ConversationUnit {
    /// Build a unit by pairing every attachment (message order, flat index)
    /// with a source and size hint. The closure gets `&mut IrAttachment` so
    /// byte-carrying exporters can `att.bytes.take()` into the source.
    pub fn from_doc(
        doc: ConversationDocument,
        source_for: impl FnMut(usize, &mut message_ir::IrAttachment) -> (AttachmentSource, Option<u64>),
    ) -> Self;
}

#[derive(Debug, Clone)]
pub struct WriteQueueOptions {
    /// The mode the user asked for. Convert/Compress stage originals here
    /// and convert in the post-pass (Task 5) — writers do not transcode.
    pub media: MediaMode,
    pub compress: CompressOptions,
    /// Skip units whose conversation file already exists (decision 25).
    pub resume: bool,
    /// 0 = default_writer_count(). The sequential drain ignores it.
    pub writer_count: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WriteQueueReport {
    pub conversations_written: usize,
    pub conversations_skipped: usize,
    /// Attachment records staged with a path and digest (duplicates included).
    pub attachments_saved: usize,
    /// Filled by the Convert/Compress post-pass (Task 5); default otherwise.
    pub media: media::MediaReport,
}

pub fn drain_write_queue_with_loader(
    output_dir: &Path,
    units: Vec<ConversationUnit>,
    options: &WriteQueueOptions,
    load: &mut dyn FnMut(&AttachmentSource) -> Result<Option<Vec<u8>>, String>,
    log: Option<&LogSink>,
    cancel: Option<&CancelFlag>,
) -> anyhow::Result<WriteQueueReport>;

/// The built-in loader the parallel drain also uses.
pub fn load_attachment_source(source: &AttachmentSource) -> Result<Option<Vec<u8>>, String>;
```

**Design constraints for the implementation:**

- Engine rejects obfuscation upstream — callers only route non-obfuscated JSONL exports here (exporters keep the `FormatSink` path otherwise). Add a `debug_assert!` comment, not a runtime check, since transforms never reach the engine.
- Per unit, in order: (1) cancel check via `CancelFlag`; (2) `let path = output_dir.join(format!("{}.jsonl", unit.doc.filename_stem()));` — if `options.resume && path.is_file()`, count `conversations_skipped`, add the unit's attachment count to the progress totals as already done, and continue **without loading anything**; (3) build `Vec<AttachmentJob>` by walking `doc.messages[mi].attachments[ai]` for each `UnitAttachment` (message order), pairing borrows with sources; (4) `run_attachment_jobs(&mut jobs, &output_dir.join("attachments"), stage_mode, &options.compress, per-unit loader, per-unit progress, None, cancel_atomic)` where `stage_mode` is `MediaMode::Clone` when `options.media` is `Clone | Convert | Compress` and `MediaMode::Disabled` when `Disabled` (writers copy originals only); (5) under `Disabled`, after the jobs run, clear `path`/`bytes`/`digest_sha256` on every attachment in the doc (same semantics as `clear_attachments_when_disabled`); (6) null `att.bytes` on every attachment; (7) `write_format(output_dir, OutputFormat::Jsonl, doc)` — the conversation file lands last.
- `load_attachment_source`: `Bytes` returns a clone? No — the per-unit loader closure owns the unit, so take the bytes with `std::mem::take` on first load (document this; each source is loaded at most once). `Path` is `fs::read` mapped to `Err(format!("read {}: {e}", path.display()))`; `Missing` is `Ok(None)`.
- Progress: keep running totals across all units (`done`, `total` = sum of unit attachment counts, `bytes_done`, `bytes_total` = sum of size hints, growing when a hint-less file loads) and emit the exact line `  attachments {done}/{total} {bytes_done}/{bytes_total}` after each attachment, via `emit_log(log, ...)`. Emit `""` then `Preparing {N} conversation file(s)...` once before the first unit (N = unit count), and after the drain `Prepared {written} conversation file(s)` plus, when resuming and `skipped > 0`, `Skipped {skipped} already staged conversation(s)`. No `  preparing {n}/{N}` lines (see Global Constraints).
- A skipped unit's attachments count toward `done`/`total` immediately (progress reflects the whole import, not just this run's work); their bytes count toward neither bytes counter (unknown without loading — acceptable, byte counters describe this run's copying).
- Any unit error aborts the drain with that error (parity with today's `?` behavior). `"canceled"` propagates as-is.

**Steps:**

- [ ] **Step 1: Write the failing tests**

Test helpers: build small `ConversationDocument`s via `message_ir::testutil::sample_document`, distinct `chat_identifier`s so stems differ. Cover:

```rust
#[test]
fn drains_units_and_writes_conversation_files_last() {
    // Two units: one with a Bytes source, one with a Path source (write a
    // real temp file). Drain with Clone. Assert: both .jsonl files exist,
    // both attachments have path + 64-char digest + size, att.bytes is None
    // in the written files, and attachments/ holds the staged files.
}

#[test]
fn resume_skips_a_unit_whose_conversation_file_exists() {
    // Drain once. Re-build identical units where the loader would panic if
    // called (AttachmentSource::Path at a now-deleted path would error, but
    // stronger: use drain_write_queue_with_loader with a loader that
    // panics). Drain with resume: true. Assert Ok, conversations_skipped == 2,
    // conversations_written == 0.
}

#[test]
fn resume_rewrites_a_unit_whose_conversation_file_is_missing() {
    // Drain once, delete one .jsonl, drain again with resume: true.
    // Assert written == 1, skipped == 1, and the deleted file is back.
}

#[test]
fn disabled_mode_marks_not_copied_and_clears_paths() {
    // One unit, media: Disabled. Assert the written doc's attachment has
    // missing_reason "not_copied", no path, no digest, and attachments/
    // holds nothing.
}

#[test]
fn missing_source_becomes_file_missing_and_the_drain_continues() { /* ... */ }

#[test]
fn progress_lines_cover_all_units_with_global_counts() {
    // LogSink into a Vec. Two units with 1 attachment each. Assert a line
    // "  attachments 2/2 ..." appears, plus the Preparing/Prepared lines.
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p message-ir-format write_queue`
Expected: compile failure (module missing).

- [ ] **Step 3: Implement the module** as specified above; wire `mod write_queue;` and `pub use write_queue::{...}` in `lib.rs`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p message-ir-format`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ir-format): conversation write queue with attachments-first commit order and resume skip"
```

---

### Task 4: Parallel drain and disk headroom (decisions 24, 32, 33)

**Files:**
- Modify: `crates/libs/ir-format/src/write_queue.rs`
- Modify: `crates/libs/ir-format/Cargo.toml` (add `fs2 = "0.4.3"`)
- Test: `write_queue.rs` test module

**Interfaces:**
- Produces:

```rust
/// Writers scale with the machine; writing is IO and hashing (decision 33).
pub fn default_writer_count() -> usize; // available_parallelism().clamp(1, 8)

pub fn drain_write_queue(
    output_dir: &Path,
    units: Vec<ConversationUnit>,
    options: &WriteQueueOptions,
    log: Option<&LogSink>,
    cancel: Option<&CancelFlag>,
) -> anyhow::Result<WriteQueueReport>;

/// None when `available` covers `needed` plus slack; otherwise the error text.
fn headroom_shortfall(needed: u64, available: u64) -> Option<String>;
```

**Design constraints:**

- **Headroom (decision 32):** before any unit runs — in BOTH drains — compute `needed = sum of every unit attachment's size_hint` (skipped-resume units excluded when their conversation file already exists? No: compute before skip checks; over-asking on a resume is conservative and resume typically has most bytes already on disk — note this in a comment). `const DISK_HEADROOM_SLACK: u64 = 64 * 1024 * 1024;`. Get `available = fs2::available_space(output_dir)` (the dir exists by now); on `Err`, skip the check (a filesystem that cannot answer must not block an export). Error text, plain and stated: `Not enough space on the staging disk: this backup needs about {needed_h}, and {available_h} is free.` — format sizes with the crate's existing byte formatter if one is exported from `media` (`format_bytes` is private to `media`; write a tiny local `human_bytes` helper: GiB/MiB/KiB with one decimal). Peak usage is originals plus one in-flight derivative (decision 28 commits per file), so the originals' sum plus slack is the honest requirement.
- **Built-in loader warnings:** when `load_attachment_source` returns `Err` for a `Path` source, the drain logs `warning: attachment {path} could not be read: {e}` through `emit_log` before handing the error to `run_attachment_jobs` (which downgrades it to `file_missing`). This preserves the phase-1 rule that an unreadable attachment is logged before it becomes a chip. Add a test: a `Path` source pointing at a missing file produces a `warning:` line and a `file_missing` attachment, and the drain still succeeds.
- **Parallel drain:** `std::thread::scope`; `writer_count = if options.writer_count == 0 { default_writer_count() } else { options.writer_count }`, clamped to `units.len().max(1)`. Queue: `Mutex<VecDeque<ConversationUnit>>`. Shared state: `AtomicUsize done`, `AtomicU64 bytes_done`, `AtomicU64 bytes_total`, `AtomicUsize attachments_saved`, `AtomicUsize written`, `AtomicUsize skipped`, an `AtomicBool abort`, and a `Mutex<Option<String>>` first-error slot. Workers pop, check `abort` and cancel, process the unit exactly like the sequential body using `load_attachment_source`, update atomics, emit the shared progress line through `emit_log` (LogSink is `Send + Sync`). On error: store into the first-error slot if empty, set `abort`, return. After the scope: if the error slot holds `Some(msg)` return `Err(anyhow::anyhow!(msg))`; a cancel stores `"canceled"` there like any error.
- Refactor so the per-unit body is one private function both drains call: `fn write_one_unit(output_dir: &Path, attachments_dir: &Path, unit: ConversationUnit, options: &WriteQueueOptions, load: &mut dyn FnMut(&AttachmentSource) -> Result<Option<Vec<u8>>, String>, progress: &dyn Fn(UnitProgress), cancel: Option<&AtomicBool>) -> anyhow::Result<UnitOutcome>` (shapes as needed; keep it private).
- `fs::create_dir_all(attachments dir)` once before spawning (avoid racing `run_attachment_jobs`' create; it is idempotent but do it up front anyway).

**Steps:**

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn parallel_drain_writes_every_unit() {
    // 12 units, Bytes sources, writer_count: 4. Assert 12 .jsonl files,
    // report.conversations_written == 12, attachments staged.
}

#[test]
fn parallel_drain_stops_on_the_first_error() {
    // One unit with AttachmentSource::Path pointing at a directory (fs::read
    // fails)... note: a load failure downgrades to file_missing, NOT an
    // error — so provoke a real error instead: make output_dir/attachments
    // an existing FILE so create/write fails for every worker. Assert Err.
}

#[test]
fn headroom_shortfall_speaks_when_space_is_short() {
    assert!(headroom_shortfall(10 * 1024 * 1024 * 1024, 1024).is_some());
    assert_eq!(headroom_shortfall(1024, 10 * 1024 * 1024 * 1024), None);
    let msg = headroom_shortfall(2 * 1024 * 1024 * 1024, 1024).unwrap();
    assert!(msg.contains("free"));
}

#[test]
fn default_writer_count_is_bounded() {
    let n = default_writer_count();
    assert!((1..=8).contains(&n));
}
```

- [ ] **Step 2: Run to see them fail** — `cargo test -p message-ir-format write_queue` → compile failure.

- [ ] **Step 3: Implement** as specified; add the `fs2` dependency.

- [ ] **Step 4: Run** `cargo test -p message-ir-format` → PASS. Also `cargo test -p message-ir-format -- --test-threads=1 write_queue` once to shake out ordering assumptions.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ir-format): parallel writer pool and the disk-headroom check"
```

---

### Task 5: Convert/Compress post-pass in the engine (decisions 26, 27, 43-CLI)

**Files:**
- Modify: `crates/libs/ir-format/src/write_queue.rs`
- Test: same module

**Interfaces:**
- Consumes: `crate::transcode::{transcode_staged, TranscodeOptions, TranscodeReport}` (already `pub use`d from `lib.rs`).
- Produces: no new names — both drains grow the post-pass internally, and `WriteQueueReport.media` is filled.

**Design constraints:**

- After a successful drain, when `options.media` is `Convert | Compress`: run

```rust
let transcode_options = TranscodeOptions {
    mode: options.media,
    compress: options.compress.clone(),
    // No vault limit applies to a local export; nothing gets written off
    // as too large here. The desktop's own media pass (which enforces the
    // real limit) never reaches this code — it stages with Clone.
    asset_max_bytes: u64::MAX,
};
let report = transcode_staged(output_dir, &transcode_options, cancel, &mut |p| {
    emit_log(log, format!("  media {}/{}", p.done, p.total));
})?;
```

then map it: `media.processed = report.converted`, `media.skipped = report.skipped + report.repointed`, `media.bytes_before/bytes_after` copied, and when `report.failed > 0` push one summary entry onto `media.errors`: `format!("{} file(s) could not be converted; their conversation entries say why", report.failed)` (per-file reasons already land in the JSONL as `convert_failed:` — decision 41). Emit a closing log line: `Attachment {mode} done: converted={} skipped={} size {} → {}` using the same `human_bytes` helper.
- The `  media {done}/{total}` line is deliberately inert to the desktop's log scraper (no `attachments` prefix, no `…`/`preparing` keyword) — the desktop never runs this branch, and the CLI just prints it.
- Cancellation inside the pass propagates its error unchanged; already-committed derivatives are the pass's own resume story (decision 28).

**Steps:**

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn clone_mode_runs_no_media_pass() {
    // Drain with Clone; assert report.media == MediaReport::default().
}

#[test]
fn convert_mode_stages_originals_before_the_pass() {
    // Only when ffmpeg is absent from PATH can this be tested hermetically:
    // with mode Convert and no ffmpeg, the drain must return Err (the pass
    // refuses up front) BUT the staged originals and conversation files
    // must already exist on disk — a transcode failure destroys nothing
    // (decision 31). Gate the test on ffmpeg being absent the same way
    // transcode.rs's own tool-missing tests do (see its test module); if
    // the repo's transcode tests instead fake tools via PATH manipulation,
    // reuse that helper.
}
```

Check `crates/libs/ir-format/src/transcode.rs`'s test module first and mirror its tool-gating pattern exactly rather than inventing one.

- [ ] **Step 2: Run to see them fail** — `cargo test -p message-ir-format write_queue` → FAIL.

- [ ] **Step 3: Implement**, refactoring the post-pass into one private `fn run_media_post_pass(...)` called by both drains.

- [ ] **Step 4: Run** `cargo test -p message-ir-format` → PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ir-format): convert and compress run as the resumable pass after the write queue drains"
```

---

### Task 6: `open_resume` and the config flag (decision 42)

**Files:**
- Modify: `crates/libs/ir-format/src/format_sink.rs` (after `open_prepared`, line 109)
- Modify: `crates/core/message-vault-io-core/src/config.rs` (`ExporterConfig`, line 101)
- Modify (constructor sites — add `resume: false` everywhere; the compiler enumerates): `crates/core/message-vault-io-core/src/exporters.rs` (`Form::to_config`, `to_format_config`), `crates/core/message-vault-io-core/src/config.rs` (any builders), the seven exporter `main.rs` files, `crates/message-vault-io-gui/src/jobs.rs`, `crates/libs/reexport/src/lib.rs` + `src/bin/message_reexporter.rs`, `src-tauri/src/commands/extract.rs`, `src-tauri/src/commands/format.rs`, and test fixtures.
- Test: `format_sink.rs` test module

**Interfaces:**
- Produces:
  - `FormatSink::open_resume(output: &Path, format: OutputFormat, transforms: ExportTransforms) -> Result<(Self, PathBuf)>`
  - `ExporterConfig.resume: bool` — false everywhere except the desktop resume path (Task 9).

**Steps:**

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn open_resume_keeps_previous_output_and_requires_the_sentinel() {
    let tmp = tempfile::tempdir().unwrap();
    // A fresh dir without the sentinel is refused.
    assert!(
        FormatSink::open_resume(tmp.path(), OutputFormat::Jsonl, ExportTransforms::none()).is_err()
    );
    // Prepare once (writes the sentinel), drop a fake previous conversation
    // file and a staged attachment, then open_resume and assert both files
    // are still there.
    let (_, att_dir) =
        FormatSink::open_prepared(tmp.path(), OutputFormat::Jsonl, ExportTransforms::none()).unwrap();
    std::fs::create_dir_all(&att_dir).unwrap();
    std::fs::write(tmp.path().join("keep.jsonl"), "x").unwrap();
    std::fs::write(att_dir.join("keep.jpg"), "y").unwrap();
    let (_sink, att_dir2) =
        FormatSink::open_resume(tmp.path(), OutputFormat::Jsonl, ExportTransforms::none()).unwrap();
    assert!(tmp.path().join("keep.jsonl").is_file());
    assert!(att_dir2.join("keep.jpg").is_file());
}
```

- [ ] **Step 2: Run to see it fail** — `cargo test -p message-ir-format open_resume` → compile failure.

- [ ] **Step 3: Implement `open_resume`**

```rust
/// Reopen `output` to continue an interrupted export (decision 42).
///
/// Unlike [`open_prepared`](Self::open_prepared), nothing is cleaned:
/// conversation files and staged attachments from the interrupted run are
/// the work a resumed run skips. The directory must already be an export
/// folder (it carries the `.message-vault-export` sentinel); resuming into
/// anything else is a caller bug, not something to repair by cleaning.
///
/// # Errors
///
/// Returns an error when the directory or its sentinel is missing, or the
/// attachments directory cannot be created.
pub fn open_resume(
    output: &Path,
    format: OutputFormat,
    transforms: ExportTransforms,
) -> Result<(Self, PathBuf)> {
    if !output.join(crate::clean::EXPORT_SENTINEL).is_file() {
        anyhow::bail!(
            "cannot resume into {}: it is not a staging folder from a previous run",
            output.display()
        );
    }
    let att_dir = output.join("attachments");
    if transforms.copies_attachments() {
        fs::create_dir_all(&att_dir).with_context(|| format!("create {}", att_dir.display()))?;
    }
    let sink = Self::open(output, format, transforms)?;
    Ok((sink, att_dir))
}
```

- [ ] **Step 4: Add `ExporterConfig.resume`**

```rust
/// Continue an interrupted export in the same output directory: previous
/// output is kept and conversations already written are skipped. Only the
/// desktop's import resume sets this; CLI runs leave it false.
pub resume: bool,
```

Run `cargo build --workspace && cargo build --manifest-path src-tauri/Cargo.toml`, adding `resume: false` at every construction site the compiler names.

- [ ] **Step 5: Run** `cargo test -p message-ir-format && cargo build --workspace && cargo build --manifest-path src-tauri/Cargo.toml` → PASS/green.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(ir-format): open_resume keeps previous output; exporter config carries the resume flag"
```

---

### Task 7: iMessage on the write queue

**Files:**
- Modify: `crates/exporters/imessage-ir-exporter/src/emit.rs` (`run_export` lines 81–236, `stage_conversation_attachments` lines 260–331)
- Modify: `crates/exporters/imessage-ir-exporter/src/options.rs` (`MailOptions` gains `resume: bool`)
- Modify: `crates/exporters/imessage-ir-exporter/src/run.rs` (thread `config.resume` into `MailOptions`)
- Test: `emit.rs` / crate tests

**Interfaces:**
- Consumes: `message_ir_format::{ConversationUnit, UnitAttachment, AttachmentSource, WriteQueueOptions, WriteQueueReport, drain_write_queue, drain_write_queue_with_loader, FormatSink}` and `read_resolved_attachment` (attachments.rs:39).
- Produces: unchanged `run_export` signature returning `FormatSinkResult`.

**Design:**

In `run_export`, after the message stream fills `conversations` (no change up to line 161):

- Compute `let use_queue = format == OutputFormat::Jsonl && !session.options.transforms.obfuscate;`.
- **Queue path** (`use_queue`): open the sink dir with `FormatSink::open_prepared` or `FormatSink::open_resume` depending on `session.options.resume` — note the open must move to AFTER this branch decision but BEFORE the message stream (cleaning must still precede parse staging for the fresh case; for the resume case nothing is cleaned, so position is safe). Concretely: replace the unconditional `open_prepared` at line 94 with:

```rust
let (mut sink, attachments_dir) = if session.options.resume {
    FormatSink::open_resume(
        &session.options.export_path,
        format,
        session.options.transforms.clone(),
    )
} else {
    FormatSink::open_prepared(
        &session.options.export_path,
        format,
        session.options.transforms.clone(),
    )
}
.map_err(|e| RuntimeError::InvalidOptions(format!("open export sink: {e:#}")))?;
```

(`resume` is only ever set by the desktop, which always exports JSONL; a resume with another format simply reopens without cleaning, which is still correct for `open_resume`'s contract.)

Then, where the old flow staged attachments and buffered docs (lines 163–233), branch:

```rust
if use_queue {
    let units = build_conversation_units(session, conversations)?;
    let options = WriteQueueOptions {
        media: session.options.transforms.media,
        compress: session.options.transforms.compress.clone(),
        resume: session.options.resume,
        writer_count: 0,
    };
    let log = session.options.log.clone();
    let cancel = session.options.cancel.as_ref();
    let encrypted = session
        .data_source
        .backup
        .as_ref()
        .is_some_and(|b| b.is_encrypted());
    let report = if encrypted {
        // crabapple's Backup holds a SQLite connection (not Sync), so the
        // decrypt loader cannot cross threads; the sequential drain keeps
        // the same commit order with one writer.
        let mut load = |source: &AttachmentSource| match source {
            AttachmentSource::Path(path) => {
                let bytes = read_resolved_attachment(session, path).map_err(|e| {
                    session.options.emit_log(format!(
                        "warning: attachment {} could not be read: {e}",
                        path.display()
                    ));
                    e.to_string()
                })?;
                Ok((!bytes.is_empty()).then_some(bytes))
            }
            other => message_ir_format::load_attachment_source(other),
        };
        message_ir_format::drain_write_queue_with_loader(
            &session.options.export_path,
            units,
            &options,
            &mut load,
            log.as_ref(),
            cancel,
        )
    } else {
        message_ir_format::drain_write_queue(
            &session.options.export_path,
            units,
            &options,
            log.as_ref(),
            cancel,
        )
    }
    .map_err(|e| RuntimeError::InvalidOptions(format!("write conversations: {e:#}")))?;
    return Ok(FormatSinkResult {
        xml_path: None,
        media: report.media,
        obfuscated_docs: 0,
    });
}
// ... existing staging + sink loop + sink.finish() unchanged for other formats ...
```

`build_conversation_units` is a new private function that reproduces the document-building loop at lines 176–213 (owner handles, `ExportMeta`, `ConversationMeta`) but instead of `sink.write_document(doc)` builds a `ConversationUnit`, pairing each attachment (message order) with its `AttachmentLoad` (the per-conversation `convo.attachment_loads`, consumed positionally exactly as the flat loop at lines 271–288 does today):

```rust
AttachmentLoad::Path { path, size_hint } => (AttachmentSource::Path(path), size_hint),
AttachmentLoad::Bytes(bytes) => {
    let hint = Some(bytes.len() as u64);
    (AttachmentSource::Bytes(bytes), hint)
}
AttachmentLoad::Missing => (AttachmentSource::Missing, att.size_bytes),
```

The empty-messages skip (`convo.messages.is_empty()`) must survive, and the unencrypted path relies on the parallel drain's built-in warning line (Task 4) for unreadable attachments — no extra logging here.

The old flat `stage_conversation_attachments` stays for the non-queue formats.

**Steps:**

- [ ] **Step 1: Extract and test the unit builder.** Write `build_conversation_units` so its core (`PendingConversation` → `ConversationDocument` + sources) is testable without a `MailSession` — factor the session-independent part (`pending_to_unit(chat_identifier, convo, owner_row: ...)`) if the existing types allow; otherwise test at the level the fixtures support. Add a test pinning the positional pairing: a conversation with two messages (one attachment each) whose `attachment_loads` are `[Path, Bytes]` must produce units whose sources land on the right attachments in order.
- [ ] **Step 2: Run to see it fail**, then implement as designed.
- [ ] **Step 3: Full crate test.** Run: `cargo test -p imessage-ir-exporter` → PASS.
- [ ] **Step 4: Workspace build** — `cargo build --workspace` → green.
- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(imessage): jsonl exports drain the conversation write queue"
```

---

### Task 8: WhatsApp and iMazing on the write queue

**Files:**
- Modify: `crates/exporters/whatsapp-exporter/src/emit.rs` (`convert_json` lines 52–146, source collection at lines 320–338)
- Modify: `crates/exporters/imazing-exporter/src/emit.rs` (lines 99–352)
- Test: each crate's tests

**Interfaces:**
- Consumes: the engine names from Task 3, `ExporterConfig.resume` via each crate's run/options plumbing (whatsapp's `convert_json` gains a `resume: bool` parameter threaded from its `run.rs`; imazing likewise through its args struct).
- Produces: unchanged public run signatures.

**Design (both exporters follow the same shape):**

- Sink open: `open_resume` vs `open_prepared` on the resume flag (same pattern as Task 7).
- `use_queue = output_format == OutputFormat::Jsonl && !<obfuscate>` — whatsapp reads obfuscation from the transforms it was handed (capture `transforms.obfuscate` before the transforms move into the sink open, alongside the existing `copy_attachments` capture at line 66); imazing the same.
- Whatsapp: today `collect_media_sources(&convo, &mut media_sources)` builds one flat `Vec<Option<PathBuf>>` aligned with the flat attachment order. On the queue path, build per-document units in the same loop that calls `pending_to_document` (lines 102–109): collect that conversation's sources into a local `Vec<Option<PathBuf>>`, build the doc, then

```rust
let mut source_iter = convo_sources.into_iter();
let unit = ConversationUnit::from_doc(doc, |_, att| {
    let hint = att.size_bytes;
    match source_iter.next().flatten() {
        Some(path) => (AttachmentSource::Path(path), hint),
        None => (AttachmentSource::Missing, hint),
    }
});
```

Both exporters build the same options:

```rust
let options = WriteQueueOptions {
    media: media_mode,
    compress: compress.clone(),
    resume,
    writer_count: 0,
};
```

Then `drain_write_queue(output, units, &options, log.as_ref(), cancel)`, add `report.conversations += queue_report.conversations_written + queue_report.conversations_skipped;` and `report.attachments_saved += queue_report.attachments_saved;`, and return with a synthesized `FormatSinkResult { xml_path: None, media: queue_report.media, obfuscated_docs: 0 }`. The non-queue path is untouched.
- iMazing: identical, using `collect_attachment_sources(&convo, &mut sources)` per conversation (its sources are also `Option<PathBuf>` aligned per attachment — see emit.rs:314). Note iMazing's size hints fall back to `fs::metadata` on the source path (attachments.rs:252-259); reproduce that in the unit builder closure (`att.size_bytes.or_else(|| path stat len)`).
- A missing size hint is fine — the engine's byte totals grow as files load.

**Steps:**

- [ ] **Step 1: Write failing tests.** Each crate has existing end-to-end fixture tests that export JSONL; find one, and add a variant asserting (a) the JSONL files and staged attachments appear (queue path active) and (b) a second run with `resume: true` against the same output dir succeeds and rewrites nothing (assert the conversation file's mtime or content is unchanged where the test harness allows; at minimum assert success and identical file sets).
- [ ] **Step 2: Run to see them fail**, then implement both exporters.
- [ ] **Step 3: Run** `cargo test -p whatsapp-exporter -p imazing-exporter` → PASS.
- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(exporters): whatsapp and imazing drain the conversation write queue"
```

---

### Task 9: SBR, GO SMS Pro, SMS Backup+, OpenExtract on the write queue

**Files:**
- Modify: `crates/exporters/sms-backup-restore-exporter/src/emit.rs` (`convert_export` lines 91–140)
- Modify: `crates/libs/ir-format/src/read_sbr.rs` (`SbrReadOptions` gains `stage_attachments: bool`, honored at the `stage_read_attachments` call ~line 196's caller)
- Modify: `crates/exporters/go-sms-pro-exporter/src/emit.rs` (lines 628–766)
- Modify: `crates/exporters/sms-backup-plus-exporter/src/emit.rs` (lines 531–712)
- Modify: `crates/exporters/openextract-exporter/src/emit.rs` (lines 68–210)
- Test: each crate's tests, `read_sbr` tests

**Interfaces:**
- Consumes: engine names; `ExporterConfig.resume` threaded through each crate's arg plumbing.
- Produces: `SbrReadOptions.stage_attachments: bool` — `true` keeps today's behavior (the reexport library path is untouched); the SBR exporter's queue path sets `stage_attachments: false, keep_attachment_bytes: true` and builds units from the bytes left on the attachments.

**Design:**

- These four exporters carry attachment bytes on the documents themselves (`att.bytes`), or have no attachments at all (openextract). The queue path for each:

```rust
let units: Vec<ConversationUnit> = documents
    .into_iter()
    .map(|doc| {
        ConversationUnit::from_doc(doc, |_, att| {
            let hint = att
                .size_bytes
                .or_else(|| att.bytes.as_ref().map(|b| b.len() as u64));
            match att.bytes.take() {
                Some(bytes) => (AttachmentSource::Bytes(bytes), hint),
                None => (AttachmentSource::Missing, hint),
            }
        })
    })
    .collect();
```

(`from_doc` hands the closure `&mut IrAttachment` exactly so the bytes can be taken — Task 3's interface.)

- SBR: on the queue path call `read_sbr_documents` with `attachments_dir: None, stage_attachments: false, keep_attachment_bytes: true` and the SAME media/compress options (they ride into the engine instead); `enrich_contacts` runs before unit building as today. `report.attachments_saved` comes from the engine (counting per staged record; the old path deduped by path — an intentional semantic simplification, note it in the commit message).
- GO SMS Pro / SMS Backup+: skip their `stage_conversation_attachments` call on the queue path; build units from `att.bytes` as above.
- OpenExtract: no attachments; units have empty attachment lists; the engine still parallelizes conversation writes and gives resume skip.
- All four: sink open honors the resume flag; non-JSONL/obfuscate path untouched; `FormatSinkResult` synthesized as in Task 8.

**Steps:**

- [ ] **Step 1: Write failing tests** — each crate's existing JSONL fixture test grows the same two assertions as Task 8 (queue output correct; resume run succeeds and skips). `read_sbr` gets a test that `stage_attachments: false` leaves `att.bytes` populated and writes nothing to disk.
- [ ] **Step 2: Run to see them fail.**
- [ ] **Step 3: Implement all four exporters + the read_sbr option.**
- [ ] **Step 4: Run** `cargo test -p sms-backup-restore-exporter -p go-sms-pro-exporter -p sms-backup-plus-exporter -p openextract-exporter -p message-ir-format` → PASS; then `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(exporters): sbr, go sms pro, sms backup+, and openextract drain the write queue"
```

---

### Task 10: Desktop resume plumbing and cancelled-extract session survival

**Files:**
- Modify: `src-tauri/src/commands/extract.rs` (`ExtractArgs` line 132, `extract` line 232)
- Modify: `web/src/lib/tauri.ts` (`invokeExtract` argument type)
- Modify: `web/src/screens/import/useImportJob.ts` (outer catch at line 1132; `startImport`)
- Test: `web/src/screens/import/useImportJob.test.ts` (or the existing test file beside it), `src-tauri` builds

**Interfaces:**
- Consumes: `ExporterConfig.resume` (Task 6).
- Produces:
  - `ExtractArgs.resume: Option<bool>` (wire name `resume`), applied as `config.resume = args.resume.unwrap_or(false);` right after `build_exporter_config` returns (extract.rs line 232).
  - `invokeExtract` accepts `resume?: boolean`.
  - `startImport(form, resume?: ResumePush, resumeWrite?: ResumeWrite)` where `export type ResumeWrite = { sessionId: number; stagingDir: string };` — Task 11 calls it.

**Design:**

- `startImport`'s `resumeWrite` branch (placed after the `resume` push-branch, replacing the create-session block when set):

```ts
if (resumeWrite) {
  // The session already exists and died (or was cancelled) during the
  // write. Reuse it and its staging folder; the exporter re-parses and
  // skips conversations already written (decisions 4 and 36).
  const outputDir = resumeWrite.stagingDir;
  setStagingDir(outputDir);
  sessionId = resumeWrite.sessionId;
  setImportSessionId(sessionId);
  await moveStage(sessionId, "write");
} else {
  // ... existing resolveImportStagingDir + POST /v1/imports + moveStage("write") ...
}
```

and the `invokeExtract` call gains `...(resumeWrite ? { resume: true, } : {}),` with `output_dir: resumeWrite ? resumeWrite.stagingDir : outputDir` folded so both paths pass the right dir (restructure so `outputDir` is a single const both branches set before the invoke).
- **Cancelled extract keeps the session** (decision 36 "Cancelled mid-run: same recovery as a crash at that stage"). In the outer catch at line 1132, mirror the media pass's pattern (lines 839–872): detect `isCancellation(msg)`; when cancelled, pass `skipComplete: true` (and `canceled: true` if `finishImport`'s signature distinguishes them — read `finishImport` at line ~612 and match the media-pass call exactly). A cancelled extract then leaves the session `running` at stage `write`, which the next Import visit resolves through the resume panel. A genuine failure still completes the session as `failed` (a failed session is not resumable — `GET /v1/imports/active` only returns running sessions — and decision 36 routes it to restart-with-settings via the session record's terminal listing; that is phase-2 behavior, unchanged).

**Steps:**

- [ ] **Step 1: Write the failing web test** — in the `useImportJob` test file, a cancelled extract (`invokeExtract` rejecting with `"canceled"`) must NOT POST `/v1/imports/{id}/complete` (assert on the mocked apiClient), while a failing extract (rejecting with another message) still must.
- [ ] **Step 2: Run** `cd web && npx vitest run src/screens/import/` → FAIL.
- [ ] **Step 3: Implement** the three files. Rust side: add the field + one line; run `cargo build --manifest-path src-tauri/Cargo.toml`.
- [ ] **Step 4: Run** `cd web && npm test` and `cargo build --manifest-path src-tauri/Cargo.toml` → PASS/green.
- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(desktop): extract accepts a resume flag, and cancelling an extract keeps the session"
```

---

### Task 11: The `resume_write` decision, fingerprint check, and panel copy (decisions 36, 38)

**Files:**
- Modify: `web/src/screens/import/resumeDecision.ts`
- Modify: `web/src/screens/import/ResumeImportPanel.tsx`
- Modify: `web/src/screens/import/ImportScreen.tsx` (`handleResumeAction` lines 275–334, resume check that computes `folderExists`)
- Test: `resumeDecision.test.ts`, `ResumeImportPanel.test.tsx`, `ImportScreen.test.tsx` (existing files beside sources)

**Interfaces:**
- Consumes: `startImport(form, undefined, { sessionId, stagingDir })` from Task 10; `invokePathStat`, `SourceFingerprint` from `importSession.ts`; `invokeDeleteStaging` (the phase-3 staging delete wrapper in `tauri.ts` — reuse exactly the call `handleDiscardResume` makes).
- Produces:

```ts
/** How the session's stored backup fingerprint compares to the backup now. */
export type FingerprintCheck = "match" | "mismatch" | "source_missing" | "unknown";

export function checkSourceFingerprint(
  stored: SourceFingerprint | null,
  stat: PathStat | null, // null: stat failed or was impossible
): FingerprintCheck;

// resumeDecisionFor gains the field:
export function resumeDecisionFor(args: {
  session: ActiveImportSession | null;
  deviceId: string;
  folderExists: boolean;
  fingerprint: FingerprintCheck;
}): ResumeDecision;

// New kinds on ResumeDecision:
//   "resume_write"    — stage "write", folder exists, fingerprint match|unknown
//   "source_changed"  — stage "write", folder exists, fingerprint mismatch|source_missing
```

**Design:**

- `checkSourceFingerprint`: no stored fingerprint → `"unknown"`; `stat` null or `!stat.exists` → `"source_missing"`; stored `size_bytes === stat.sizeBytes && modified_unix_ms === stat.modifiedUnixMs` → `"match"`, else `"mismatch"`. (Directory sources have the documented blind spot — `importSession.ts:73-75`; a change the stat cannot see resumes and re-parses, and unchanged conversation boundaries make the skip correct. Decision 38 fires on every mismatch we can see.)
- Decision table (order matters; insert between the `folder_missing` row and the `pushing` row so device/folder checks still win):

```ts
if (stage === "write" && (fingerprint === "mismatch" || fingerprint === "source_missing")) {
  return { kind: "source_changed", canResume: false, session };
}
if (stage === "write") {
  return { kind: "resume_write", canResume: true, session };
}
```

`parse` continues to fall through to `restart`. Stages other than `write` ignore the fingerprint entirely (decision 36: "Source changed or missing … irrelevant at either gate and during `pushing`").
- `ImportScreen`: where the resume check currently computes `folderExists`, also stat the source: `const sourceStat = session?.source_fingerprint?.path ? await invokePathStat(session.source_fingerprint.path).catch(() => null) : null;` then `fingerprint: checkSourceFingerprint(session?.source_fingerprint ?? null, sourceStat)`.
- `handleResumeAction`:
  - `resume_write` → `await startImport(restoredForm, undefined, { sessionId: session.id, stagingDir: session.staging_dir! })` (the decision guarantees `staging_dir` and folder existence).
  - `source_changed` → the restart tail (discard session, start fresh) — its primary button is "Start over".
  - The existing restart tail (which `restart` and `source_changed` both reach) additionally deletes the old staging folder before starting over, for this-device sessions with a `staging_dir` — reuse the exact `invokeDeleteStaging` guard/call from `handleDiscardResume` (decision 36: died in parse → "folder deleted"). Best-effort like the discard path.
- `ResumeImportPanel` copy (states, never warns; no internal stage names):

```ts
resume_write: {
  heading: () => "Finish copying your backup",
  body: () =>
    "The copy did not finish. Picking up where you left off reads the backup again and skips the conversations already copied.",
  primary: { label: "Pick up", action: "resume" },
  secondary: { label: "Discard this import", action: "discard" },
},
source_changed: {
  heading: () => "The backup has changed",
  body: (s) =>
    s.source_fingerprint?.path
      ? `This import was reading ${s.source_fingerprint.path}, and that backup is different now. Starting over reads it fresh with the same settings.`
      : "The backup this import was reading is different now. Starting over reads it fresh with the same settings.",
  primary: { label: "Start over", action: "resume" },
  secondary: { label: "Discard this import", action: "discard" },
},
```

(`source_changed`'s primary routes through the restart tail, so `action: "resume"` is correct — the screen's resume handler maps kinds to behaviors, exactly as `restart` already does.)

**Steps:**

- [ ] **Step 1: Write the failing tests.**
  - `checkSourceFingerprint` truth table (stored null → unknown; stat null → source_missing; equal → match; size differs → mismatch; mtime differs → mismatch).
  - `resumeDecisionFor`: write+match → resume_write (canResume); write+unknown → resume_write; write+mismatch → source_changed; write+source_missing → source_changed; parse+anything → restart; pushing+mismatch → resume_push (fingerprint ignored); folder missing still wins over everything at write.
  - Panel copy renders the two new kinds with their headings and both buttons.
  - ImportScreen: restart action deletes the old staging folder for a this-device session (mock `invokeDeleteStaging`, assert called with the session's dir).
- [ ] **Step 2: Run** `cd web && npx vitest run src/screens/import/` → FAIL.
- [ ] **Step 3: Implement** the four files.
- [ ] **Step 4: Run** `cd web && npm test && npm run lint` → PASS.
- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(web): resume an interrupted copy, and say so when the backup changed"
```

---

### Task 12: Spec amendment and full verification

**Files:**
- Modify: `docs/superpowers/specs/2026-08-29-import-session-design.md` (decision 12, lines 191–213; decision 43, line 454)
- Verify: everything

**Steps:**

- [ ] **Step 1: Amend decision 12** with the correction phase 3 established (compress re-encodes with libx265 first, so HEVC only grows under *convert*): after the sentence ending "HEVC to H.264 typically grows 30–50%." add: `Compress mode re-encodes with libx265 first, so growth from HEVC applies to convert, not compress.` Amend decision 43's text to reflect where the call now lives: replace the sentence with `**Convert and compress report progress on every path.** The desktop's media pass reports through its own events; the CLI's write queue and media pass log through the shared sink (the old inline pass reported nothing at all).` Keep the decision number.
- [ ] **Step 2: Run the full gate.**

```bash
./scripts/check-pr.sh
```

Expected: every step green (rustfmt both trees, workspace build+test, src-tauri build, Biome, Vitest, docs). Fix anything it surfaces.
- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "docs(spec): record the compress/libx265 reality and where convert progress now flows"
```

---

## Rulings made while planning

1. **Decision 23 is already satisfied** — every exporter buffers all documents and stages attachments only after the full scan; no task exists for it.
2. **The engine lives in `message-ir-format`**, not core: it needs `write_format` and `transcode_staged` (ir-format) plus `run_attachment_jobs` (core), and ir-format already depends on core. Cost if wrong: a move later, mechanical.
3. **Obfuscated exports and non-JSONL formats keep the FormatSink path.** Obfuscation is stateful-shared (one `Obfuscator` across docs) and stages no attachments; XML is a single merged file; EML/MBOX embed media at finish. The queue's benefits (resume, parallel writers) matter on the import path, which is JSONL and never obfuscated. Cost if wrong: those paths stay serial — today's behavior.
4. **Encrypted iOS backups drain sequentially** (`crabapple::Backup` holds a rusqlite `Connection`, not `Sync`). One writer preserves every invariant; decrypt-bound throughput would not have parallelized well anyway.
5. **CLI JSONL convert/compress now runs as the post-pass**, so converted files gain `-mv` stems and the pass's resume/commit semantics. Output content is equivalent; filenames differ from pre-phase-4 CLI output. Non-JSONL CLI formats keep the inline pass, which now logs (decision 43's letter).
6. **`asset_max_bytes: u64::MAX` for the CLI post-pass** — no vault limit applies to a local export, so nothing is written off as too large.
7. **Headroom check uses the sum of size hints plus 64 MiB slack, and skips when the OS cannot answer.** Hints are a lower bound (hint-less files uncounted); a check that only fires when even the known bytes cannot fit never false-positives. Cost if wrong: an export that would have failed with ENOSPC partway fails at the same place it does today.
8. **Fingerprint compare uses what phase 2 stored** (path/size/mtime). Directory sources carry the documented blind spot; an unseen change resumes and re-parses, and deterministic stems keep the skip set consistent unless conversation boundaries moved — decision 38 fires on every mismatch the stat can see. `message_count` stays null (nothing fills it; out of scope, listed for #230).
9. **A failed (non-cancelled) extract still completes the session as `failed`** — active-session lookup only returns running sessions, so a resumable failed-write would need server changes decision 36 does not ask for; restart-with-settings already covers it.
10. **`attachments_saved` counts staged records without path-dedupe on the engine path** (SBR's old path deduped). One semantics for all exporters; the count feeds a log line, not the vault.
11. **Stray `{name}.{seq}.tmp` files from a crash are left in place** — they are invisible to the media pass and the push (nothing references them) and are removed with the folder on decline; sweeping them on resume risks deleting a concurrent writer's in-flight temp.
12. **Progress lines during the drain are attachment lines only** (plus the one `Preparing …` banner); per-conversation `  preparing n/N` count lines would be misfiled by the desktop's log scraper once interleaved with attachment lines. The byte counter is the write phase's honest progress (decision 8).
