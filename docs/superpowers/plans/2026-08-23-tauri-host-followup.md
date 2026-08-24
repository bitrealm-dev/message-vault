# Tauri Host Follow-up Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply the seven Tauri-host audit findings to `src-tauri/` and the CI workflow: share the job scaffolding, wire push's cancel flag, remove the unsound env writes, split extract.rs, document the IPC DTO surface, and gate clippy + tests in CI.

**Architecture:** Two new `pub(crate)` helpers in `commands/jobs.rs` (`reset_and_clone_cancel`, `spawn_job`) collapse the duplicated cancel/spawn/error scaffolding in all four job commands. The log-progress parser moves verbatim to `commands/progress.rs`. The ffmpeg command stops writing `MESSAGE_VAULT_IO_BIN` and uses the media crate's in-process state. `#![warn(missing_docs)]` plus field docs covers the JSON wire contract. The existing `check-tauri` CI job gains clippy and test steps.

**Tech Stack:** Rust edition 2024, Tauri v2 (`src-tauri` is NOT a workspace member — always pass `--manifest-path`), serde, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-08-23-tauri-host-followup-design.md` — the spec is the binding authority; this plan argues from it.

## Global Constraints

- Behavior preservation at the product boundary: event names, payload shapes, summary strings, and command rejections visible to `web/` must not change except as listed in the spec's behavior-delta catalog.
- The rustdoc style guide (`docs/src/content/docs/vault/developer/rustdoc-style.md`) governs all new doc text.
- One implementation PR against `main`; no version bumps; no tag pushes.
- No changes to crates outside `src-tauri/` and `.github/workflows/ci.yml` (the media and whatsapp-exporter env reads stay — they are the CLI users' configuration surface).
- `#![warn(missing_docs)]` is a warning gate, matching the other crates in this series.
- All commands for src-tauri use `--manifest-path src-tauri/Cargo.toml`; it is excluded from the root workspace.

## File Map

- Create: `src-tauri/src/commands/jobs.rs` — cancel reset+clone and spawn+error-routing helpers
- Create: `src-tauri/src/commands/progress.rs` — extract log-progress parser (moved verbatim)
- Modify: `src-tauri/src/commands/mod.rs` — register `jobs` and `progress` modules; complete the event list
- Modify: `src-tauri/src/commands/pull.rs`, `format.rs`, `extract.rs`, `push.rs` — adopt the helpers; push wires cancel
- Modify: `src-tauri/src/commands/ffmpeg.rs` — remove env writes
- Modify: `src-tauri/src/commands/events.rs`, `paths.rs` — field docs
- Modify: `src-tauri/src/lib.rs` — `#![warn(missing_docs)]`
- Modify: `.github/workflows/ci.yml` — clippy + test steps in `check-tauri`
- Modify: `CHANGELOG.md` — one bullet under `[Unreleased]` → `Changed`

Tasks are strictly sequential: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9. Each task ends with clippy-clean, tests-green, committed code.

---

### Task 1: Shared job helpers, adopted in pull and format

**Files:**
- Create: `src-tauri/src/commands/jobs.rs`
- Modify: `src-tauri/src/commands/mod.rs` (register the module)
- Modify: `src-tauri/src/commands/pull.rs` (adopt)
- Modify: `src-tauri/src/commands/format.rs` (adopt)

**Interfaces:**
- Produces: `pub(crate) fn reset_and_clone_cancel(state: &Arc<Mutex<AppState>>) -> Result<CancelFlag, String>` — one lock round-trip: stores `false` in the shared flag, returns a clone. `pub(crate) fn spawn_job<F>(app: AppHandle, run: F) where F: FnOnce() -> anyhow::Result<()> + Send + 'static` — spawns a thread; on `Err` emits `extract:error` with `detail: format!("{err:#}")` and `user_message: None`.
- Consumes: `AppState` (crate::state), `ExtractErrorEvent` (super::events), `CancelFlag` (message_vault_io_core).

- [ ] **Step 1: Create `src-tauri/src/commands/jobs.rs`**

Write the file exactly as follows:

