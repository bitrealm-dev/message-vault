//! Load attachment bytes (macOS path or encrypted iOS decrypt-to-temp).

use std::{
    fs,
    path::{Path, PathBuf},
};

use crabapple::error::BackupError;
use imessage_database::tables::attachment::Attachment;

use crate::{
    backup::decrypt_file, error::RuntimeError, options::AttachmentEmbed, session::MailSession,
};

/// Read attachment bytes for embedding. Empty when embed is disabled or read fails.
pub(crate) fn load_attachment_bytes(
    session: &MailSession,
    attachment: &Attachment,
) -> Result<Vec<u8>, RuntimeError> {
    if session.options.attachment_embed == AttachmentEmbed::Disabled {
        return Ok(Vec::new());
    }

    let Some(source) = attachment.resolved_attachment_path(
        &session.options.platform,
        &session.options.db_path,
        session.options.attachment_root.as_deref(),
    ) else {
        return Ok(Vec::new());
    };
    read_resolved_attachment(session, &PathBuf::from(source))
}

/// Read a previously resolved attachment path (plain file or encrypted backup).
///
/// # Errors
///
/// Returns a fatal decrypt or temp-file error. Missing files return empty bytes.
pub(crate) fn read_resolved_attachment(
    session: &MailSession,
    source: &Path,
) -> Result<Vec<u8>, RuntimeError> {
    if let Some(backup) = &session.data_source.backup
        && backup.is_encrypted()
    {
        // A missing attachment (not present in the encrypted backup's
        // Manifest.db) is non-fatal: log it and continue without bytes rather
        // than dropping the entire message. Other errors (temp file creation,
        // I/O, crypto failures) still propagate as an `Err` from this
        // function, but they no longer kill the run: the loader closure in
        // `emit.rs` logs them, and `run_attachment_jobs` downgrades them to
        // a `file_missing` attachment rather than aborting.
        let temp = match decrypt_file(backup, source) {
            Ok(temp) => temp,
            Err(RuntimeError::BackupError(BackupError::FileNotFoundInBackup(_))) => {
                session.options.emit_log(format!(
                    "warning: attachment {} not found in encrypted backup; skipping bytes",
                    source.display()
                ));
                return Ok(Vec::new());
            }
            Err(e) => return Err(e),
        };
        let bytes = match fs::read(&temp) {
            Ok(b) => b,
            Err(e) => {
                session.options.emit_log(format!(
                    "warning: failed to read decrypted attachment {}: {e}",
                    temp.display()
                ));
                Vec::new()
            }
        };
        if let Err(why) = fs::remove_file(&temp) {
            session.options.emit_log(format!(
                "Unable to remove encrypted temp file {}: {why}",
                temp.display()
            ));
        }
        return Ok(bytes);
    }

    if source.is_file() {
        match fs::read(source) {
            Ok(b) => Ok(b),
            Err(e) => {
                session.options.emit_log(format!(
                    "warning: failed to read attachment {}: {e}",
                    source.display()
                ));
                Ok(Vec::new())
            }
        }
    } else {
        Ok(Vec::new())
    }
}
