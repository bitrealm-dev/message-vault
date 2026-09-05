//! Stream every row of `chat.db` to the app as protocol events.
//!
//! Each row becomes one [`MessageRecord`], already classified (tapback,
//! announcement, balloon, plain text) with its Apple-specific fields filled
//! in. The first row of each conversation is preceded by a
//! [`ConversationRecord`] carrying the roster. Nothing is written to disk
//! here; the app owns the output.

use std::collections::HashSet;

use imessage_database::{
    message_types::variants::{Announcement, Tapback, TapbackAction, Variant},
    tables::{
        chat::Chat,
        messages::{
            Message,
            models::{GroupAction, Service},
        },
        table::{ME, ORPHANED, Table, YOU},
    },
    util::dates::TIMESTAMP_FACTOR,
};
use imessage_reader_protocol::{
    Conversation as ConversationRecord, Event, Imessage as ImessageRecord,
    Message as MessageRecord, Participant, Progress,
};
use serde_json::Value;

use crate::{
    attachments_emit::collect_parts_and_attachments,
    body::apply_body,
    error::RuntimeError,
    fields::{
        TapbackCell, balloon_kind_label, balloon_summary, build_balloon_value, build_edit_records,
        expressive_label, parse_thread_part, shared_location_label,
    },
    log::emit,
    session::MailSession,
};

/// Report often enough that long attachment decrypts between ticks do not
/// look frozen on large backups.
const MESSAGE_PROGRESS_EVERY: u64 = 1_000;

/// Poll votes and updates: noise that CSV and HTML export skip too.
fn is_poll_noise(message: &Message) -> bool {
    message.is_poll_vote() || message.is_poll_update()
}

/// Stream every row of `chat.db` as events. A row that fails to convert is
/// logged and counted, not fatal.
///
/// # Errors
///
/// Returns an error when the database cannot be read.
pub(crate) fn stream_export(session: &MailSession) -> Result<(), RuntimeError> {
    let mut announced: HashSet<String> = HashSet::new();
    let mut last_row = -1;
    let mut seen = 0u64;
    let mut failures = 0u64;
    let total = Message::get_count(session.data_source.db(), &session.options.query_context)?;
    let mut statement =
        Message::stream_rows(session.data_source.db(), &session.options.query_context)?;

    for message in Message::rows(&mut statement, [])? {
        let mut msg = message?;
        seen += 1;
        // The stream repeats a row once per attachment join; keep the first.
        if msg.rowid == last_row {
            continue;
        }
        last_row = msg.rowid;
        if !msg.is_edited() && is_poll_noise(&msg) {
            continue;
        }
        apply_body(&mut msg, session.data_source.db());
        if is_poll_noise(&msg) {
            continue;
        }

        match build_record(session, &msg) {
            Ok((conversation, record)) => {
                if announced.insert(conversation.chat_identifier.clone()) {
                    emit(&Event::Conversation(conversation));
                }
                emit(&Event::Message(Box::new(record)));
            }
            Err(why) => {
                failures += 1;
                session.options.emit_log(format!(
                    "Skipping message (rowid={}, guid={}): {}",
                    msg.rowid, msg.guid, why
                ));
            }
        }
        if seen.is_multiple_of(MESSAGE_PROGRESS_EVERY) {
            session.options.emit_log(format!("  …{seen}/{total}"));
            emit(&Event::Progress(Progress::Parse {
                done: seen,
                total: u64::try_from(total).unwrap_or(u64::MAX),
            }));
        }
    }

    if failures > 0 {
        session.options.emit_log(format!(
            "{failures} messages skipped due to formatting errors."
        ));
    }
    emit(&Event::ExportDone {
        messages_seen: seen,
        failures,
    });
    Ok(())
}

/// Message time as milliseconds since 1970-01-01 UTC.
fn timestamp_unix_ms(message: &Message, offset: i64) -> i64 {
    if let Ok(dt) = message.date(offset) {
        return dt.timestamp_millis();
    }
    let stamp = message.date;
    let seconds_since_2001 = if stamp >= 1_000_000_000_000 {
        stamp / TIMESTAMP_FACTOR
    } else {
        stamp
    };
    (seconds_since_2001 + offset).saturating_mul(1000)
}

