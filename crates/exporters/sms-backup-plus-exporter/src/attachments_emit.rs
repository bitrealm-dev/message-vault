//! Attachment helpers: write blobs to disk and map staged attachments onto
//! the shared [`IrAttachment`] shape.

use crate::types::AttachmentBlob;
use message_ir::{IrAttachment, PendingAttachment};
use message_vault_io_core::{ExportReport, write_if_missing};
use std::collections::HashSet;
use std::path::Path;

/// Write attachment blobs, returning the ones that succeeded.
///
/// A single failing attachment (disk full, permissions, ENAMETOOLONG) must not
/// drop the whole message: the failure is recorded in `report.errors` and the
/// message is kept without that attachment.
pub(super) fn write_attachments(
    blobs: &[AttachmentBlob],
    attachments_dir: &Path,
    report: &mut ExportReport,
    copy_attachments: bool,
    path_display: &str,
) -> Vec<PendingAttachment> {
    if !copy_attachments {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(blobs.len());
    for blob in blobs {
        let path = attachments_dir.join(&blob.filename);
        match write_if_missing(&path, &blob.data) {
            Ok(true) => {
                report.attachments_saved += 1;
            }
            Ok(false) => {}
            Err(err) => {
                report.errors.push(format!(
                    "{path_display}: failed to write attachment {}: {err}",
                    blob.filename
                ));
                continue;
            }
        }
        out.push(PendingAttachment {
            rel_path: format!("attachments/{}", blob.filename),
            content_type: blob.mime_type.clone().unwrap_or_default(),
            extension: Path::new(&blob.filename)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string(),
            digest_sha256: Some(blob.digest_hex.clone()),
            name_hint: blob.original_name.clone(),
        });
    }
    out
}

/// Union attachment lists by content digest so flat↔archive dedupe does not drop media.
pub(super) fn merge_attachments(into: &mut Vec<PendingAttachment>, from: Vec<PendingAttachment>) {
    let mut seen: HashSet<String> = into
        .iter()
        .map(|a| a.digest_sha256.clone().unwrap_or_default())
        .collect();
    for att in from {
        if seen.insert(att.digest_sha256.clone().unwrap_or_default()) {
            into.push(att);
        }
    }
}

/// Map a staged attachment onto the shared [`IrAttachment`] shape.
pub(super) fn pending_attachment_to_ir(a: &PendingAttachment) -> IrAttachment {
    IrAttachment {
        path: Some(a.rel_path.clone()),
        original_name: a.name_hint.clone(),
        mime_type: a.mime_type(),
        digest_sha256: a.digest_sha256.clone(),
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        size_bytes: None,
        missing_reason: None,
        bytes: None,
    }
}
