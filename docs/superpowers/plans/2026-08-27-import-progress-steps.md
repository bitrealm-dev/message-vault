# Four-step Import Progress Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show four honest Import steps (parse, attachments with file count plus size, Preparing messages, upload) by deferring attachment I/O to a shared runner, then writing each `.jsonl` once.

**Architecture:** Parse records pending attachment jobs and does not write media. `run_attachment_jobs` in `message-vault-io-core` loads, copies or converts, and fills path/hash/size. `FormatSink::finish` no longer runs ffmpeg. Tauri events use `parse` / `attachments` / `prepare` / `upload`. Vault schema version 2 replaces `convert_ms` with `attachments_ms` and `prepare_ms`.

**Tech Stack:** Rust 2024 workspace crates, Tauri extract events, React 19 + TypeScript in `web/`, Vitest, SQLite/Postgres schema in `schema/sql/`.

**Spec:** `docs/superpowers/specs/2026-08-27-import-progress-steps-design.md`

## Global Constraints

- Four steps on every Import, including Skip attachments.
- Order is parse → attachments → prepare → upload. Do not write `.jsonl` before attachments.
- Attachment detail always contains the word `attachments` plus a file count and a size (example: `Copied 120/840 attachments (1.2 GB / 4.0 GB)`).
- Convert/Compress uses `Converted`. Skip uses `Skipped`.
- Progress and issue step names: `parse`, `attachments`, `prepare`, `upload`. Do not produce `convert`.
- Stored times: `parse_ms`, `attachments_ms`, `prepare_ms`, `upload_ms`. Remove `convert_ms`.
- `SCHEMA_VERSION` becomes 2. An old database is rebuilt empty. No mapping of old import history.
- Shared runner lives in `message-vault-io-core`. Every desktop exporter uses it. CLI exporters that share those crates use the same order.
- `FormatSink::finish` does not run convert/compress. Obfuscation may still run at write time.
- Import is desktop-only. Prove UI with Vitest, not Playwright.
- Do not change Import Errors grouping rules. Only allowed `step` values change.
- Prefer a real fix over `biome-ignore`. Prefix unused bindings with `_`.
- Never commit to `main`. Work on `fix/import-progress-steps`.
- Product version files stay at the current lockstep value.
- Do not commit `docs/package.json` or `docs/package-lock.json`.
- Do not add personal backups. Use committed fixtures only.

## File map

| File | Responsibility |
|---|---|
| `crates/core/message-vault-io-core/src/attachment_jobs.rs` | Pending-job types, `run_attachment_jobs`, progress struct |
| `crates/core/message-vault-io-core/src/attachment_jobs.rs` tests (in-file `#[cfg(test)]`) | Runner: copy, skip, missing, cancel, progress counts |
| `crates/core/message-vault-io-core/src/lib.rs` | Re-export runner types |
| `crates/libs/ir-format/src/export_transforms.rs` | `apply_transforms` skips `process_attachments_dir` |
| `crates/libs/ir-format/src/format_sink.rs` | Finish writes documents; no ffmpeg |
| Desktop exporters listed below | Parse records jobs; call runner; then `FormatSink` |
| `src-tauri/src/commands/events.rs` | `bytes_done` / `bytes_total` on progress |
| `src-tauri/src/commands/progress.rs` | Stages `Parse` / `Attachments` / `Prepare` |
| `schema/sql/accounts.sql`, `schema/sql/pg_accounts.sql` | Timing columns |
| `crates/vault/server/src/db/schema.rs` | `SCHEMA_VERSION = 2` |
| Vault import complete / history Rust + web types | New field names |
| `web/src/lib/attachmentProgressCopy.ts` | Detail line with file count + size |
| `web/src/screens/import/useImportJob.ts` | Four steps and four durations |
| `web/src/components/import/ImportSummaryPanel.tsx` | History four steps |
| `CHANGELOG.md` | Unreleased Changed note dated 2026-08-27 |

Exporter persist call sites to stop writing during parse:

- `crates/exporters/imessage-ir-exporter/src/emit.rs` (`persist_attachment` in `mail_message_to_ir`)
- `crates/exporters/whatsapp-exporter/src/emit.rs` (`copy_media`)
- `crates/exporters/sms-backup-plus-exporter/src/attachments_emit.rs`
- `crates/exporters/go-sms-pro-exporter/src/attachments_emit.rs`
- `crates/exporters/imazing-exporter/src/attachments.rs` (`copy_if_missing`)
- `crates/exporters/sms-backup-restore-exporter` and `openextract-exporter` (same pattern as their emit helpers)

---

### Task 0: Branch and record the plan

