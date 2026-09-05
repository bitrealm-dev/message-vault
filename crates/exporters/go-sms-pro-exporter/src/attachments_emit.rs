//! Attachment helpers for the emitter.

use go_sms_mms::ParsedPdu;
use message_ir::PendingAttachment;
use message_vault_io_core::digest_prefix;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Queue PDU attachment parts as metadata. Bytes stay in `blob_bytes` until
/// the shared runner writes them.
pub(super) fn queue_pdu_attachments(
    parsed: &ParsedPdu,
    copy_attachments: bool,
    blob_bytes: &mut HashMap<String, Vec<u8>>,
) -> Vec<PendingAttachment> {
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
    out
}
