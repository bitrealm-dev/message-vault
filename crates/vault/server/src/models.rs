//! Import-side records mapped from message-ir JSONL.

use anyhow::{Context, Result, bail};
use chrono::{Local, TimeZone, Utc};
use message_ir::{
    ConversationHeader, HandleService, HandleType, IrAttachment, IrDirection, IrImessage, IrMessage,
    IrMessageKind, IrService, SCHEMA_VERSION,
};
use phone::sanitize_number;
use serde::Deserialize;
use serde_json::Value;

/// One JSONL conversation after IR → vault-row mapping.
#[derive(Debug, Clone)]
pub enum ExportRecord {
    Conversation(ConversationRecord),
    Message(MessageRecord),
}

#[derive(Debug, Clone)]
pub struct ConversationRecord {
    pub chat_identifier: String,
    pub service: Option<String>,
    pub conversation_type: String,
    pub group_title: Option<String>,
    pub participants: Vec<ParticipantRecord>,
    pub exported_at: Option<String>,
    /// IR `export.source` — used as `messages.source` for directory import.
    pub export_source: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParticipantRecord {
    pub handle: String,
    pub name_alias: Option<String>,
    #[allow(dead_code)] // consumed by import-time handle resolution
    pub handle_type: Option<HandleType>,
}

#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub guid: Option<String>,
    pub timestamp: String,
    pub timestamp_utc: Option<String>,
    pub is_from_me: bool,
    pub sender: Option<String>,
    #[allow(dead_code)] // sender identity for import-time handle resolution
    pub sender_handle_type: Option<HandleType>,
    /// Per-message transport (`sms` / `imessage` / `rcs` / `whatsapp` / …).
    pub service: Option<String>,
    pub subject: Option<String>,
    pub text: Option<String>,
    pub is_announcement: bool,
    pub announcement: Option<String>,
    pub attachments: Vec<AttachmentRecord>,
    pub tapbacks: Vec<TapbackRecord>,
    pub is_reply: bool,
    pub thread_originator_guid: Option<String>,
    pub thread_originator_part: Option<i64>,
    pub num_replies: i64,
}

#[derive(Debug, Clone)]
pub struct AttachmentRecord {
    pub path: Option<String>,
    pub original_name: Option<String>,
    pub mime_type: Option<String>,
    pub sha256: Option<String>,
    pub is_sticker: bool,
    pub transcription: Option<String>,
    pub size_bytes: Option<u64>,
    pub missing_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TapbackRecord {
    pub part_index: i64,
    pub kind: String,
    pub emoji: Option<String>,
    pub is_from_me: bool,
    pub sender: Option<String>,
}

/// Strip Apple's attachment object-replacement character (U+FFFC) from body text.
pub fn clean_body(text: Option<&str>) -> Option<String> {
    text.map(|s| s.replace('\u{FFFC}', "").trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Parse message-ir JSONL lines into import records.
///
/// Accepts one or more concatenated conversations (each: header line, then
/// message lines). Remote push clients batch multiple conversations this way.
pub fn parse_ir_lines(lines: impl IntoIterator<Item = String>) -> Result<Vec<ExportRecord>> {
    let mut out = Vec::new();
    let mut saw_header = false;
    for (i, line) in lines.into_iter().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let line_no = i + 1;
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("parse JSON on message-ir line {line_no}"))?;
        if is_ir_header(&value) {
            let header: ConversationHeader = serde_json::from_value(value).with_context(|| {
                format!("parse message-ir conversation header on line {line_no}")
            })?;
            if header.schema_version != SCHEMA_VERSION {
                bail!(
                    "unsupported schema_version {} (expected {}) on line {line_no}",
                    header.schema_version,
                    SCHEMA_VERSION
                );
            }
            out.push(ExportRecord::Conversation(conversation_from_ir(&header)));
            saw_header = true;
        } else {
            if !saw_header {
                bail!(
                    "message-ir JSONL missing conversation header before message on line {line_no}"
                );
            }
            let msg: IrMessage = serde_json::from_value(value)
                .with_context(|| format!("parse message-ir message on line {line_no}"))?;
            out.push(ExportRecord::Message(message_from_ir(&msg)?));
        }
    }
    if out.is_empty() {
        bail!("empty message-ir JSONL (missing conversation header)");
    }
    Ok(out)
}

fn is_ir_header(value: &Value) -> bool {
    value.get("schema_version").is_some() && value.get("conversation").is_some()
}

fn conversation_from_ir(header: &ConversationHeader) -> ConversationRecord {
    let export_source = {
        let s = header.export.source.trim();
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    };
    ConversationRecord {
        chat_identifier: header.conversation.chat_identifier.clone(),
        // Platform identity for handles (phone | whatsapp), not SMS/iMessage/RCS.
        service: Some(
            if export_source
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case("whatsapp"))
            {
                HandleService::Whatsapp.as_str().to_string()
            } else {
                HandleService::Phone.as_str().to_string()
            },
        ),
        conversation_type: header.conversation.conversation_type.as_str().to_string(),
        group_title: header.conversation.group_title.clone(),
        participants: header
            .conversation
            .participants
            .iter()
            .map(|p| ParticipantRecord {
                handle: p.handle.clone(),
                name_alias: p.display_name.clone(),
                handle_type: p.handle_type,
            })
            .collect(),
        exported_at: None,
        export_source,
    }
}