/// Raw handle string for a Messages `handle_id`, if the participant is known.
fn raw_handle(session: &MailSession, handle_id: i32) -> Option<String> {
    session
        .resolve_participant(handle_id)
        .map(|name| name.details.clone())
}

/// Contact display name for a Messages `handle_id`, falling back to the handle.
fn display_name_for(session: &MailSession, handle_id: i32) -> Option<String> {
    session.resolve_participant(handle_id).map(|name| {
        if name.full.is_empty() {
            name.details.clone()
        } else {
            name.full.clone()
        }
    })
}

/// Participants and conversation type (`individual` / `group`) for one chat room.
fn participants_for(session: &MailSession, chatroom: &Chat) -> (Vec<Participant>, &'static str) {
    let mut records = Vec::new();
    // Only non-empty handles are written, so only count those. A raw handle
    // row count over-counts empty handles and misclassifies the chat.
    let mut count = 0;
    if let Some(handles) = session.chatroom_participants.get(&chatroom.rowid) {
        for handle_id in handles {
            let name = session.resolve_participant(*handle_id);
            let (handle, display_name) = match name {
                Some(n) => (
                    n.details.clone(),
                    if n.full.is_empty() {
                        None
                    } else {
                        Some(n.full.clone())
                    },
                ),
                None => (String::new(), None),
            };
            if !handle.is_empty() {
                records.push(Participant {
                    handle,
                    display_name,
                });
                count += 1;
            }
        }
    }
    // A user-named chat is a group even when it has shrunk to two members.
    let named = chatroom.display_name().is_some();
    let conversation_type = if count > 1 || named {
        "group"
    } else {
        "individual"
    };
    (records, conversation_type)
}

/// Human-readable text for a group announcement (rename, add, leave, and similar).
fn announcement_text(session: &MailSession, msg: &Message) -> Option<String> {
    let announcement = msg.get_announcement()?;
    let mut who = session.who(
        msg.handle_id,
        msg.is_from_me(),
        msg.destination_caller_id.as_deref(),
    );
    if who == ME {
        who = YOU;
    }
    let participant_name = match &announcement {
        Announcement::GroupAction(
            GroupAction::ParticipantAdded(handle) | GroupAction::ParticipantRemoved(handle),
        ) => session.who(Some(*handle), false, msg.destination_caller_id.as_deref()),
        _ => "someone",
    };

    let body = match &announcement {
        Announcement::AudioMessageKept => "kept an audio message.".to_string(),
        Announcement::FullyUnsent => "unsent a message!".to_string(),
        Announcement::Unknown(num) => format!("performed unknown action {num}."),
        Announcement::GroupAction(group) => match group {
            GroupAction::ParticipantAdded(_) => {
                format!("added {participant_name} to the conversation.")
            }
            GroupAction::ParticipantRemoved(_) => {
                format!("removed {participant_name} from the conversation.")
            }
            GroupAction::NameChange(name) => format!("named the conversation {name}"),
            GroupAction::ParticipantLeft => "left the conversation.".to_string(),
            GroupAction::GroupIconChanged => "changed the group photo.".to_string(),
            GroupAction::GroupIconRemoved => "removed the group photo.".to_string(),
            GroupAction::ChatBackgroundChanged => "changed the chat background.".to_string(),
            GroupAction::ChatBackgroundRemoved => "removed the chat background.".to_string(),
            GroupAction::PhoneNumberChanged(_) => "changed their phone number.".to_string(),
        },
    };
    Some(format!("{who} {body}"))
}

/// Owner display name from the destination caller id, or `Me`, when that option is on.
fn owner_display_name(session: &MailSession, message: &Message) -> Option<String> {
    if session.options.use_caller_id {
        message
            .destination_caller_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| Some(ME.to_string()))
    } else {
        None
    }
}

