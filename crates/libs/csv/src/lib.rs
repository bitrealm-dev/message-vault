//! Shared CSV helpers for writing conversation files.

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
    pub path: Option<String>,
    pub original_name: Option<String>,
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest_sha256: Option<String>,
    #[serde(default)]
    pub is_sticker: bool,
    pub transcription: Option<String>,
    pub sticker_effect: Option<String>,
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

/// Max peer phones included in an untitled group filename stem.
const GROUP_FILENAME_MAX_PHONES: usize = 10;

fn sanitize_stem(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '+' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn is_phone_handle(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    if let Some(rest) = value.strip_prefix('+') {
        !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
    } else {
        value.chars().all(|c| c.is_ascii_digit())
    }
}

fn with_suffix(stem: &str, suffix: Option<&str>) -> String {
    match suffix {
        Some(s) if !s.is_empty() => format!("{stem}{s}.csv"),
        _ => format!("{stem}.csv"),
    }
}

/// Standard per-conversation CSV filename.
///
/// - Individual → `safe_filename(chat_id)` (+ optional suffix)
/// - Group with a real `group_title` → sanitized title
/// - Untitled group → `group_+A_+B_…` (sorted unique E.164, max 10);
///   if more than 10 peers, append `_<16 hex>` of SHA-256 over the full roster
/// - Untitled group with empty roster → `group_unknown` (or hash of `chat_id`)
pub fn conversation_filename(
    conversation_type: &str,
    chat_id: &str,
    group_title: Option<&str>,
    participant_e164s: &[String],
    suffix: Option<&str>,
) -> String {
    let is_group = conversation_type.eq_ignore_ascii_case("group");
    if !is_group {
        let stem = sanitize_stem(chat_id);
        return with_suffix(&stem, suffix);
    }

    if let Some(title) = group_title.map(str::trim).filter(|t| !t.is_empty()) {
        let stem = sanitize_stem(title);
        if !stem.is_empty() && !stem.chars().all(|c| c == '_') {
            return with_suffix(&stem, suffix);
        }
    }

    let phones = unique_sorted_phone_handles(participant_e164s);

    if phones.is_empty() {
        let stem = if chat_id.trim().is_empty() {
            "group_unknown".to_string()
        } else {
            let digest = hex::encode(Sha256::digest(chat_id.as_bytes()));
            format!("group_{}", &digest[..16])
        };
        return with_suffix(&stem, suffix);
    }

    let mut stem = String::from("group");
    for phone in phones.iter().take(GROUP_FILENAME_MAX_PHONES) {
        stem.push('_');
        stem.push_str(phone);
    }
    if phones.len() > GROUP_FILENAME_MAX_PHONES {
        let joined = phones.join("|");
        let digest = hex::encode(Sha256::digest(joined.as_bytes()));
        stem.push('_');
        stem.push_str(&digest[..16]);
    }
    with_suffix(&stem, suffix)
}

/// Trim, keep phone-looking handles, sort, and drop duplicates.
fn unique_sorted_phone_handles(participant_e164s: &[String]) -> Vec<String> {
    let mut phones: Vec<String> = participant_e164s
        .iter()
        .map(|p| p.trim().to_string())
        .filter(|p| is_phone_handle(p))
        .collect();
    phones.sort();
    phones.dedup();
    phones
}

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