fn message_from_ir(msg: &IrMessage) -> Result<MessageRecord> {
    let secs = msg.timestamp_unix_ms.div_euclid(1000);
    let (timestamp, timestamp_utc) = format_timestamps(secs).with_context(|| {
        format!(
            "unrepresentable timestamp_unix_ms {}",
            msg.timestamp_unix_ms
        )
    })?;
    let is_from_me = msg.direction == IrDirection::Outgoing;
    let im = msg.imessage.as_ref();
    let text = {
        let t = msg.text.trim();
        if t.is_empty() {
            None
        } else {
            Some(msg.text.clone())
        }
    };
    let mut tapbacks = tapbacks_from_im(im, is_from_me, msg.sender_handle.as_deref());
    if tapbacks.is_empty()
        && let Some(kind) = im
            .and_then(|i| i.tapback_kind.as_ref())
            .filter(|s| !s.is_empty())
        {
            tapbacks.push(TapbackRecord {
                part_index: i64::from(im.and_then(|i| i.associated_part).unwrap_or(0)),
                kind: kind.clone(),
                emoji: im.and_then(|i| i.tapback_emoji.clone()),
                is_from_me,
                sender: if is_from_me {
                    None
                } else {
                    msg.sender_handle.clone()
                },
            });
        }

    Ok(MessageRecord {
        guid: if msg.guid.trim().is_empty() {
            None
        } else {
            Some(msg.guid.clone())
        },
        timestamp,
        timestamp_utc: Some(timestamp_utc),
        is_from_me,
        sender: if is_from_me {
            None
        } else {
            msg.sender_handle.clone()
        },
        sender_handle_type: if is_from_me {
            None
        } else {
            infer_sender_handle_type(msg.sender_handle.as_deref(), msg.service)
        },
        service: Some(msg.service.as_str().to_string()),
        subject: msg.subject.clone().filter(|s| !s.is_empty()),
        text,
        is_announcement: im.map(|i| i.announcement.is_some()).unwrap_or(false)
            || matches!(msg.message_kind, IrMessageKind::Announcement),
        announcement: im.and_then(|i| i.announcement.clone()),
        attachments: msg.attachments.iter().map(attachment_from_ir).collect(),
        tapbacks,
        is_reply: im.map(|i| i.is_reply).unwrap_or(false),
        thread_originator_guid: im.and_then(|i| i.in_reply_to_guid.clone()),
        thread_originator_part: im.and_then(|i| i.thread_originator_part.map(i64::from)),
        num_replies: im
            .and_then(|i| i.num_replies.map(i64::from))
            .unwrap_or(0),
    })
}

/// Infer the sender's handle type for import records.
///
/// IR participants carry an explicit `handle_type` when the source knows it;
/// message rows only carry a raw sender handle, so the type is inferred here
/// from the handle shape plus the service. Handles containing `@` are emails;
/// SMS/iMessage/WhatsApp/RCS handles that sanitize as phone numbers are
/// phones; anything else is `Other`.
fn infer_sender_handle_type(sender_handle: Option<&str>, service: IrService) -> Option<HandleType> {
    let handle = sender_handle?.trim();
    if handle.is_empty() {
        return None;
    }
    if handle.contains('@') {
        return Some(HandleType::Email);
    }
    if matches!(
        service,
        IrService::Sms | IrService::IMessage | IrService::Whatsapp | IrService::Rcs
    ) && sanitize_number(handle).is_some()
    {
        return Some(HandleType::Phone);
    }
    Some(HandleType::Other)
}

fn attachment_from_ir(a: &IrAttachment) -> AttachmentRecord {
    AttachmentRecord {
        path: a.path.clone(),
        original_name: a.original_name.clone(),
        mime_type: a.mime_type.clone(),
        sha256: a.digest_sha256.clone(),
        is_sticker: a.is_sticker,
        transcription: a.transcription.clone(),
        size_bytes: a.size_bytes,
        missing_reason: a.missing_reason.clone(),
    }
}

