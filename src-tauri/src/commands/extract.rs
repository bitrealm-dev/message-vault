//! `extract` and `cancel` commands.
//!
//! `extract` starts the selected exporter on a background thread and returns
//! immediately. Progress is sent back as Tauri events:
//! `extract:log` (one log line), `extract:progress` (bar position),
//! `extract:finished` (a summary string or JSON object), and `extract:error`
//! ([`ExtractErrorEvent`]).
//!
//! The shared cancel flag lives in [`AppState`]. `cancel` sets it to true.
//! `extract` turns it off at the start of a job. The exporter checks it
//! between steps through `ExporterConfig.cancel`.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use media::{CompressOptions, MaxResolution, MediaMode};
use message_vault_io_core::{
    ApplePlatform, AttachmentMedia, Exporter, ExporterConfig, Form, GoSmsProConfig, ImazingConfig,
    LogSink, MediaConfig, ObfuscateConfig, OpenExtractConfig, OutputFormat, SmsBackupPlusConfig,
    SmsBackupRestoreConfig, SourceConfig, WhatsappConfig, WhatsappPlatform, parse_date_range,
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
use super::jobs::{reset_and_clone_cancel, spawn_job};
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
    /// Backup source key, for example `imessage-ios` or `whatsapp-android`.
    pub source: String,
    /// Path to the phone backup (a folder, database file, or XML file).
    pub path: String,
    /// Folder the exporter writes conversation files into.
    pub output_dir: String,
    /// Password for encrypted backups, when the source needs one.
    pub backup_password: Option<String>,
    /// Attachment handling choice: `copy`, `convert`, `compress`, or `skip`.
    pub attachment_media: Option<String>,
    /// Video/image size cap for convert and compress: `720p`, `1080p`, or `4k`.
    pub media_max_resolution: Option<String>,
    /// Frame-rate cap for compressed video, for example `30`.
    pub media_max_fps: Option<String>,
    /// Smallest media file size that still counts as an attachment, for example `20M`.
    pub media_min_size: Option<String>,
    /// Conversation filter string passed to the exporter.
    pub conversation_filter: Option<String>,
    /// Export start date, inclusive, in `YYYY-MM-DD` form.
    pub start_date: Option<String>,
    /// Export end date, inclusive, in `YYYY-MM-DD` form.
    pub end_date: Option<String>,
    /// When true, replace names and phone numbers with fake ones.
    pub obfuscate: Option<bool>,
    /// Owner phone numbers for Android SMS exporters (SMS Backup & Restore).
    pub owner_phones: Option<Vec<String>>,
    /// Alternate folder for Attachments and StickerCache (Mac and jailbreak).
    pub attachment_root: Option<String>,
    /// Path to an Apple AddressBook file (Mac and jailbreak).
    pub apple_contacts: Option<String>,
    /// WhatsApp decryption key or key-file path (Android crypt backups).
    pub whatsapp_key: Option<String>,
    /// Optional WhatsApp contacts database (`wa.db` / `ContactsV2.sqlite`).
    pub whatsapp_wa: Option<String>,
    /// Optional WhatsApp media folder.
    pub whatsapp_media: Option<String>,
    /// Optional explicit `msgstore.db` path.
    pub whatsapp_db: Option<String>,
    /// WhatsApp Business backup (iPhone only; Android stays false).
    pub whatsapp_business: Option<bool>,
    /// Continue an interrupted export in the same output folder: previous
    /// output is kept and conversations already written are skipped.
    pub resume: Option<bool>,
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
        owner_phones: args
            .owner_phones
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
        attachment_root: optional_trimmed(args.attachment_root.as_deref())
            .map(str::to_string)
            .unwrap_or_default(),
        apple_contacts: optional_trimmed(args.apple_contacts.as_deref())
            .map(str::to_string)
            .unwrap_or_default(),
        whatsapp_key: optional_trimmed(args.whatsapp_key.as_deref())
            .map(str::to_string)
            .unwrap_or_default(),
        whatsapp_wa: optional_trimmed(args.whatsapp_wa.as_deref())
            .map(str::to_string)
            .unwrap_or_default(),
        whatsapp_media: optional_trimmed(args.whatsapp_media.as_deref())
            .map(str::to_string)
            .unwrap_or_default(),
        whatsapp_db: optional_trimmed(args.whatsapp_db.as_deref())
            .map(str::to_string)
            .unwrap_or_default(),
        whatsapp_business: args.whatsapp_business.unwrap_or(false),
    };

    let output_dir = args.output_dir;
    let mut config = build_exporter_config(&args.source, &args.path, &output_dir, &options)?;
    config.resume = args.resume.unwrap_or(false);

    let cancel = reset_and_clone_cancel(&state)?;

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

    spawn_job(app, move || {
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
            Err(err) => return Err(err),
        }
        Ok(())
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
    owner_phones: Vec<String>,
    attachment_root: String,
    apple_contacts: String,
    whatsapp_key: String,
    whatsapp_wa: String,
    whatsapp_media: String,
    whatsapp_db: String,
    whatsapp_business: bool,
}

