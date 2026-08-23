//! Shared CSV helpers for writing conversation files.

#![warn(missing_docs)]

mod date_range;
mod utc_offset;

pub use date_range::DateRange;
pub use utc_offset::parse_utc_offset;

use chrono::{Local, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One attachment object written into `attachments_json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct AttachmentCell {
    /// Shared attachment metadata (serialized inline — same JSON shape as before).
    #[serde(flatten)]
    pub meta: message_ir::AttachmentMeta,
    /// Sticker flag.
    #[serde(default)]
    pub is_sticker: bool,
    /// Transcribed text of the attachment (e.g., OCR of an image or a
    /// voice-note transcript).
    pub transcription: Option<String>,
    /// iMessage sticker effect name.
    pub sticker_effect: Option<String>,
}

impl From<AttachmentCell> for message_ir::IrAttachment {
    fn from(cell: AttachmentCell) -> Self {
        let AttachmentCell {
            meta,
            is_sticker,
            transcription,
            sticker_effect,
        } = cell;
        Self {
            path: meta.path,
            original_name: meta.original_name,
            mime_type: meta.mime_type,
            digest_sha256: meta.digest_sha256,
            is_sticker,
            transcription,
            sticker_effect,
            size_bytes: None,
            missing_reason: None,
            bytes: None,
        }
    }
}

/// Format a Unix second as local / UTC / display strings.
///
/// Returns `None` when the timestamp cannot be represented in local or UTC.
pub fn format_local_ts(secs: i64) -> Option<(String, String, String)> {
    let local = Local.timestamp_opt(secs, 0).single().or_else(|| {
        Utc.timestamp_opt(secs, 0)
            .single()
            .map(|utc| Local.from_utc_datetime(&utc.naive_utc()))
    })?;
    let utc = local.with_timezone(&Utc);
    let display = local.format("%b %e, %Y %I:%M:%S %p").to_string();
    Some((
        local.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        display,
    ))
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

/// Serialize a value for a CSV JSON cell (`null` on failure).
pub fn json_cell(value: &impl Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// Standard per-conversation CSV filename (defined in `message-ir`, where the
/// IR's `filename_stem` shares it; re-exported here for existing callers).
pub use message_ir::conversation_filename;

#[cfg(test)]
mod tests {
    use super::conversation_filename;

    #[test]
    fn individual_uses_chat_id() {
        assert_eq!(
            conversation_filename("individual", "+15551212", None, &[], None),
            "+15551212.csv"
        );
    }

    #[test]
    fn group_with_title_uses_title() {
        assert_eq!(
            conversation_filename("group", "chat-x", Some("Family Chat"), &[], None),
            "Family_Chat.csv"
        );
    }

    #[test]
    fn untitled_group_lists_sorted_phones() {
        let peers = vec!["+18285532527".into(), "+14073109632".into()];
        assert_eq!(
            conversation_filename("group", "chat-group-x", None, &peers, None),
            "group_+14073109632_+18285532527.csv"
        );
    }

    #[test]
    fn untitled_group_over_ten_appends_hash() {
        let peers: Vec<String> = (1..=13).map(|i| format!("+1555555{:04}", i)).collect();
        let name = conversation_filename("group", "chat-x", None, &peers, None);
        let stem = name.strip_suffix(".csv").unwrap();
        assert!(stem.starts_with("group_+15555550001_"));
        assert!(stem.contains("+15555550010_"));
        assert!(!stem.contains("+15555550011"));
        let hash = stem.rsplit('_').next().unwrap();
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(
            name,
            conversation_filename("group", "other-id", None, &peers, None)
        );
    }

    #[test]
    fn whatsapp_suffix() {
        let peers = vec!["+15555550100".into()];
        assert_eq!(
            conversation_filename("group", "x", None, &peers, Some("__whatsapp")),
            "group_+15555550100__whatsapp.csv"
        );
    }

    #[test]
    fn none_title_uses_phones_not_synthetic() {
        let peers = vec!["+15555550100".into()];
        assert_eq!(
            conversation_filename("group", "chat-group-x", None, &peers, None),
            "group_+15555550100.csv"
        );
    }
}
