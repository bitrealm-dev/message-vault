//! WhatsApp JID helpers.

use phone::{PhoneRegion, normalize_guarded, sanitize_number};

/// True for `@g.us` group JIDs.
pub(crate) fn is_group_jid(jid: &str) -> bool {
    jid.trim().ends_with("@g.us")
}

/// Map a user JID / phone-like sender to E.164 when possible.
///
/// - `15551234567@s.whatsapp.net` → `+15551234567`
/// - bare digits / `+E164` → guarded normalization (E.164 when unambiguous)
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
    // JID locals are bare digits (no `+`), so the US region applies; guarded
    // so a non-NANP digit string is never fabricated into `+0…`.
    sanitize_number(local).map(|digits| normalize_guarded(&digits, PhoneRegion::Usa).normalized)
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