/// One-line description of a tapback (Loved, Liked, Removed Heart, and similar).
fn tapback_human_line(kind: &str, emoji: Option<&str>, action: &str) -> String {
    if action == "remove" {
        return match kind {
            "loved" => "Removed Heart".into(),
            "liked" => "Removed Like".into(),
            "disliked" => "Removed Dislike".into(),
            "laughed" => "Removed Laugh".into(),
            "emphasized" => "Removed Exclamation".into(),
            "questioned" => "Removed Question Mark".into(),
            "emoji" => format!("Removed {}", emoji.unwrap_or("emoji")),
            "sticker" => "Removed Sticker".into(),
            other => format!("Removed {other}"),
        };
    }
    match kind {
        "loved" => "Loved a message".into(),
        "liked" => "Liked a message".into(),
        "disliked" => "Disliked a message".into(),
        "laughed" => "Laughed at a message".into(),
        "emphasized" => "Emphasized a message".into(),
        "questioned" => "Questioned a message".into(),
        "emoji" => format!("{} reacted", emoji.unwrap_or("Emoji")),
        "sticker" => "Reacted with a sticker".into(),
        other => format!("{other} reaction"),
    }
}

/// The name of a reaction and, for an emoji reaction, the emoji itself.
fn tapback_kind(kind: Tapback<'_>) -> (&'static str, Option<String>) {
    match kind {
        Tapback::Loved => ("loved", None),
        Tapback::Liked => ("liked", None),
        Tapback::Disliked => ("disliked", None),
        Tapback::Laughed => ("laughed", None),
        Tapback::Emphasized => ("emphasized", None),
        Tapback::Questioned => ("questioned", None),
        Tapback::Emoji(e) => ("emoji", e.map(str::to_string)),
        Tapback::Sticker => ("sticker", None),
    }
}

/// JSON array of tapbacks on this message, if any exist.
fn build_parent_tapbacks(session: &MailSession, message: &Message) -> Option<Value> {
    let parts = session.tapbacks.get(&message.guid)?;
    let mut sortable: Vec<(usize, i64, i32, TapbackCell)> = Vec::new();
    for (&part_index, tapbacks) in parts {
        for tapback in tapbacks {
            let Variant::Tapback(_, action, kind) = tapback.variant() else {
                continue;
            };
            if matches!(action, TapbackAction::Removed) {
                continue;
            }
            let (kind, emoji) = tapback_kind(kind);
            let (reactor_handle, reactor_display_name) = if tapback.is_from_me() {
                (
                    None,
                    Some(owner_display_name(session, tapback).unwrap_or_else(|| ME.to_string())),
                )
            } else if let Some(handle_id) = tapback.handle_id {
                (
                    raw_handle(session, handle_id),
                    display_name_for(session, handle_id),
                )
            } else {
                (None, None)
            };
            sortable.push((
                part_index,
                tapback.date,
                tapback.rowid,
                TapbackCell {
                    part_index,
                    kind,
                    emoji,
                    reactor_handle,
                    reactor_display_name,
                },
            ));
        }
    }
    if sortable.is_empty() {
        return None;
    }
    sortable.sort_by_key(|(part, date, rowid, _)| (*part, *date, *rowid));
    let cells: Vec<_> = sortable.into_iter().map(|(_, _, _, c)| c).collect();
    serde_json::to_value(&cells).ok()
}

/// Chat id, roster and sender fields for one row.
struct RowContext {
    conversation: ConversationRecord,
    is_from_me: bool,
    sender_handle: Option<String>,
    sender_display_name: Option<String>,
    service: String,
}

