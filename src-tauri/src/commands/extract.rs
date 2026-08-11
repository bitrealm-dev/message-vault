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

use media::MaxResolution;
use message_vault_io_core::{
    ApplePlatform, AttachmentMedia, CancelFlag, Exporter, ExporterConfig, Form, GoSmsProConfig,
    ImazingConfig, LogSink, MediaConfig, ObfuscateConfig, OpenExtractConfig, OutputFormat,
    SmsBackupPlusConfig, SmsBackupRestoreConfig, SourceConfig, WhatsappConfig, WhatsappPlatform,
    parse_date_range,
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

use super::events::{ExtractErrorEvent, ExtractProgressEvent};
use crate::state::AppState;

#[tauri::command]
pub async fn cancel(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.cancel_flag.store(true, Ordering::SeqCst);
    Ok(())
}

/// Start an extraction job. Returns immediately; progress is emitted as
/// `extract:log` / `extract:finished` / `extract:error` events.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn extract(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
    source: String,
    path: String,
    output_dir: String,
    backup_password: Option<String>,
    attachment_media: Option<String>,
    media_max_resolution: Option<String>,
    media_max_fps: Option<String>,
    media_min_size: Option<String>,
    conversation_filter: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    obfuscate: Option<bool>,
) -> Result<(), String> {
    let options = ExtractOptions {
        backup_password: backup_password.unwrap_or_default(),
        attachment_media: parse_attachment_media(attachment_media.as_deref())?,
        media_max_resolution: parse_max_resolution(media_max_resolution.as_deref())?,
        media_max_fps: media_max_fps.unwrap_or_else(|| "30".into()),
        media_min_size: media_min_size.unwrap_or_else(|| "20M".into()),
        conversation_filter: conversation_filter.unwrap_or_default(),
        start_date: start_date.unwrap_or_default(),
        end_date: end_date.unwrap_or_default(),
        obfuscate: obfuscate.unwrap_or(false),
    };

    let mut config = build_exporter_config(&source, &path, &output_dir, &options)?;

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

    thread::spawn(move || {
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

struct ExtractOptions {
    backup_password: String,
    attachment_media: AttachmentMedia,
    media_max_resolution: MaxResolution,
    media_max_fps: String,
    media_min_size: String,
    conversation_filter: String,
    start_date: String,
    end_date: String,
    obfuscate: bool,
}

fn parse_attachment_media(raw: Option<&str>) -> Result<AttachmentMedia, String> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(AttachmentMedia::default());
    };
    let lowered = raw.to_ascii_lowercase();
    let key = match lowered.as_str() {
        "copy" => "clone",
        "skip" => "disabled",
        other => other,
    };
    AttachmentMedia::from_ini_str(key).ok_or_else(|| {
        format!("invalid attachment_media '{raw}' (expected copy, convert, compress, or skip)")
    })
}

fn parse_max_resolution(raw: Option<&str>) -> Result<MaxResolution, String> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(MaxResolution::default());
    };
    MaxResolution::parse(raw)
        .ok_or_else(|| format!("invalid media_max_resolution '{raw}' (expected 720p, 1080p, or 4k)"))
}

fn build_exporter_config(
    source: &str,
    path: &str,
    output_dir: &str,
    options: &ExtractOptions,
) -> Result<ExporterConfig, String> {
    match source {
        "imessage-ios" | "imessage-macos" => {
            let mut form = Form::default();
            form.db_path = path.to_string();
            form.output = output_dir.to_string();
            form.apple_platform = if source == "imessage-ios" {
                ApplePlatform::Ios
            } else {
                ApplePlatform::MacOs
            };
            form.backup_password = options.backup_password.clone();
            form.attachment_media = options.attachment_media;
            form.media_max_resolution = options.media_max_resolution;
            form.media_max_fps = options.media_max_fps.clone();
            form.media_min_size = options.media_min_size.clone();
            form.conversation_filter = options.conversation_filter.clone();
            form.start_date = options.start_date.clone();
            form.end_date = options.end_date.clone();
            form.obfuscate = options.obfuscate;
            // Import / push pipeline reads JSONL conversation files.
            form.output_format = OutputFormat::Jsonl;
            form.to_config(Exporter::Imessage)
                .map_err(|errors| errors.join("; "))
        }
        other => {
            let source_config = match other {
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

            let date_range = parse_date_range(
                nonempty(&options.start_date),
                nonempty(&options.end_date),
            )?;

            let compress = if matches!(options.attachment_media, AttachmentMedia::Compress) {
                media::compress_options_from_cli(
                    options.media_max_resolution,
                    options
                        .media_max_fps
                        .parse::<f32>()
                        .map_err(|_| format!("invalid media_max_fps '{}'", options.media_max_fps))?,
                    &options.media_min_size,
                    true,
                )
                .map_err(|e| e.to_string())?
            } else {
                media::CompressOptions::default()
            };

            Ok(ExporterConfig {
                inputs: vec![PathBuf::from(path)],
                output: PathBuf::from(output_dir),
                date_range,
                timezone: None,
                contacts: None,
                obfuscate: ObfuscateConfig {
                    enabled: options.obfuscate,
                    seed: None,
                },
                media: MediaConfig {
                    mode: options.attachment_media.media_mode(),
                    compress,
                },
                cancel: None,
                log: None,
                output_format: OutputFormat::Jsonl,
                source: source_config,
            })
        }
    }
}

fn nonempty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t) }
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

#[derive(Copy, Clone, Eq, PartialEq)]
enum ExtractProgressStage {
    Parse,
    Convert,
}

fn extract_progress_from_log(
    line: &str,
    stage: &Arc<Mutex<ExtractProgressStage>>,
) -> Option<ExtractProgressEvent> {
    if line.contains("Writing ") && line.contains("conversation file(s)") {
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

    let Some((done, total)) = extract_progress_ratio(line) else {
        return None;
    };

    let step = match stage
        .lock()
        .map(|current_stage| *current_stage)
        .unwrap_or(ExtractProgressStage::Parse)
    {
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

fn extract_progress_ratio(line: &str) -> Option<(usize, usize)> {
    if !(line.contains('…') || line.contains("messages") || line.contains("wrote")) {
        return None;
    }

    let (left, right) = line.split_once('/')?;
    let done = trailing_usize(left)?;
    let total = leading_usize(right)?;
    Some((done, total))
}

fn trailing_usize(text: &str) -> Option<usize> {
    let digits: String = text
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.chars().rev().collect::<String>().parse().ok()
}

fn leading_usize(text: &str) -> Option<usize> {
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
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

        let banner = extract_progress_from_log("Writing 3 conversation file(s)...", &stage).unwrap();
        assert_eq!(banner.step, "convert");
        assert_eq!(banner.done, 0);
        assert_eq!(banner.total, 0);
        assert_eq!(banner.status.as_deref(), Some("included_in_extract"));

        let ignored = extract_progress_from_log("[1/5] Deriving backup keys...", &stage);
        assert!(ignored.is_none());

        let convert = extract_progress_from_log("  wrote 2/3 messages", &stage).unwrap();
        assert_eq!(convert.step, "convert");
        assert_eq!(convert.done, 2);
        assert_eq!(convert.total, 3);
        assert_eq!(convert.status, None);
    }
}