```rust
//! Shared scaffolding for the background job commands (`extract`, `format`,
//! `pull`, `push`).
//!
//! Every job command clears a leftover cancel flag, shares a clone of the
//! flag with its worker thread, spawns the worker, and reports a failed job
//! as an `extract:error` event. These helpers hold that repeated part. What
//! differs per command — building the config, mapping progress events, and
//! shaping the finished summary — stays in the command.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;

use message_vault_io_core::CancelFlag;
use tauri::{AppHandle, Emitter};

use super::events::ExtractErrorEvent;
use crate::state::AppState;

/// Clear a leftover cancel from a previous job and return a clone of the
/// shared flag for the worker thread.
///
/// One lock round-trip replaces the earlier two (reset, then clone), so a
/// `cancel` call cannot slip between them and start the new job cancelled.
///
/// # Errors
///
/// Returns an error if another thread panicked while holding the shared
/// state lock.
pub(crate) fn reset_and_clone_cancel(
    state: &Arc<Mutex<AppState>>,
) -> Result<CancelFlag, String> {
    let st = state.lock().map_err(|e| e.to_string())?;
    st.cancel_flag.store(false, Ordering::SeqCst);
    Ok(st.cancel_flag.clone())
}

/// Spawn the worker thread and report a failed job as an `extract:error`
/// event carrying the full error chain.
pub(crate) fn spawn_job<F>(app: AppHandle, run: F)
where
    F: FnOnce() -> anyhow::Result<()> + Send + 'static,
{
    thread::spawn(move || {
        if let Err(err) = run() {
            let _ = app.emit(
                "extract:error",
                ExtractErrorEvent {
                    detail: format!("{err:#}"),
                    user_message: None,
                },
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_and_clone_returns_a_fresh_false_flag() {
        let state = Arc::new(Mutex::new(AppState::new()));
        let cancel = reset_and_clone_cancel(&state).unwrap();
        assert!(!cancel.load(Ordering::SeqCst));
    }

    #[test]
    fn reset_and_clone_clears_a_previous_cancel_and_shares_the_flag() {
        let state = Arc::new(Mutex::new(AppState::new()));
        state
            .lock()
            .unwrap()
            .cancel_flag
            .store(true, Ordering::SeqCst);
        let cancel = reset_and_clone_cancel(&state).unwrap();
        assert!(!cancel.load(Ordering::SeqCst));
        assert!(Arc::ptr_eq(&cancel, &state.lock().unwrap().cancel_flag));
    }
}
```

- [ ] **Step 2: Register the module in `src-tauri/src/commands/mod.rs`**

Insert `pub mod jobs;` into the module list so it reads:

```rust
pub mod events;
pub mod extract;
pub mod ffmpeg;
pub mod format;
pub mod jobs;
pub mod paths;
pub mod pull;
pub mod push;
```

- [ ] **Step 3: Rewrite `src-tauri/src/commands/pull.rs`**

Replace the entire file with (the doc comments and `PullArgs` are unchanged; only the command body and imports change):

```rust
//! `pull` command — download messages from a Message Vault server.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::Emitter;
use vault_pull::{ProgressEvent, VaultPullConfig, run as run_pull};

use super::jobs::{reset_and_clone_cancel, spawn_job};
use crate::state::AppState;

/// User-facing parameters for the `pull` command.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullArgs {
    pub base_url: String,
    pub username: String,
    pub key: String,
    pub out_dir: String,
    pub query: String,
    pub skip_attachments: bool,
}

/// Ask this process to download conversations from a vault server.
///
/// Returns as soon as the background thread starts. Log lines and the final
/// summary use the same `extract:log` / `extract:finished` / `extract:error`
/// events as Extract.
///
/// # Errors
///
/// Returns an error if another thread panicked while holding the shared
/// state lock. Failures during the download are sent as `extract:error`.
#[tauri::command]
pub async fn pull(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
    args: PullArgs,
) -> Result<(), String> {
    let cancel = reset_and_clone_cancel(&state)?;

    let app_handle = app.clone();
    spawn_job(app, move || {
        let cfg = VaultPullConfig {
            out_dir: PathBuf::from(&args.out_dir),
            base_url: args.base_url,
            username: args.username,
            key: args.key,
            query: args.query,
            after: None,
            before: None,
            source: None,
            skip_attachments: args.skip_attachments,
            page_limit: 100,
            expected_messages: None,
            cancel: Some(cancel),
            asset_download_workers: 8,
            force: false,
            journal_path: None,
        };

        let mut progress = |event: ProgressEvent| match event {
            ProgressEvent::Log(line) => {
                let _ = app_handle.emit("extract:log", line);
            }
            ProgressEvent::Auth { .. } => {}
            ProgressEvent::Page {
                messages,
                total_so_far,
            } => {
                let _ = app_handle.emit(
                    "extract:log",
                    format!("{messages} messages (total: {total_so_far})"),
                );
            }
            ProgressEvent::Done(_) => {}
        };

        match run_pull(&cfg, Some(&mut progress)) {
            Ok(report) => {
                let summary = format!(
                    "Pull complete: {} messages, {} conversations",
                    report.messages, report.conversations,
                );
                let _ = app_handle.emit("extract:finished", summary);
            }
            Err(err) => return Err(err),
        }
        Ok(())
    });

    Ok(())
}
```

- [ ] **Step 4: Rewrite `src-tauri/src/commands/format.rs`**

Replace the entire file with:

