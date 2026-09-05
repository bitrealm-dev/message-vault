//! Attachment records and part-index helpers for the emitter.

use imessage_database::{
    message_types::handwriting::HandwrittenMessage,
    tables::{attachment::Attachment, messages::Message},
};
use imessage_reader_protocol::{Attachment as AttachmentRecord, AttachmentSource};

use crate::{
    attachments::resolved_path,
    body::referenced_attachment_indices,
    error::RuntimeError,
    fields::{PartRecord, build_part_records, sticker_extras, transcription_for_attachment},
    session::MailSession,
};

/// Render handwriting ink as an SVG attachment, if this message is handwriting.
fn try_handwriting_svg(session: &MailSession, message: &Message) -> Option<AttachmentRecord> {
    if !message.is_handwriting() {
        return None;
    }
    let payload = message.raw_payload_data(session.data_source.db())?;
    let hw = HandwrittenMessage::from_payload(&payload).ok()?;
    Some(AttachmentRecord {
        original_name: Some(format!("{}.svg", message.guid)),
        mime_type: Some("image/svg+xml".into()),
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        source: AttachmentSource::Inline {
            text: hw.render_svg(),
        },
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

/// Body parts and the attachment records the body references, in body order.
///
/// No bytes are read here. Each record names the file Messages resolved (or
/// says there is none), and the app reads or asks for it when it writes.
///
/// # Errors
///
/// Returns an error when attachments cannot be loaded from the database.
pub(super) fn collect_parts_and_attachments(
    session: &MailSession,
    message: &Message,
) -> Result<(Vec<PartRecord>, Vec<AttachmentRecord>), RuntimeError> {
    let attachments = Attachment::from_message(session.data_source.db(), message)?;
    let referenced = referenced_attachment_indices(message, &attachments);
    let index_by_full: std::collections::HashMap<usize, usize> = referenced
        .iter()
        .enumerate()
        .map(|(kept, &full)| (full, kept))
        .collect();

    let mut parts = build_part_records(message, &attachments);
    remap_part_attachment_indices(&mut parts, &index_by_full);

    let mut records = Vec::new();
    for &idx in &referenced {
        let attachment = &attachments[idx];
        let transcription = transcription_for_attachment(message, attachment);
        let (_prompt, sticker_effect) = sticker_extras(
            attachment,
            &session.options.platform,
            session.options.db_path.as_path(),
            session.options.attachment_root.as_deref(),
        );
        let size_hint = (attachment.total_bytes > 0).then_some(attachment.total_bytes as u64);
        let source = match resolved_path(session, attachment) {
            Some(path) => AttachmentSource::Path { path, size_hint },
            None => AttachmentSource::Missing,
        };
        records.push(AttachmentRecord {
            original_name: attachment.transfer_name.clone(),
            mime_type: attachment.mime_type.clone(),
            is_sticker: attachment.is_sticker,
            transcription,
            sticker_effect,
            source,
        });
    }

    if let Some(svg) = try_handwriting_svg(session, message) {
        records.push(svg);
    }

    Ok((parts, records))
}