**Files:**
- Create: this plan at `docs/superpowers/plans/2026-08-27-import-progress-steps.md`
- Existing: `docs/superpowers/specs/2026-08-27-import-progress-steps-design.md`

**Interfaces:**
- Consumes: locked spec on disk
- Produces: git branch `fix/import-progress-steps` with spec + plan committed

- [ ] **Step 1: Confirm or create the branch**

```bash
cd /home/mbeisser/repo/message-vault
git fetch
git branch --show-current
```

If the current branch is `docs/import-progress-steps-design` (or already has the spec), create the implementation branch from it:

```bash
git checkout -b fix/import-progress-steps
```

If the current branch is `main`, stop and branch from the spec commit.

Expected: `git branch --show-current` prints `fix/import-progress-steps`.

- [ ] **Step 2: Commit this plan** (skip if `git status` already shows it committed)

```bash
git add docs/superpowers/plans/2026-08-27-import-progress-steps.md
git commit -m "$(cat <<'EOF'
docs: add four-step import progress plan

The spec locks parse, attachments, prepare, then upload. This plan
is the TDD sequence for the runner, exporters, schema, and UI.
EOF
)"
```

Do not stage `docs/package.json` or `docs/package-lock.json`.

---

### Task 1: `run_attachment_jobs` helper

**Files:**
- Create: `crates/core/message-vault-io-core/src/attachment_jobs.rs`
- Modify: `crates/core/message-vault-io-core/src/lib.rs`

**Interfaces:**
- Consumes: `IrAttachment` from `message_ir`; `MediaMode` and `CompressOptions` from `media`; `attachment_dest_name` / `write_if_missing` from `attachments`
- Produces:

```rust
pub struct AttachmentProgress {
    pub done: usize,
    pub total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

pub struct AttachmentJob<'a> {
    pub attachment: &'a mut IrAttachment,
    pub timestamp_unix_ms: i64,
    pub size_hint: Option<u64>,
}

/// `load(i)` returns `Ok(None)` when the source is missing.
/// `Ok(Some(bytes))` is the file to stage.
pub fn run_attachment_jobs(
    jobs: &mut [AttachmentJob<'_>],
    attachments_dir: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
    mut load: impl FnMut(usize) -> Result<Option<Vec<u8>>, String>,
    mut on_progress: impl FnMut(AttachmentProgress),
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<(), String>
```

Rules:

- `MediaMode::Disabled`: do not call `load`. Set `missing_reason` to `skipped` on every job. Emit one progress event with `done == total`, `bytes_done == 0`, `bytes_total == 0`.
- `MediaMode::Clone`: for each job, check cancel, call `load(i)`. `Ok(None)` or empty bytes → `missing_reason = "file_missing"`, leave path/digest unset. `Ok(Some(bytes))` → SHA-256, `attachment_dest_name`, write under `attachments_dir`, set `path` to `attachments/{name}`, `digest_sha256`, `size_bytes`, clear `missing_reason`.
- `bytes_total` starts as the sum of `size_hint` values that are `Some`. When a loaded file has no hint, add `bytes.len()` to `bytes_total` after the load. `total` (file count) is `jobs.len()` and does not change.
- After all clone writes, if `mode` is `Convert` or `Compress`, call `media::process_attachments_dir(output_dir_parent, mode, compress)` where `output_dir_parent` is the parent of `attachments_dir`. Apply the remap to each job’s `path`. Refresh digest and `size_bytes` from the file on disk (same idea as `refresh_missing_attachment_digests` in ir-format). A per-file ffmpeg failure is recorded on that attachment (`missing_reason` or leave path and let the caller log); do not abort remaining jobs unless the directory is unwritable.
- Disk-full / cannot create `attachments_dir` returns `Err`.
- Cancel: if `cancel` is `Some` and `true`, return `Err("canceled".into())` before starting the next job. Do not start convert/compress if cancel is set after clone writes.

- [ ] **Step 1: Write the failing tests**

