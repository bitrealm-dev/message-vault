//! Slim options for iMessage IR export.

use std::path::{Path, PathBuf};

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
    /// CSV, EML, MBOX, JSON, or JSONL.
    pub output_format: OutputFormat,
    /// Mid-run progress / warnings (GUI sink or stderr).
    pub log: Option<LogSink>,
    /// Cooperative cancel flag, checked periodically inside long loops.
    pub cancel: Option<CancelFlag>,
}

impl MailOptions {
    /// Messages database path for the selected platform.
    pub fn get_db_path(&self) -> PathBuf {
        match self.platform {
            Platform::iOS => self.db_path.join(DEFAULT_PATH_IOS),
            Platform::macOS => self.db_path.clone(),
        }
    }

    pub fn emit_log(&self, line: impl AsRef<str>) {
        emit_log(self.log.as_ref(), line);
    }
}

/// Validate export directory does not already contain mail-archive data for `format`.
///
/// Prefer [`message_ir_format::clean_previous_ir_output`] for re-runs (used by
/// `run_export`). This stricter refuse-on-existing check remains available for
/// callers that want to abort instead of cleaning.
#[allow(dead_code)]
pub(crate) fn validate_export_path(
    export_path: &Path,
    format: OutputFormat,
) -> Result<PathBuf, RuntimeError> {
    let resolved = export_path.to_path_buf();
    if resolved.exists() {
        match resolved.read_dir() {
            Ok(files) => {
                for file in files.flatten() {
                    let path = file.path();
                    match format {
                        OutputFormat::Eml => {
                            if path.is_dir() {
                                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                                if name != "attachments" && dir_contains_eml(&path) {
                                    return Err(RuntimeError::InvalidOptions(format!(
                                        "Specified export path {} contains existing \"eml\" export data!",
                                        resolved.display()
                                    )));
                                }
                            }
                        }
                        OutputFormat::Mbox => {
                            if path
                                .extension()
                                .and_then(|s| s.to_str())
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("mbox"))
                            {
                                return Err(RuntimeError::InvalidOptions(format!(
                                    "Specified export path {} contains existing \"mbox\" export data!",
                                    resolved.display()
                                )));
                            }
                        }
                        OutputFormat::Csv => {
                            if path
                                .extension()
                                .and_then(|s| s.to_str())
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
                            {
                                return Err(RuntimeError::InvalidOptions(format!(
                                    "Specified export path {} contains existing \"csv\" export data!",
                                    resolved.display()
                                )));
                            }
                        }
                        OutputFormat::Json => {
                            if path
                                .extension()
                                .and_then(|s| s.to_str())
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                            {
                                return Err(RuntimeError::InvalidOptions(format!(
                                    "Specified export path {} contains existing \"json\" export data!",
                                    resolved.display()
                                )));
                            }
                        }
                        OutputFormat::Jsonl => {
                            if path
                                .extension()
                                .and_then(|s| s.to_str())
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
                            {
                                return Err(RuntimeError::InvalidOptions(format!(
                                    "Specified export path {} contains existing \"jsonl\" export data!",
                                    resolved.display()
                                )));
                            }
                        }
                        OutputFormat::Xml => {
                            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                            if name == "smses.xml"
                                || name.ends_with(".xml.tmp")
                                || name.ends_with(".xml.sbrbody")
                            {
                                return Err(RuntimeError::InvalidOptions(format!(
                                    "Specified export path {} contains existing \"xml\" export data!",
                                    resolved.display()
                                )));
                            }
                        }
                    }
                }
            }
            Err(why) => {
                return Err(RuntimeError::InvalidOptions(format!(
                    "Specified export path {} is not a valid directory: {why}",
                    resolved.display()
                )));
            }
        }
    }
    Ok(resolved)
}

fn dir_contains_eml(dir: &Path) -> bool {
    let Ok(entries) = dir.read_dir() else {
        return false;
    };
    entries.flatten().any(|e| {
        e.path()
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("eml"))
    })
}
