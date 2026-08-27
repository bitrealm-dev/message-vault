//! Attachment helpers: queue blobs during parse, then map staged attachments
//! onto the shared [`IrAttachment`] shape after the runner writes files.

use crate::types::AttachmentBlob;
use media::{CompressOptions, MediaMode};
use message_ir::{IrAttachment, PendingAttachment};
use message_vault_io_core::{
    AttachmentJob, CancelFlag, ExportReport, LogSink, emit_log, run_attachment_jobs,
};
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

/// Write queued attachment bytes after parse and before conversation files.
pub(super) fn stage_conversation_attachments(
    documents: &mut [message_ir::ConversationDocument],
    attachments_dir: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
    log: Option<&LogSink>,
    cancel: Option<&CancelFlag>,
    report: &mut ExportReport,
) -> Result<(), String> {
    let payloads: Vec<Option<Vec<u8>>> = documents
        .iter()
        .flat_map(|doc| {
            doc.messages
                .iter()
                .flat_map(|msg| msg.attachments.iter().map(|att| att.bytes.clone()))
        })
        .collect();

    let mut jobs = Vec::new();
    for doc in documents.iter_mut() {
        for msg in &mut doc.messages {
            let ts = msg.timestamp_unix_ms;
            for att in &mut msg.attachments {
                let hint = att
                    .size_bytes
                    .or_else(|| att.bytes.as_ref().map(|b| b.len() as u64));
                jobs.push(AttachmentJob {
                    attachment: att,
                    timestamp_unix_ms: ts,
                    size_hint: hint,
                });
            }
        }
    }
    run_attachment_jobs(
        &mut jobs,
        attachments_dir,
        mode,
        compress,
        |i| Ok(payloads.get(i).cloned().flatten()),
        |progress| {
            emit_log(
                log,
                format!(
                    "  attachments {}/{} {}/{}",
                    progress.done, progress.total, progress.bytes_done, progress.bytes_total
                ),
            );
        },
        cancel.map(|flag| flag.as_ref()),
    )?;

    for job in &jobs {
        if job.attachment.path.is_some() && job.attachment.digest_sha256.is_some() {
            report.attachments_saved += 1;
        }
    }
    for doc in documents.iter_mut() {
        for msg in &mut doc.messages {
            for att in &mut msg.attachments {
                att.bytes = None;
            }
        }
    }
    Ok(())
}