/// Resolve the conversation and sender a row belongs to. A row whose chat is
/// gone lands in the `orphaned` conversation.
fn resolve_context(session: &MailSession, message: &Message) -> RowContext {
    let conversation = match session.conversation(message) {
        Some((chatroom, _)) => {
            let (participants, conversation_type) = participants_for(session, chatroom);
            ConversationRecord {
                chat_identifier: if chatroom.chat_identifier.is_empty() {
                    ORPHANED.to_string()
                } else {
                    chatroom.chat_identifier.clone()
                },
                conversation_type: conversation_type.to_string(),
                group_title: chatroom
                    .display_name()
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(str::to_string),
                participants,
            }
        }
        None => ConversationRecord {
            chat_identifier: ORPHANED.to_string(),
            conversation_type: "individual".to_string(),
            group_title: None,
            participants: Vec::new(),
        },
    };

    let is_from_me = message.is_from_me();
    let (sender_handle, sender_display_name) = if is_from_me {
        (None, None)
    } else if let Some(handle_id) = message.handle_id {
        (
            raw_handle(session, handle_id),
            display_name_for(session, handle_id),
        )
    } else {
        (None, None)
    };

    let service = match message.service() {
        Service::Unknown => String::new(),
        other => other.to_string(),
    };

    RowContext {
        conversation,
        is_from_me,
        sender_handle,
        sender_display_name,
        service,
    }
}

/// Build the conversation and message records for one row.
///
/// # Errors
///
/// Returns an error when body parts or attachments cannot be loaded.
fn build_record(
    session: &MailSession,
    message: &Message,
) -> Result<(ConversationRecord, MessageRecord), RuntimeError> {
    let context = resolve_context(session, message);
    let (parts, attachments) = collect_parts_and_attachments(session, message)?;
    let mut row = classify_row(session, message, &context.service, !attachments.is_empty());
    let kind = row.kind;
    let text = std::mem::take(&mut row.text);
    let imessage = imessage_fields(session, message, row, &parts);

    let record = MessageRecord {
        chat_identifier: context.conversation.chat_identifier.clone(),
        guid: message.guid.clone(),
        timestamp_unix_ms: timestamp_unix_ms(message, session.offset),
        outgoing: context.is_from_me,
        service: context.service,
        message_kind: kind.to_string(),
        sender_handle: context.sender_handle,
        sender_display_name: context.sender_display_name,
        subject: message.subject.clone().filter(|s| !s.is_empty()),
        text,
        owner_handle: message.destination_caller_id.clone().unwrap_or_default(),
        owner_display_name: owner_display_name(session, message),
        imessage: (!is_empty(&imessage)).then_some(imessage),
        attachments,
    };
    Ok((context.conversation, record))
}

/// Which of the message kinds a row is, the text that stands for it, and the
/// values that decided it (which the Apple-specific fields report too).
struct RowKind {
    kind: &'static str,
    text: String,
    /// The announcement's text, for announcement rows.
    announcement: Option<String>,
    /// The shared-location label, for location rows.
    shared_location: Option<String>,
    /// The send effect's label, already appended to `text`.
    send_effect: Option<String>,
    /// The app balloon's payload, for balloon rows.
    app: Option<Value>,
    tapback: Option<TapbackFields>,
}

/// A tapback row: which reaction, added or removed, on which message part.
struct TapbackFields {
    kind: &'static str,
    /// The emoji, for the `emoji` kind.
    emoji: Option<String>,
    action: &'static str,
    /// Guid of the message reacted to.
    associated_guid: Option<String>,
    /// Part index within that message.
    associated_part: Option<u32>,
}

impl TapbackFields {
    /// Read the reaction out of a row's [`Variant::Tapback`].
    fn from_variant(message: &Message, action: TapbackAction, kind: Tapback<'_>) -> Self {
        let (kind, emoji) = tapback_kind(kind);
        let action = match action {
            TapbackAction::Added => "add",
            TapbackAction::Removed => "remove",
        };
        let target = message.clean_associated_guid();
        Self {
            kind,
            emoji,
            action,
            associated_guid: target.map(|(_, guid)| guid.to_string()),
            associated_part: target.map(|(part, _)| part as u32),
        }
    }

