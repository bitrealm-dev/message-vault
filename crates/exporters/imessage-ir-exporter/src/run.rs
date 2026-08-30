//! Library entry: [`ExporterConfig`] to the shared conversation structure, then
//! the chosen output format.

use std::path::Path;

use imessage_database::{
    tables::table::DEFAULT_PATH_IOS,
    util::{
        dates::{TIMESTAMP_FACTOR, get_offset},
        dirs::default_db_path,
        platform::Platform,
        query_context::QueryContext,
    },
};
use message_ir_format::ExportTransforms;
use message_vault_io_core::{ApplePlatform, ExporterConfig, RunResult, SourceConfig};

use crate::{
    backup::ios_backup_encrypted_flag,
    emit::run_export,
    error::{
        APPLE_CONTACTS_MISSING, ATTACHMENT_FOLDER_MISSING, MESSAGES_DATABASE_MISSING,
        NOT_AN_IPHONE_BACKUP, RuntimeError,
    },
    options::{MailOptions, attachment_embed_from_copy_method},
    session::MailSession,
};

/// Build options from [`ExporterConfig`], open the Messages database, and write
/// the export.
///
/// # Errors
///
/// Returns an error when the source is not Apple Messages, the database cannot
/// be opened, conversion fails, media processing fails for every candidate
/// file, or the user cancels.
pub fn run(config: &ExporterConfig) -> anyhow::Result<RunResult> {
    check_cancel(config)?;

    let options = options_from_export_config(config)?;
    let format = options.output_format;
    let mut session = MailSession::new(options)?;
    session.resolve_filtered_handles();
    check_cancel(config)?;
    let sink = run_export(&session)?;
    check_cancel(config)?;

    if !sink.media.errors.is_empty() && sink.media.processed == 0 && config.media.mode.needs_tools()
    {
        return Err(RuntimeError::InvalidOptions(
            "media processing failed for all candidate files".to_string(),
        )
        .into());
    }

    let mut messages = sink.log_lines();
    messages.push(format!(
        "Wrote {} export under {}",
        format.as_str(),
        config.output.display()
    ));
    Ok(RunResult { messages })
}

fn check_cancel(config: &ExporterConfig) -> Result<(), RuntimeError> {
    message_vault_io_core::check_cancel(config.cancel.as_ref())
        .map_err(|msg| RuntimeError::InvalidOptions(msg.to_string()))
}

