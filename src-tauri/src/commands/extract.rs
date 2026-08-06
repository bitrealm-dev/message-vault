//! `extract` / `cancel` Tauri commands.
//!
//! `extract` starts the selected exporter on a background thread and returns
//! immediately. Progress streams back as Tauri events:
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
    AppleConfig, ApplePlatform, CancelFlag, ExporterConfig, GoSmsProConfig, ImazingConfig,
    LogSink, MediaConfig, OpenExtractConfig, SmsBackupPlusConfig, SmsBackupRestoreConfig,
    SourceConfig, WhatsappConfig, WhatsappPlatform,
};
use tauri::Emitter;

// Exporter run functions — aliased to keep the dispatch match legible.
use go_sms_pro_exporter::run as run_go_sms_pro;
use imazing_exporter::run as run_imazing;
use imessage_ir_exporter::run as run_imessage;
use openextract_exporter::run as run_openextract;
use sms_backup_plus_exporter::run as run_sms_plus;
use sms_backup_restore_exporter::run as run_sms_restore;
use whatsapp_exporter::run as run_whatsapp;

use super::events::ExtractErrorEvent;
use crate::state::AppState;

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
    // Build the source config from the frontend source id.
    let source_config = match source.as_str() {
        "sms-backup-restore" => SourceConfig::SmsBackupRestore(SmsBackupRestoreConfig {
            owner_phones: Vec::new(),
        }),
        "go-sms-pro" => SourceConfig::GoSmsPro(GoSmsProConfig {
            owner_phones: Vec::new(),
        }),
        "sms-backup-plus" => SourceConfig::SmsBackupPlus(SmsBackupPlusConfig {
            owner_phones: Vec::new(),
            owner_emails: Vec::new(),
            name_mapping: None,
            verbose: false,
            include_summary: false,
        }),
        "openextract" => SourceConfig::OpenExtract(OpenExtractConfig {}),
        "imazing" => SourceConfig::Imazing(ImazingConfig {}),
        "imessage-ios" => SourceConfig::Apple(AppleConfig {
            platform: Some(ApplePlatform::Ios),
            ..Default::default()
        }),
        "imessage-macos" => SourceConfig::Apple(AppleConfig {
            platform: Some(ApplePlatform::MacOs),
            ..Default::default()
        }),
        "whatsapp-android" => SourceConfig::Whatsapp(WhatsappConfig {
            platform: Some(WhatsappPlatform::Android),
            ..Default::default()
        }),
        "whatsapp-ios" => SourceConfig::Whatsapp(WhatsappConfig {
            platform: Some(WhatsappPlatform::Ios),
            ..Default::default()
        }),
        _ => return Err(format!("unsupported source '{source}'")),
    };

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
            output_format: Default::default(),
            source: source_config,
        };

        let result = run_exporter(&config);

        match result {
            Ok(run_result) => {
                let summary = run_result
                    .messages
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "Export complete.".to_string());
                for line in run_result.messages {
                    let _ = app_handle.emit("extract:log", line);
                }
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

/// Dispatch to the correct exporter's `run()` function based on the source
/// config. Mirrors `message-vault-io-gui::jobs::run_exporter()`.
fn run_exporter(config: &ExporterConfig) -> anyhow::Result<message_vault_io_core::RunResult> {
    match &config.source {
        SourceConfig::GoSmsPro(_) => run_go_sms_pro(config),
        SourceConfig::SmsBackupRestore(_) => run_sms_restore(config),
        SourceConfig::SmsBackupPlus(_) => run_sms_plus(config),
        SourceConfig::OpenExtract(_) => run_openextract(config),
        SourceConfig::Imazing(_) => run_imazing(config),
        SourceConfig::Apple(_) => run_imessage(config),
        SourceConfig::Whatsapp(_) => run_whatsapp(config),
        SourceConfig::Format(_) => Err(anyhow::anyhow!("Format conversion not yet wired")),
    }
}
