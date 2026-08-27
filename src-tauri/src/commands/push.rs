//! `push` command — upload an extract folder to a Message Vault server.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::Emitter;
use vault_push::{ProgressEvent, VaultPushConfig, run as run_push};

use super::events::ExtractProgressEvent;
use super::jobs::{reset_and_clone_cancel, spawn_job};
use crate::state::AppState;

/// Convert a report count to the `usize` the progress event uses.
fn as_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// Progress bar update and finished JSON payload after a push completes.
fn finished_push_events(
    report: &vault_push::PushReport,
) -> (ExtractProgressEvent, serde_json::Value) {
    let progress = ExtractProgressEvent {
        step: "upload".into(),
        done: as_usize(report.conversations_total),
        total: as_usize(report.conversations_total),
        bytes_done: None,
        bytes_total: None,
        status: None,
    };
    let summary = serde_json::json!({
        "summary": format!(
            "Push complete: {} new, {} deduped, {} failed of {} attempted; {}/{} conversations ok; {} assets uploaded",
            report.messages_inserted,
            report.messages_deduped,
            report.messages_failed,
            report.messages_attempted,
            report.conversations_ok,
            report.conversations_total,
            report.assets_uploaded
        ),
        "ok": report.ok,
        "messages": report.messages,
        "messages_attempted": report.messages_attempted,
        "messages_inserted": report.messages_inserted,
        "messages_deduped": report.messages_deduped,
        "messages_failed": report.messages_failed,
        "assets_uploaded": report.assets_uploaded,
        "assets_bytes": report.assets_bytes,
        "conversations_ok": report.conversations_ok,
        "conversations_total": report.conversations_total,
        "conversations_failed": report.conversations_failed,
        "conversations_skipped": report.conversations_skipped,
        "results": report.results,
    });
    (progress, summary)
}

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

/// Ask this process to upload extracted conversations to a vault server.
///
/// Returns as soon as the background thread starts. Upload progress uses the
/// same `extract:*` events as Extract so the UI can reuse one progress view.
///
/// # Errors
///
/// Returns an error if another thread panicked while holding the shared
/// state lock. Failures during the upload are sent as `extract:error`.
#[tauri::command]
pub async fn push(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
    args: PushArgs,
) -> Result<(), String> {
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
            // Pack until vault_push::MAX_IMPORT_BODY_BYTES (64 MiB); do not stop at a message count.
            batch_size: vault_push::NO_MESSAGE_COUNT_LIMIT,
            // Above the CLI default (8): desktop imports are often many small files.
            asset_upload_workers: 16,
            // Above the CLI default (3): hide more hashing behind in-flight imports.
            prepare_ahead: 8,
            // Above the CLI default (2): more of the prepare-ahead queue runs at once.
            prepare_workers: 4,
            asset_multipart_threshold: 5 * 1024 * 1024,
            // Per-file attachment cap. JSONL import batches use MAX_IMPORT_BODY_BYTES.
            asset_max_bytes: 50 * 1024 * 1024,
            report_path: None,
            log_path: None,
            // Relies on one preflight HEAD per run instead of a persisted journal.
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
                        bytes_done: None,
                        bytes_total: None,
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vault_push::{FileResult, PushReport};

    #[test]
    fn finished_push_event_reports_complete_upload_and_totals() {
        let report = PushReport {
            ok: true,
            account: "account".into(),
            username: "user".into(),
            mode: "append".into(),
            started_at: "2026-08-11T00:00:00Z".into(),
            finished_at: "2026-08-11T00:00:01Z".into(),
            elapsed_ms: 1_000,
            conversations_total: 3,
            conversations_ok: 2,
            conversations_failed: 0,
            conversations_skipped: 1,
            messages_attempted: 45,
            messages_inserted: 42,
            messages_deduped: 2,
            messages_failed: 1,
            messages: 42,
            assets_uploaded: 4,
            assets_skipped: 1,
            assets_bytes: 12_345,
            results: vec![FileResult {
                file: "failed.jsonl".into(),
                status: "failed".into(),
                error: Some("attachment exceeds limit".into()),
                messages: 0,
                attachments: 0,
                profile: None,
            }],
        };

        let (progress, summary) = finished_push_events(&report);

        assert_eq!(progress.step, "upload");
        assert_eq!(progress.done, 3);
        assert_eq!(progress.total, 3);
        assert_eq!(summary["messages"], 42);
        assert_eq!(summary["messages_attempted"], 45);
        assert_eq!(summary["messages_inserted"], 42);
        assert_eq!(summary["messages_deduped"], 2);
        assert_eq!(summary["messages_failed"], 1);
        assert_eq!(summary["conversations_failed"], 0);
        assert_eq!(summary["conversations_skipped"], 1);
        assert_eq!(summary["results"][0]["error"], "attachment exceeds limit");
        assert_eq!(
            summary["summary"],
            "Push complete: 42 new, 2 deduped, 1 failed of 45 attempted; 2/3 conversations ok; 4 assets uploaded"
        );
        assert_eq!(summary["assets_bytes"], 12_345);
        assert_eq!(summary["conversations_ok"], 2);
        assert_eq!(summary["conversations_total"], 3);
    }
}
