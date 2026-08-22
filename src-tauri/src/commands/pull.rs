//! `pull` command — download messages from a Message Vault server.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;

use tauri::Emitter;
use vault_pull::{ProgressEvent, VaultPullConfig, run as run_pull};

use super::events::ExtractErrorEvent;
use crate::state::AppState;

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
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn pull(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
    base_url: String,
    username: String,
    key: String,
    out_dir: String,
    query: String,
    skip_attachments: bool,
) -> Result<(), String> {
    {
        let st = state.lock().map_err(|e| e.to_string())?;
        st.cancel_flag.store(false, Ordering::SeqCst);
    }

    let cancel = {
        let st = state.lock().map_err(|e| e.to_string())?;
        st.cancel_flag.clone()
    };

    let app_handle = app.clone();

    thread::spawn(move || {
        let cfg = VaultPullConfig {
            out_dir: PathBuf::from(&out_dir),
            base_url,
            username,
            key,
            query,
            after: None,
            before: None,
            source: None,
            skip_attachments,
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
