//! `extract` / `cancel` Tauri commands.
//!
//! `extract` starts the SMS Backup & Restore exporter on a background thread
//! and returns immediately. Progress streams back as Tauri events:
//! `extract:log` (String line), `extract:finished` (String summary), and
//! `extract:error` (`ExtractErrorEvent`).
//!
//! The shared cancel flag lives in `AppState` as `CancelFlag`
//! (`Arc<AtomicBool>`): `cancel` flips it, `extract` resets it at job start,
//! and the exporter polls it cooperatively via `ExporterConfig.cancel`.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;

use message_vault_io_core::{
    CancelFlag, ExporterConfig, LogSink, MediaConfig, OutputFormat, SmsBackupRestoreConfig,
    SourceConfig,
};
use tauri::Emitter;

use super::events::ExtractErrorEvent;
use crate::state::AppState;

/// The `source` id the frontend sends for SMS Backup & Restore
/// (see `web/src/lib/types.ts` and the Task 5 Extract screen).
const SOURCE_SMS_BACKUP_RESTORE: &str = "sms-backup-restore";

#[tauri::command]
pub async fn cancel(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.cancel_flag.store(true, Ordering::SeqCst);
    Ok(())
}

/// Start an extraction job. Returns immediately; progress is emitted as
/// `extract:log` / `extract:finished` / `extract:error` events.
#[tauri::command]
pub async fn extract(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
    source: String,
    path: String,
    output_dir: String,
) -> Result<(), String> {
    // Only SMS Backup & Restore is wired so far; later tasks extend this dispatch.
    if source != SOURCE_SMS_BACKUP_RESTORE {
        return Err(format!(
            "unsupported source '{source}' (expected '{SOURCE_SMS_BACKUP_RESTORE}')"
        ));
    }

    // Reset the shared cancel flag so a previous run's cancel doesn't abort
    // this one. The job below polls this flag during the export.
    {
        let st = state.lock().map_err(|e| e.to_string())?;
        st.cancel_flag.store(false, Ordering::SeqCst);
    }

    // Cloning the flag shares the atomic — the cancel command flips it, the
    // exporter polls it.
    let cancel: CancelFlag = {
        let st = state.lock().map_err(|e| e.to_string())?;
        st.cancel_flag.clone()
    };

    let app_handle = app.clone();

    thread::spawn(move || {
        let log_app = app_handle.clone();
        let config = ExporterConfig {
            inputs: vec![PathBuf::from(&path)],
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
            output_format: OutputFormat::default(),
            source: SourceConfig::SmsBackupRestore(SmsBackupRestoreConfig {
                owner_phones: Vec::new(),
            }),
        };

        match sms_backup_restore_exporter::run(&config) {
            Ok(result) => {
                // `RunResult.messages` holds the exporter's report lines (skips,
                // attachment counts, and the final "Wrote ... export under ..."
                // summary). Forward them all to the log, then re-emit the last
                // line as the finished summary.
                let summary = result
                    .messages
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "Export complete.".to_string());
                for line in result.messages {
                    let _ = app_handle.emit("extract:log", line);
                }
                let _ = app_handle.emit("extract:finished", summary);
            }
            Err(err) => {
                // `{:#}` prints the anyhow error chain (cause + context).
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
