# Tauri Host Follow-up Design

> Final group of the product Rust audit follow-ups. The audit report is
> `docs/superpowers/reports/2026-08-23-rust-audit.md` (merged via PR #90).
> Prior groups are all merged: server (#93), libs (#96/#102), exporters
> (#146), CLI tools (#151).

## Goal

Apply the seven Tauri-host findings to `src-tauri/` and the CI workflow,
preserving behavior at the product boundary. One implementation PR.

## Findings and current status

| # | Severity | Finding | Status on main |
|---|----------|---------|----------------|
| 1 | medium | Cancel-flag reset, thread spawn, and `extract:*` error routing duplicated across all four job commands (`extract.rs:179`) | Live |
| 2 | low | No `#![warn(missing_docs)]`; nearly every IPC DTO struct field undocumented (`lib.rs:15`) | Live |
| 3 | low | Module intros enumerate `extract:*` events incompletely — `extract:issue` and `extract:progress` omitted (`commands/mod.rs:8`) | Live |
| 4 | medium | `push` never wires the shared cancel flag, so Cancel has no effect on an in-flight push (`push.rs:113`) | Live |
| 5 | low | `extract.rs` (625 lines) mixes command, config building, JSONL counting, and the log-progress parser | Live |
| 6 | medium | src-tauri is excluded from the workspace, so CI never builds, tests, or lints it on PRs (`src-tauri/Cargo.toml:15`) | Partially resolved: PR #108 added a `check-tauri` CI job running `cargo check`. Clippy and tests still never run. |
| 7 | medium | SAFETY comments on `env::set_var`/`remove_var` justify variable ownership, not the std-required no-concurrent-env-access condition (`commands/ffmpeg.rs:62`) | Live |

## Adjudicated decisions

1. **F7 — drop the env writes.** The desktop host stops calling
   `std::env::set_var`/`remove_var` entirely. It relies on
   `media::set_tools_dir` (a `Mutex<ToolsState>` behind a `OnceLock` in the
   media crate) as its single source of truth. The process becomes
   environment-read-only, which makes the concurrent env *reads* on job
   threads (`media::tools.rs` and `whatsapp-exporter` fallbacks) sound by
   construction. Rejected alternatives: persist + set once at startup (new
   persistence plumbing that does not exist today — the web UI holds the
   setting); comments only (honest, but the precondition still is not met).
2. **F6 — extend the existing `check-tauri` job.** Add
   `cargo clippy --all-targets -- -D warnings` and `cargo test` to the job
   PR #108 added, as separate steps on the same runner. Baseline measured
   on main: clippy with `-D warnings` exits 0 and all 3 tests pass, so the
   gate is enforceable from day one. Rejected: clippy without `-D warnings`
   (drift allowed), joining the root workspace (shared lockfile, dependency
   unification against `dirs 6` etc., version-lockstep implications).
3. **F1 + F4 — a full shared job helper.** New `commands/jobs.rs` with
   `reset_and_clone_cancel` (one lock round-trip) and `spawn_job` (spawn +
   uniform `extract:error` routing). All four job commands use it; `push`
   gains cancel wiring through it. Rejected: cancel-helper only, and
   skipping F1.

## Component design

### `commands/jobs.rs` — shared job scaffolding (F1, F4)

Two `pub(crate)` helpers. Every job command calls
`reset_and_clone_cancel` before building its config, then hands its
Ok-side work to `spawn_job`.

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
```

Per-command shape (pull is the smallest example):

```rust
let cancel = reset_and_clone_cancel(&state)?;

let app_handle = app.clone();
spawn_job(app, move || {
    // ... run, map progress events, emit the finished summary ...
    Ok(())
});
```

Command-level errors stay command-level: each command keeps its config
building, form parsing, and progress-callback setup *before* `spawn_job`,
so those failures remain `invoke` rejections rather than becoming
`extract:error` events. Ok-side routing stays in the closure because it
genuinely differs per command:

- `extract`: replay log lines, `count_jsonl_output`, emit the JSON
  finished payload; the count-verification failure keeps its own error
  event (with `user_message: Some(...)`) and returns `Ok(())`.
- `format`: replay log lines, emit `last_log_line_or(..., "Format
  conversion complete.")`.
- `pull`: build the pull summary string, emit it.
- `push`: map `ProgressEvent`s including `Issue`/`Finished(report)`, emit
  issues, progress, and the JSON summary; return `Ok(())`.

`push` changes: the `_state` parameter becomes `state` and the command
calls `reset_and_clone_cancel`, passing `cancel: Some(cancel)` into
`VaultPushConfig` (the vault-push pipeline honors it via `check_cancel`).
The invoke signature is unchanged — `state` is injected by Tauri, not a
JavaScript argument.

The `extract:error` payload emitted by `spawn_job` is byte-identical to
what all four commands emit today (`detail: {err:#}`, `user_message: None`).

### `commands/progress.rs` — log-progress parser (F5)

Move `ExtractProgressStage`, `extract_progress_from_log`,
`is_writing_conversation_files_banner`, `has_bracketed_step_ratio`,
`extract_progress_ratio`, `trailing_usize`, `leading_usize`, and their
test from `extract.rs` into this module, verbatim. Visibility:
`ExtractProgressStage` and `extract_progress_from_log` become
`pub(crate)`; the other five helpers stay private to the module.
`extract.rs` keeps the command, config building, and JSONL counting and
drops from 625 to roughly 450 lines.

### `commands/ffmpeg.rs` — env writes removed (F7)

Delete both `unsafe` blocks. The command uses only the media crate's
in-process state:

```rust
/// Ask this process to remember where ffmpeg and ffprobe live for this session.
///
/// An empty `dir` clears the override and goes back to the default search
/// path. This process never writes environment variables: the override
/// lives in media-crate process state, and `MESSAGE_VAULT_IO_BIN` stays a
/// user-set fallback that is only ever read here (by the media and
/// whatsapp-exporter resolution paths, which is sound because nothing in
/// this process writes the environment).
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

The module intro gains the same rule: the desktop host never mutates the
environment; the override is process-local state.

### DTO wire-contract docs (F2)

Add `#![warn(missing_docs)]` to `lib.rs` and document every
undocumented public item — the JSON wire contract with
`web/src/lib/types.ts`. All `#[tauri::command]` functions already carry
docs, so the remaining surface is struct fields:

- `ExtractArgs` (12 fields): source, path, output_dir, backup_password,
  attachment_media, media_max_resolution, media_max_fps, media_min_size,
  conversation_filter, start_date, end_date, obfuscate.
- `PushArgs` (11 fields): base_url, username, key, input_dir, mode,
  force, continue_on_error, skip_attachments, trust_export,
  contact_name_mode, import_id. The `mode` doc reuses the CLI's wording:
  "append: add to existing data (safe to re-run); replace: delete
  existing messages for this source, then import".
- `PullArgs` (6 fields): base_url, username, key, out_dir, query,
  skip_attachments.
- `ExtractProgressEvent` (4 fields): step, done, total, status.
- `ExtractErrorEvent` (2 fields): detail, user_message.
- `FfmpegToolsProbeDto` (4 fields): ok, ffmpeg_path, ffprobe_path, error.
- `HomeDirInfo.path` (`os` is already documented).

Known risk: the `#[tauri::command]` macro may generate undocumented
`pub` items. If so, a targeted `#[allow(missing_docs)]` with a comment on
the macro-generated items only — never on hand-written code.

### Event lists (F3)

`commands/mod.rs` intro: add `extract:issue` to the event list.
`extract.rs` intro: add `extract:progress` to its event list.
Doc-only.

### CI workflow (F6)

In the existing `check-tauri` job, after the `cargo check` step, add two
steps:

```yaml
      - name: Clippy src-tauri
        run: cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
      - name: Test src-tauri
        run: cargo test --manifest-path src-tauri/Cargo.toml
```

Same job, same runner, same dependency install and cache. No workspace
membership change.

## Behavior-delta catalog

| Change | Delta |
|--------|-------|
| F1/F4 jobs helper | None in the wire format — same events, same payloads, same error shapes. Two improvements: (a) `push` now honors Cancel (the fix F4 demands); (b) the reset/clone race closes, so a cancel click can no longer leak into the next job. |
| F7 env writes removed | The desktop process never mutates the environment. Delta: a KnugiHK binary placed in a *custom tools dir* is no longer found by WhatsApp-Android export in the desktop app (bundled/next-to-binary and PATH lookups are unchanged). CLI processes are unaffected — they read the user-set variable as before. Settings probe/set flows are unchanged; they use in-process state. |
| F2/F3/F5/F6 | None. |

## Global constraints

- Behavior preservation at the product boundary: the event names,
  payload shapes, summary strings, and command rejections visible to
  `web/` must not change except as listed in the behavior-delta catalog.
- The rustdoc style guide
  (`docs/src/content/docs/vault/developer/rustdoc-style.md`) governs all
  new doc text.
- One implementation PR against `main`; no version bumps; no tag pushes.
- No changes to crates outside `src-tauri/` and `.github/workflows/ci.yml`
  (the media and whatsapp-exporter env *reads* stay — they are the CLI
  users' configuration surface).
- `#![warn(missing_docs)]` is a warning gate, matching the other crates in
  this series.

## Out of scope

- Joining `src-tauri` into the root workspace.
- Persisting the custom tools dir across app restarts.
- Changes to the media crate or whatsapp-exporter (including their
  test-only env writes, which run single-threaded).
- Legacy `message-vault-io-gui` and `web-next`.

## Testing

- Existing src-tauri tests (3) stay green; the progress-parser test moves
  with its module unchanged.
- New unit tests for `reset_and_clone_cancel`: a fresh state returns a
  false-flag clone; a previously-set flag is cleared and the returned
  clone shares the same `Arc`.
- Verification: `cargo clippy --manifest-path src-tauri/Cargo.toml
  --all-targets -- -D warnings` and `cargo test --manifest-path
  src-tauri/Cargo.toml`; the updated `check-tauri` CI job runs both on
  the PR. Workspace build/test are unaffected (src-tauri is excluded).