Add at the bottom of the new module (or in `src/attachment_jobs.rs` under `#[cfg(test)]`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use media::{CompressOptions, MediaMode};
    use message_ir::IrAttachment;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    fn empty_att(name: &str) -> IrAttachment {
        IrAttachment {
            path: None,
            original_name: Some(name.into()),
            mime_type: Some("image/jpeg".into()),
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
            size_bytes: None,
            missing_reason: None,
            bytes: None,
        }
    }

    #[test]
    fn clone_writes_file_and_fills_hash() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        let mut att = empty_att("photo.jpg");
        let bytes = b"hello-photo";
        let progress = Mutex::new(Vec::new());
        {
            let mut jobs = [AttachmentJob {
                attachment: &mut att,
                timestamp_unix_ms: 1_609_459_200_000,
                size_hint: Some(bytes.len() as u64),
            }];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                MediaMode::Clone,
                &CompressOptions::default(),
                |_| Ok(Some(bytes.to_vec())),
                |p| progress.lock().unwrap().push(p),
                None,
            )
            .unwrap();
        }
        assert!(att.path.as_deref().unwrap().starts_with("attachments/"));
        assert_eq!(att.size_bytes, Some(bytes.len() as u64));
        assert!(att.digest_sha256.as_ref().unwrap().len() == 64);
        let dest = dir.path().join(att.path.as_ref().unwrap());
        assert_eq!(std::fs::read(dest).unwrap(), bytes);
        let last = progress.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last.done, 1);
        assert_eq!(last.total, 1);
        assert_eq!(last.bytes_done, bytes.len() as u64);
        assert_eq!(last.bytes_total, bytes.len() as u64);
    }

    #[test]
    fn disabled_skips_without_loading() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        let mut att = empty_att("photo.jpg");
        let loaded = AtomicBool::new(false);
        {
            let mut jobs = [AttachmentJob {
                attachment: &mut att,
                timestamp_unix_ms: 0,
                size_hint: Some(99),
            }];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                MediaMode::Disabled,
                &CompressOptions::default(),
                |_| {
                    loaded.store(true, Ordering::SeqCst);
                    Ok(Some(b"x".to_vec()))
                },
                |_| {},
                None,
            )
            .unwrap();
        }
        assert!(!loaded.load(Ordering::SeqCst));
        assert_eq!(att.missing_reason.as_deref(), Some("skipped"));
        assert!(att.path.is_none());
        assert!(!att_dir.exists() || std::fs::read_dir(&att_dir).unwrap().next().is_none());
    }

    #[test]
    fn missing_source_is_file_missing_and_continues() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        let mut a = empty_att("a.jpg");
        let mut b = empty_att("b.jpg");
        {
            let mut jobs = [
                AttachmentJob {
                    attachment: &mut a,
                    timestamp_unix_ms: 0,
                    size_hint: None,
                },
                AttachmentJob {
                    attachment: &mut b,
                    timestamp_unix_ms: 0,
                    size_hint: Some(4),
                },
            ];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                MediaMode::Clone,
                &CompressOptions::default(),
                |i| {
                    if i == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(b"data".to_vec()))
                    }
                },
                |_| {},
                None,
            )
            .unwrap();
        }
        assert_eq!(a.missing_reason.as_deref(), Some("file_missing"));
        assert!(b.path.is_some());
    }

    #[test]
    fn cancel_stops_before_next_job() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        let mut a = empty_att("a.jpg");
        let mut b = empty_att("b.jpg");
        let cancel = AtomicBool::new(false);
        let err = {
            let mut jobs = [
                AttachmentJob {
                    attachment: &mut a,
                    timestamp_unix_ms: 0,
                    size_hint: Some(1),
                },
                AttachmentJob {
                    attachment: &mut b,
                    timestamp_unix_ms: 0,
                    size_hint: Some(1),
                },
            ];
            run_attachment_jobs(
                &mut jobs,
                &att_dir,
                MediaMode::Clone,
                &CompressOptions::default(),
                |i| {
                    if i == 0 {
                        cancel.store(true, Ordering::SeqCst);
                    }
                    Ok(Some(b"x".to_vec()))
                },
                |_| {},
                Some(&cancel),
            )
            .unwrap_err()
        };
        assert_eq!(err, "canceled");
        assert!(a.path.is_some());
        assert!(b.path.is_none());
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

```bash
cd /home/mbeisser/repo/message-vault
cargo test -p message-vault-io-core --lib attachment_jobs
```

Expected: FAIL because `attachment_jobs` does not exist or `run_attachment_jobs` is not defined.

- [ ] **Step 3: Write the minimal runner and export it**

Create `attachment_jobs.rs` implementing the rules above. Use `sha2::Sha256` (add `sha2` to `message-vault-io-core` `Cargo.toml` if it is not already a dependency; `message-ir` / other crates already use it — add `sha2 = "0.10"` to this crate). Write files with a `.tmp` + rename like iMessage `persist_attachment`.

In `lib.rs`:

```rust
pub mod attachment_jobs;
pub use attachment_jobs::{AttachmentJob, AttachmentProgress, run_attachment_jobs};
```

- [ ] **Step 4: Run the tests and confirm they pass**

```bash
cargo test -p message-vault-io-core --lib attachment_jobs
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/message-vault-io-core/src/attachment_jobs.rs crates/core/message-vault-io-core/src/lib.rs crates/core/message-vault-io-core/Cargo.toml
git commit -m "$(cat <<'EOF'
feat(core): add shared attachment job runner

Import needs attachment copy after parse. This runner writes files
and fills hashes before conversation documents are written.
EOF
)"
```