fn options_from_export_config(config: &ExporterConfig) -> Result<MailOptions, RuntimeError> {
    let SourceConfig::Apple(source) = &config.source else {
        return Err(RuntimeError::InvalidOptions(
            "imessage-ir-exporter requires SourceConfig::Apple".to_string(),
        ));
    };

    let mut query_context = QueryContext::default();
    // QueryContext stores nanoseconds since the Apple epoch (2001-01-01 UTC);
    // DateRange stores Unix seconds at local midnight. Convert the same way
    // `QueryContext::set_start`/`set_end` convert `YYYY-MM-DD` strings.
    let offset_ns = get_offset() * TIMESTAMP_FACTOR;
    query_context.start = config
        .date_range
        .start_secs
        .map(|s| unix_secs_to_apple_ns(s, offset_ns));
    // DateRange.end_secs is exclusive (`secs >= end` rejected). QueryContext
    // filters with inclusive `m.date <= end`, so subtract 1 ns.
    query_context.end = config
        .date_range
        .end_secs
        .map(|s| exclusive_unix_end_to_inclusive_apple_ns(s, offset_ns));

    let db_path = match config.primary_input() {
        Some(path) if !path.as_os_str().is_empty() => path.to_path_buf(),
        _ => default_db_path(),
    };
    let platform = match source.platform {
        Some(ApplePlatform::MacOs) => Platform::from_cli("macOS").ok_or_else(|| {
            RuntimeError::InvalidOptions(
                "macOS is not a valid platform! Must be one of <macOS, iOS>".to_string(),
            )
        })?,
        Some(ApplePlatform::Ios) => Platform::from_cli("iOS").ok_or_else(|| {
            RuntimeError::InvalidOptions(
                "iOS is not a valid platform! Must be one of <macOS, iOS>".to_string(),
            )
        })?,
        Some(ApplePlatform::Auto) | None => Platform::determine(&db_path)?,
    };

    if source.backup_password.is_some() && !matches!(platform, Platform::iOS) {
        return Err(RuntimeError::InvalidOptions(
            "backup password is enabled; it can only be used with iOS backups.".to_string(),
        ));
    }

    if let Some(path) = &source.attachment_root {
        if !Path::new(path).exists() {
            return Err(RuntimeError::InvalidOptions(
                ATTACHMENT_FOLDER_MISSING.to_string(),
            ));
        }
        if platform == Platform::iOS {
            config.emit_log(format!(
                "Option attachment-root is enabled, but the platform is {}, so the root will have no effect!",
                Platform::iOS
            ));
        }
    }

    if let Some(path) = &source.apple_contacts {
        if !path.exists() {
            return Err(RuntimeError::InvalidOptions(
                APPLE_CONTACTS_MISSING.to_string(),
            ));
        }
        if platform == Platform::iOS {
            config.emit_log(format!(
                "Option contacts path is enabled, but the platform is {}, so the path will have no effect!",
                Platform::iOS
            ));
        }
    }

    match platform {
        Platform::macOS => {
            if !db_path.is_file() {
                return Err(RuntimeError::InvalidOptions(
                    MESSAGES_DATABASE_MISSING.to_string(),
                ));
            }
        }
        Platform::iOS => {
            let manifest = db_path.join("Manifest.plist");
            if !db_path.is_dir() || !manifest.is_file() {
                return Err(RuntimeError::InvalidOptions(
                    NOT_AN_IPHONE_BACKUP.to_string(),
                ));
            }
            if ios_backup_encrypted_flag(&db_path) == Some(false)
                && !db_path.join(DEFAULT_PATH_IOS).is_file()
            {
                return Err(RuntimeError::InvalidOptions(
                    NOT_AN_IPHONE_BACKUP.to_string(),
                ));
            }
        }
    }

    let attachment_embed = attachment_embed_from_copy_method(&source.copy_method)?;

    // Create the output directory; prior IR artifacts are removed in `run_export`
    // via FormatSink::open_prepared.
    std::fs::create_dir_all(&config.output)?;
    let export_path = config.output.clone();

    Ok(MailOptions {
        db_path,
        attachment_root: source.attachment_root.clone(),
        export_path,
        query_context,
        use_caller_id: source.use_caller_id,
        platform,
        conversation_filter: source.conversation_filter.clone(),
        cleartext_password: source.backup_password.clone(),
        contacts_path: source.apple_contacts.clone(),
        attachment_embed,
        transforms: {
            let mut transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
            transforms.log = config.log.clone();
            transforms
        },
        output_format: config.output_format,
        log: config.log.clone(),
        cancel: config.cancel.clone(),
    })
}

fn unix_secs_to_apple_ns(secs: i64, offset_ns: i64) -> i64 {
    secs * TIMESTAMP_FACTOR - offset_ns
}

