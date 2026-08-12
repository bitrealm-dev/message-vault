//! Shared handle identity helpers (normalize + infer type from shape).

use message_ir::HandleType;

/// Canonical form of a handle for identity matching, per type, plus a
/// human-readable note when the canonical form is ambiguous (guarded policy).
///
/// Phone: E.164 when the raw is unambiguous (`+`-prefixed, or a US national
/// number); otherwise digits-as-is with a review note — a trunk-zero
/// `020 7946 0000` becomes `02079460000` flagged, never `+02079460000`.
/// Email: lowercased. Username/Other: verbatim (trimmed).
pub fn normalize_handle(raw: &str, handle_type: HandleType) -> (String, Option<String>) {
    match handle_type {
        HandleType::Phone => {
            let guarded = phone::normalize_guarded(raw, phone::PhoneRegion::for_raw(raw));
            if guarded.normalized.is_empty() {
                // No usable digits: fall back to the raw, unflagged.
                (raw.trim().to_string(), None)
            } else {
                (guarded.normalized, guarded.note)
            }
        }
        HandleType::Email => (raw.trim().to_lowercase(), None),
        HandleType::Username | HandleType::Other => (raw.trim().to_string(), None),
    }
}

/// Infer a handle type from the handle's shape when the source does not say.
///
/// Mirrors the shared rule in message-ir-format: `@` → Email; digit-heavy
/// phone-shaped strings → Phone (covers SMS/iMessage/WhatsApp numbers);
/// anything else (Discord usernames, group chat ids) → Other.
pub fn infer_handle_type_from_shape(handle: &str) -> HandleType {
    let h = handle.trim();
    if h.contains('@') {
        return HandleType::Email;
    }
    let has_digit = h.bytes().any(|b| b.is_ascii_digit());
    let all_phone_chars = h.bytes().all(|b| {
        b.is_ascii_digit() || matches!(b, b'+' | b'-' | b' ' | b'(' | b')' | b'.' | b'#' | b'*')
    });
    if !h.is_empty() && has_digit && all_phone_chars {
        return HandleType::Phone;
    }
    HandleType::Other
}