---

### Task 2: Stop convert/compress in `FormatSink::finish`

**Files:**
- Modify: `crates/libs/ir-format/src/export_transforms.rs` (`apply_transforms`)
- Test: existing `apply_transforms` tests in that file; add one that clone-mode files are not remapped when media is Convert but attachments are already final — **or** change `apply_transforms` so it never calls `process_attachments_dir`

**Interfaces:**
- Consumes: documents whose attachments already have final path/hash from the runner
- Produces: `apply_transforms` still obfuscates and still `clear_attachments_when_disabled`. It does **not** call `media::process_attachments_dir`. Convert/compress is the runner’s job.

- [ ] **Step 1: Write a failing test in `export_transforms.rs` tests**

Add a test that writes a small non-image file as `attachments/keep.bin`, sets `transforms.media = MediaMode::Convert`, calls `apply_transforms`, and asserts the file is still `keep.bin` (no ffmpeg remap). If a Convert test today expects remap at finish, change that test to expect no remap and note that convert is covered by the runner.

- [ ] **Step 2: Run the test and confirm it fails**

```bash
cargo test -p message-ir-format apply_transforms
```

Expected: FAIL because `process_attachments_dir` still rewrites media (or the new assertion is not met).

- [ ] **Step 3: Remove the `process_attachments_dir` call from `apply_transforms`**

Keep obfuscation and `clear_attachments_when_disabled`. Leave remap helpers in the file for the runner to reuse if needed; `apply_media_remap` can stay `pub(crate)` unused or be moved later. Do not delete `refresh_missing_attachment_digests` if the runner needs the same logic — copy the small refresh into the runner instead of calling a private ir-format function.

- [ ] **Step 4: Run ir-format tests**

```bash
cargo test -p message-ir-format
```

Expected: PASS. Fix any finish/convert tests that assumed ffmpeg at write time.

- [ ] **Step 5: Commit**

```bash
git add crates/libs/ir-format/src/export_transforms.rs crates/libs/ir-format/src/format_sink.rs
git commit -m "$(cat <<'EOF'
fix(ir-format): do not convert media when writing conversations

Attachment hashes must be final before .jsonl is written. Convert
now runs in the shared runner, not in FormatSink::finish.
EOF
)"
```

---

### Task 3: iMessage parse records jobs, then runs the runner

**Files:**
- Modify: `crates/exporters/imessage-ir-exporter/src/emit.rs`
- Modify: `crates/exporters/imessage-ir-exporter/src/attachments_emit.rs`
- Test: existing persist tests; add an emit test that after the message loop and before the runner, `attachments/` has no files (or only empty dir)

**Interfaces:**
- Consumes: `run_attachment_jobs`, `AttachmentJob`
- Produces: `mail_message_to_ir` no longer calls `persist_attachment` for JSONL. It leaves `IrAttachment` with `original_name` / `mime_type` and no path. After all messages are collected into documents (or a vec of pending attachments parallel to messages), `run_export` calls `run_attachment_jobs` with a `load` that calls `load_attachment_bytes`. Then existing `sink.write_document` / `finish` runs.

Parse must not call `load_attachment_bytes` during `collect_mail_parts_and_attachments`. Store enough to load later: keep `Attachment` metadata (resolved path / transfer_name / mime / size if the DB has it) on the pending job. Handwriting SVG can stay as in-memory bytes on a job with `size_hint = Some(svg.len())` and a load closure that returns those bytes.

After parse, `attachments/` is empty (dir may exist from `open_prepared`). After the runner, files exist and IR attachments have hashes. Then write `.jsonl`.

- [ ] **Step 1: Write a failing unit test** that `mail_message_to_ir` / collect does not create files under `attachments/` when `copy_attachments` is true (use the existing persist test fixture style in `emit.rs`).

- [ ] **Step 2: Run the test and confirm it fails**

```bash
cargo test -p imessage-ir-exporter persist_attachment_uses_temp_then_rename
cargo test -p imessage-ir-exporter --lib
```

Expected: existing persist test may still pass. The new “no files during collect” test FAIL.

- [ ] **Step 3: Defer persist; call `run_attachment_jobs` after the message loop and before the write loop in `run_export`**

Do not load attachment bytes in `collect_mail_parts_and_attachments`. Build `MailAttachment` with empty `bytes` plus metadata. The runner load closure uses `load_attachment_bytes` with session + stored `Attachment` (keep a `Vec` of load keys next to jobs).

