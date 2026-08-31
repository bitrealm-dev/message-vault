//! Slim options for iMessage IR export.

use std::path::PathBuf;

use imessage_database::{
    tables::table::DEFAULT_PATH_IOS,
    util::{platform::Platform, query_context::QueryContext},
};
use message_ir_format::ExportTransforms;
use message_vault_io_core::{CancelFlag, LogSink, OutputFormat, emit_log};

use crate::error::RuntimeError;

/// Whether to resolve attachment bytes for embedding (`.eml` / `.mbox`) or
/// persisting under `attachments/` (CSV / JSON).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachmentEmbed {
    /// Resolve and embed media bytes (macOS path or iOS decrypt).
    Embed,
    /// Skip media bytes (empty attachment parts still possible via other fields).
    Disabled,
}

/// Map `AppleConfig.copy_method` to attachment handling.
///
/// `clone` copies files, `basic` embeds thumbnails, and `full` embeds
/// originals — all three resolve bytes through the same embed path in this
/// exporter. `disabled` skips media bytes entirely.
///
/// # Errors
///
/// Returns an error when `copy_method` is not `clone`, `basic`, `full`, or
/// `disabled`.
pub(crate) fn attachment_embed_from_copy_method(
    copy_method: &str,
) -> Result<AttachmentEmbed, RuntimeError> {
    match copy_method.trim().to_ascii_lowercase().as_str() {
        "disabled" => Ok(AttachmentEmbed::Disabled),
        "clone" | "basic" | "full" => Ok(AttachmentEmbed::Embed),
        other => Err(RuntimeError::InvalidOptions(format!(
            "{other} is not a valid attachment mode! Must be one of <clone, basic, full, disabled>"
        ))),
    }
}

/// Parsed options for one mail export run.
#[derive(Debug)]
pub(crate) struct MailOptions {
    pub db_path: PathBuf,
    pub attachment_root: Option<String>,
    pub export_path: PathBuf,
    pub query_context: QueryContext,
    pub use_caller_id: bool,
    pub platform: Platform,
    pub conversation_filter: Option<String>,
    pub cleartext_password: Option<String>,
    pub contacts_path: Option<PathBuf>,
    pub attachment_embed: AttachmentEmbed,
    /// Media / obfuscate transforms applied by [`message_ir_format::FormatSink`].
    pub transforms: ExportTransforms,
    /// CSV, EML, MBOX, JSON, or JSON Lines (one JSON object per line).
    pub output_format: OutputFormat,
    /// Mid-run progress / warnings (GUI sink or stderr).
    pub log: Option<LogSink>,
    /// Cooperative cancel flag, checked periodically inside long loops.
    pub cancel: Option<CancelFlag>,
    /// Continue an interrupted export: keep previous output and skip the
    /// conversations already written.
    pub resume: bool,
}

impl MailOptions {
    /// Messages database path for the selected platform.
    pub fn get_db_path(&self) -> PathBuf {
        match self.platform {
            Platform::iOS => self.db_path.join(DEFAULT_PATH_IOS),
            Platform::macOS => self.db_path.clone(),
        }
    }

    /// Write one log line when a log sink is configured.
    pub fn emit_log(&self, line: impl AsRef<str>) {
        emit_log(self.log.as_ref(), line);
    }
}
