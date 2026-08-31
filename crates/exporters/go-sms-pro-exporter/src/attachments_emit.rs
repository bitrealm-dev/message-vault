//! Attachment helpers for the emitter.

use anyhow::Result;
use go_sms_mms::ParsedPdu;
use message_ir::{IrAttachment, PendingAttachment};
use message_vault_io_core::digest_prefix;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Queue PDU attachment parts as metadata. Bytes stay in `blob_bytes` until
/// the shared runner writes them.
pub(super) fn queue_pdu_attachments(
    parsed: &ParsedPdu,
    copy_attachments: bool,
    blob_bytes: &mut HashMap<String, Vec<u8>>,
) -> Result<Vec<PendingAttachment>> {
    let mut out = Vec::new();
    for (idx, att) in parsed.attachments.iter().enumerate() {
        let digest_hex = hex::encode(Sha256::digest(&att.data));
        if copy_attachments && !att.data.is_empty() {
            blob_bytes
                .entry(digest_hex.clone())
                .or_insert_with(|| att.data.clone());
        }
        let digest_prefix = digest_prefix(&digest_hex);
        let name = format!(
            "I_{}_{}_{}{}",
            parsed.timestamp,
            digest_prefix,
            idx + 1,
            att.ext
        );
        out.push(PendingAttachment {
            rel_path: String::new(),
            content_type: media::mime_for_ext(&att.ext).unwrap_or("").to_string(),
            extension: att.ext.trim_start_matches('.').to_string(),
            digest_sha256: Some(digest_hex),
            name_hint: att.smil_name.clone().or(Some(name)),
        });
    }
    Ok(out)
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