Emit log lines the Tauri parser will map later:

- After jobs are known: nothing extra required if the runner’s `on_progress` logs `  …{done}/{total} attachments` and a second token the UI can ignore — **prefer** `session.options.emit_log(format!("  attachments {done}/{total} {bytes_done}/{bytes_total}"))` so Task 5 can parse it.

- Change the write banner from `Writing N conversation file(s)...` to `Preparing N conversation file(s)...` and `  wrote` lines to `  preparing {written}/{total}`.

- [ ] **Step 4: Run iMessage exporter tests**

```bash
cargo test -p imessage-ir-exporter
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/exporters/imessage-ir-exporter
git commit -m "$(cat <<'EOF'
feat(imessage): copy attachments after parse

Attachment decrypt and copy ran during the message loop, so the
UI labeled that wait as Parse. The runner now runs before .jsonl.
EOF
)"
```

---

### Task 4: WhatsApp, SMS, iMazing, OpenExtract, GO SMS

**Files:**
- Modify: `crates/exporters/whatsapp-exporter/src/emit.rs` (stop `copy_media` during scan; queue source `PathBuf`; run runner after chats are built, before `sink.write_document`)
- Modify: `crates/exporters/sms-backup-plus-exporter/src/attachments_emit.rs` and `emit.rs`
- Modify: `crates/exporters/go-sms-pro-exporter/src/attachments_emit.rs`
- Modify: `crates/exporters/imazing-exporter/src/attachments.rs` and `emit.rs`
- Modify: `crates/exporters/sms-backup-restore-exporter/src/emit.rs` (and its attachment helper if any)
- Modify: `crates/exporters/openextract-exporter` emit/attachment helper
- Test: existing `convert_smoke` / `attachments_saved` tests in each crate

**Interfaces:**
- Consumes: same `run_attachment_jobs` as Task 1
- Produces: each `run` / `emit` function copies zero files during parse; `attachments_saved` counts successful runner writes

For WhatsApp, `copy_attachments == false` (skip) still builds metadata-only attachments and calls the runner with `MediaMode::Disabled`.

Load closure for path sources: `std::fs::read(path).ok()` mapped to `Ok(Some)` / `Ok(None)`.

- [ ] **Step 1: For each crate, run the existing attachment smoke test and confirm it still expects files after a full `run` (it should keep passing after the change). Add one assertion where cheap: after building messages and before the runner, `attachments/` is empty.**

- [ ] **Step 2: Implement deferral + runner in that crate**

- [ ] **Step 3: Run that crate’s tests**

```bash
cargo test -p whatsapp-exporter
cargo test -p sms-backup-plus-exporter
cargo test -p go-sms-pro-exporter
cargo test -p imazing-exporter
cargo test -p sms-backup-restore-exporter
cargo test -p openextract-exporter
```

Expected: PASS for each crate after its change. Commit per crate (or one commit if the diffs are small):

```bash
git commit -m "$(cat <<'EOF'
feat(exporters): run shared attachment pass after parse

Desktop Import copies media in the message loop. Each exporter
now records jobs and lets the shared runner write files.
EOF
)"
```

Use one commit if that is cleaner; do not leave a crate half-migrated.

---

### Task 5: Tauri progress events

**Files:**
- Modify: `src-tauri/src/commands/events.rs`
- Modify: `src-tauri/src/commands/progress.rs`
- Test: `progress.rs` `extract_progress_parser_tracks_parse_and_convert` (rename to prepare)

**Interfaces:**
- Consumes: log lines from exporters
- Produces:

```rust
pub struct ExtractProgressEvent {
    pub step: String, // parse | attachments | prepare | upload
    pub done: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_done: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}
```

Stages: `Parse`, `Attachments`, `Prepare`.

- Line containing `attachments` and `N/M` file counts (and optional `bytes_done/bytes_total`) → `step: "attachments"`, set stage Attachments. Parse `attachments 120/840 1288490188/4294967296` as file ratio then byte ratio.
- `Preparing N conversation file(s)` → `step: "prepare"`, `status: None` (do not use `included_in_extract`).
- `preparing N/M` or `wrote N/M` after that banner → `prepare`.
- `…N/M` without `attachments` → `parse` while stage is Parse.

Remove `included_in_extract` for the write banner. Remove `ExtractProgressStage::Convert`.

- [ ] **Step 1: Update the parser test**

Replace the convert banner assertions:

