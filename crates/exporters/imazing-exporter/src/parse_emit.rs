//! Peer, timezone, and row-classification helpers for the emitter.

use crate::emit::TransportFamily;
use crate::parse::{RawRow, SourceKind};
use anyhow::Result;
use chrono::{FixedOffset, Local, LocalResult, NaiveDateTime, TimeZone};
use contacts::ContactsBook;
use message_csv::parse_utc_offset;
use message_ir::HandleType;
use phone::sanitize_number;
use std::collections::HashSet;

impl TransportFamily {
    pub(super) fn from_kind(kind: SourceKind) -> Self {
        match kind {
            SourceKind::Messages => Self::Messages,
            SourceKind::WhatsApp => Self::WhatsApp,
        }
    }
}

#[derive(Debug)]
pub(super) struct PeerInfo {
    pub(super) chat_id: String,
    pub(super) contact_name: String,
    pub(super) group: bool,
    pub(super) unresolved_chat: bool,
    pub(super) unresolved_roster_labels: u64,
}

pub(super) fn collect_peer_info(
    book: &ContactsBook,
    kind: SourceKind,
    session: &str,
    rows: &[&RawRow],
) -> PeerInfo {
    let mut handles: HashSet<String> = HashSet::new();
    for row in rows {
        let sid = row.sender_id.trim();
        // Email first: a sender like `bob2024@gmail.com` has 4+ digits and
        // must never be reduced to a phone number.
        if sid.contains('@') {
            handles.insert(sid.to_string());
        } else if sanitize_number(sid).is_some() {
            // Format as E.164 (the international phone-number format that starts
            // with +) when unambiguous. Otherwise keep digits as-is. Never invent `+0…`.
            handles.insert(phone::normalize_lenient(sid));
        }
        for phone in phones_in_text(&row.chat_session) {
            handles.insert(phone);
        }
    }

    let mut unresolved_roster_labels = 0u64;
    // Messages group rosters encode members as "A & B & C". Resolve silent members via contacts.
    if kind == SourceKind::Messages && session.contains(" & ") {
        for part in session.split(" & ") {
            let label = part.trim();
            if label.is_empty() {
                continue;
            }
            if label.contains('@') {
                handles.insert(label.to_string());
                continue;
            }
            if sanitize_number(label).is_some() {
                handles.insert(phone::normalize_lenient(label));
                continue;
            }
            if let Some((e164, _)) = book.lookup_handle_by_name(label) {
                handles.insert(e164);
            } else {
                unresolved_roster_labels += 1;
            }
        }
    }

    let mut peer_handles: Vec<String> = handles.into_iter().collect();
    peer_handles.sort();

    let group = match kind {
        SourceKind::Messages => session.contains(" & ") || peer_handles.len() >= 2,
        // WhatsApp has no roster column; multiple distinct senders imply a group.
        SourceKind::WhatsApp => peer_handles.len() >= 2,
    };

    let (chat_id, contact_name, unresolved_chat) =
        resolve_chat_identifier(book, session, &peer_handles, group);
    PeerInfo {
        chat_id,
        contact_name,
        group,
        unresolved_chat,
        unresolved_roster_labels,
    }
}

#[derive(Debug)]
pub(super) enum TzMode {
    Local,
    Fixed(FixedOffset),
}

/// Parse a timezone string into local time or a fixed UTC offset.
pub(super) fn resolve_tz(timezone: Option<&str>) -> Result<TzMode> {
    match timezone.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(TzMode::Local),
        Some(name) => {
            let offset = parse_utc_offset(name).map_err(anyhow::Error::msg)?;
            Ok(TzMode::Fixed(offset))
        }
    }
}

/// Parse an iMazing date string into `(unix_secs, date_ms)`; DST-ambiguous
/// times resolve to the earliest occurrence.
pub(super) fn parse_message_date(raw: &str, tz: &TzMode) -> Option<(i64, String)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M"))
        .ok()?;
    let secs = match tz {
        // Ambiguous (DST fall-back) hours resolve to the earliest instant
        // instead of silently dropping the message.
        TzMode::Local => match Local.from_local_datetime(&naive) {
            LocalResult::Single(dt) => dt.timestamp(),
            LocalResult::Ambiguous(earliest, _latest) => earliest.timestamp(),
            LocalResult::None => return None,
        },
        TzMode::Fixed(offset) => match offset.from_local_datetime(&naive) {
            LocalResult::Single(dt) => dt.timestamp(),
            LocalResult::Ambiguous(earliest, _latest) => earliest.timestamp(),
            LocalResult::None => return None,
        },
    };
    Some((secs, (secs * 1000).to_string()))
}