```rust
//! `format` command — convert an existing extract folder to another format.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use message_vault_io_core::{
    ExporterConfig, FormatConfig, LogSink, MediaConfig, OutputFormat, SourceConfig,
};
use tauri::Emitter;

use super::jobs::{reset_and_clone_cancel, spawn_job};
use super::last_log_line_or;
use crate::state::AppState;

/// Ask this process to rewrite an extract folder in a different file format.
///
/// Returns as soon as the background thread starts. Log lines and the final
/// summary use the same `extract:log` / `extract:finished` / `extract:error`
/// events as Extract, so the UI can reuse one progress view.
///
/// # Errors
///
/// Returns an error if `output_format` is not one of json, jsonl, csv, eml,
/// mbox, or xml, or if another thread panicked while holding the shared
/// state lock. Failures during conversion are sent as `extract:error`.
#[tauri::command]
pub async fn format(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
    input_dir: String,
    output_dir: String,
    output_format: String,
) -> Result<(), String> {
    let fmt = match output_format.as_str() {
        "json" => OutputFormat::Json,
        "jsonl" => OutputFormat::Jsonl,
        "csv" => OutputFormat::Csv,
        "eml" => OutputFormat::Eml,
        "mbox" => OutputFormat::Mbox,
        "xml" => OutputFormat::Xml,
        _ => return Err(format!("unsupported output format '{output_format}'")),
    };

    let cancel = reset_and_clone_cancel(&state)?;

    let app_handle = app.clone();
    spawn_job(app, move || {
        let log_app = app_handle.clone();
        let config = ExporterConfig {
            inputs: vec![PathBuf::from(&input_dir)],
            output: PathBuf::from(&output_dir),
            date_range: Default::default(),
            timezone: None,
            contacts: None,
            obfuscate: Default::default(),
            media: MediaConfig::default(),
            cancel: Some(cancel),
            log: Some(LogSink::new(move |line: &str| {
                let _ = log_app.emit("extract:log", line.to_string());
            })),
            output_format: fmt,
            source: SourceConfig::Format(FormatConfig {}),
        };

        match message_reexport::run(&config) {
            Ok(run_result) => {
                let summary =
                    last_log_line_or(&run_result.messages, "Format conversion complete.");
                for line in run_result.messages {
                    let _ = app_handle.emit("extract:log", line);
                }
                let _ = app_handle.emit("extract:finished", summary);
            }
            Err(err) => return Err(err),
        }
        Ok(())
    });

    Ok(())
}
```

- [ ] **Step 5: Format, lint, and test**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`
Then: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: exit 0 (the new helpers are exercised by both commands and by their tests, so no dead-code warnings).
Then: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: 5 passed (2 new jobs.rs tests + the 3 existing).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/jobs.rs src-tauri/src/commands/mod.rs \
  src-tauri/src/commands/pull.rs src-tauri/src/commands/format.rs
git commit -m "refactor(tauri): share cancel/spawn/error scaffolding across pull and format"
```

---

### Task 2: Move the log-progress parser to its own module

**Files:**
- Create: `src-tauri/src/commands/progress.rs`
- Modify: `src-tauri/src/commands/extract.rs` (delete the moved section, fix imports)
- Modify: `src-tauri/src/commands/mod.rs` (register the module)

**Interfaces:**
- Produces: `pub(crate) enum ExtractProgressStage { Parse, Convert }` and `pub(crate) fn extract_progress_from_log(line: &str, stage: &Arc<Mutex<ExtractProgressStage>>) -> Option<ExtractProgressEvent>` in `super::progress`.
- Consumes: `ExtractProgressEvent` (super::events).

- [ ] **Step 1: Create `src-tauri/src/commands/progress.rs`**

The content is moved verbatim from `extract.rs` (the enum, `extract_progress_from_log`, and the five private helpers), with the module doc added and visibility adjusted as shown:

```rust
//! Progress parsing for `extract` log lines.
//!
//! Exporters write log lines with progress counts. This module turns the
//! lines that carry counts into [`ExtractProgressEvent`]s the UI can use.

use std::sync::{Arc, Mutex};

use super::events::ExtractProgressEvent;

/// Whether log lines are still about reading the backup, or already about
/// writing conversation files.
#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum ExtractProgressStage {
    Parse,
    Convert,
}

/// Turn an exporter log line into a progress event, if the line has counts.
pub(crate) fn extract_progress_from_log(
    line: &str,
    stage: &Arc<Mutex<ExtractProgressStage>>,
) -> Option<ExtractProgressEvent> {
    if is_writing_conversation_files_banner(line) {
        if let Ok(mut current_stage) = stage.lock() {
            *current_stage = ExtractProgressStage::Convert;
        }
        return Some(ExtractProgressEvent {
            step: "convert".into(),
            done: 0,
            total: 0,
            status: Some("included_in_extract".into()),
        });
    }

    let (done, total) = extract_progress_ratio(line)?;

    let current_stage = match stage.lock() {
        Ok(guard) => *guard,
        Err(_) => ExtractProgressStage::Parse,
    };
    let step = match current_stage {
        ExtractProgressStage::Parse => "parse",
        ExtractProgressStage::Convert => "convert",
    };

    Some(ExtractProgressEvent {
        step: step.into(),
        done,
        total,
        status: None,
    })
}

/// True for the log line that means "finished reading, now writing files".
fn is_writing_conversation_files_banner(line: &str) -> bool {
    line.contains("Writing ") && line.contains("conversation file(s)")
}

/// True for backup-setup lines like `[1/5] Deriving backup keys...`.
///
/// Those counts are setup steps, not message progress, so they must not
/// move the progress bar.
fn has_bracketed_step_ratio(line: &str) -> bool {
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        rest = &rest[open + 1..];
        let Some((left, after_left)) = rest.split_once('/') else {
            continue;
        };
        if left.is_empty() || !left.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Some((right, after_right)) = after_left.split_once(']') else {
            continue;
        };
        if !right.is_empty() && right.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
        rest = after_right;
    }
    false
}

/// Read `done/total` from a message-progress log line.
fn extract_progress_ratio(line: &str) -> Option<(usize, usize)> {
    if has_bracketed_step_ratio(line) {
        return None;
    }

    let looks_like_message_progress = line.contains('…') || line.contains("wrote");
    if !looks_like_message_progress {
        return None;
    }

    let (left, right) = line.split_once('/')?;
    let done = trailing_usize(left)?;
    let total = leading_usize(right)?;
    Some((done, total))
}

/// Parse the integer at the end of `text`, if any.
fn trailing_usize(text: &str) -> Option<usize> {
    let mut reversed_digits = String::new();
    for ch in text.chars().rev() {
        if !ch.is_ascii_digit() {
            break;
        }
        reversed_digits.push(ch);
    }
    if reversed_digits.is_empty() {
        return None;
    }
    let digits: String = reversed_digits.chars().rev().collect();
    digits.parse().ok()
}

/// Parse the integer at the start of `text`, if any.
fn leading_usize(text: &str) -> Option<usize> {
    let mut digits = String::new();
    for ch in text.chars() {
        if !ch.is_ascii_digit() {
            break;
        }
        digits.push(ch);
    }
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_progress_parser_tracks_parse_and_convert() {
        let stage = Arc::new(Mutex::new(ExtractProgressStage::Parse));

        let parse = extract_progress_from_log("  …500/12345 messages", &stage).unwrap();
        assert_eq!(parse.step, "parse");
        assert_eq!(parse.done, 500);
        assert_eq!(parse.total, 12345);
        assert_eq!(parse.status, None);

        let banner =
            extract_progress_from_log("Writing 3 conversation file(s)...", &stage).unwrap();
        assert_eq!(banner.step, "convert");
        assert_eq!(banner.done, 0);
        assert_eq!(banner.total, 0);
        assert_eq!(banner.status.as_deref(), Some("included_in_extract"));

        let ignored = extract_progress_from_log("[1/5] Deriving backup keys...", &stage);
        assert!(ignored.is_none());

        let backup_step = extract_progress_from_log("[2/5] Resolving messages database...", &stage);
        assert!(backup_step.is_none());

        let convert = extract_progress_from_log("  wrote 2/3 messages", &stage).unwrap();
        assert_eq!(convert.step, "convert");
        assert_eq!(convert.done, 2);
        assert_eq!(convert.total, 3);
        assert_eq!(convert.status, None);
    }
}
```

