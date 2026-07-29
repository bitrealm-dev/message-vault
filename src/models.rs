//! Import helpers around shared NDJSON schemas.

use anyhow::{Context, Result, bail};
use serde_json::Value;

#[allow(unused_imports)] // re-exported for callers
pub use message_json::vault::{
    AttachmentRecord, ConversationRecord, ExportRecord, MessageRecord, ParticipantRecord,
    TapbackRecord,
};

/// Strip Apple's attachment object-replacement character (U+FFFC) from body text.
pub fn clean_body(text: Option<&str>) -> Option<String> {
    text.map(|s| s.replace('\u{FFFC}', "").trim().to_string())
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WireSchema {
    /// Standard vault NDJSON (`schema: "vault"`), also used for CSV ingest.
    Vault,
    /// Legacy iOS exporter (`schema: "imessage"`). Same field layout as vault.
    Imessage,
    /// Legacy SMS Backup+ NDJSON (`schema: "sms"`).
    Sms,
}

impl WireSchema {
    fn from_conversation_value(v: &Value) -> Self {
        if let Some(s) = v.get("schema").and_then(|x| x.as_str()) {
            return match s {
                "sms" => Self::Sms,
                "vault" => Self::Vault,
                "imessage" => Self::Imessage,
                _ => Self::Vault,
            };
        }
        // No schema field: schema_version 2 ⇒ sms; otherwise legacy imessage / vault layout.
        match v.get("schema_version").and_then(|x| x.as_u64()) {
            Some(2) => Self::Sms,
            _ => Self::Imessage,
        }
    }
}

fn participant_from_sms(p: message_json::sms::ParticipantRecord) -> ParticipantRecord {
    ParticipantRecord {
        handle: p.handle,
        name_hint: p.name_hint,
    }
}

fn attachment_from_sms(a: message_json::sms::AttachmentRecord) -> AttachmentRecord {
    AttachmentRecord {
        path: a.path,
        original_name: a.original_name,
        mime_type: a.mime_type,
        sha256: None,
        is_sticker: false,
        transcription: None,
    }
}

fn conversation_from_sms(c: message_json::sms::ConversationRecord) -> ConversationRecord {
    ConversationRecord {
        schema: message_json::vault::SCHEMA_NAME.to_string(),
        schema_version: message_json::vault::SCHEMA_VERSION,
        chat_identifier: c.chat_identifier,
        service: c.service,
        conversation_type: c.conversation_type,
        group_title: c.group_title,
        participants: c
            .participants
            .into_iter()
            .map(participant_from_sms)
            .collect(),
        exported_at: c.exported_at,
        export_source: None,
        export_tool: None,
        export_tool_version: None,
    }
}

fn message_from_sms(m: message_json::sms::MessageRecord) -> MessageRecord {
    MessageRecord {
        guid: m.guid,
        timestamp: m.timestamp,
        timestamp_utc: m.timestamp_utc,
        is_from_me: m.is_from_me,
        sender: m.sender,
        service: m.service,
        subject: None,
        text: m.text,
        read_receipt: None,
        is_deleted: false,
        send_effect: None,
        shared_location: None,
        is_announcement: false,
        announcement: None,
        attachments: m.attachments.into_iter().map(attachment_from_sms).collect(),
        tapbacks: Vec::new(),
        parts: Vec::new(),
        edits: Vec::new(),
        app: None,
        is_reply: false,
        thread_originator_guid: None,
        thread_originator_part: None,
        num_replies: 0,
    }
}

fn normalize_to_vault_conversation(mut c: ConversationRecord) -> ConversationRecord {
    c.schema = message_json::vault::SCHEMA_NAME.to_string();
    c.schema_version = message_json::vault::SCHEMA_VERSION;
    c
}

/// Parse NDJSON lines from one file into vault records.
///
/// Conversation `schema` selects sms vs vault/imessage parsing for following messages.
/// Missing `schema` with schema_version ≠ 2 is treated as imessage-shaped layout.
pub fn parse_export_lines(lines: impl IntoIterator<Item = String>) -> Result<Vec<ExportRecord>> {
    let mut active_schema: Option<WireSchema> = None;
    let mut records = Vec::new();

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        records.push(parse_export_line(line, &mut active_schema)?);
    }

    Ok(records)
}

fn parse_export_line(line: &str, active_schema: &mut Option<WireSchema>) -> Result<ExportRecord> {
    let value: Value = serde_json::from_str(line).context("invalid JSON")?;
    let record = value.get("record").and_then(|r| r.as_str()).unwrap_or("");

    match record {
        "conversation" => {
            let schema = WireSchema::from_conversation_value(&value);
            *active_schema = Some(schema);
            match schema {
                WireSchema::Sms => {
                    let c: message_json::sms::ConversationRecord =
                        serde_json::from_value(value).context("sms conversation")?;
                    Ok(ExportRecord::Conversation(conversation_from_sms(c)))
                }
                WireSchema::Vault | WireSchema::Imessage => {
                    let c: ConversationRecord =
                        serde_json::from_value(value).context("conversation")?;
                    Ok(ExportRecord::Conversation(normalize_to_vault_conversation(
                        c,
                    )))
                }
            }
        }
        "message" => {
            let schema = active_schema.unwrap_or(WireSchema::Vault);
            match schema {
                WireSchema::Sms => {
                    let m: message_json::sms::MessageRecord =
                        serde_json::from_value(value).context("sms message")?;
                    Ok(ExportRecord::Message(message_from_sms(m)))
                }
                WireSchema::Vault | WireSchema::Imessage => {
                    let m: MessageRecord = serde_json::from_value(value).context("message")?;
                    Ok(ExportRecord::Message(m))
                }
            }
        }
        other => bail!("unknown NDJSON record type '{other}'"),
    }
}