/// True for rows the exporter treats as sent (`outgoing`/`sent` types).
pub(super) fn is_outgoing(msg_type: &str) -> bool {
    matches!(
        msg_type.trim().to_ascii_lowercase().as_str(),
        "outgoing" | "sent"
    )
}

pub(super) fn is_notification(msg_type: &str) -> bool {
    msg_type.trim().eq_ignore_ascii_case("notification")
}

fn phones_in_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i > start + 1 && sanitize_number(&text[start..i]).is_some() {
                let e164 = phone::normalize_lenient(&text[start..i]);
                if !out.contains(&e164) {
                    out.push(e164);
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Resolve a session into `(chat_identifier, contact_name, unresolved_phone)`.
///
/// The third value is `true` only when the chat id could not be resolved
/// and callers should record the raw phone as unresolved.
fn resolve_chat_identifier(
    book: &ContactsBook,
    session: &str,
    peer_handles: &[String],
    group: bool,
) -> (String, String, bool) {
    if group {
        if !peer_handles.is_empty() {
            let title = session.trim().to_string();
            return (peer_handles.join(","), title, false);
        }
        return (
            message_vault_io_core::name_stem(session),
            session.trim().to_string(),
            true,
        );
    }

    if let Some(handle) = peer_handles.first() {
        let contact_name = if sanitize_number(handle).is_some() {
            // The book keys entries by the shared guarded policy.
            book.lookup_name_by_handle(
                &phone::normalize_typed_handle(handle, HandleType::Phone).0,
                HandleType::Phone,
            )
            .unwrap_or("")
            .to_string()
        } else {
            String::new()
        };
        let contact_name = if contact_name.is_empty() {
            session.trim().to_string()
        } else {
            contact_name
        };
        return (handle.clone(), contact_name, false);
    }

    let session = session.trim();
    if session.is_empty() {
        return ("unknown".to_string(), String::new(), true);
    }
    // Email first: an address like `bob2024@gmail.com` has 4+ digits and must
    // not be treated as a phone number.
    if session.contains('@') {
        return (session.to_string(), String::new(), false);
    }
    if sanitize_number(session).is_some() {
        // Format as E.164 when unambiguous. Otherwise keep digits as-is. Never
        // invent `+0…`. The contacts book keys entries by the same policy.
        let handle = phone::normalize_lenient(session);
        let name = book
            .lookup_name_by_handle(&handle, HandleType::Phone)
            .unwrap_or("")
            .to_string();
        return (handle, name, false);
    }
    if let Some((e164, _)) = book.lookup_handle_by_name(session) {
        return (e164, session.to_string(), false);
    }
    (
        message_vault_io_core::name_stem(session),
        session.to_string(),
        true,
    )
}

pub(super) fn resolve_sender(
    book: &ContactsBook,
    row: &RawRow,
    is_from_me: bool,
    is_notification: bool,
    chat_id: &str,
    contact_name: &str,
) -> (String, String) {
    if is_from_me {
        return (String::new(), String::new());
    }
    if is_notification {
        // Keep any available identity from the notification row; often empty.
        // Email first: an address like `bob2024@gmail.com` has 4+ digits and
        // must not be reduced to a phone number.
        let handle = if row.sender_id.contains('@') {
            row.sender_id.trim().to_string()
        } else if sanitize_number(&row.sender_id).is_some() {
            phone::normalize_lenient(&row.sender_id)
        } else {
            String::new()
        };
        return (handle, row.sender_name.trim().to_string());
    }

    let mut handle = String::new();
    if row.sender_id.contains('@') {
        handle = row.sender_id.trim().to_string();
    } else if sanitize_number(&row.sender_id).is_some() {
        // Format as E.164 when unambiguous. Otherwise keep digits as-is. Never invent `+0…`.
        handle = phone::normalize_lenient(&row.sender_id);
    } else if !chat_id.contains('@')
        && (chat_id.starts_with('+') || sanitize_number(chat_id).is_some())
    {
        handle = phone::normalize_lenient(chat_id);
    } else if !row.sender_name.is_empty()
        && let Some((e164, _)) = book.lookup_handle_by_name(&row.sender_name)
    {
        handle = e164;
    }

    let mut display = row.sender_name.trim().to_string();
    if display.is_empty() && sanitize_number(&handle).is_some() {
        // The book keys entries by the shared guarded policy.
        display = book
            .lookup_name_by_handle(
                &phone::normalize_typed_handle(&handle, HandleType::Phone).0,
                HandleType::Phone,
            )
            .unwrap_or("")
            .to_string();
    }
    if display.is_empty() && !contact_name.is_empty() {
        display = contact_name.to_string();
    }

    (handle, display)
}