- [ ] **Step 2: Trim `src-tauri/src/commands/extract.rs`**

Delete from `extract.rs`, verbatim (do not edit their content in any other way):

- the `ExtractProgressStage` enum and its doc comment,
- `extract_progress_from_log`,
- `is_writing_conversation_files_banner`,
- `has_bracketed_step_ratio`,
- `extract_progress_ratio`,
- `trailing_usize`,
- `leading_usize`,
- the whole `extract_progress_parser_tracks_parse_and_convert` test.

Keep `counts_exact_messages_written_to_jsonl_output` and everything above line 440.

- [ ] **Step 3: Fix `extract.rs` imports**

Change:

```rust
use super::events::{ExtractErrorEvent, ExtractProgressEvent};
```

to:

```rust
use super::events::ExtractErrorEvent;
use super::progress::{extract_progress_from_log, ExtractProgressStage};
```

(`ExtractProgressEvent` is no longer named in extract.rs — the LogSink closure passes the parsed event straight to `emit`.)

- [ ] **Step 4: Register the module in `src-tauri/src/commands/mod.rs`**

Insert `pub mod progress;` so the list reads:

```rust
pub mod events;
pub mod extract;
pub mod ffmpeg;
pub mod format;
pub mod jobs;
pub mod paths;
pub mod progress;
pub mod pull;
pub mod push;
```

- [ ] **Step 5: Format, lint, and test**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`
Then: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: exit 0.
Then: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: 5 passed (the parser test now runs from progress.rs).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/progress.rs src-tauri/src/commands/extract.rs \
  src-tauri/src/commands/mod.rs
git commit -m "refactor(tauri): move the extract log-progress parser to its own module"
```

---

### Task 3: Adopt the helpers in extract

**Files:**
- Modify: `src-tauri/src/commands/extract.rs`

**Interfaces:**
- Consumes: `super::jobs::{reset_and_clone_cancel, spawn_job}` (Task 1), `super::progress::{extract_progress_from_log, ExtractProgressStage}` (Task 2).

- [ ] **Step 1: Fix imports in `src-tauri/src/commands/extract.rs`**

Remove the line `use std::thread;`. Add after the `super::events` import:

```rust
use super::jobs::{reset_and_clone_cancel, spawn_job};
```

- [ ] **Step 2: Replace the spawn scaffolding in `extract`**

Replace the region from the comment `// Clear a leftover cancel from a previous job. Otherwise this new export` / `// would stop immediately.` through the end of the `thread::spawn(move || { ... });` block (the `});` immediately before `Ok(())`) with:

```rust
    let cancel = reset_and_clone_cancel(&state)?;

    let app_handle = app.clone();
    config.cancel = Some(cancel);
    let log_app = app_handle.clone();
    let progress_stage = Arc::new(Mutex::new(ExtractProgressStage::Parse));
    let log_progress_stage = Arc::clone(&progress_stage);
    config.log = Some(LogSink::new(move |line: &str| {
        let _ = log_app.emit("extract:log", line.to_string());
        if let Some(progress) = extract_progress_from_log(line, &log_progress_stage) {
            let _ = log_app.emit("extract:progress", progress);
        }
    }));

    spawn_job(app, move || {
        let result = run_exporter(&config);

        match result {
            Ok(run_result) => {
                let summary = last_log_line_or(&run_result.messages, "Export complete.");
                for line in run_result.messages {
                    let _ = app_handle.emit("extract:log", line);
                }
                match count_jsonl_output(Path::new(&output_dir)) {
                    Ok(counts) => {
                        let payload = serde_json::json!({
                            "summary": summary,
                            "files_parsed": counts.files,
                            "messages_parsed": counts.messages,
                        });
                        let _ = app_handle.emit("extract:finished", payload.to_string());
                    }
                    Err(err) => {
                        let _ = app_handle.emit(
                            "extract:error",
                            ExtractErrorEvent {
                                detail: format!(
                                    "count extracted JSON Lines records in {output_dir}: {err:#}"
                                ),
                                user_message: Some(
                                    "Extraction completed, but the generated message count could not be verified."
                                        .into(),
                                ),
                            },
                        );
                    }
                }
            }
            Err(err) => return Err(err),
        }
        Ok(())
    });
```

The count-verification failure path keeps its own error event (with `user_message: Some(...)`) and returns `Ok(())` — it is a finished job whose count could not be verified, not a failed job.

- [ ] **Step 3: Format, lint, and test**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`
Then: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: exit 0.
Then: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: 5 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/extract.rs
git commit -m "refactor(tauri): use the shared job scaffolding in extract"
```

---

### Task 4: Adopt the helpers in push and wire the cancel flag

**Files:**
- Modify: `src-tauri/src/commands/push.rs`

**Interfaces:**
- Consumes: `super::jobs::{reset_and_clone_cancel, spawn_job}` (Task 1).

- [ ] **Step 1: Fix imports in `src-tauri/src/commands/push.rs`**

Remove the line `use std::thread;`. Change:

```rust
use super::events::{ExtractErrorEvent, ExtractProgressEvent};
```

to:

```rust
use super::events::ExtractProgressEvent;
use super::jobs::{reset_and_clone_cancel, spawn_job};
```

- [ ] **Step 2: Wire cancel and replace the spawn scaffolding in `push`**

Rename the `_state` parameter to `state`, and replace the region from `let app_handle = app.clone();` through the end of the `thread::spawn(move || { ... });` block (the `});` immediately before `Ok(())`) with:

```rust
    let cancel = reset_and_clone_cancel(&state)?;

    let app_handle = app.clone();
    let contact_name_mode = args
        .contact_name_mode
        .unwrap_or_else(|| "fill_missing".into());

    spawn_job(app, move || {
        let cfg = VaultPushConfig {
            input: PathBuf::from(&args.input_dir),
            base_url: args.base_url,
            username: args.username,
            key: args.key,
            mode: args.mode,
            continue_on_error: args.continue_on_error,
            force: args.force,
            skip_attachments: args.skip_attachments,
            trust_export: args.trust_export,
            verify_digests: false,
            max_retries: 3,
            batch_size: 100,
            asset_upload_workers: 8,
            asset_multipart_threshold: 5 * 1024 * 1024,
            asset_max_bytes: 50 * 1024 * 1024,
            report_path: None,
            log_path: None,
            journal_path: None,
            cancel: Some(cancel),
            contact_name_mode,
            import_id: args.import_id,
        };

        let mut progress = |event: ProgressEvent| match event {
            ProgressEvent::Log(line) => {
                let _ = app_handle.emit("extract:log", line);
            }
            ProgressEvent::Auth { .. } => {}
            ProgressEvent::FileStart { index, total, file } => {
                let _ = app_handle.emit("extract:log", format!("Starting: {file}"));
                let _ = app_handle.emit(
                    "extract:progress",
                    ExtractProgressEvent {
                        step: "upload".into(),
                        done: index.saturating_sub(1),
                        total,
                        status: None,
                    },
                );
            }
            ProgressEvent::FileDone { file, status } => {
                let _ = app_handle.emit("extract:log", format!("Done: {file} ({status})"));
            }
            ProgressEvent::Issue {
                kind,
                step,
                item,
                reason,
            } => {
                let _ = app_handle.emit(
                    "extract:issue",
                    serde_json::json!({
                        "kind": kind,
                        "step": step,
                        "item": item,
                        "reason": reason,
                    }),
                );
            }
            ProgressEvent::Finished(report) => {
                for result in &report.results {
                    if result.status == "failed" {
                        let reason = result.error.as_deref().unwrap_or("upload failed");
                        let _ = app_handle.emit(
                            "extract:issue",
                            serde_json::json!({
                                "kind": "error",
                                "step": "upload",
                                "item": result.file,
                                "reason": reason,
                            }),
                        );
                    } else if result.status == "skipped" {
                        let reason = result
                            .error
                            .as_deref()
                            .unwrap_or("already imported or skipped");
                        let _ = app_handle.emit(
                            "extract:issue",
                            serde_json::json!({
                                "kind": "skip",
                                "step": "upload",
                                "item": result.file,
                                "reason": reason,
                            }),
                        );
                    }
                }
                let (progress, summary) = finished_push_events(&report);
                let _ = app_handle.emit("extract:progress", progress);
                let _ = app_handle.emit("extract:finished", summary.to_string());
            }
        };

        match run_push(&cfg, Some(&mut progress)) {
            Ok(_) => {}
            Err(err) => return Err(err),
        }
        Ok(())
    });
```

