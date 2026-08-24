//! `extract` and `cancel` commands.
//!
//! `extract` starts the selected exporter on a background thread and returns
//! immediately. Progress is sent back as Tauri events:
//! `extract:log` (one log line), `extract:finished` (a summary string or JSON
//! object), and `extract:error` ([`ExtractErrorEvent`]).
//!
//! The shared cancel flag lives in [`AppState`]. `cancel` sets it to true.
//! `extract` turns it off at the start of a job. The exporter checks it
//! between steps through `ExporterConfig.cancel`.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
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

// Short names so the match in `run_exporter` stays easy to read.
use go_sms_pro_exporter::run as run_go_sms_pro;
use imazing_exporter::run as run_imazing;
use imessage_ir_exporter::run as run_imessage;
use openextract_exporter::run as run_openextract;
use sms_backup_plus_exporter::run as run_sms_plus;
use sms_backup_restore_exporter::run as run_sms_restore;
use whatsapp_exporter::run as run_whatsapp;

use super::events::ExtractErrorEvent;
use super::progress::{ExtractProgressStage, extract_progress_from_log};
use super::{last_log_line_or, optional_trimmed};
use crate::state::AppState;

/// Ask this process to stop the export that is currently running.
///
/// Sets the shared cancel flag. The exporter checks the flag between steps
/// and exits on its own. There is no hard kill.
///
/// # Errors
///
/// Returns an error if another thread panicked while holding the shared
/// state lock.
#[tauri::command]
pub async fn cancel(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.cancel_flag.store(true, Ordering::SeqCst);
    Ok(())
}

/// How many conversation files and messages an extract wrote.
///
/// Each JSON Lines file (one JSON object per line) starts with a conversation
/// header. That header is not counted as a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct JsonlOutputCounts {
    files: usize,
    messages: usize,
}

/// True when `path` looks like a JSON Lines file (one JSON object per line).
fn is_json_lines_file(path: &Path) -> bool {
    let Some(extension) = path.extension() else {
        return false;
    };
    let Some(extension) = extension.to_str() else {
        return false;
    };
    extension == "jsonl"
}

/// Walk `root` and count JSON Lines conversation files and the messages in them.
///
/// The first non-empty line of each file is the conversation header, so it is
/// subtracted from the message total.
///
/// # Errors
///
/// Returns an error if a directory cannot be listed or a file cannot be opened.
fn count_jsonl_output(root: &Path) -> anyhow::Result<JsonlOutputCounts> {
    let mut counts = JsonlOutputCounts {
        files: 0,
        messages: 0,
    };
    let mut directories = vec![root.to_path_buf()];

    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                directories.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if !is_json_lines_file(&entry.path()) {
                continue;
            }

            let mut reader = BufReader::new(File::open(entry.path())?);
            let mut line = String::new();
            let mut nonempty_lines = 0usize;
            while reader.read_line(&mut line)? != 0 {
                if !line.trim().is_empty() {
                    nonempty_lines = nonempty_lines.saturating_add(1);
                }
                line.clear();
            }
            if nonempty_lines > 0 {
                counts.files = counts.files.saturating_add(1);
                let message_lines = nonempty_lines.saturating_sub(1);
                counts.messages = counts.messages.saturating_add(message_lines);
            }
        }
    }

    Ok(counts)
}

/// User-facing parameters for the `extract` command (before defaults/parsing).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractArgs {
    pub source: String,
    pub path: String,
    pub output_dir: String,
    pub backup_password: Option<String>,
    pub attachment_media: Option<String>,
    pub media_max_resolution: Option<String>,
    pub media_max_fps: Option<String>,
    pub media_min_size: Option<String>,
    pub conversation_filter: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub obfuscate: Option<bool>,
}

