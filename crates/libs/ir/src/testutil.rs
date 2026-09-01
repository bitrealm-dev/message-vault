//! Shared test fixture for crate tests (behind the `testutil` feature).

use crate::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, IrConversationType,
    IrDirection, IrMessage, IrMessageKind, IrParticipant, IrService, IrSource, SCHEMA_VERSION,
};

/// One-message conversation fixture: an incoming SMS from `+15555550101`.
///
/// `text` becomes the message body. Stats are computed on return.
pub fn sample_document(text: &str) -> ConversationDocument {
    let mut doc = ConversationDocument {
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
                handle: Some("+15555550101".into()),
                display_name: Some("Sam".into()),
                handle_type: Some(crate::HandleType::Phone),
            }],
            stats: ConversationStats::default(),
        },
        messages: vec![IrMessage {
            guid: "aabbccddeeff00112233445566778899".into(),
            timestamp_unix_ms: 1_400_773_261_000,
            direction: IrDirection::Incoming,
            service: IrService::Sms,
            message_kind: IrMessageKind::Sms,
            sender_handle: Some("+15555550101".into()),
            sender_display_name: Some("Sam".into()),
            subject: None,
            text: text.into(),
            attachments: vec![],
            imessage: None,
            source: Some(IrSource {
                android_type: Some(1),
                fields: {
                    let mut m = serde_json::Map::new();
                    m.insert("address".into(), serde_json::json!("+15555550101"));
                    m
                },
            }),
        }],
        packaging_stem_suffix: None,
    };
    doc.finalize_stats();
    doc
}
