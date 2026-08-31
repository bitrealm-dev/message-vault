//! Chat-id helpers: E.164-guarded phone formatting and group chat ids.

use message_ir::HandleType;
use phone::OwnerHandleSet;
use sha2::{Digest, Sha256};

/// Format as E.164 (the international phone-number format that starts with +)
/// when the digits are unambiguous for the US-centric crate. Otherwise keep
/// the digits as-is. Never invent `+0…`.
pub(super) fn guarded_phone(digits: &str) -> String {
    phone::normalize_digits_us(digits).unwrap_or_default()
}

/// Format digit strings as E.164 (the international phone-number format that
/// starts with +) when unambiguous, then join with `", "`.
fn join_guarded_phones(digits: &[String]) -> String {
    digits
        .iter()
        .map(|d| guarded_phone(d))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Length-prefix each number so `["12","34"]` and `["123","4"]` cannot both
/// become `12_34`.
fn group_id_slug(digits: &[String]) -> String {
    digits
        .iter()
        .map(|d| format!("{}:{}", d.len(), d))
        .collect::<Vec<_>>()
        .join("_")
}

/// Chat id for a 1:1 conversation: E.164 when unambiguous.
pub(super) fn chat_id_individual(digits: &str) -> String {
    guarded_phone(digits)
}

/// Group chat id and display title from participant digits (owner excluded).
pub(super) fn chat_id_group(
    participant_digits: &[String],
    owners: &OwnerHandleSet,
) -> (String, String) {
    let mut others: Vec<String> = participant_digits
        .iter()
        .filter(|d| !d.is_empty() && !owners.is_owner(d, HandleType::Phone))
        .cloned()
        .collect();
    others.sort();
    others.dedup();
    let title = if others.is_empty() {
        "Group".to_string()
    } else if others.len() <= 4 {
        format!("Group: {}", join_guarded_phones(&others))
    } else {
        format!(
            "Group: {}, and {} others",
            join_guarded_phones(&others[..4]),
            others.len() - 4
        )
    };
    let slug = group_id_slug(&others);
    let id = if slug.is_empty() {
        "chat-group-unknown".to_string()
    } else {
        format!("chat-group-{slug}")
    };
    // Keep filesystem-safe length.
    let id = if id.len() > 180 {
        let digest = hex::encode(Sha256::digest(id.as_bytes()));
        format!("chat-group-{}", &digest[..16])
    } else {
        id
    };
    (id, title)
}