/// Ask this process to parse a phone backup and write conversation files.
///
/// Returns as soon as the background thread starts. Log lines, progress, and
/// the final summary are sent as `extract:log`, `extract:progress`,
/// `extract:finished`, and `extract:error`. Output is JSON Lines (one JSON
/// object per line) so the Import and Push screens can read it later.
///
/// # Errors
///
/// Returns an error if a form field is invalid, the source is unknown, or
/// another thread panicked while holding the shared state lock. Failures
/// during the export itself are sent as `extract:error`, not returned here.
#[tauri::command]
pub async fn extract(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
    args: ExtractArgs,
) -> Result<(), String> {
    let options = ExtractOptions {
        backup_password: args.backup_password.unwrap_or_default(),
        attachment_media: parse_attachment_media(args.attachment_media.as_deref())?,
        media_max_resolution: parse_max_resolution(args.media_max_resolution.as_deref())?,
        media_max_fps: args.media_max_fps.unwrap_or_else(|| "30".into()),
        media_min_size: args.media_min_size.unwrap_or_else(|| "20M".into()),
        conversation_filter: args.conversation_filter.unwrap_or_default(),
        start_date: args.start_date.unwrap_or_default(),
        end_date: args.end_date.unwrap_or_default(),
        obfuscate: args.obfuscate.unwrap_or(false),
    };

    let output_dir = args.output_dir;
    let mut config = build_exporter_config(&args.source, &args.path, &output_dir, &options)?;

    // Clear a leftover cancel from a previous job. Otherwise this new export
    // would stop immediately.
    {
        let st = state.lock().map_err(|e| e.to_string())?;
        st.cancel_flag.store(false, Ordering::SeqCst);
    }

    // Share the same cancel flag with the background thread. The cancel
    // command sets it; the exporter reads it.
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
                let summary = last_log_line_or(&run_result.messages, "Export complete.");
                for line in run_result.messages {
                    let _ = app_handle.emit("extract:log", line);
                }
                match count_jsonl_output(Path::new(&output_dir)) {
                    Ok(counts) => {
                        let payload = serde_json::json!({
                            "summary": summary,
                            "files_parsed": counts.files,
                            "messages_parsed": counts.messages,
                        });
                        let _ = app_handle.emit("extract:finished", payload.to_string());
                    }
                    Err(err) => {
                        let _ = app_handle.emit(
                            "extract:error",
                            ExtractErrorEvent {
                                detail: format!(
                                    "count extracted JSON Lines records in {output_dir}: {err:#}"
                                ),
                                user_message: Some(
                                    "Extraction completed, but the generated message count could not be verified."
                                        .into(),
                                ),
                            },
                        );
                    }
                }
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

/// Form fields from the Extract screen after defaults are filled in.
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

/// Parse the attachment handling choice from the Extract form.
///
/// The UI says "copy" and "skip". The exporter config uses "clone" and
/// "disabled" for those same choices.
///
/// # Errors
///
/// Returns an error if the string is not copy, convert, compress, or skip.
fn parse_attachment_media(raw: Option<&str>) -> Result<AttachmentMedia, String> {
    let Some(raw) = optional_trimmed(raw) else {
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

/// Parse the max video/image size from the Extract form.
///
/// # Errors
///
/// Returns an error if the string is not 720p, 1080p, or 4k.
fn parse_max_resolution(raw: Option<&str>) -> Result<MaxResolution, String> {
    let Some(raw) = optional_trimmed(raw) else {
        return Ok(MaxResolution::default());
    };
    MaxResolution::parse(raw).ok_or_else(|| {
        format!("invalid media_max_resolution '{raw}' (expected 720p, 1080p, or 4k)")
    })
}

/// Build the exporter config the background thread will run.
///
/// iMessage uses the shared form helper. Other sources fill `ExporterConfig`
/// directly. Every path writes JSON Lines (one JSON object per line).
///
/// # Errors
///
/// Returns an error if the source is unknown, a date cannot be parsed, or
/// compress options are invalid.
fn build_exporter_config(
    source: &str,
    path: &str,
    output_dir: &str,
    options: &ExtractOptions,
) -> Result<ExporterConfig, String> {
    match source {
        "imessage-ios" | "imessage-macos" => {
            let form = Form {
                db_path: path.to_string(),
                output: output_dir.to_string(),
                apple_platform: if source == "imessage-ios" {
                    ApplePlatform::Ios
                } else {
                    ApplePlatform::MacOs
                },
                backup_password: options.backup_password.clone(),
                attachment_media: options.attachment_media,
                media_max_resolution: options.media_max_resolution,
                media_max_fps: options.media_max_fps.clone(),
                media_min_size: options.media_min_size.clone(),
                conversation_filter: options.conversation_filter.clone(),
                start_date: options.start_date.clone(),
                end_date: options.end_date.clone(),
                obfuscate: options.obfuscate,
                // Import and Push read conversation files as JSON Lines (one JSON
                // object per line).
                output_format: OutputFormat::Jsonl,
                ..Default::default()
            };
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

            let start_date = nonempty(&options.start_date);
            let end_date = nonempty(&options.end_date);
            let date_range = parse_date_range(start_date, end_date)?;

            let compress = if matches!(options.attachment_media, AttachmentMedia::Compress) {
                let fps = options
                    .media_max_fps
                    .parse::<f32>()
                    .map_err(|_| format!("invalid media_max_fps '{}'", options.media_max_fps))?;
                media::compress_options_from_cli(
                    options.media_max_resolution,
                    fps,
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

/// Return `s` trimmed, or `None` when it is empty.
fn nonempty(s: &str) -> Option<&str> {
    optional_trimmed(Some(s))
}

/// Call the exporter that matches `config.source`.
///
/// # Errors
///
/// Returns an error if the exporter fails, or if the source is format
/// conversion (that job uses the `format` command instead).
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn counts_exact_messages_written_to_jsonl_output() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("message-vault-extract-count-{unique}"));
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(
            root.join("one.jsonl"),
            "{\"conversation\":{}}\n{\"guid\":\"one\"}\n{\"guid\":\"two\"}\n",
        )
        .unwrap();
        fs::write(
            root.join("nested/two.jsonl"),
            "{\"conversation\":{}}\n{\"guid\":\"three\"}\n",
        )
        .unwrap();
        fs::write(root.join("ignored.txt"), "not jsonl\n").unwrap();

        let counts = count_jsonl_output(&root).unwrap();

        assert_eq!(counts.files, 2);
        assert_eq!(counts.messages, 3);
        fs::remove_dir_all(root).unwrap();
    }
}
