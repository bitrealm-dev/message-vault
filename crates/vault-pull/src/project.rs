//! Map vault export API messages into message-ir conversation documents.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDateTime, Utc};
use message_ir::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, IrAttachment,
    IrConversationType, IrDirection, IrImessage, IrMessage, IrMessageKind, IrParticipant, IrService,
    IrSource, SCHEMA_VERSION,
};
use serde_json::{Value, json};

use crate::http::{ExportAttachment, ExportMessage, ExportTapback};

pub fn conversation_key(msg: &ExportMessage) -> String {
    format!("{}::{}", msg.source, msg.conversation.chat_identifier)
}

pub fn build_document(
    source: &str,
    seed: &ExportMessage,
    messages: Vec<IrMessage>,
) -> ConversationDocument {
    let conversation_type = IrConversationType::parse(&seed.conversation.conversation_type);
    let participants = seed
        .conversation
        .participants
        .iter()
        .map(|p| IrParticipant {
            handle: p.handle.clone(),
            display_name: p.name_hint.clone(),
        })
        .collect::<Vec<_>>();
    let mut attachment_count = 0u64;
    let mut first_ts = None;
    let mut last_ts = None;
    for m in &messages {
        attachment_count += m.attachments.len() as u64;
        first_ts = Some(first_ts.map_or(m.timestamp_unix_ms, |t: i64| t.min(m.timestamp_unix_ms)));
        last_ts = Some(last_ts.map_or(m.timestamp_unix_ms, |t: i64| t.max(m.timestamp_unix_ms)));
    }

    ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export: ExportMeta {
            source: source.to_string(),
            tool: "message-vault".into(),
            tool_version: env!("CARGO_PKG_VERSION").into(),
            owner_handle: None,
            owner_display_name: Some("Me".into()),
        },
        conversation: ConversationMeta {
            chat_identifier: seed.conversation.chat_identifier.clone(),
            conversation_type,
            group_title: seed.conversation.group_title.clone(),
            participants,
            stats: ConversationStats {
                message_count: messages.len() as u64,
                attachment_count,
                first_timestamp_unix_ms: first_ts,
                last_timestamp_unix_ms: last_ts,
            },
        },
        messages,
        packaging_stem_suffix: None,
    }
}

pub fn to_ir_message(msg: &ExportMessage, skip_attachments: bool) -> Result<IrMessage> {
    let timestamp_unix_ms = parse_timestamp_unix_ms(
        msg.timestamp_utc
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(msg.timestamp.as_str()),
    )
    .with_context(|| format!("message {} timestamp", msg.id))?;

    let service = IrService::parse(msg.conversation.service.as_deref().unwrap_or(""));
    let direction = if msg.is_from_me {
        IrDirection::Outgoing
    } else {
        IrDirection::Incoming
    };
    let message_kind = infer_kind(msg, service);

    let attachments = if skip_attachments {
        Vec::new()
    } else {
        msg.attachments
            .iter()
            .map(to_ir_attachment)
            .collect::<Vec<_>>()
    };

    let mut imessage = IrImessage {
        is_reply: msg.is_reply,
        in_reply_to_guid: msg.thread_originator_guid.clone(),
        thread_originator_part: msg.thread_originator_part.and_then(|n| u32::try_from(n).ok()),
        num_replies: u32::try_from(msg.num_replies).ok().filter(|&n| n > 0),
        announcement: msg.is_announcement.then(|| msg.text.clone().unwrap_or_default()),
        tapbacks: tapbacks_json(&msg.tapbacks),
        ..Default::default()
    };
    if imessage.announcement.as_deref().is_some_and(str::is_empty) {
        imessage.announcement = None;
    }

    let guid = msg
        .guid
        .clone()
        .filter(|g| !g.trim().is_empty())
        .unwrap_or_else(|| format!("vault:{}", msg.id));

    Ok(IrMessage {
        guid,
        timestamp_unix_ms,
        direction,
        service,
        message_kind,
        sender_handle: msg.sender.clone(),
        sender_display_name: None,
        subject: msg.subject.clone(),
        text: msg.text.clone().unwrap_or_default(),
        attachments,
        imessage: imessage.into_option(),
        source: Some(IrSource {
            android_type: None,
            fields: {
                let mut map = serde_json::Map::new();
                map.insert("vault_message_id".into(), json!(msg.id));
                map.insert("vault_source".into(), json!(msg.source));
                map
            },
        })
        .and_then(|s| s.into_option()),
    })
}

