//! Attachment persistence and part-index helpers for the emitter.

use std::fs;
use std::path::Path;

use imessage_database::{
    message_types::handwriting::HandwrittenMessage,
    tables::{attachment::Attachment, messages::Message},
};
use mail::MailAttachment;
use sha2::{Digest, Sha256};

use crate::{
    attachments::load_attachment_bytes,
    body::referenced_attachment_indices,
    error::RuntimeError,
    fields::{PartRecord, build_part_records, sticker_extras, transcription_for_attachment},
    session::MailSession,
};

/// Destination file name for a persisted attachment: `<local-date>-<digest16><ext>`.
fn attachment_dest_name(
    timestamp_unix_ms: i64,
    digest_hex: &str,
    original_name: Option<&str>,
) -> String {
    let secs = timestamp_unix_ms.div_euclid(1000);
    let ext = original_name
        .and_then(|n| Path::new(n).extension())
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    message_vault_io_core::attachments::attachment_dest_name(secs, digest_hex, &ext)
}

/// Write attachment bytes under `attachments_dir` (idempotent by digest name).
///
/// Writes via `{name}.tmp` then renames into place so a crash mid-write cannot
/// leave a short final file that later runs treat as complete. Hashes once.
///
/// Returns the export-relative path (`attachments/<name>`), the sha256 digest,
/// and the byte length of the persisted file.
///
/// # Errors
///
/// Returns an error when the temp file cannot be written or renamed.
pub(super) fn persist_attachment(
    attachments_dir: &Path,
    timestamp_unix_ms: i64,
    bytes: &[u8],
    original_name: Option<&str>,
) -> Result<(String, String, u64), RuntimeError> {
    let digest_hex = hex::encode(Sha256::digest(bytes));
    let name = attachment_dest_name(timestamp_unix_ms, &digest_hex, original_name);
    let dest = attachments_dir.join(&name);
    let byte_len = bytes.len() as u64;
    let needs_write = match fs::metadata(&dest) {
        Ok(meta) => meta.len() != byte_len,
        Err(_) => true,
    };
    if needs_write {
        let tmp = attachments_dir.join(format!("{name}.tmp"));
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, &dest)?;
    }
    Ok((format!("attachments/{name}"), digest_hex, byte_len))
}

/// Render handwriting ink as an SVG attachment, if this message is handwriting.
fn try_handwriting_svg(session: &MailSession, message: &Message) -> Option<MailAttachment> {
    if !message.is_handwriting() {
        return None;
    }
    let payload = message.raw_payload_data(session.data_source.db())?;
    let hw = HandwrittenMessage::from_payload(&payload).ok()?;
    let svg = hw.render_svg();
    Some(MailAttachment {
        bytes: svg.into_bytes(),
        meta: message_ir::AttachmentMeta {
            path: None,
            original_name: Some(format!("{}.svg", message.guid)),
            mime_type: Some("image/svg+xml".into()),
            digest_sha256: None,
        },
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
    })
}

/// Rewrite part attachment indices so they match the kept (referenced) list,
/// not the full attachment list from the database.
fn remap_part_attachment_indices(
    parts: &mut [PartRecord],
    index_by_full: &std::collections::HashMap<usize, usize>,
) {
    for part in parts {
        part.attachment_indices = part
            .attachment_indices
            .iter()
            .filter_map(|full| index_by_full.get(full).copied())
            .collect();
    }
}

/// Load body parts and attachment bytes for one Apple message.
///
/// # Errors
///
/// Returns an error when attachments cannot be loaded from the database or
/// decrypted from an iOS backup.
pub(super) fn collect_mail_parts_and_attachments(
    session: &MailSession,
    message: &Message,
) -> Result<(Vec<PartRecord>, Vec<MailAttachment>), RuntimeError> {
    let mut attachments = Attachment::from_message(session.data_source.db(), message)?;
    let referenced = referenced_attachment_indices(message, &attachments);
    let index_by_full: std::collections::HashMap<usize, usize> = referenced
        .iter()
        .enumerate()
        .map(|(kept, &full)| (full, kept))
        .collect();

    let mut parts = build_part_records(message, &attachments);
    remap_part_attachment_indices(&mut parts, &index_by_full);

    let mut mail_attachments = Vec::new();
    for &idx in &referenced {
        let attachment = &mut attachments[idx];
        let bytes = load_attachment_bytes(session, attachment)?;
        let transcription = transcription_for_attachment(message, attachment);
        let (_prompt, sticker_effect) = sticker_extras(
            attachment,
            &session.options.platform,
            session.options.db_path.as_path(),
            session.options.attachment_root.as_deref(),
        );
        mail_attachments.push(MailAttachment {
            bytes,
            meta: message_ir::AttachmentMeta {
                path: None,
                original_name: attachment.transfer_name.clone(),
                mime_type: attachment.mime_type.clone(),
                digest_sha256: None,
            },
            is_sticker: attachment.is_sticker,
            transcription,
            sticker_effect,
        });
    }

    if let Some(svg) = try_handwriting_svg(session, message) {
        mail_attachments.push(svg);
    }

    Ok((parts, mail_attachments))
}
