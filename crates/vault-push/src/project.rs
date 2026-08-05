//! Serialize message-ir v3 JSONL lines for Message Vault import.

use anyhow::{Context, Result, bail};
use message_ir::{ConversationDocument, ConversationHeader, IrMessage, SCHEMA_VERSION};

pub fn validate_header(header: &ConversationHeader) -> Result<String> {
    if header.schema_version != SCHEMA_VERSION {
        bail!(
            "unsupported schema_version {} (expected {})",
            header.schema_version,
            SCHEMA_VERSION
        );
    }
    let source = header.export.source.trim();
    if source.is_empty() {
        bail!("export.source is empty");
    }
    Ok(source.to_string())
}

/// Header line for a conversation document (message-ir JSONL).
pub fn document_header_line(doc: &ConversationDocument) -> Result<Vec<u8>> {
    let header = ConversationHeader::from_document(doc);
    validate_header(&header)?;
    let mut out = serde_json::to_vec(&header).context("serialize message-ir conversation header")?;
    out.push(b'\n');
    Ok(out)
}

/// Message line with attachment digests and sizes filled from upload scan.
/// Each entry is `(attachment_index, sha256_hex, size_bytes)`.
pub fn message_line(
    msg: &IrMessage,
    digests: &[(usize, String, u64)],
) -> Result<(Vec<u8>, String)> {
    let mut msg = msg.clone();
    for (i, digest, size) in digests {
        if let Some(att) = msg.attachments.get_mut(*i) {
            att.digest_sha256 = Some(digest.clone());
            att.size_bytes = Some(*size);
        }
    }
    serialize_message(&msg)
}

/// Message line with attachments stripped (text-only import).
pub fn message_line_without_attachments(msg: &IrMessage) -> Result<(Vec<u8>, String)> {
    let mut msg = msg.clone();
    msg.attachments.clear();
    serialize_message(&msg)
}

fn serialize_message(msg: &IrMessage) -> Result<(Vec<u8>, String)> {
    let mut out = serde_json::to_vec(msg).context("serialize message-ir message")?;
    out.push(b'\n');
    let guid = if msg.guid.trim().is_empty() {
        format!("unguided:{}", msg.timestamp_unix_ms)
    } else {
        msg.guid.clone()
    };
    Ok((out, guid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use message_ir::{
        ConversationMeta, ConversationStats, ExportMeta, IrConversationType, IrDirection,
        IrMessageKind, IrParticipant, IrService,
    };

    #[test]
    fn serializes_ir_sms() {
        let doc = ConversationDocument {
            schema_version: SCHEMA_VERSION,
            export: ExportMeta {
                source: "sms-backup-restore".into(),
                tool: "SMS Backup & Restore".into(),
                tool_version: "10.26.003".into(),
                owner_handle: Some("+15555550100".into()),
                owner_display_name: Some("Me".into()),
            },
            conversation: ConversationMeta {
                chat_identifier: "+15555550101".into(),
                conversation_type: IrConversationType::Individual,
                group_title: None,
                participants: vec![IrParticipant {
                    handle: "+15555550101".into(),
                    display_name: Some("Sam".into()),
                    handle_type: None,
                }],
                stats: ConversationStats::default(),
            },
            messages: vec![],
            packaging_stem_suffix: None,
        };
        let header = String::from_utf8(document_header_line(&doc).unwrap()).unwrap();
        assert!(header.contains(r#""schema_version":3"#));
        assert!(header.contains(r#""sms-backup-restore""#));
        assert!(!header.contains(r#""record":"conversation""#));

        let msg = IrMessage {
            guid: "g1".into(),
            timestamp_unix_ms: 1_400_773_261_000,
            direction: IrDirection::Incoming,
            service: IrService::Sms,
            message_kind: IrMessageKind::Sms,
            sender_handle: Some("+15555550101".into()),
            sender_display_name: Some("Sam".into()),
            subject: None,
            text: "hello".into(),
            attachments: vec![],
            imessage: None,
            source: None,
        };
        let (line, guid) = message_line(&msg, &[]).unwrap();
        assert_eq!(guid, "g1");
        let s = String::from_utf8(line).unwrap();
        assert!(s.contains(r#""direction":"incoming""#));
        assert!(!s.contains(r#""record":"message""#));
    }
}