The progress closure and `finished_push_events` are unchanged — only `cancel: None` becomes `cancel: Some(cancel)` and the error path routes through `spawn_job` instead of an inline emit.

- [ ] **Step 3: Format, lint, and test**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`
Then: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: exit 0.
Then: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: 5 passed (including `finished_push_event_reports_complete_upload_and_totals`).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/push.rs
git commit -m "fix(tauri): wire the shared cancel flag into push"
```

---

### Task 5: Remove the runtime env writes from the ffmpeg command

**Files:**
- Modify: `src-tauri/src/commands/ffmpeg.rs`

**Interfaces:**
- Produces: unchanged command surface (`probe_ffmpeg_tools`, `set_ffmpeg_tools_dir`, `FfmpegToolsProbeDto`).
- Consumes: `media::set_tools_dir`, `media::probe_ffmpeg_tools` (unchanged media-crate API).

- [ ] **Step 1: Replace `src-tauri/src/commands/ffmpeg.rs`**

Replace the entire file with (behavior of both commands is identical; only the env writes disappear):

```rust
//! Commands that find ffmpeg and ffprobe, and remember which folder they live in.
//!
//! Attachment convert and compress need those programs. The WebView cannot
//! search the disk or set process environment variables, so this process does
//! both. This process never *writes* environment variables: the tools-folder
//! override lives in media-crate process state, and `MESSAGE_VAULT_IO_BIN`
//! stays a user-set fallback that is only ever read here (by the media and
//! whatsapp-exporter resolution paths, which is sound because nothing in
//! this process writes the environment).

use std::path::{Path, PathBuf};

use super::optional_trimmed;

/// Paths the Settings screen shows after looking for ffmpeg and ffprobe.
#[derive(serde::Serialize)]
pub struct FfmpegToolsProbeDto {
    pub ok: bool,
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub error: Option<String>,
}

/// Copy probe results into the JSON shape the UI expects.
fn probe_to_dto(probe: media::FfmpegToolsProbe) -> FfmpegToolsProbeDto {
    let ffmpeg_path = probe.ffmpeg_path.map(|path| path.display().to_string());
    let ffprobe_path = probe.ffprobe_path.map(|path| path.display().to_string());
    FfmpegToolsProbeDto {
        ok: probe.ok,
        ffmpeg_path,
        ffprobe_path,
        error: probe.error,
    }
}

/// Treat a blank folder string as "search the default PATH instead".
fn optional_tools_dir(dir: Option<&str>) -> Option<&Path> {
    let dir = optional_trimmed(dir)?;
    Some(Path::new(dir))
}

/// Ask this process whether ffmpeg and ffprobe are available.
///
/// When `dir` is set, look in that folder. When it is empty, search the
/// process PATH.
#[tauri::command]
pub fn probe_ffmpeg_tools(dir: Option<String>) -> FfmpegToolsProbeDto {
    let tools_dir = optional_tools_dir(dir.as_deref());
    let probe = media::probe_ffmpeg_tools(tools_dir);
    probe_to_dto(probe)
}

/// Ask this process to remember where ffmpeg and ffprobe live for this session.
///
/// An empty `dir` clears the override and goes back to the default search
/// path. This process never writes environment variables: the override
/// lives in media-crate process state, and `MESSAGE_VAULT_IO_BIN` stays a
/// user-set fallback that is only ever read here.
///
/// # Errors
///
/// Returns an error if `dir` is set but ffmpeg or ffprobe cannot be found
/// there.
#[tauri::command]
pub fn set_ffmpeg_tools_dir(dir: Option<String>) -> Result<FfmpegToolsProbeDto, String> {
    let trimmed = optional_trimmed(dir.as_deref());
    match trimmed {
        None => {
            media::set_tools_dir(None);
            Ok(probe_ffmpeg_tools(None))
        }
        Some(dir) => {
            let path = PathBuf::from(dir);
            let probe = media::probe_ffmpeg_tools(Some(path.as_path()));
            if !probe.ok {
                return Err(probe
                    .error
                    .unwrap_or_else(|| "ffmpeg tools not found".into()));
            }
            media::set_tools_dir(Some(path));
            Ok(probe_to_dto(probe))
        }
    }
}
```

- [ ] **Step 2: Format, lint, and test**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`
Then: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: exit 0.
Then: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: 5 passed.
Sanity: `grep -rn "unsafe" src-tauri/src` — expected: zero hits (the two env blocks were the only unsafe sites).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/ffmpeg.rs
git commit -m "refactor(tauri): stop writing MESSAGE_VAULT_IO_BIN at runtime"
```

---

### Task 6: Enable `missing_docs` and document the IPC DTO fields

**Files:**
- Modify: `src-tauri/src/lib.rs` (the gate)
- Modify: `src-tauri/src/commands/events.rs` (6 field docs)
- Modify: `src-tauri/src/commands/extract.rs` (12 field docs)
- Modify: `src-tauri/src/commands/pull.rs` (6 field docs)
- Modify: `src-tauri/src/commands/push.rs` (11 field docs)
- Modify: `src-tauri/src/commands/ffmpeg.rs` (4 field docs)
- Modify: `src-tauri/src/commands/paths.rs` (1 field doc)