```rust
let banner = extract_progress_from_log("Preparing 3 conversation file(s)...", &stage).unwrap();
assert_eq!(banner.step, "prepare");

let attachments = extract_progress_from_log("  attachments 2/3 100/500", &stage).unwrap();
assert_eq!(attachments.step, "attachments");
assert_eq!(attachments.done, 2);
assert_eq!(attachments.total, 3);
assert_eq!(attachments.bytes_done, Some(100));
assert_eq!(attachments.bytes_total, Some(500));

let prepare = extract_progress_from_log("  preparing 2/3", &stage).unwrap();
assert_eq!(prepare.step, "prepare");
```

Keep ignoring `[1/5] Deriving backup keys...`.

- [ ] **Step 2: Run the test and confirm it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml extract_progress_parser
```

Expected: FAIL (old convert banner / missing fields).

- [ ] **Step 3: Implement parser + event fields. Update every `ExtractProgressEvent { ... }` construction in `src-tauri` (push.rs tests too) to include `bytes_done: None, bytes_total: None` or `..` if you add `Default`.**

- [ ] **Step 4: Run Tauri command tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/events.rs src-tauri/src/commands/progress.rs src-tauri/src/commands/push.rs
git commit -m "$(cat <<'EOF'
feat(tauri): emit parse, attachments, and prepare progress

The UI treated conversation writes as convert. Events now match
the four extract passes, including attachment byte counts.
EOF
)"
```

---

### Task 6: Vault schema version 2 and import timings

**Files:**
- Modify: `schema/sql/accounts.sql`, `schema/sql/pg_accounts.sql`
- Modify: `tests/fixtures/schema/v0-vault.sql` (same columns)
- Modify: `crates/vault/server/src/db/schema.rs` (`SCHEMA_VERSION` 1 → 2)
- Modify: `crates/vault/server/src/db/vault_imports.rs`
- Modify: `crates/vault/server/src/import/mod.rs`
- Modify: `crates/vault/server/src/guest_clone.rs`
- Modify: `crates/vault/server/src/server.rs` tests
- Do not edit `web-next/` unless a workspace test fails because of it

**Interfaces:**
- Consumes: complete-import JSON body
- Produces: columns and serde fields `parse_ms`, `attachments_ms`, `prepare_ms`, `upload_ms` only. No `convert_ms`. Issue `step` is a free string (already).

Replace every `convert_ms` bind/select/struct field. SQL comments: attachments time vs preparing-messages time.

- [ ] **Step 1: Change SQL and `SCHEMA_VERSION`. Run schema tests expecting version 2**

```bash
# After edits:
rg -n convert_ms schema/sql crates/vault tests/fixtures/schema
```

Expected: no `convert_ms` in those trees.

- [ ] **Step 2: Update Rust structs and tests that use `convert_ms: Some(22_000)` to `attachments_ms: Some(22_000), prepare_ms: Some(4_000)` (or any two numbers that sum reasonably).**

- [ ] **Step 3: Run vault tests**

```bash
cargo test -p message-vault-server
```

Expected: PASS. Schema tests that assert `SCHEMA_VERSION` must expect `2`.

- [ ] **Step 4: Commit**

```bash
git add schema/sql/accounts.sql schema/sql/pg_accounts.sql tests/fixtures/schema/v0-vault.sql \
  crates/vault/server/src/db/schema.rs crates/vault/server/src/db/vault_imports.rs \
  crates/vault/server/src/import/mod.rs crates/vault/server/src/guest_clone.rs \
  crates/vault/server/src/server.rs
git commit -m "$(cat <<'EOF'
feat(vault): store attachment and prepare import times

convert_ms mixed the short .jsonl write with attachment work.
New vaults store attachments_ms and prepare_ms instead.
EOF
)"
```

---

### Task 7: Web progress copy and types

**Files:**
- Create: `web/src/lib/attachmentProgressCopy.ts`
- Create: `web/src/lib/attachmentProgressCopy.test.ts`
- Modify: `web/src/lib/types.ts`
- Modify: `web/src/lib/attachmentStepCopy.ts` only if done-detail must mention attachments (keep titles)

**Interfaces:**
- Consumes: `ImportProgressEvent` with optional `bytes_done` / `bytes_total`
- Produces:

```ts
export type ImportProgressEvent = {
  step: "parse" | "attachments" | "prepare" | "upload";
  done: number;
  total: number;
  bytes_done?: number;
  bytes_total?: number;
  status?: string;
};

export type ImportIssueEvent = {
  kind: "error" | "skip";
  step: "parse" | "attachments" | "prepare" | "upload";
  item: string;
  reason: string;
};

export function formatAttachmentProgress(args: {
  mode: AttachmentMediaMode;
  done: number;
  total: number;
  bytesDone: number;
  bytesTotal: number;
}): string
```