fn to_ir_attachment(att: &ExportAttachment) -> IrAttachment {
    let path = att.path.clone().or_else(|| {
        att.sha256
            .as_ref()
            .map(|sha| format!("attachments/{sha}"))
    });
    IrAttachment {
        path,
        original_name: att.original_name.clone(),
        mime_type: att.mime_type.clone(),
        digest_sha256: att.sha256.clone(),
        is_sticker: att.is_sticker,
        transcription: att.transcription.clone(),
        sticker_effect: None,
        size_bytes: att.size_bytes,
        bytes: None,
    }
}

fn tapbacks_json(tapbacks: &[ExportTapback]) -> Option<Value> {
    if tapbacks.is_empty() {
        return None;
    }
    Some(Value::Array(
        tapbacks
            .iter()
            .map(|t| {
                json!({
                    "part_index": t.part_index,
                    "kind": t.kind,
                    "emoji": t.emoji,
                    "is_from_me": t.is_from_me,
                    "sender": t.sender,
                })
            })
            .collect(),
    ))
}

fn infer_kind(msg: &ExportMessage, service: IrService) -> IrMessageKind {
    if msg.is_announcement {
        return IrMessageKind::Announcement;
    }
    if !msg.attachments.is_empty() && matches!(service, IrService::Sms | IrService::Rcs) {
        return IrMessageKind::Mms;
    }
    match service {
        IrService::IMessage => IrMessageKind::IMessage,
        IrService::Sms => IrMessageKind::Sms,
        IrService::Rcs => IrMessageKind::Sms,
        IrService::Whatsapp => IrMessageKind::Unknown,
        IrService::Unknown => {
            if msg.attachments.is_empty() {
                IrMessageKind::Sms
            } else {
                IrMessageKind::Mms
            }
        }
    }
}

fn parse_timestamp_unix_ms(raw: &str) -> Result<i64> {
    let t = raw.trim();
    if t.is_empty() {
        bail!("empty timestamp");
    }
    if let Ok(ms) = t.parse::<i64>() {
        // Heuristic: seconds vs millis.
        // Any numeric value below 10^10 is a seconds timestamp (year 2286 in
        // seconds, well beyond any real SMS data). Values at or above 10^10
        // are millisecond timestamps (1973-03-03 in ms, still before SMS but
        // close enough that real data won't hit the ambiguity).
        return Ok(if ms.abs() < 10_000_000_000 {
            ms.saturating_mul(1000)
        } else {
            ms
        });
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(t) {
        return Ok(dt.timestamp_millis());
    }
    // Common vault form without offset: treat as UTC.
    if let Ok(ndt) = NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(ndt.and_utc().timestamp_millis());
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S") {
        return Ok(ndt.and_utc().timestamp_millis());
    }
    // Last resort: chrono's RFC3339-ish with space
    if let Ok(dt) = DateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S %z") {
        return Ok(dt.timestamp_millis());
    }
    let _ = Utc::now();
    bail!("unrecognized timestamp: {t}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{ExportConversation, ExportParticipant};

    #[test]
    fn parses_rfc3339() {
        let ms = parse_timestamp_unix_ms("2015-03-12T18:05:22Z").unwrap();
        assert!(ms > 0);
    }

    #[test]
    fn maps_basic_message() {
        let msg = ExportMessage {
            id: 1,
            source: "imessage".into(),
            guid: Some("g1".into()),
            timestamp: "2015-03-12T14:05:22-04:00".into(),
            timestamp_utc: Some("2015-03-12T18:05:22Z".into()),
            is_from_me: false,
            sender: Some("+1".into()),
            subject: None,
            text: Some("hi".into()),
            is_announcement: false,
            is_reply: false,
            thread_originator_guid: None,
            thread_originator_part: None,
            num_replies: 0,
            conversation: ExportConversation {
                id: 9,
                chat_identifier: "+1".into(),
                service: Some("iMessage".into()),
                conversation_type: "individual".into(),
                group_title: None,
                participants: vec![ExportParticipant {
                    handle: "+1".into(),
                    name_hint: Some("Sam".into()),
                }],
            },
            attachments: vec![],
            tapbacks: vec![],
        };
        let ir = to_ir_message(&msg, false).unwrap();
        assert_eq!(ir.guid, "g1");
        assert_eq!(ir.text, "hi");
        assert_eq!(ir.service, IrService::IMessage);
    }
}
