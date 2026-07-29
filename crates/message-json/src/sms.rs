//! Common SMS/MMS NDJSON schema for Android backup exporters
//! (SMS Backup+).
//!
//! Field names match [`crate::imessage`] where overlapping; this module only
//! includes the subset needed to fully represent those exports.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_NAME: &str = "sms";
pub const SCHEMA_VERSION: u32 = 2;

pub const RECORD_CONVERSATION: &str = "conversation";
pub const RECORD_MESSAGE: &str = "message";
pub const SERVICE_SMS: &str = "SMS";

/// Top-level NDJSON line: tagged on `"record"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
pub enum ExportRecord {
    Conversation(ConversationRecord),
    Message(MessageRecord),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRecord {
    #[serde(default = "default_schema_name")]
    pub schema: String,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub chat_identifier: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    pub conversation_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_title: Option<String>,
    #[serde(default)]
    pub participants: Vec<ParticipantRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_at: Option<String>,
}

fn default_schema_name() -> String {
    SCHEMA_NAME.to_string()
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantRecord {
    pub handle: String,
    #[serde(default, alias = "display_name", skip_serializing_if = "Option::is_none")]
    pub name_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    pub timestamp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_utc: Option<String>,
    pub is_from_me: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

impl ConversationRecord {
    /// Conversation header for SMS-style exports.
    pub fn header(
        chat_identifier: impl Into<String>,
        conversation_type: impl Into<String>,
        group_title: Option<String>,
        participants: Vec<ParticipantRecord>,
        exported_at: impl Into<String>,
    ) -> Self {
        Self {
            schema: SCHEMA_NAME.to_string(),
            schema_version: SCHEMA_VERSION,
            chat_identifier: chat_identifier.into(),
            service: Some(SERVICE_SMS.to_string()),
            conversation_type: conversation_type.into(),
            group_title,
            participants,
            exported_at: Some(exported_at.into()),
        }
    }
}

impl MessageRecord {
    /// Plain text/SMS message.
    pub fn text_message(
        guid: impl Into<String>,
        timestamp: impl Into<String>,
        timestamp_utc: Option<String>,
        is_from_me: bool,
        sender: Option<String>,
        text: Option<String>,
        attachments: Vec<AttachmentRecord>,
    ) -> Self {
        Self {
            guid: Some(guid.into()),
            timestamp: timestamp.into(),
            timestamp_utc,
            is_from_me,
            sender,
            service: Some(SERVICE_SMS.to_string()),
            text,
            attachments,
        }
    }
}

/// Deterministic message GUID from chat + timestamp + direction + body + attachment digests.
pub fn stable_guid(
    chat_id: &str,
    timestamp: &str,
    is_from_me: bool,
    text: &str,
    att_digests: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(chat_id.as_bytes());
    hasher.update(b"|");
    hasher.update(timestamp.as_bytes());
    hasher.update(b"|");
    hasher.update(if is_from_me { b"1" } else { b"0" });
    hasher.update(b"|");
    hasher.update(text.as_bytes());
    for d in att_digests {
        hasher.update(b"|");
        hasher.update(d.as_bytes());
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_conversation() {
        let c = ConversationRecord::header(
            "+15551212",
            "individual",
            None,
            vec![ParticipantRecord {
                handle: "+15551212".into(),
                name_hint: Some("Ada".into()),
            }],
            "2024-01-01T00:00:00Z",
        );
        let line = serde_json::to_string(&ExportRecord::Conversation(c)).unwrap();
        assert!(line.contains(r#""record":"conversation""#));
        assert!(line.contains(r#""schema":"sms""#));
        let back: ExportRecord = serde_json::from_str(&line).unwrap();
        match back {
            ExportRecord::Conversation(c) => {
                assert_eq!(c.schema, "sms");
                assert_eq!(c.schema_version, 2);
                assert_eq!(c.service.as_deref(), Some("SMS"));
                assert_eq!(c.conversation_type, "individual");
                assert!(line.contains(r#""conversation_type":"individual""#));
            }
            _ => panic!("expected conversation"),
        }
    }

    #[test]
    fn stable_guid_is_stable() {
        let a = stable_guid("c1", "2020-01-01 00:00:00", true, "hi", &[]);
        let b = stable_guid("c1", "2020-01-01 00:00:00", true, "hi", &[]);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }
}
