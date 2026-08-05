//! Canonical conversation intermediate representation (IR) — schema types only.
//!
//! Source exporters parse vendor formats into [`ConversationDocument`]. Packaging
//! (FormatSink, readers/writers) lives in `message-ir-format`; directory convert
//! in `message-reexport`. See the [message-ir architecture](../../../docs/maintainers/architecture/message-ir.md).

use message_csv::conversation_filename;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const SCHEMA_VERSION: u32 = 3;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationDocument {
    pub schema_version: u32,
    pub export: ExportMeta,
    pub conversation: ConversationMeta,
    pub messages: Vec<IrMessage>,
    /// On-disk stem suffix (e.g. `__whatsapp`). Never serialized into JSON/JSONL.
    #[serde(skip)]
    pub packaging_stem_suffix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMeta {
    pub source: String,
    pub tool: String,
    pub tool_version: String,
    pub owner_handle: Option<String>,
    /// Outgoing display name; emitters should set when known (iMessage caller-id / `"Me"`).
    pub owner_display_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IrConversationType {
    Individual,
    Group,
}

impl IrConversationType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "individual",
            Self::Group => "group",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "group" => Self::Group,
            _ => Self::Individual,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub chat_identifier: String,
    pub conversation_type: IrConversationType,
    pub group_title: Option<String>,
    pub participants: Vec<IrParticipant>,
    pub stats: ConversationStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConversationStats {
    pub message_count: u64,
    pub attachment_count: u64,
    pub first_timestamp_unix_ms: Option<i64>,
    pub last_timestamp_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrParticipant {
    pub handle: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IrService {
    Sms,
    #[serde(rename = "imessage")]
    IMessage,
    Whatsapp,
    Rcs,
    Unknown,
}

impl IrService {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sms => "sms",
            Self::IMessage => "imessage",
            Self::Whatsapp => "whatsapp",
            Self::Rcs => "rcs",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "sms" => Self::Sms,
            "imessage" => Self::IMessage,
            "whatsapp" => Self::Whatsapp,
            "rcs" => Self::Rcs,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrMessageKind {
    Sms,
    Mms,
    #[serde(rename = "imessage")]
    IMessage,
    Tapback,
    StickerTapback,
    Announcement,
    LocationShare,
    Balloon,
    Unknown,
}

impl IrMessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sms => "sms",
            Self::Mms => "mms",
            Self::IMessage => "imessage",
            Self::Tapback => "tapback",
            Self::StickerTapback => "sticker_tapback",
            Self::Announcement => "announcement",
            Self::LocationShare => "location_share",
            Self::Balloon => "balloon",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "sms" => Self::Sms,
            "mms" => Self::Mms,
            "imessage" => Self::IMessage,
            "tapback" => Self::Tapback,
            "sticker_tapback" => Self::StickerTapback,
            "announcement" => Self::Announcement,
            "location_share" => Self::LocationShare,
            "balloon" => Self::Balloon,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrMessage {
    pub guid: String,
    pub timestamp_unix_ms: i64,
    pub direction: IrDirection,
    pub service: IrService,
    pub message_kind: IrMessageKind,
    pub sender_handle: Option<String>,
    pub sender_display_name: Option<String>,
    pub subject: Option<String>,
    pub text: String,
    pub attachments: Vec<IrAttachment>,
    pub imessage: Option<IrImessage>,
    pub source: Option<IrSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IrDirection {
    Incoming,
    Outgoing,
}

impl IrDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrAttachment {
    pub path: Option<String>,
    pub original_name: Option<String>,
    pub mime_type: Option<String>,
    pub digest_sha256: Option<String>,
    pub is_sticker: bool,
    pub transcription: Option<String>,
    pub sticker_effect: Option<String>,
    /// On-disk / vault asset length in bytes (not file contents).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// In-memory bytes for EML embedding; never written to JSON.
    #[serde(skip)]
    pub bytes: Option<Vec<u8>>,
}

/// Vendor leftovers. Display names live on `sender_display_name`, not here.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IrSource {
    pub android_type: Option<i32>,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

impl IrSource {
    pub fn is_empty(&self) -> bool {
        self.android_type.is_none() && self.fields.is_empty()
    }

    pub fn into_option(self) -> Option<Self> {
        if self.is_empty() { None } else { Some(self) }
    }
}

/// iMessage extensions. Nested Apple blobs remain JSON values (not strings).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IrImessage {
    pub is_reply: bool,
    pub in_reply_to_guid: Option<String>,
    pub thread_originator_part: Option<u32>,
    pub num_replies: Option<u32>,
    pub is_deleted: bool,
    pub send_effect: Option<String>,
    pub shared_location: Option<String>,
    pub announcement: Option<String>,
    pub read_receipt_rfc3339: Option<String>,
    pub parts: Option<Value>,
    pub edits: Option<Value>,
    pub tapbacks: Option<Value>,
    pub app: Option<Value>,
    pub balloon_bundle_id: Option<String>,
    pub balloon_kind: Option<String>,
    pub associated_guid: Option<String>,
    pub associated_part: Option<u32>,
    pub tapback_kind: Option<String>,
    pub tapback_emoji: Option<String>,
    pub tapback_action: Option<String>,
}

impl IrImessage {
    pub fn is_empty(&self) -> bool {
        !self.is_reply
            && self.in_reply_to_guid.is_none()
            && self.thread_originator_part.is_none()
            && self.num_replies.is_none()
            && !self.is_deleted
            && self.send_effect.is_none()
            && self.shared_location.is_none()
            && self.announcement.is_none()
            && self.read_receipt_rfc3339.is_none()
            && self.parts.is_none()
            && self.edits.is_none()
            && self.tapbacks.is_none()
            && self.app.is_none()
            && self.balloon_bundle_id.is_none()
            && self.balloon_kind.is_none()
            && self.associated_guid.is_none()
            && self.associated_part.is_none()
            && self.tapback_kind.is_none()
            && self.tapback_emoji.is_none()
            && self.tapback_action.is_none()
    }

    pub fn into_option(self) -> Option<Self> {
        if self.is_empty() { None } else { Some(self) }
    }
}

impl ConversationDocument {
    pub fn filename_stem(&self) -> String {
        let handles: Vec<String> = self
            .conversation
            .participants
            .iter()
            .map(|p| p.handle.clone())
            .collect();
        let csv = conversation_filename(
            self.conversation.conversation_type.as_str(),
            &self.conversation.chat_identifier,
            self.conversation.group_title.as_deref(),
            &handles,
            self.packaging_stem_suffix.as_deref(),
        );
        csv.strip_suffix(".csv").unwrap_or(csv.as_str()).to_string()
    }

    /// Recompute [`ConversationMeta::stats`] from `messages`.
    pub fn finalize_stats(&mut self) {
        self.conversation.stats = compute_stats(&self.messages);
    }
}

fn compute_stats(messages: &[IrMessage]) -> ConversationStats {
    let message_count = messages.len() as u64;
    let attachment_count = messages.iter().map(|m| m.attachments.len() as u64).sum();
    let mut first = None;
    let mut last = None;
    for msg in messages {
        first = Some(first.map_or(msg.timestamp_unix_ms, |f: i64| f.min(msg.timestamp_unix_ms)));
        last = Some(last.map_or(msg.timestamp_unix_ms, |l: i64| l.max(msg.timestamp_unix_ms)));
    }
    ConversationStats {
        message_count,
        attachment_count,
        first_timestamp_unix_ms: first,
        last_timestamp_unix_ms: last,
    }
}

/// Owner identity for outgoing rows: handle + display (`"Me"` if handle set but name missing).
pub fn owner_sender(export: &ExportMeta) -> (Option<String>, Option<String>) {
    let handle = export
        .owner_handle
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let display = export
        .owner_display_name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| handle.as_ref().map(|_| "Me".into()));
    (handle, display)
}

/// Parse Android type strings / numbers into `i32`.
pub fn parse_android_type(s: &str) -> Option<i32> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<i32>().ok()
}

/// Parse a JSON string into a [`Value`], or return the string as a JSON string value.
pub fn parse_json_value(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|_| json!(s))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationHeader {
    pub schema_version: u32,
    pub export: ExportMeta,
    pub conversation: ConversationMeta,
}

impl ConversationHeader {
    pub fn from_document(doc: &ConversationDocument) -> Self {
        Self {
            schema_version: doc.schema_version,
            export: doc.export.clone(),
            conversation: doc.conversation.clone(),
        }
    }
}

