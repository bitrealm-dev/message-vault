//! `format` Tauri command — wraps `message_reexport::run()` to convert an
//! existing extract directory to a different output format.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;

use message_vault_io_core::{
    CancelFlag, ExporterConfig, FormatConfig, LogSink, MediaConfig, OutputFormat, SourceConfig,
};
use tauri::Emitter;

use super::events::ExtractErrorEvent;
use crate::state::AppState;

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

    // Reset cancel flag
    {
        let st = state.lock().map_err(|e| e.to_string())?;
        st.cancel_flag.store(false, Ordering::SeqCst);
    }

    let cancel: CancelFlag = {
        let st = state.lock().map_err(|e| e.to_string())?;
        st.cancel_flag.clone()
    };

    let app_handle = app.clone();

    thread::spawn(move || {
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
                let summary = run_result
                    .messages
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "Format conversion complete.".to_string());
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