Use existing `formatBytes` from `web/src/screens/settings/storage/storageUtils.ts` (or move `formatBytes` to `web/src/lib/formatBytes.ts` if importing storageUtils from lib is backwards — **do not** import a settings module from a generic lib if that creates a cycle; copy the same `formatBytes` into `attachmentProgressCopy.ts` only if needed, or import from `storageUtils` if tests already do).

Copy mode: `Copied ${done}/${total} attachments (${formatBytes(bytesDone)} / ${formatBytes(bytesTotal)})`.  
Convert/compress: `Converted …`.  
Skip: `Skipped …` with bytes shown as `0 B / 0 B` when both are 0.

- [ ] **Step 1: Write `attachmentProgressCopy.test.ts`**

```ts
import { describe, expect, it } from "vitest";
import { formatAttachmentProgress } from "./attachmentProgressCopy";

describe("formatAttachmentProgress", () => {
  it("says attachments and includes file count and size for copy", () => {
    const line = formatAttachmentProgress({
      mode: "copy",
      done: 120,
      total: 840,
      bytesDone: 1.2 * 1024 * 1024 * 1024,
      bytesTotal: 4 * 1024 * 1024 * 1024,
    });
    expect(line).toContain("attachments");
    expect(line).toContain("120/840");
    expect(line).toMatch(/1(\.0|\.2)? GB/);
    expect(line).toContain("4 GB");
    expect(line.startsWith("Copied")).toBe(true);
  });

  it("uses Converted for convert and Skipped for skip", () => {
    expect(
      formatAttachmentProgress({
        mode: "convert",
        done: 1,
        total: 1,
        bytesDone: 0,
        bytesTotal: 0,
      }),
    ).toMatch(/^Converted 1\/1 attachments/);
    expect(
      formatAttachmentProgress({
        mode: "skip",
        done: 0,
        total: 0,
        bytesDone: 0,
        bytesTotal: 0,
      }),
    ).toBe("Skipped 0/0 attachments (0 B / 0 B)");
  });
});
```

- [ ] **Step 2: Run the test and confirm it fails**

```bash
cd /home/mbeisser/repo/message-vault/web
npm test -- src/lib/attachmentProgressCopy.test.ts
```

Expected: FAIL (module missing).

- [ ] **Step 3: Implement helper + update `types.ts`**

- [ ] **Step 4: Run the test**

```bash
npm test -- src/lib/attachmentProgressCopy.test.ts src/lib/attachmentStepCopy.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/attachmentProgressCopy.ts web/src/lib/attachmentProgressCopy.test.ts web/src/lib/types.ts
git commit -m "$(cat <<'EOF'
feat(web): format attachment import progress

The middle step must show a file count, a size, and the word
attachments so the wait matches the real copy pass.
EOF
)"
```

---

### Task 8: `useImportJob` four steps and four durations

**Files:**
- Modify: `web/src/screens/import/useImportJob.ts`
- Modify: `web/src/screens/import/ImportProgressView.test.tsx`
- Test: add or extend a unit test if `useImportJob` is tested; otherwise update `ImportProgressView` fixtures to four steps

**Interfaces:**
- Consumes: `formatAttachmentProgress`, new event steps
- Produces: `initialSteps` length 4:

  1. Parse backup  
  2. `attachmentStepCopy(mode).label`  
  3. Preparing messages  
  4. Upload to vault  

`stepIndexFor`: parse=0, attachments=1, prepare=2, upload=3.

`progressVerb`: parse → `Parsing`, attachments unused (detail comes from `formatAttachmentProgress`), prepare → `Preparing`, upload → `Uploading`.

Timing ref: `attachmentsStartedAt` / `attachmentsEndedAt` / `prepareStartedAt` / `prepareEndedAt` instead of a single `convert*`. After extract: `attachmentsMs` = attachments window; `prepareMs` = prepare window (or extract end minus prepare start).

`complete` body: `parse_ms`, `attachments_ms`, `prepare_ms`, `upload_ms`. No `convert_ms`.

When extract finishes, set step 2 detail to `attachmentStepCopy(mode).doneDetail` only if that string contains `attachments`; otherwise set done detail to `formatAttachmentProgress` with final counts if those were stored on the ref.

Keep a `lastAttachmentProgress` ref updated on `attachments` events so the done line can show the last `Copied N/M attachments (…)` or fall back to `Copied attachments`.

- [ ] **Step 1: Update `ImportProgressView.test.tsx`** so every fixture has four steps including `Preparing messages`. Replace `Copy attachments` as step 2 and insert Preparing messages as step 3.

- [ ] **Step 2: Run the test and confirm it fails** (if the view only renders what it is given, the test change may pass immediately — then add an assertion in a `useImportJob` test file if one exists). Search `useImportJob.test` — if missing, the progress view test is enough for labels; the hook change is verified by TypeScript compile.