    /// The message kind: stickers sent as reactions are their own kind.
    fn message_kind(&self) -> &'static str {
        if self.kind == "sticker" {
            "sticker_tapback"
        } else {
            "tapback"
        }
    }

    /// The human-readable line that stands in for the reaction's text.
    fn text(&self) -> String {
        tapback_human_line(self.kind, self.emoji.as_deref(), self.action)
    }
}

/// Decide a row's kind and text. The order matters: a tapback, SharePlay
/// end, or announcement wins over its (usually empty) text; a shared
/// location or app balloon over a plain text; and a plain text is iMessage,
/// MMS, or SMS by its service and whether it carries attachments.
fn classify_row(
    session: &MailSession,
    message: &Message,
    service: &str,
    has_attachments: bool,
) -> RowKind {
    let shared_location = message
        .shared_location_kind()
        .map(shared_location_label)
        .map(str::to_string);
    let app = build_balloon_value(session.data_source.db(), message);
    let plain = || message.text.clone().unwrap_or_default();

    let (kind, text, announcement, tapback) =
        if let Variant::Tapback(_, action, kind) = message.variant() {
            let tapback = TapbackFields::from_variant(message, action, kind);
            (tapback.message_kind(), tapback.text(), None, Some(tapback))
        } else if message.is_shareplay() {
            let text = "SharePlay Message Ended".to_string();
            ("announcement", text.clone(), Some(text), None)
        } else if message.is_announcement() {
            let text = announcement_text(session, message).unwrap_or_default();
            ("announcement", text.clone(), Some(text), None)
        } else if let Some(location) = shared_location.as_deref() {
            let text = message
                .text
                .clone()
                .unwrap_or_else(|| format!("Shared location {location}"));
            ("location_share", text, None, None)
        } else if let Some(app) = &app {
            (
                "balloon",
                balloon_summary(app, message.text.as_deref()),
                None,
                None,
            )
        } else if service.eq_ignore_ascii_case("imessage") {
            ("imessage", plain(), None, None)
        } else if has_attachments {
            ("mms", plain(), None, None)
        } else {
            ("sms", plain(), None, None)
        };

    let send_effect = expressive_label(message.get_expressive());
    RowKind {
        kind,
        text: with_send_effect(text, send_effect.as_deref()),
        announcement,
        shared_location,
        send_effect,
        app,
        tapback,
    }
}

/// The text with the send effect's label appended, unless it already names it.
fn with_send_effect(text: String, effect: Option<&str>) -> String {
    match effect {
        None => text,
        Some(effect) if text.is_empty() => effect.to_string(),
        Some(effect) if text.contains(effect) => text,
        Some(effect) => format!("{text}\n\n{effect}"),
    }
}

/// Reply threading for a row.
struct ThreadFields {
    /// A reply in a thread (never true for a tapback).
    is_reply: bool,
    /// The message replied to: the reacted-to message for a tapback, else
    /// the thread originator.
    in_reply_to_guid: Option<String>,
    /// Part index within the thread originator.
    thread_originator_part: Option<u32>,
}

/// Where a row points: a tapback at the message it reacts to, any other
/// reply at its thread originator, everything else nowhere.
fn thread_fields(message: &Message, tapback: Option<&TapbackFields>) -> ThreadFields {
    if let Some(tapback) = tapback {
        return ThreadFields {
            is_reply: false,
            in_reply_to_guid: tapback.associated_guid.clone(),
            thread_originator_part: None,
        };
    }
    if !message.is_reply() {
        return ThreadFields {
            is_reply: false,
            in_reply_to_guid: None,
            thread_originator_part: None,
        };
    }
    ThreadFields {
        is_reply: true,
        in_reply_to_guid: message.thread_originator_guid.clone(),
        thread_originator_part: message
            .thread_originator_part
            .as_deref()
            .and_then(parse_thread_part),
    }
}

