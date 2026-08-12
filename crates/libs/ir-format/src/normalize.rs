//! Normalize IR documents before content equality checks.

#[cfg(test)]
use message_ir::{ConversationDocument, IrAttachment};
use message_ir::{IrImessage, IrSource};

/// Prepare a document for content equality after round-trip.
///
/// - Recomputes conversation stats
/// - Collapses empty `source` / `imessage` bags to `None`
/// - Clears packaging stem suffix and attachment bytes (not part of JSON content)
#[cfg(test)]
pub(crate) fn normalize_document_for_compare(doc: &mut ConversationDocument) {
    for msg in &mut doc.messages {
        if let Some(source) = msg.source.take() {
            msg.source = source.into_option();
        }
        if let Some(mut imessage) = msg.imessage.take() {
            // Match CSV packaging: a single text/run part equal to `text` is omitted.
            if crate::write::parts_are_trivial_text_duplicate(&msg.text, imessage.parts.as_ref()) {
                imessage.parts = None;
            }
            msg.imessage = imessage.into_option();
        }
        for att in &mut msg.attachments {
            clear_attachment_ephemera(att);
        }
        // Normalize empty option strings.
        empty_to_none(&mut msg.sender_handle);
        empty_to_none(&mut msg.sender_display_name);
        empty_to_none(&mut msg.subject);
    }
    empty_to_none(&mut doc.export.owner_handle);
    empty_to_none(&mut doc.export.owner_display_name);
    empty_to_none(&mut doc.conversation.group_title);
    for p in &mut doc.conversation.participants {
        empty_to_none(&mut p.display_name);
    }
    doc.packaging_stem_suffix = None;
    doc.schema_version = message_ir::SCHEMA_VERSION;
    doc.finalize_stats();
}

#[cfg(test)]
fn clear_attachment_ephemera(att: &mut IrAttachment) {
    att.bytes = None;
    empty_to_none(&mut att.path);
    empty_to_none(&mut att.original_name);
    empty_to_none(&mut att.mime_type);
    empty_to_none(&mut att.digest_sha256);
    empty_to_none(&mut att.transcription);
    empty_to_none(&mut att.sticker_effect);
}

#[cfg(test)]
fn empty_to_none(v: &mut Option<String>) {
    if let Some(s) = v.as_ref() {
        if s.trim().is_empty() {
            *v = None;
        }
    }
}

/// Build [`IrImessage`] from optional pieces; returns `None` when empty.
pub(crate) fn imessage_from_parts(im: IrImessage) -> Option<IrImessage> {
    im.into_option()
}

/// Build [`IrSource`] from android type + fields JSON cell.
pub(crate) fn source_from_parts(android_type: Option<i32>, fields_json: &str) -> Option<IrSource> {
    let fields = if fields_json.trim().is_empty() {
        Default::default()
    } else {
        serde_json::from_str(fields_json).unwrap_or_default()
    };
    IrSource {
        android_type,
        fields,
    }
    .into_option()
}
