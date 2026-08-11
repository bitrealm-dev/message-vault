//! `push` Tauri command — wraps `vault_push::run()` to import message-ir
//! exports into a Message Vault server.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use tauri::Emitter;
use vault_push::{ProgressEvent, VaultPushConfig, run as run_push};

use super::events::ExtractErrorEvent;
use crate::state::AppState;

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
    let contact_name_mode = contact_name_mode
        .unwrap_or_else(|| "fill_missing".into());

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
            ProgressEvent::FileStart { file, .. } => {
                let _ = app_handle.emit("extract:log", format!("Starting: {file}"));
            }
            ProgressEvent::FileDone { file, status } => {
                let _ = app_handle.emit("extract:log", format!("Done: {file} ({status})"));
            }
            ProgressEvent::Finished(_) => {}
        };

        match run_push(&cfg, Some(&mut progress)) {
            Ok(report) => {
                let summary = format!(
                    "Push complete: {} messages, {}/{} conversations ok, {} assets uploaded",
                    report.messages, report.conversations_ok, report.conversations_total, report.assets_uploaded
                );
                let _ = app_handle.emit("extract:finished", summary);
            }
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
