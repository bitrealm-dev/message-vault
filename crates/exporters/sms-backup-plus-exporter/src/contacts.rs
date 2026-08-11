//! Apply shared contact book / name mapping to SMS Backup+ messages.

use crate::types::ParsedMessage;
use contacts::{ContactsBook, NameMapping};
use message_ir::HandleType;

/// Fill empty peer phone from the contacts book using current `name_alias`.
///
/// Returns `Some((display_name, phone))` when a phone fill happened.
/// Call [`apply_name_mapping`] first so incorrect names resolve to a phone.
pub(crate) fn fill_unknown_phone(
    msg: &mut ParsedMessage,
    book: &ContactsBook,
) -> Option<(String, String)> {
    if !msg.chat_key.is_empty() {
        return None;
    }
    let display = msg
        .name_alias
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let (phone, _) = book.lookup_handle_by_name(&display)?;
    msg.chat_key = phone.clone();
    if !msg.is_from_me {
        msg.sender_digits = Some(phone.clone());
    }
    msg.participant_digits = vec![(phone.clone(), Some(display.clone()))];
    Some((display, phone))
}

/// When the EML name appears as `Incorrect Name`, set the peer phone from the mapping.
///
/// Clears a non-book display name so [`enrich_display_names`] can fill from contacts.
/// Returns `Some((incorrect_name, phone_digits))` when a phone was applied.
pub(crate) fn apply_name_mapping(
    msg: &mut ParsedMessage,
    mapping: &NameMapping,
    book: &ContactsBook,
) -> Option<(String, String)> {
    if !msg.chat_key.is_empty() {
        return None;
    }
    let raw = msg
        .name_alias
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let (phone, handle_type) = mapping.handle_for_incorrect_name(&raw)?.clone();
    let display = book
        .lookup_name_by_handle(&phone, handle_type)
        .map(str::to_string);
    msg.chat_key = phone.clone();
    if !msg.is_from_me {
        msg.sender_digits = Some(phone.clone());
    }
    // Prefer book name now; otherwise blank so enrich_display_names can fill.
    msg.name_alias = Some(display.clone().unwrap_or_default());
    msg.participant_digits = vec![(phone.clone(), display)];
    Some((raw, phone))
}

/// Fill blank/unknown display names from phone→name when the peer phone is known.
pub(crate) fn enrich_display_names(msg: &mut ParsedMessage, book: &ContactsBook) {
    if let Some(ref digits) = msg.sender_digits {
        if let Some(name) = book.enrich_display_name(
            digits,
            HandleType::Phone,
            msg.name_alias.as_deref().unwrap_or(""),
        ) {
            msg.name_alias = Some(name);
        }
    }
    if !msg.chat_key.is_empty() {
        if let Some(name) = book.enrich_display_name(
            &msg.chat_key,
            HandleType::Phone,
            msg.name_alias.as_deref().unwrap_or(""),
        ) {
            msg.name_alias = Some(name);
        }
    }
    for (digits, name) in &mut msg.participant_digits {
        let current = name.as_deref().unwrap_or("");
        if let Some(resolved) = book.enrich_display_name(digits, HandleType::Phone, current) {
            *name = Some(resolved);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contacts::ContactsBook;
    use std::io::Write;
    use std::path::PathBuf;

    fn write_csv(dir: &tempfile::TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "{body}").unwrap();
        path
    }

    #[test]
    fn name_mapping_sets_phone_then_enrich_name() {
        let dir = tempfile::tempdir().unwrap();
        let contacts = write_csv(
            &dir,
            "contacts.csv",
            "First Name,Last Name,Mobile Phone\n\
Jordan,Alias,15555550144\n",
        );
        let mapping_path = write_csv(
            &dir,
            "mapping.csv",
            "Phone,Incorrect Name\n\
+15555550144,Jordan Alias (SKIP)\n",
        );
        let book = ContactsBook::load_vcard_csv(&contacts).unwrap();
        let mapping = NameMapping::load(&mapping_path).unwrap();

        let mut msg = ParsedMessage {
            chat_key: String::new(),
            conversation_type: "individual".into(),
            group_title: None,
            participant_digits: vec![],
            timestamp_secs: 1.0,
            is_from_me: false,
            sender_digits: None,
            text: "hi".into(),
            attachments: vec![],
            name_alias: Some("Jordan Alias (SKIP)".into()),
            smssync_id: None,
            source_kind: "flat".into(),
            android_type: String::new(),
            eml_path: String::new(),
        };
        let mapped = apply_name_mapping(&mut msg, &mapping, &book).unwrap();
        assert_eq!(mapped.1, "+15555550144");
        assert_eq!(msg.chat_key, "+15555550144");
        assert_eq!(msg.name_alias.as_deref(), Some("Jordan Alias"));
    }
}
