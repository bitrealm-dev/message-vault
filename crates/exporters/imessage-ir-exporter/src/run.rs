//! Library entrypoint: [`ExporterConfig`] → common message → packaging.

use std::path::PathBuf;

use imessage_database::util::{
    dirs::default_db_path, platform::Platform, query_context::QueryContext,
};
use message_vault_io_core::{RunResult, ApplePlatform, ExporterConfig, SourceConfig};
use message_ir_format::ExportTransforms;

use crate::{
    emit::run_export,
    error::RuntimeError,
    options::{AttachmentEmbed, MailOptions, validate_export_path},
    session::MailSession,
};

/// Build options from [`ExporterConfig`], open the DB, and write the export.
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
    if let Some(start) = &source.start_date
        && let Err(why) = query_context.set_start(start)
    {
        return Err(RuntimeError::InvalidOptions(format!("{why}")));
    }
    if let Some(end) = &source.end_date
        && let Err(why) = query_context.set_end(end)
    {
        return Err(RuntimeError::InvalidOptions(format!("{why}")));
    }

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
        let custom_attachment_path = PathBuf::from(path);
        if !custom_attachment_path.exists() {
            return Err(RuntimeError::InvalidOptions(format!(
                "Supplied attachment-root `{path}` does not exist!"
            )));
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
            return Err(RuntimeError::InvalidOptions(format!(
                "Supplied contacts path `{}` does not exist!",
                path.display()
            )));
        }
        if platform == Platform::iOS {
            config.emit_log(format!(
                "Option contacts path is enabled, but the platform is {}, so the path will have no effect!",
                Platform::iOS
            ));
        }
    }

    let attachment_embed = match source.copy_method.to_ascii_lowercase().as_str() {
        "disabled" => AttachmentEmbed::Disabled,
        "clone" | "basic" | "full" => AttachmentEmbed::Embed,
        other => {
            return Err(RuntimeError::InvalidOptions(format!(
                "{other} is not a valid attachment mode! Must be one of <clone, basic, full, disabled>"
            )));
        }
    };

    let export_path = validate_export_path(&config.output, config.output_format)?;
    std::fs::create_dir_all(&export_path)?;

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
