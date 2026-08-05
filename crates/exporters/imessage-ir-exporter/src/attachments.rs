//! Load attachment bytes (macOS path or encrypted iOS decrypt-to-temp).

use std::{fs, path::PathBuf};

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
    let source = PathBuf::from(source);

    if let Some(backup) = &session.data_source.backup
        && backup.is_encrypted()
    {
        let temp = decrypt_file(backup, &source)?;
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
        match fs::read(&source) {
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
