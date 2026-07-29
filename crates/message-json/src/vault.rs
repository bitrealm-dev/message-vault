//! Standard vault NDJSON schema for **all** message sources.
//!
//! This is not iMessage-specific. Message Exporters map into this
//! shape; optional rich fields (tapbacks, replies, parts, …) are omitted when unused.
//!
//! Wire: `"schema": "vault"`, `schema_version` 1.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_NAME: &str = "vault";
pub const SCHEMA_VERSION: u32 = 1;

pub const RECORD_CONVERSATION: &str = "conversation";
pub const RECORD_MESSAGE: &str = "message";

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
    /// Carrier / app channel: `"SMS"`, `"iMessage"`, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    pub conversation_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_title: Option<String>,
    #[serde(default)]
    pub participants: Vec<ParticipantRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_at: Option<String>,
    /// Wire source id (`imessage`, `sms-backup-plus`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_source: Option<String>,
    /// Upstream export tool name (e.g. `iMessage Exporter`, `SMS Backup+`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_tool: Option<String>,
    /// Upstream tool / app version this export targets (e.g. `4.2.0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_tool_version: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    pub timestamp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_utc: Option<String>,
    /// True when the vault owner sent the message (CSV `direction=outgoing`).
    pub is_from_me: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_receipt: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_deleted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_effect: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_location: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_announcement: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub announcement: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tapbacks: Vec<TapbackRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<PartRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<EditEventRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<Value>,
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
    /// Content hash of the attachment bytes (lowercase hex). When set, import may
    /// resolve a previously uploaded blob under the account/source asset store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
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

/// One logical message body part (iMessage multi-part bubbles).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartRecord {
    pub index: u64,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachment_indices: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub emoji_image: bool,
}

/// One historical edit (or unsent marker) for a body part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditEventRecord {
    pub part_index: u64,
    pub status: String,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
}

impl ConversationRecord {
    /// Conversation header for vault ingest NDJSON.
    pub fn header(
        chat_identifier: impl Into<String>,
        conversation_type: impl Into<String>,
        group_title: Option<String>,
        participants: Vec<ParticipantRecord>,
        service: impl Into<String>,
        exported_at: impl Into<String>,
    ) -> Self {
        Self {
            schema: SCHEMA_NAME.to_string(),
            schema_version: SCHEMA_VERSION,
            chat_identifier: chat_identifier.into(),
            service: Some(service.into()),
            conversation_type: conversation_type.into(),
            group_title,
            participants,
            exported_at: Some(exported_at.into()),
            export_source: None,
            export_tool: None,
            export_tool_version: None,
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
            "SMS",
            "2024-01-01T00:00:00Z",
        );
        let line = serde_json::to_string(&ExportRecord::Conversation(c)).unwrap();
        assert!(line.contains(r#""record":"conversation""#));
        assert!(line.contains(r#""schema":"vault""#));
        let back: ExportRecord = serde_json::from_str(&line).unwrap();
        match back {
            ExportRecord::Conversation(c) => {
                assert_eq!(c.schema, "vault");
                assert_eq!(c.schema_version, 1);
                assert_eq!(c.service.as_deref(), Some("SMS"));
            }
            _ => panic!("expected conversation"),
        }
    }

    #[test]
    fn lean_sms_omits_rich_fields() {
        let m = MessageRecord {
            guid: Some("g1".into()),
            timestamp: "2021-01-01T00:00:00Z".into(),
            timestamp_utc: None,
            is_from_me: false,
            sender: Some("+15551212".into()),
            service: Some("SMS".into()),
            subject: None,
            text: Some("hi".into()),
            read_receipt: None,
            is_deleted: false,
            send_effect: None,
            shared_location: None,
            is_announcement: false,
            announcement: None,
            attachments: Vec::new(),
            tapbacks: Vec::new(),
            parts: Vec::new(),
            edits: Vec::new(),
            app: None,
            is_reply: false,
            thread_originator_guid: None,
            thread_originator_part: None,
            num_replies: 0,
        };
        let line = serde_json::to_string(&ExportRecord::Message(m)).unwrap();
        assert!(!line.contains("tapbacks"));
        assert!(!line.contains("parts"));
        assert!(!line.contains("send_effect"));
        assert!(!line.contains("is_deleted"));
        assert!(!line.contains("is_reply"));
    }
}