/// Parse the attachment handling choice from the Extract form.
///
/// The UI says "copy" and "skip". The exporter config uses "clone" and
/// "disabled" for those same choices.
///
/// # Errors
///
/// Returns an error if the string is not copy, convert, compress, or skip.
pub(crate) fn parse_attachment_media(raw: Option<&str>) -> Result<AttachmentMedia, String> {
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
pub(crate) fn parse_max_resolution(raw: Option<&str>) -> Result<MaxResolution, String> {
    let Some(raw) = optional_trimmed(raw) else {
        return Ok(MaxResolution::default());
    };
    MaxResolution::parse(raw).ok_or_else(|| {
        format!("invalid media_max_resolution '{raw}' (expected 720p, 1080p, or 4k)")
    })
}

/// Media mode the exporter is asked for.
///
/// Convert and Compress become Clone: the desktop stages originals, shows the
/// first gate, and runs the media pass itself, so the expensive work happens
/// after the user has approved it rather than before. Copy and Skip have no
/// media step and reach the exporter unchanged.
pub(crate) fn exporter_media_mode(chosen: AttachmentMedia) -> MediaMode {
    exporter_attachment_media(chosen).media_mode()
}

/// `AttachmentMedia` the exporter's `Form` is asked for.
///
/// Same mapping as [`exporter_media_mode`], kept in `AttachmentMedia`'s own
/// domain because `Form::attachment_media` also drives the iMessage path's
/// upfront ffmpeg-availability check and Apple `copy_method` — both of which
/// must not see Convert or Compress either, or the exporter would demand
/// ffmpeg (and stage a converted file) before the user has approved anything.
fn exporter_attachment_media(chosen: AttachmentMedia) -> AttachmentMedia {
    match chosen {
        AttachmentMedia::Convert | AttachmentMedia::Compress => AttachmentMedia::Clone,
        other => other,
    }
}

/// Build the `CompressOptions` a media pass will use, from the same
/// max-resolution/fps/min-size fields the Extract form parses.
///
/// `CompressOptions` only takes effect under [`MediaMode::Compress`] — this
/// mirrors `build_exporter_config`'s non-iMessage branch, which built the
/// real options only when `Compress` was chosen and used
/// `CompressOptions::default()` otherwise. Shared so the desktop's own media
/// pass (`commands::staging`) parses these fields the same way Extract does,
/// rather than re-deriving the parsing.
///
/// # Errors
///
/// Returns an error if `max_fps` is not a number or `min_size` cannot be
/// parsed as a byte size.
pub(crate) fn parse_compress_options(
    chosen: AttachmentMedia,
    max_resolution: MaxResolution,
    max_fps: &str,
    min_size: &str,
) -> Result<CompressOptions, String> {
    if !matches!(chosen, AttachmentMedia::Compress) {
        return Ok(CompressOptions::default());
    }
    let fps = max_fps
        .parse::<f32>()
        .map_err(|_| format!("invalid media_max_fps '{max_fps}'"))?;
    media::compress_options_from_cli(max_resolution, fps, min_size, true).map_err(|e| e.to_string())
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
        "imessage-ios" | "imessage-macos" | "imessage-jailbreak" => {
            let form = Form {
                db_path: path.to_string(),
                output: output_dir.to_string(),
                apple_platform: if source == "imessage-ios" {
                    ApplePlatform::Ios
                } else {
                    ApplePlatform::MacOs
                },
                backup_password: if source == "imessage-ios" {
                    options.backup_password.clone()
                } else {
                    String::new()
                },
                attachment_root: if source == "imessage-ios" {
                    String::new()
                } else {
                    options.attachment_root.clone()
                },
                apple_contacts: if source == "imessage-ios" {
                    String::new()
                } else {
                    options.apple_contacts.clone()
                },
                // See `exporter_media_mode`'s docs: the exporter is asked for
                // Clone whenever the user chose Convert or Compress, so it
                // stages originals and the desktop runs the media pass
                // itself, after the gate.
                attachment_media: exporter_attachment_media(options.attachment_media),
                media_max_resolution: options.media_max_resolution,
                media_max_fps: options.media_max_fps.clone(),
                media_min_size: options.media_min_size.clone(),
                conversation_filter: options.conversation_filter.clone(),
                start_date: options.start_date.clone(),
                end_date: options.end_date.clone(),
                obfuscate: source == "imessage-ios" && options.obfuscate,
                // Import and Push read conversation files as JSON Lines (one JSON
                // object per line).
                output_format: OutputFormat::Jsonl,
                ..Default::default()
            };
            // `Form`'s own compress validation only fires when
            // `Form.attachment_media` is `Compress` — and that field now
            // reads `Clone` for a real Convert/Compress choice (see
            // `exporter_attachment_media`'s docs), so it would otherwise stay
            // silent about a malformed `media_max_fps`/`media_min_size`
            // until the desktop's own media pass parses the same fields
            // again at the approval gate, hours later. Validate against the
            // REAL chosen mode here so a bad value still fails immediately;
            // the parsed value itself is unused here — the exporter's own
            // media step is a no-op under Clone.
            parse_compress_options(
                options.attachment_media,
                options.media_max_resolution,
                &options.media_max_fps,
                &options.media_min_size,
            )?;
            form.to_config(Exporter::Imessage)
                .map_err(|errors| errors.join("; "))
        }
        other => {
            let source_config = match other {
                "sms-backup-restore" => {
                    if options.owner_phones.is_empty() {
                        return Err(
                            "SMS Backup & Restore requires at least one backup device phone number"
                                .into(),
                        );
                    }
                    SourceConfig::SmsBackupRestore(SmsBackupRestoreConfig {
                        owner_phones: options.owner_phones.clone(),
                    })
                }
                "go-sms-pro" => SourceConfig::GoSmsPro(GoSmsProConfig {
                    owner_phones: options.owner_phones.clone(),
                }),
                "sms-backup-plus" => SourceConfig::SmsBackupPlus(SmsBackupPlusConfig {
                    owner_phones: options.owner_phones.clone(),
                    owner_emails: Vec::new(),
                    name_mapping: None,
                    verbose: false,
                    include_summary: false,
                }),
                "openextract" => SourceConfig::OpenExtract(OpenExtractConfig {}),
                "imazing" => SourceConfig::Imazing(ImazingConfig {}),
                "whatsapp-android" => SourceConfig::Whatsapp(WhatsappConfig {
                    platform: Some(WhatsappPlatform::Android),
                    key: nonempty(&options.whatsapp_key).map(str::to_string),
                    backup: None,
                    wa: nonempty(&options.whatsapp_wa).map(PathBuf::from),
                    media: nonempty(&options.whatsapp_media).map(PathBuf::from),
                    db: nonempty(&options.whatsapp_db).map(PathBuf::from),
                    business: false,
                    ..Default::default()
                }),
                "whatsapp-ios" => SourceConfig::Whatsapp(WhatsappConfig {
                    platform: Some(WhatsappPlatform::Ios),
                    backup: Some(PathBuf::from(path)),
                    wa: nonempty(&options.whatsapp_wa).map(PathBuf::from),
                    media: None,
                    db: None,
                    business: options.whatsapp_business,
                    ..Default::default()
                }),
                _ => return Err(format!("unsupported source '{source}'")),
            };

            let start_date = nonempty(&options.start_date);
            let end_date = nonempty(&options.end_date);
            let date_range = parse_date_range(start_date, end_date)?;

            let compress = parse_compress_options(
                options.attachment_media,
                options.media_max_resolution,
                &options.media_max_fps,
                &options.media_min_size,
            )?;

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
                    mode: exporter_media_mode(options.attachment_media),
                    compress,
                },
                cancel: None,
                log: None,
                output_format: OutputFormat::Jsonl,
                resume: false,
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

    fn test_options(owner_phones: Vec<String>) -> ExtractOptions {
        ExtractOptions {
            backup_password: String::new(),
            attachment_media: AttachmentMedia::default(),
            media_max_resolution: MaxResolution::default(),
            media_max_fps: "30".into(),
            media_min_size: "20M".into(),
            conversation_filter: String::new(),
            start_date: String::new(),
            end_date: String::new(),
            obfuscate: false,
            owner_phones,
            attachment_root: String::new(),
            apple_contacts: String::new(),
            whatsapp_key: String::new(),
            whatsapp_wa: String::new(),
            whatsapp_media: String::new(),
            whatsapp_db: String::new(),
            whatsapp_business: false,
        }
    }

    #[test]
    fn convert_and_compress_stage_originals_and_defer_the_media_step() {
        // The desktop runs conversion as its own pass so a gate can sit in
        // front of it. Asking the exporter to convert would spend the time
        // before the user has approved anything. Checked against the
        // iMessage source, which routes attachment_media through `Form` —
        // the only path that also exercises `exporter_attachment_media`.
        for chosen in [AttachmentMedia::Convert, AttachmentMedia::Compress] {
            let mut options = test_options(vec!["+15550100".into()]);
            options.attachment_media = chosen;
            let config =
                build_exporter_config("imessage-ios", "/backup", "/out", &options).unwrap();
            assert_eq!(
                config.media.mode,
                MediaMode::Clone,
                "{chosen:?} must stage originals"
            );
        }
    }

    #[test]
    fn copy_and_skip_reach_the_exporter_unchanged() {
        for chosen in [AttachmentMedia::Clone, AttachmentMedia::Disabled] {
            let mut options = test_options(vec!["+15550100".into()]);
            options.attachment_media = chosen;
            let config =
                build_exporter_config("imessage-ios", "/backup", "/out", &options).unwrap();
            assert_eq!(
                config.media.mode,
                chosen.media_mode(),
                "{chosen:?} must reach the exporter unchanged"
            );
        }
    }

    #[test]
    fn non_imessage_sources_defer_the_media_step_too() {
        // `exporter_media_mode` also gates `MediaConfig.mode` on the
        // non-iMessage branch of `build_exporter_config` (whatsapp-android
        // here), which does not go through `Form`/`exporter_attachment_media`
        // at all.
        for chosen in [AttachmentMedia::Convert, AttachmentMedia::Compress] {
            let mut options = test_options(Vec::new());
            options.attachment_media = chosen;
            let config =
                build_exporter_config("whatsapp-android", "/tmp/android-dump", "/out", &options)
                    .unwrap();
            assert_eq!(
                config.media.mode,
                MediaMode::Clone,
                "{chosen:?} must stage originals"
            );
        }
    }

    #[test]
    fn imessage_compress_still_validates_media_fields_up_front() {
        // `Form.attachment_media` reads Clone for a real Compress choice (so
        // the exporter stages originals instead of converting), which means
        // `Form`'s own compress validation no longer runs for it. Without the
        // explicit `parse_compress_options` call in `build_exporter_config`,
        // a malformed `media_min_size` would sail through here and only
        // surface hours later, at the approval gate.
        let mut options = test_options(Vec::new());
        options.attachment_media = AttachmentMedia::Compress;
        options.media_min_size = "banana".into();
        let err = build_exporter_config("imessage-ios", "/backup", "/out", &options).unwrap_err();
        assert!(
            err.contains("banana"),
            "expected the malformed min-size value to be named: {err}"
        );
    }

    #[test]
    fn jailbreak_uses_macos_platform_and_attachment_root() {
        let mut options = test_options(Vec::new());
        options.attachment_root = "/mnt/iphone/Library/SMS".into();
        options.apple_contacts = "/mnt/iphone/AddressBook.sqlitedb".into();
        options.obfuscate = true;
        let config = build_exporter_config(
            "imessage-jailbreak",
            "/mnt/iphone/sms.db",
            "/tmp/out",
            &options,
        )
        .unwrap();
        match config.source {
            SourceConfig::Apple(apple) => {
                assert_eq!(apple.platform, Some(ApplePlatform::MacOs));
                assert_eq!(
                    apple.attachment_root.as_deref(),
                    Some("/mnt/iphone/Library/SMS")
                );
                assert_eq!(
                    apple.apple_contacts.as_deref(),
                    Some(std::path::Path::new("/mnt/iphone/AddressBook.sqlitedb"))
                );
                assert!(apple.backup_password.is_none());
            }
            other => panic!("expected Apple, got {other:?}"),
        }
        assert!(!config.obfuscate.enabled);
    }

    #[test]
    fn ios_backup_does_not_forward_attachment_root() {
        let mut options = test_options(Vec::new());
        options.attachment_root = "/ignored".into();
        options.apple_contacts = "/ignored-contacts".into();
        options.backup_password = "pw".into();
        let config =
            build_exporter_config("imessage-ios", "/backups/iphone", "/tmp/out", &options).unwrap();
        match config.source {
            SourceConfig::Apple(apple) => {
                assert_eq!(apple.platform, Some(ApplePlatform::Ios));
                assert_eq!(apple.backup_password.as_deref(), Some("pw"));
                // extract.rs blanks both extras for imessage-ios.
                assert!(apple.attachment_root.is_none());
                assert!(apple.apple_contacts.is_none());
            }
            other => panic!("expected Apple, got {other:?}"),
        }
    }

    #[test]
    fn macos_forwards_optional_attachment_root() {
        let mut options = test_options(Vec::new());
        options.attachment_root = "/Users/sam/Library/Messages".into();
        let config = build_exporter_config(
            "imessage-macos",
            "/Users/sam/Library/Messages/chat.db",
            "/tmp/out",
            &options,
        )
        .unwrap();
        match config.source {
            SourceConfig::Apple(apple) => {
                assert_eq!(apple.platform, Some(ApplePlatform::MacOs));
                assert_eq!(
                    apple.attachment_root.as_deref(),
                    Some("/Users/sam/Library/Messages")
                );
            }
            other => panic!("expected Apple, got {other:?}"),
        }
    }

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

    #[test]
    fn sms_backup_restore_requires_owner_phones() {
        let err = build_exporter_config(
            "sms-backup-restore",
            "/tmp/backup",
            "/tmp/out",
            &test_options(Vec::new()),
        )
        .unwrap_err();
        assert!(
            err.contains("phone number"),
            "expected phone requirement error, got {err}"
        );
    }

    #[test]
    fn sms_backup_restore_passes_owner_phones() {
        let config = build_exporter_config(
            "sms-backup-restore",
            "/tmp/backup",
            "/tmp/out",
            &test_options(vec!["+15551111".into(), "+15552222".into()]),
        )
        .unwrap();
        match config.source {
            SourceConfig::SmsBackupRestore(s) => {
                assert_eq!(s.owner_phones, vec!["+15551111", "+15552222"]);
            }
            other => panic!("expected SmsBackupRestore, got {other:?}"),
        }
    }

    #[test]
    fn whatsapp_android_forwards_key_and_optional_paths() {
        let mut options = test_options(Vec::new());
        options.whatsapp_key = "deadbeef".into();
        options.whatsapp_wa = "/tmp/wa.db".into();
        options.whatsapp_media = "/tmp/WhatsApp".into();
        options.whatsapp_db = "/tmp/msgstore.db".into();
        options.whatsapp_business = true;
        let config = build_exporter_config(
            "whatsapp-android",
            "/tmp/android-dump",
            "/tmp/out",
            &options,
        )
        .unwrap();
        match config.source {
            SourceConfig::Whatsapp(wa) => {
                assert_eq!(wa.platform, Some(WhatsappPlatform::Android));
                assert_eq!(wa.key.as_deref(), Some("deadbeef"));
                assert_eq!(wa.wa.as_deref(), Some(std::path::Path::new("/tmp/wa.db")));
                assert_eq!(
                    wa.media.as_deref(),
                    Some(std::path::Path::new("/tmp/WhatsApp"))
                );
                assert_eq!(
                    wa.db.as_deref(),
                    Some(std::path::Path::new("/tmp/msgstore.db"))
                );
                assert!(wa.backup.is_none());
                assert!(!wa.business);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn whatsapp_ios_omits_leftover_android_media_and_db() {
        let mut options = test_options(Vec::new());
        options.whatsapp_media = "/tmp/WhatsApp".into();
        options.whatsapp_db = "/tmp/msgstore.db".into();
        options.whatsapp_wa = "/tmp/ContactsV2.sqlite".into();
        let config =
            build_exporter_config("whatsapp-ios", "/tmp/ios-backup", "/tmp/out", &options).unwrap();
        match config.source {
            SourceConfig::Whatsapp(wa) => {
                assert!(wa.media.is_none());
                assert!(wa.db.is_none());
                assert_eq!(
                    wa.wa.as_deref(),
                    Some(std::path::Path::new("/tmp/ContactsV2.sqlite"))
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn whatsapp_ios_sets_backup_from_folder_and_business() {
        let mut options = test_options(Vec::new());
        options.whatsapp_business = true;
        let config =
            build_exporter_config("whatsapp-ios", "/tmp/ios-backup", "/tmp/out", &options).unwrap();
        match config.source {
            SourceConfig::Whatsapp(wa) => {
                assert_eq!(wa.platform, Some(WhatsappPlatform::Ios));
                assert_eq!(
                    wa.backup.as_deref(),
                    Some(std::path::Path::new("/tmp/ios-backup"))
                );
                assert!(wa.business);
                assert!(wa.key.is_none());
            }
            other => panic!("{other:?}"),
        }
    }
}