/// Everything Apple-specific the core message fields do not carry. Blank
/// strings become `None` so the record stays small.
fn imessage_fields(
    session: &MailSession,
    message: &Message,
    row: RowKind,
    parts: &[crate::fields::PartRecord],
) -> ImessageRecord {
    let thread = thread_fields(message, row.tapback.as_ref());
    let edits = message
        .edited_parts
        .as_ref()
        .map(|edited| build_edit_records(edited, &session.offset))
        .unwrap_or_default();
    let read_receipt = message
        .date_read(session.offset)
        .ok()
        .map(|d| d.to_rfc3339());
    // A tapback has no tapbacks of its own.
    let tapbacks = if row.tapback.is_some() {
        None
    } else {
        build_parent_tapbacks(session, message)
    };
    let tapback = row.tapback.as_ref();
    ImessageRecord {
        is_reply: thread.is_reply,
        in_reply_to_guid: trimmed(thread.in_reply_to_guid),
        thread_originator_part: thread.thread_originator_part,
        num_replies: (message.num_replies > 0).then_some(message.num_replies as u32),
        is_deleted: message.is_deleted(),
        send_effect: trimmed(row.send_effect),
        shared_location: trimmed(row.shared_location),
        announcement: trimmed(row.announcement),
        read_receipt_rfc3339: trimmed(read_receipt),
        parts: json_if_any(parts),
        edits: json_if_any(&edits),
        tapbacks,
        balloon_kind: trimmed(row.app.as_ref().and_then(balloon_kind_label)),
        balloon_bundle_id: trimmed(message.balloon_bundle_id.clone()),
        associated_guid: trimmed(tapback.and_then(|t| t.associated_guid.clone())),
        associated_part: tapback.and_then(|t| t.associated_part),
        tapback_kind: tapback.map(|t| t.kind.to_string()),
        tapback_emoji: trimmed(tapback.and_then(|t| t.emoji.clone())),
        tapback_action: tapback.map(|t| t.action.to_string()),
        app: row.app,
    }
}

/// Whether every Apple-specific field is empty, so the record can be dropped.
fn is_empty(fields: &ImessageRecord) -> bool {
    !fields.is_reply
        && fields.in_reply_to_guid.is_none()
        && fields.thread_originator_part.is_none()
        && fields.num_replies.is_none()
        && !fields.is_deleted
        && fields.send_effect.is_none()
        && fields.shared_location.is_none()
        && fields.announcement.is_none()
        && fields.read_receipt_rfc3339.is_none()
        && fields.parts.is_none()
        && fields.edits.is_none()
        && fields.tapbacks.is_none()
        && fields.app.is_none()
        && fields.balloon_bundle_id.is_none()
        && fields.balloon_kind.is_none()
        && fields.associated_guid.is_none()
        && fields.associated_part.is_none()
        && fields.tapback_kind.is_none()
        && fields.tapback_emoji.is_none()
        && fields.tapback_action.is_none()
}

/// The string trimmed, or `None` when blank.
fn trimmed(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// The items as a JSON array, or `None` when there are none.
fn json_if_any<T: serde::Serialize>(items: &[T]) -> Option<Value> {
    if items.is_empty() {
        return None;
    }
    serde_json::to_value(items).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_effect_is_appended_once() {
        assert_eq!(with_send_effect("hi".into(), None), "hi");
        assert_eq!(with_send_effect(String::new(), Some("Slam")), "Slam");
        assert_eq!(with_send_effect("hi".into(), Some("Slam")), "hi\n\nSlam");
        assert_eq!(with_send_effect("hi Slam".into(), Some("Slam")), "hi Slam");
    }

    #[test]
    fn tapback_lines_name_the_reaction() {
        assert_eq!(tapback_human_line("loved", None, "add"), "Loved a message");
        assert_eq!(tapback_human_line("loved", None, "remove"), "Removed Heart");
        assert_eq!(tapback_human_line("emoji", Some("🔥"), "add"), "🔥 reacted");
    }

    #[test]
    fn empty_fields_are_dropped() {
        assert!(is_empty(&ImessageRecord::default()));
        let fields = ImessageRecord {
            is_deleted: true,
            ..ImessageRecord::default()
        };
        assert!(!is_empty(&fields));
    }
}
