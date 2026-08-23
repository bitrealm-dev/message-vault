//! Attachment helpers for the emitter.

use anyhow::Result;
use chrono::{Local, TimeZone};
use go_sms_mms::ParsedPdu;
use message_ir::{IrAttachment, PendingAttachment};
use message_vault_io_core::{ExportReport, digest_prefix, write_if_missing};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// Write PDU (binary SMS/MMS) attachment parts under `attachments_dir`.
///
/// # Errors
///
/// Returns an error when an attachment file cannot be written.
pub(super) fn save_pdu_attachments(
    parsed: &ParsedPdu,
    attachments_dir: &Path,
    report: &mut ExportReport,
    copy_attachments: bool,
) -> Result<Vec<PendingAttachment>> {
    if !copy_attachments {
        return Ok(Vec::new());
    }
    fs::create_dir_all(attachments_dir)?;
    let date_prefix = Local
        .timestamp_opt(parsed.timestamp, 0)
        .single()
        .map(|t| t.format("%Y%m%d_%H%M%S").to_string())
        .unwrap_or_else(|| parsed.timestamp.to_string());

    let mut out = Vec::new();
    for (idx, att) in parsed.attachments.iter().enumerate() {
        let digest_hex = hex::encode(Sha256::digest(&att.data));
        let digest_prefix = digest_prefix(&digest_hex);
        let name = format!(
            "{}-I_{}_{}_{}{}",
            date_prefix,
            parsed.timestamp,
            digest_prefix,
            idx + 1,
            att.ext
        );
        let path = attachments_dir.join(&name);
        // Content-addressed name: rewrite only when missing (same bytes → same path).
        if write_if_missing(&path, &att.data)? {
            report.attachments_saved += 1;
        }
        out.push(PendingAttachment {
            rel_path: format!("attachments/{name}"),
            content_type: media::mime_for_ext(&att.ext)
                .or(match att.ext.as_str() {
                    ".wav" => Some("audio/wav"),
                    _ => None,
                })
                .unwrap_or("")
                .to_string(),
            extension: att.ext.trim_start_matches('.').to_string(),
            digest_sha256: Some(digest_hex),
            name_hint: att.smil_name.clone().or(Some(name)),
        });
    }
    Ok(out)
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