/// Convert exclusive Unix end seconds to an inclusive Apple-ns bound for `m.date <= end`.
fn exclusive_unix_end_to_inclusive_apple_ns(end_secs: i64, offset_ns: i64) -> i64 {
    unix_secs_to_apple_ns(end_secs, offset_ns).saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{
        APPLE_CONTACTS_MISSING, ATTACHMENT_FOLDER_MISSING, MESSAGES_DATABASE_MISSING,
        NOT_AN_IPHONE_BACKUP,
    };
    use message_vault_io_core::{AppleConfig, MediaConfig, OutputFormat, parse_date_range};
    use std::{fs, path::Path};

    fn apple_cfg(input: &Path, apple: AppleConfig) -> ExporterConfig {
        ExporterConfig {
            inputs: vec![input.to_path_buf()],
            output: input.join("_export_out"),
            date_range: parse_date_range(None, None).unwrap(),
            timezone: None,
            contacts: None,
            obfuscate: Default::default(),
            media: MediaConfig::default(),
            cancel: None,
            log: None,
            output_format: OutputFormat::Jsonl,
            resume: false,
            source: SourceConfig::Apple(apple),
        }
    }

    #[test]
    fn exclusive_end_maps_to_inclusive_sql_bound() {
        let offset_ns = 0;
        let end_secs = 1_578_009_600; // 2020-01-03 00:00:00 UTC
        let inclusive = exclusive_unix_end_to_inclusive_apple_ns(end_secs, offset_ns);
        let at_bound = unix_secs_to_apple_ns(end_secs, offset_ns);
        assert_eq!(inclusive, at_bound - 1);
        // A message stamped exactly at the exclusive midnight must fail `<= inclusive`.
        assert!(at_bound > inclusive);
    }

    #[test]
    fn missing_chat_db_uses_locked_copy() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("chat.db");
        let err = options_from_export_config(&apple_cfg(
            &missing,
            AppleConfig {
                platform: Some(ApplePlatform::MacOs),
                ..AppleConfig::default()
            },
        ))
        .unwrap_err();
        assert_eq!(err.to_string(), MESSAGES_DATABASE_MISSING);
    }

    #[test]
    fn missing_attachment_folder_uses_locked_copy() {
        let dir = tempfile::tempdir().unwrap();
        let chat = dir.path().join("chat.db");
        fs::write(&chat, b"sqlite").unwrap();
        let err = options_from_export_config(&apple_cfg(
            &chat,
            AppleConfig {
                platform: Some(ApplePlatform::MacOs),
                attachment_root: Some(dir.path().join("no-such-root").display().to_string()),
                ..AppleConfig::default()
            },
        ))
        .unwrap_err();
        assert_eq!(err.to_string(), ATTACHMENT_FOLDER_MISSING);
    }

    #[test]
    fn missing_apple_contacts_uses_locked_copy() {
        let dir = tempfile::tempdir().unwrap();
        let chat = dir.path().join("chat.db");
        fs::write(&chat, b"sqlite").unwrap();
        let err = options_from_export_config(&apple_cfg(
            &chat,
            AppleConfig {
                platform: Some(ApplePlatform::MacOs),
                apple_contacts: Some(dir.path().join("no-such.abcddb")),
                ..AppleConfig::default()
            },
        ))
        .unwrap_err();
        assert_eq!(err.to_string(), APPLE_CONTACTS_MISSING);
    }

    #[test]
    fn empty_folder_is_not_an_iphone_backup() {
        let dir = tempfile::tempdir().unwrap();
        let err = options_from_export_config(&apple_cfg(
            dir.path(),
            AppleConfig {
                platform: Some(ApplePlatform::Ios),
                ..AppleConfig::default()
            },
        ))
        .unwrap_err();
        assert_eq!(err.to_string(), NOT_AN_IPHONE_BACKUP);
    }

    #[test]
    fn unencrypted_backup_missing_messages_uses_locked_copy() {
        let dir = tempfile::tempdir().unwrap();
        // Manifest.plist present, IsEncrypted false, hashed sms.db missing.
        fs::write(
            dir.path().join("Manifest.plist"),
            br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>IsEncrypted</key><false/></dict></plist>"#,
        )
        .unwrap();
        let err = options_from_export_config(&apple_cfg(
            dir.path(),
            AppleConfig {
                platform: Some(ApplePlatform::Ios),
                ..AppleConfig::default()
            },
        ))
        .unwrap_err();
        assert_eq!(err.to_string(), NOT_AN_IPHONE_BACKUP);
    }
}