**Interfaces:**
- Consumes: none; pure documentation. This is the JSON wire contract with `web/src/lib/types.ts`, so field docs must describe what the UI sends/expects, not implementation trivia.

- [ ] **Step 1: Add the gate to `src-tauri/src/lib.rs`**

Insert directly after the `//!` module doc block (before `pub mod commands;`):

```rust
#![warn(missing_docs)]
```

- [ ] **Step 2: Document `ExtractProgressEvent` and `ExtractErrorEvent` fields in `events.rs`**

```rust
/// Progress numbers the UI uses to update the progress bar.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractProgressEvent {
    /// Current pipeline stage, for example `parse`, `convert`, or `upload`.
    pub step: String,
    /// Number of items finished so far.
    pub done: usize,
    /// Total items, or 0 when the total is unknown.
    pub total: usize,
    /// Extra step status the UI shows, for example `included_in_extract`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Failure details for the `extract:error` event.
///
/// When `user_message` is missing, it is left out of the JSON so the
/// TypeScript type can treat it as optional.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractErrorEvent {
    /// Full error chain, for logs and the advanced-details view.
    pub detail: String,
    /// Friendlier message for the UI, when one is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<String>,
}
```

- [ ] **Step 3: Document `ExtractArgs` fields in `extract.rs`**

```rust
/// User-facing parameters for the `extract` command (before defaults/parsing).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractArgs {
    /// Backup source key, for example `imessage-ios` or `whatsapp-android`.
    pub source: String,
    /// Path to the phone backup (a folder, database file, or XML file).
    pub path: String,
    /// Folder the exporter writes conversation files into.
    pub output_dir: String,
    /// Password for encrypted backups, when the source needs one.
    pub backup_password: Option<String>,
    /// Attachment handling choice: `copy`, `convert`, `compress`, or `skip`.
    pub attachment_media: Option<String>,
    /// Video/image size cap for convert and compress: `720p`, `1080p`, or `4k`.
    pub media_max_resolution: Option<String>,
    /// Frame-rate cap for compressed video, for example `30`.
    pub media_max_fps: Option<String>,
    /// Smallest media file size that still counts as an attachment, for example `20M`.
    pub media_min_size: Option<String>,
    /// Conversation filter string passed to the exporter.
    pub conversation_filter: Option<String>,
    /// Export start date, inclusive, in `YYYY-MM-DD` form.
    pub start_date: Option<String>,
    /// Export end date, inclusive, in `YYYY-MM-DD` form.
    pub end_date: Option<String>,
    /// When true, replace names and phone numbers with fake ones.
    pub obfuscate: Option<bool>,
}
```

- [ ] **Step 4: Document `PullArgs` fields in `pull.rs`**

```rust
/// User-facing parameters for the `pull` command.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullArgs {
    /// Base URL of the vault server, for example `http://127.0.0.1:8080`.
    pub base_url: String,
    /// Vault account name.
    pub username: String,
    /// API token or account password for the vault.
    pub key: String,
    /// Folder the pulled conversation files are written into.
    pub out_dir: String,
    /// Vault search query selecting which conversations to pull.
    pub query: String,
    /// When true, skip attachments and download messages only.
    pub skip_attachments: bool,
}
```

- [ ] **Step 5: Document `PushArgs` fields in `push.rs`**

```rust
/// User-facing parameters for the `push` command.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushArgs {
    /// Base URL of the vault server, for example `http://127.0.0.1:8080`.
    pub base_url: String,
    /// Vault account name.
    pub username: String,
    /// API token or account password for the vault.
    pub key: String,
    /// Folder of conversation files to upload.
    pub input_dir: String,
    /// Import mode. `append` adds to existing data (safe to re-run);
    /// `replace` deletes existing messages for this source, then imports.
    pub mode: String,
    /// When true, ignore the journal and re-upload assets and re-import
    /// messages.
    pub force: bool,
    /// When true, continue after a failed conversation.
    pub continue_on_error: bool,
    /// When true, import messages without uploading attachments.
    pub skip_attachments: bool,
    /// When true, trust export metadata: skip re-hashing attachments when
    /// size_bytes matches the file size on disk. Without this flag every
    /// attachment is re-hashed.
    pub trust_export: bool,
    /// Server-side import option that controls how missing contact names
    /// are filled in, for example `fill_missing`.
    pub contact_name_mode: Option<String>,
    /// Import id of an earlier import to resume, when set.
    pub import_id: Option<i64>,
}
```

- [ ] **Step 6: Document `FfmpegToolsProbeDto` fields in `ffmpeg.rs`**

```rust
/// Paths the Settings screen shows after looking for ffmpeg and ffprobe.
#[derive(serde::Serialize)]
pub struct FfmpegToolsProbeDto {
    /// Whether both tools were found and pass `-version`.
    pub ok: bool,
    /// Resolved ffmpeg path, when found.
    pub ffmpeg_path: Option<String>,
    /// Resolved ffprobe path, when found.
    pub ffprobe_path: Option<String>,
    /// What was missing, when `ok` is false.
    pub error: Option<String>,
}
```

- [ ] **Step 7: Document `HomeDirInfo.path` in `paths.rs`**

```rust
/// The signed-in user's home folder, plus which OS this process is running on.
#[derive(Debug, Clone, Serialize)]
pub struct HomeDirInfo {
    /// Home folder as an absolute path the UI can join onto.
    pub path: String,
    /// Operating system name as Rust reports it, for example `linux`, `macos`,
    /// or `windows`.
    pub os: String,
}
```

- [ ] **Step 8: Format, lint, and test**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`
Then: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: exit 0, zero missing-docs warnings (a probe on main showed exactly the 40 fields above; all are now documented).
Then: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: 5 passed.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/commands/events.rs \
  src-tauri/src/commands/extract.rs src-tauri/src/commands/pull.rs \
  src-tauri/src/commands/push.rs src-tauri/src/commands/ffmpeg.rs \
  src-tauri/src/commands/paths.rs
