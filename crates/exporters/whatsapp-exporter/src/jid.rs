//! WhatsApp JID helpers.
//!
//! A JID is WhatsApp's address for a user or group (for example
//! `15551234567@s.whatsapp.net` or `120363042@g.us`).

use phone::{PhoneRegion, normalize_guarded, sanitize_number};

/// True for `@g.us` group JIDs (WhatsApp's group address suffix).
pub(crate) fn is_group_jid(jid: &str) -> bool {
    jid.trim().ends_with("@g.us")
}

/// Map a user JID / phone-like sender to E.164 (the international phone-number
/// format that starts with +) when possible.
///
/// - `15551234567@s.whatsapp.net` → `+15551234567`
/// - `447911123456@s.whatsapp.net` → `+447911123456` (country-code locals)
/// - bare digits / `+E164` → E.164 when unambiguous
/// - trunk-zero locals stay without a fabricated `+0…`
/// - otherwise `None`
pub(crate) fn jid_to_e164(jid: &str) -> Option<String> {
    let jid = jid.trim();
    if jid.is_empty() {
        return None;
    }
    let local = jid.split('@').next().unwrap_or(jid);
    if local.is_empty() || local.eq_ignore_ascii_case("status") {
        return None;
    }
    // Linked-device / LID ids are not phone numbers.
    if jid.contains("@lid") {
        return None;
    }
    let digits = sanitize_number(local)?;
    // Prefer the US form when it produces a real E.164 (`+…`).
    let usa = normalize_guarded(&digits, PhoneRegion::Usa).normalized;
    if usa.starts_with('+') {
        return Some(usa);
    }
    // WhatsApp JID locals are country-code–prefixed digit strings. When the US
    // guard leaves bare digits, keep a leading `+` for international lengths
    // that are not trunk-zero.
    if (8..=15).contains(&digits.len()) && !digits.starts_with('0') {
        return Some(format!("+{digits}"));
    }
    Some(usa)
}

/// Chat identifier for CSV: E.164 for 1:1 user JIDs; otherwise the raw JID.
pub(crate) fn chat_id_from_jid(jid: &str) -> String {
    if is_group_jid(jid) {
        jid.trim().to_string()
    } else {
        jid_to_e164(jid).unwrap_or_else(|| jid.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_jid_to_e164() {
        assert_eq!(
            jid_to_e164("15555550122@s.whatsapp.net").as_deref(),
            Some("+15555550122")
        );
        assert_eq!(jid_to_e164("+15555550122").as_deref(), Some("+15555550122"));
    }

    #[test]
    fn international_jid_gets_plus() {
        assert_eq!(
            jid_to_e164("447911123456@s.whatsapp.net").as_deref(),
            Some("+447911123456")
        );
    }

    #[test]
    fn trunk_zero_not_fabricated_into_plus_zero() {
        let out = jid_to_e164("02079460000@s.whatsapp.net");
        assert!(
            out.as_deref().is_none_or(|s| !s.starts_with("+0")),
            "unexpected {out:?}"
        );
    }

    #[test]
    fn group_jid_detection() {
        assert!(is_group_jid("120363042@g.us"));
        assert!(!is_group_jid("15555550122@s.whatsapp.net"));
    }

    #[test]
    fn chat_id_keeps_group_jid() {
        assert_eq!(chat_id_from_jid("120363042@g.us"), "120363042@g.us");
        assert_eq!(
            chat_id_from_jid("15555550122@s.whatsapp.net"),
            "+15555550122"
        );
    }
}
