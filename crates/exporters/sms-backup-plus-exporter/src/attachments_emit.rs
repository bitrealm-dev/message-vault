//! Attachment helpers: queue blobs during parse, then map staged attachments
//! onto the shared [`IrAttachment`] shape after the runner writes files.

use crate::types::AttachmentBlob;
use message_ir::{IrAttachment, PendingAttachment};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Queue attachment blobs as metadata. Bytes stay in `blob_bytes` (keyed by
/// digest) until the shared runner writes them.
pub(super) fn queue_attachments(
    blobs: &[AttachmentBlob],
    copy_attachments: bool,
    blob_bytes: &mut HashMap<String, Vec<u8>>,
) -> Vec<PendingAttachment> {
    blobs
        .iter()
        .map(|blob| {
            if copy_attachments && !blob.data.is_empty() {
                blob_bytes
                    .entry(blob.digest_hex.clone())
                    .or_insert_with(|| blob.data.clone());
            }
            PendingAttachment {
                rel_path: String::new(),
                content_type: blob.mime_type.clone().unwrap_or_default(),
                extension: Path::new(&blob.filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_string(),
                digest_sha256: Some(blob.digest_hex.clone()),
                name_hint: blob
                    .original_name
                    .clone()
                    .or_else(|| Some(blob.filename.clone())),
            }
        })
        .collect()
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

/// Map a queued attachment onto the shared [`IrAttachment`] shape.
pub(super) fn pending_attachment_to_ir(
    a: &PendingAttachment,
    blob_bytes: &HashMap<String, Vec<u8>>,
) -> IrAttachment {
    let digest = a.digest_sha256.clone();
    let bytes = digest.as_ref().and_then(|d| blob_bytes.get(d).cloned());
    IrAttachment {
        path: None,
        original_name: a.name_hint.clone(),
        mime_type: a.mime_type(),
        digest_sha256: digest,
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        size_bytes: bytes.as_ref().map(|b| b.len() as u64),
        missing_reason: None,
        bytes,
    }
}