```bash
cd /home/mbeisser/repo/message-vault/web
npm test -- src/screens/import/ImportProgressView.test.tsx
```

- [ ] **Step 3: Implement `useImportJob` four-step state and complete payload**

- [ ] **Step 4: Run web tests for import screens**

```bash
npm test -- src/screens/import src/lib/attachmentProgressCopy.test.ts
npx tsc --noEmit
```

Expected: PASS / no type errors.

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/import/useImportJob.ts web/src/screens/import/ImportProgressView.test.tsx
git commit -m "$(cat <<'EOF'
fix(import): show four extract steps while a job runs

Copy attachments finished in a second because it tracked .jsonl
writes. The hook now times attachments and Preparing messages.
EOF
)"
```

---

### Task 9: Summary panel and Settings history

**Files:**
- Modify: `web/src/components/import/ImportSummaryPanel.tsx`
- Modify: `web/src/components/import/ImportSummaryPanel.test.tsx`
- Modify: `web/src/screens/settings/storage/storageUtils.ts`
- Modify: `web/src/screens/settings/storage/storageUtils.test.ts`

**Interfaces:**
- Consumes: `ImportSummaryView.parseMs`, `attachmentsMs`, `prepareMs`, `uploadMs`
- Produces: `historySteps` of four items: Parse backup, Convert/Copy attachments (use `attachmentStepCopy` if mode is on the summary; if mode is unknown, label **Copy attachments** for completed history unless `summary.attachmentMedia` exists — **do not invent a fifth API field**. Use fixed labels: Parse backup, Copy attachments, Preparing messages, Upload to vault. History does not know the form mode. Spec: “Attachment step title still follows the form” on the live job; history can show **Copy attachments** as the second label, or “Attachments” — use **Attachments** as the history-only second label so skip/convert runs are not lied about.

Decision locked here: Settings history second step label is **Attachments** (not Convert attachments). Third is **Preparing messages**.

`toImportSummaryView` reads `attachments_ms` and `prepare_ms`. Duration sum uses all four times.

- [ ] **Step 1: Update storageUtils tests and panel tests** to expect `attachmentsMs` / `prepareMs` and four step labels.

- [ ] **Step 2: Run tests; expect FAIL**

```bash
cd /home/mbeisser/repo/message-vault/web
npm test -- src/components/import/ImportSummaryPanel.test.tsx src/screens/settings/storage/storageUtils.test.ts
```

- [ ] **Step 3: Implement view fields and `historySteps`**

- [ ] **Step 4: Re-run those tests plus `ImportProgressView`**

```bash
npm test -- src/components/import src/screens/import src/screens/settings/storage/storageUtils.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/import/ImportSummaryPanel.tsx web/src/components/import/ImportSummaryPanel.test.tsx \
  web/src/screens/settings/storage/storageUtils.ts web/src/screens/settings/storage/storageUtils.test.ts
git commit -m "$(cat <<'EOF'
fix(import): show four timings on import history

History still labeled the middle time Convert attachments.
It now shows Attachments and Preparing messages separately.
EOF
)"
```

---

### Task 10: Changelog

**Files:**
- Modify: `CHANGELOG.md` under `[Unreleased]` → `### Changed` (add heading if needed)

- [ ] **Step 1: Add**

```md
- 2026-08-27: Desktop Import shows four steps: parse the backup, copy or convert attachments (file count and size), prepare conversation files, then upload. Attachment work no longer appears as an instant second step. Import history stores `attachments_ms` and `prepare_ms` instead of `convert_ms` (vault schema 2; existing databases are rebuilt empty).
```

- [ ] **Step 2: Commit**

```bash
git add CHANGELOG.md
git commit -m "$(cat <<'EOF'
docs: note four-step import progress

Users saw Copy attachments finish in a second. The unreleased
notes describe the real attachment pass and schema 2 timings.
EOF
)"
```

---

## Self-review (spec coverage)

| Spec requirement | Task |
|---|---|
| Four sequential steps, skip still shown | 8, 9 |
| Attachments before `.jsonl` | 1, 2, 3, 4 |
| File count + size + word attachments | 7, 8 |
| Shared runner in core | 1 |
| All desktop exporters | 3, 4 |
| FormatSink no ffmpeg | 2 |
| Events `parse` / `attachments` / `prepare` / `upload` | 5 |
| Four DB times, no `convert_ms`, schema 2 | 6 |
| Errors use new step names | 3–5, 8 (types); grouping unchanged |
| Cancel between attachment jobs | 1 |
| Vitest not Playwright | 7–9 |
| Unknown sizes grow `bytes_total` | 1 |
