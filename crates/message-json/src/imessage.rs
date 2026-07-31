//! iMessage / iOS JSONL schema (`schema_version` 4; historically imessage-exporter-json v3).

use serde::{Deserialize, Serialize};

pub const SCHEMA_NAME: &str = "imessage";
pub const SCHEMA_VERSION: u32 = 4;

pub const RECORD_CONVERSATION: &str = "conversation";
pub const RECORD_MESSAGE: &str = "message";
pub const SERVICE_IMESSAGE: &str = "iMessage";

/// Top-level JSONL line: tagged on `"record"`.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_announcement: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub announcement: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tapbacks: Vec<TapbackRecord>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_reply: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_originator_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_originator_part: Option<i64>,
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub num_replies: i64,
}

fn is_zero_i64(n: &i64) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_sticker: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapbackRecord {
    #[serde(default)]
    pub part_index: i64,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(default)]
    pub is_from_me: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
}

impl ConversationRecord {
    /// Conversation header for iMessage-style exports.
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
            service: Some(SERVICE_IMESSAGE.to_string()),
            conversation_type: conversation_type.into(),
            group_title,
            participants,
            exported_at: Some(exported_at.into()),
        }
    }
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
        assert!(line.contains(r#""schema":"imessage""#));
        let back: ExportRecord = serde_json::from_str(&line).unwrap();
        match back {
            ExportRecord::Conversation(c) => {
                assert_eq!(c.schema, "imessage");
                assert_eq!(c.schema_version, 4);
                assert_eq!(c.service.as_deref(), Some("iMessage"));
                assert_eq!(c.conversation_type, "individual");
                assert!(line.contains(r#""conversation_type":"individual""#));
            }
            _ => panic!("expected conversation"),
        }
    }

    #[test]
    fn without_schema_defaults_to_imessage() {
        let line = r#"{"record":"conversation","schema_version":4,"chat_identifier":"+1","conversation_type":"individual","participants":[]}"#;
        let back: ExportRecord = serde_json::from_str(line).unwrap();
        match back {
            ExportRecord::Conversation(c) => {
                assert_eq!(c.schema, "imessage");
                assert_eq!(c.schema_version, 4);
                assert_eq!(c.conversation_type, "individual");
            }
            _ => panic!("expected conversation"),
        }
    }
}
