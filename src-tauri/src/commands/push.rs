//! `push` Tauri command — wraps `vault_push::run()` to import message-ir
//! exports into a Message Vault server.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use tauri::Emitter;
use vault_push::{ProgressEvent, VaultPushConfig, run as run_push};

use super::events::{ExtractErrorEvent, ExtractProgressEvent};
use crate::state::AppState;

fn as_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn finished_push_events(
    report: &vault_push::PushReport,
) -> (ExtractProgressEvent, serde_json::Value) {
    (
        ExtractProgressEvent {
            step: "upload".into(),
            done: as_usize(report.conversations_total),
            total: as_usize(report.conversations_total),
            status: None,
        },
        serde_json::json!({
            "summary": format!(
                "Push complete: {} messages, {}/{} conversations ok, {} assets uploaded",
                report.messages, report.conversations_ok, report.conversations_total, report.assets_uploaded
            ),
            "ok": report.ok,
            "messages": report.messages,
            "assets_uploaded": report.assets_uploaded,
            "assets_bytes": report.assets_bytes,
            "conversations_ok": report.conversations_ok,
            "conversations_total": report.conversations_total,
        }),
    )
}

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
                if status.as_str() != "ok" && status.as_str() != "skipped" {
                    let _ = app_handle.emit(
                        "extract:issue",
                        serde_json::json!({
                            "kind": "error",
                            "step": "upload",
                            "item": file,
                            "reason": status,
                        }),
                    );
                }
            }
            ProgressEvent::Finished(report) => {
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
    use vault_push::PushReport;

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
            messages: 42,
            assets_uploaded: 4,
            assets_skipped: 1,
            assets_bytes: 12_345,
            results: vec![],
        };

        let (progress, summary) = finished_push_events(&report);

        assert_eq!(progress.step, "upload");
        assert_eq!(progress.done, 3);
        assert_eq!(progress.total, 3);
        assert_eq!(summary["messages"], 42);
        assert_eq!(
            summary["summary"],
            "Push complete: 42 messages, 2/3 conversations ok, 4 assets uploaded"
        );
        assert_eq!(summary["assets_bytes"], 12_345);
        assert_eq!(summary["conversations_ok"], 2);
        assert_eq!(summary["conversations_total"], 3);
    }
}