fn tapbacks_from_im(
    im: Option<&IrImessage>,
    fallback_from_me: bool,
    fallback_sender: Option<&str>,
) -> Vec<TapbackRecord> {
    let Some(im) = im else {
        return Vec::new();
    };
    let Some(raw) = im.tapbacks.as_ref() else {
        return Vec::new();
    };
    let items = match raw {
        Value::Array(items) => items.clone(),
        other if !other.is_null() => vec![other.clone()],
        _ => return Vec::new(),
    };
    items
        .into_iter()
        .filter_map(|v| {
            let t: WireTapback = serde_json::from_value(v).ok()?;
            Some(TapbackRecord {
                part_index: t.part_index,
                kind: t.kind,
                emoji: t.emoji,
                is_from_me: t.is_from_me.unwrap_or(fallback_from_me),
                sender: t
                    .sender
                    .or_else(|| fallback_sender.map(|s| s.to_string())),
            })
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct WireTapback {
    #[serde(default)]
    part_index: i64,
    kind: String,
    #[serde(default)]
    emoji: Option<String>,
    #[serde(default)]
    is_from_me: Option<bool>,
    #[serde(default)]
    sender: Option<String>,
}

fn format_timestamps(secs: i64) -> Option<(String, String)> {
    let local = Local.timestamp_opt(secs, 0).single().or_else(|| {
        Utc.timestamp_opt(secs, 0)
            .single()
            .map(|utc| Local.from_utc_datetime(&utc.naive_utc()))
    })?;
    let utc = local.with_timezone(&Utc);
    Some((
        local.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ir_sms_without_imessage_bag() {
        let lines = [
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"t","tool_version":"1","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550101","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550101","display_name":"Sam"}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1400773261000,"last_timestamp_unix_ms":1400773261000}}}"#.to_string(),
            r#"{"guid":"g1","timestamp_unix_ms":1400773261000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550101","sender_display_name":"Sam","subject":null,"text":"hello","attachments":[],"imessage":null,"source":null}"#.to_string(),
        ];
        let records = parse_ir_lines(lines).unwrap();
        assert_eq!(records.len(), 2);
        match &records[1] {
            ExportRecord::Message(m) => {
                assert_eq!(m.guid.as_deref(), Some("g1"));
                assert!(!m.is_from_me);
                assert_eq!(m.text.as_deref(), Some("hello"));
                assert_eq!(m.service.as_deref(), Some("sms"));
                assert_eq!(m.sender_handle_type, Some(HandleType::Phone));
                assert!(m.tapbacks.is_empty());
                assert!(!m.is_reply);
            }
            _ => panic!("expected message"),
        }
    }

    #[test]
    fn parses_concatenated_ir_conversations() {
        let header = |chat: &str| {
            format!(
                r#"{{"schema_version":3,"export":{{"source":"sms-backup-restore","tool":"t","tool_version":"1","owner_handle":null,"owner_display_name":null}},"conversation":{{"chat_identifier":"{chat}","conversation_type":"individual","group_title":null,"participants":[{{"handle":"{chat}","display_name":null}}],"stats":{{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1400773261000,"last_timestamp_unix_ms":1400773261000}}}}}}"#
            )
        };
        let msg = |guid: &str, handle: &str| {
            format!(
                r#"{{"guid":"{guid}","timestamp_unix_ms":1400773261000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"{handle}","sender_display_name":null,"subject":null,"text":"hi","attachments":[],"imessage":null,"source":null}}"#
            )
        };
        let records = parse_ir_lines([
            header("+15555550101"),
            msg("g1", "+15555550101"),
            header("+15555550102"),
            msg("g2", "+15555550102"),
        ])
        .unwrap();
        assert_eq!(records.len(), 4);
        match &records[0] {
            ExportRecord::Conversation(c) => assert_eq!(c.chat_identifier, "+15555550101"),
            _ => panic!("expected conversation"),
        }
        match &records[2] {
            ExportRecord::Conversation(c) => assert_eq!(c.chat_identifier, "+15555550102"),
            _ => panic!("expected conversation"),
        }
    }

    #[test]
    fn infers_sender_handle_type_from_handle_and_service() {
        assert_eq!(
            infer_sender_handle_type(Some("alice@example.com"), IrService::Unknown),
            Some(HandleType::Email)
        );
        assert_eq!(
            infer_sender_handle_type(Some("+15555550101"), IrService::Sms),
            Some(HandleType::Phone)
        );
        assert_eq!(
            infer_sender_handle_type(Some("+15555550101"), IrService::Signal),
            Some(HandleType::Other)
        );
        assert_eq!(
            infer_sender_handle_type(Some("alice_discord"), IrService::Discord),
            Some(HandleType::Other)
        );
        assert_eq!(infer_sender_handle_type(None, IrService::Sms), None);
    }
}
