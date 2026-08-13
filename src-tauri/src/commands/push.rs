//! `push` command — upload an extract folder to a Message Vault server.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use tauri::Emitter;
use vault_push::{ProgressEvent, VaultPushConfig, run as run_push};

use super::events::{ExtractErrorEvent, ExtractProgressEvent};
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

/// Ask this process to upload extracted conversations to a vault server.
///
/// Returns as soon as the background thread starts. Upload progress uses the
/// same `extract:*` events as Extract so the UI can reuse one progress view.
///
/// # Errors
///
/// This command always returns `Ok` after the thread starts. Failures during
/// the upload are sent as `extract:error`.
#[tauri::command]
pub async fn push(
    _state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
    base_url: String,
    username: String,
    key: String,
    input_dir: String,
    mode: String,
    force: bool,
    continue_on_error: bool,
    skip_attachments: bool,
    trust_export: bool,
    contact_name_mode: Option<String>,
    import_id: Option<i64>,
) -> Result<(), String> {
    let app_handle = app.clone();
    let contact_name_mode = contact_name_mode.unwrap_or_else(|| "fill_missing".into());

    thread::spawn(move || {
        let cfg = VaultPushConfig {
            input: PathBuf::from(&input_dir),
            base_url,
            username,
            key,
            mode,
            continue_on_error,
            force,
            skip_attachments,
            trust_export,
            verify_digests: false,
            max_retries: 3,
            batch_size: 100,
            asset_upload_workers: 8,
            asset_multipart_threshold: 5 * 1024 * 1024,
            asset_max_bytes: 50 * 1024 * 1024,
            report_path: None,
            log_path: None,
            journal_path: None,
            cancel: None,
            contact_name_mode,
            import_id,
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
                        let reason = match result.error.as_deref() {
                            Some(error) => error,
                            None => "upload failed",
                        };
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
                        let reason = match result.error.as_deref() {
                            Some(error) => error,
                            None => "already imported or skipped",
                        };
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
            Err(err) => {
                let _ = app_handle.emit(
                    "extract:error",
                    ExtractErrorEvent {
                        detail: format!("{err:#}"),
                        user_message: None,
                    },
                );
            }
        }
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