git commit -m "docs(tauri): document the IPC DTO wire contract and enable missing_docs"
```

---

### Task 7: Complete the event lists in the module intros

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/commands/extract.rs`

**Interfaces:**
- Consumes: none; doc-only.

- [ ] **Step 1: Complete the event list in `commands/mod.rs`**

Change the intro paragraph:

```rust
//! Long jobs return as soon as the background thread starts. Progress, log
//! lines, and errors are sent as Tauri events (`extract:log`,
//! `extract:progress`, `extract:finished`, `extract:error`).
```

to:

```rust
//! Long jobs return as soon as the background thread starts. Progress, log
//! lines, issues, and errors are sent as Tauri events (`extract:log`,
//! `extract:progress`, `extract:issue`, `extract:finished`,
//! `extract:error`).
```

- [ ] **Step 2: Complete the event list in `commands/extract.rs`**

Change the intro paragraph:

```rust
//! `extract` starts the selected exporter on a background thread and returns
//! immediately. Progress is sent back as Tauri events:
//! `extract:log` (one log line), `extract:finished` (a summary string or JSON
//! object), and `extract:error` ([`ExtractErrorEvent`]).
```

to:

```rust
//! `extract` starts the selected exporter on a background thread and returns
//! immediately. Progress is sent back as Tauri events:
//! `extract:log` (one log line), `extract:progress` (bar position),
//! `extract:finished` (a summary string or JSON object), and `extract:error`
//! ([`ExtractErrorEvent`]).
```

- [ ] **Step 3: Lint and commit**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: exit 0.

```bash
git add src-tauri/src/commands/mod.rs src-tauri/src/commands/extract.rs
git commit -m "docs(tauri): complete the extract event lists in module intros"
```

---

### Task 8: Lint and test src-tauri in CI

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: none. CI-only.

- [ ] **Step 1: Update the job comment in `.github/workflows/ci.yml`**

Change:

```yaml
  # ── Always: cargo check src-tauri ─────────────────────────────────────
  # src-tauri is not a workspace member, so the workspace build above skips
  # it; this is its first compile gate. Same system packages as the release
  # job's Linux installer build.
```

to:

```yaml
  # ── Always: check, lint, and test src-tauri ───────────────────────────
  # src-tauri is not a workspace member, so the workspace build above skips
  # it; this is its compile, lint, and test gate. Same system packages as
  # the release job's Linux installer build.
```

- [ ] **Step 2: Add clippy and test steps**

Insert immediately after the existing step:

```yaml
      - name: Check src-tauri
        run: cargo check --manifest-path src-tauri/Cargo.toml
```

these two steps, at the same indentation:

```yaml
      - name: Clippy src-tauri
        run: cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings

      - name: Test src-tauri
        run: cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 3: Sanity-check the YAML**

Run: `git diff .github/workflows/ci.yml`
Expected: exactly the comment change plus the two steps, with indentation matching the neighboring steps (six spaces for `- name:` / `run:` at this nesting level). The job keeps its existing dependency install and cache steps untouched.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: lint and test src-tauri on PRs"
```

---

### Task 9: CHANGELOG and final verification

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Add the bullet**

In `CHANGELOG.md`, under `## [Unreleased]` → `### Changed`, after the **CLI tools** bullet, add:

```markdown
- **Desktop host:** share the cancel/spawn/error scaffolding across the four
  job commands, wire the shared cancel flag into push, drop the runtime
  `MESSAGE_VAULT_IO_BIN` environment writes (sound env access), split the
  extract progress parser into its own module, document the IPC DTO wire
  contract, and gate src-tauri with clippy and tests in CI. Push now honors
  Cancel; the only other product delta is that a KnugiHK binary placed in a
  custom tools folder is no longer found by WhatsApp-Android export.
```

- [ ] **Step 2: Full verification sweep**

Run, in order:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
./scripts/check-pr.sh
```

Expected: everything exits 0. `check-pr.sh` also covers the workspace, web, and docs trees; it stops on the first failure. If any step fails, fix and re-run until green.

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md
git commit -m "chore: changelog entry for the desktop-host cleanup"
```

## Final state

The branch ends with 9 commits, clippy-clean, tests-green, `check-pr.sh` green. The PR's `check-tauri` job now runs check + clippy + test (Task 8). Open the implementation PR with the title `refactor(tauri): shared job scaffolding, sound env access, and DTO docs`, referencing the spec in the body, and note the two intended behavior deltas from the spec's behavior-delta catalog (push honors Cancel; KnugiHK custom-dir fallback removed).
