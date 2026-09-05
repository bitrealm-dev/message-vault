//! Library entry: [`ExporterConfig`] to the shared conversation structure, then
//! the chosen output format.

use std::path::Path;

use imessage_database::{
    tables::table::DEFAULT_PATH_IOS,
    util::{dirs::default_db_path, platform::Platform, query_context::QueryContext},
};
use message_ir_format::ExportTransforms;
use message_vault_io_core::{AppleConfig, ApplePlatform, ExporterConfig, RunResult, SourceConfig};

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
    let session = MailSession::new(options)?;
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

/// The shared cancel check, mapped onto this exporter's error type.
fn check_cancel(config: &ExporterConfig) -> Result<(), RuntimeError> {
    message_vault_io_core::check_cancel(config.cancel.as_ref())
        .map_err(|msg| RuntimeError::InvalidOptions(msg.to_string()))
}

/// Translate the shared exporter config into this exporter's mail options, rejecting non-Apple sources.
fn options_from_export_config(config: &ExporterConfig) -> Result<MailOptions, RuntimeError> {
    let SourceConfig::Apple(source) = &config.source else {
        return Err(RuntimeError::InvalidOptions(
            "imessage-ir-exporter requires SourceConfig::Apple".to_string(),
        ));
    };

    let db_path = match config.primary_input() {
        Some(path) if !path.as_os_str().is_empty() => path.to_path_buf(),
        _ => default_db_path(),
    };
    let platform = platform_for(source, &db_path)?;

    if source.backup_password.is_some() && platform != Platform::iOS {
        return Err(RuntimeError::InvalidOptions(
            "backup password is enabled; it can only be used with iOS backups.".to_string(),
        ));
    }
    check_macos_only_path(
        config,
        &platform,
        source.attachment_root.as_deref().map(Path::new),
        ATTACHMENT_FOLDER_MISSING,
        "Option attachment-root is enabled, but the platform is iOS, so the root will have no effect!",
    )?;
    check_macos_only_path(
        config,
        &platform,
        source.apple_contacts.as_deref(),
        APPLE_CONTACTS_MISSING,
        "Option contacts path is enabled, but the platform is iOS, so the path will have no effect!",
    )?;
    check_db_path(&platform, &db_path)?;

    let attachment_embed = attachment_embed_from_copy_method(&source.copy_method)?;

    // Create the output directory; prior IR artifacts are removed in `run_export`
    // via FormatSink::open_prepared.
    std::fs::create_dir_all(&config.output)?;
    let export_path = config.output.clone();

    Ok(MailOptions {
        db_path,
        attachment_root: source.attachment_root.clone(),
        export_path,
        query_context: QueryContext::default(),
        use_caller_id: source.use_caller_id,
        platform,
        cleartext_password: source.backup_password.clone(),
        contacts_path: source.apple_contacts.clone(),
        attachment_embed,
        transforms: ExportTransforms::from_config(config),
        output_format: config.output_format,
        log: config.log.clone(),
        progress: config.progress.clone(),
        cancel: config.cancel.clone(),
        resume: config.resume,
    })
}

/// The platform the source names, or the one the backup's layout shows.
fn platform_for(source: &AppleConfig, db_path: &Path) -> Result<Platform, RuntimeError> {
    match source.platform {
        Some(ApplePlatform::MacOs) => named_platform("macOS"),
        Some(ApplePlatform::Ios) => named_platform("iOS"),
        Some(ApplePlatform::Auto) | None => Ok(Platform::determine(db_path)?),
    }
}

/// The platform `imessage-database` knows by that name.
fn named_platform(name: &str) -> Result<Platform, RuntimeError> {
    Platform::from_cli(name).ok_or_else(|| {
        RuntimeError::InvalidOptions(format!(
            "{name} is not a valid platform! Must be one of <macOS, iOS>"
        ))
    })
}

/// A path option that only a macOS export reads: refused when it points
/// nowhere, and noted in the log as having no effect on an iOS backup.
fn check_macos_only_path(
    config: &ExporterConfig,
    platform: &Platform,
    path: Option<&Path>,
    missing: &str,
    ignored_on_ios: &str,
) -> Result<(), RuntimeError> {
    let Some(path) = path else {
        return Ok(());
    };
    if !path.exists() {
        return Err(RuntimeError::InvalidOptions(missing.to_string()));
    }
    if *platform == Platform::iOS {
        config.emit_log(ignored_on_ios);
    }
    Ok(())
}

/// The backup must be laid out as the platform expects: a messages
/// database file on macOS; on iOS a backup folder with its manifest and,
/// when the backup is not encrypted, the database at its hashed path.
fn check_db_path(platform: &Platform, db_path: &Path) -> Result<(), RuntimeError> {
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
            if ios_backup_encrypted_flag(db_path) == Some(false)
                && !db_path.join(DEFAULT_PATH_IOS).is_file()
            {
                return Err(RuntimeError::InvalidOptions(
                    NOT_AN_IPHONE_BACKUP.to_string(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{
        APPLE_CONTACTS_MISSING, ATTACHMENT_FOLDER_MISSING, MESSAGES_DATABASE_MISSING,
        NOT_AN_IPHONE_BACKUP,
    };
    use message_vault_io_core::{AppleConfig, MediaConfig, OutputFormat};
    use std::{fs, path::Path};

    fn apple_cfg(input: &Path, apple: AppleConfig) -> ExporterConfig {
        ExporterConfig {
            inputs: vec![input.to_path_buf()],
            output: input.join("_export_out"),
            timezone: None,
            obfuscate: Default::default(),
            media: MediaConfig::default(),
            cancel: None,
            log: None,
            progress: None,
            output_format: OutputFormat::Jsonl,
            resume: false,
            source: SourceConfig::Apple(apple),
        }
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
