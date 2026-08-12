//! Stream messages → [`MailMessage`] → common message → packaging via FormatSink.
//!
//! Every message is built once via [`build_mail_message`] (unchanged Apple →
//! `MailMessage` mapping), converted to [`IrMessage`] (core fields + a nested
//! `imessage` extension bag), and accumulated per conversation.
//! After the DB stream ends, conversations are written via
//! [`message_ir_format::FormatSink`] (see the [message-ir architecture](../../../docs/maintainers/architecture/message-ir.md)).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::{Local, TimeZone};
use imessage_database::{
    message_types::{
        handwriting::HandwrittenMessage,
        variants::{Announcement, Tapback, TapbackAction, Variant},
    },
    tables::{
        attachment::Attachment,
        chat::Chat,
        messages::{
            Message,
            models::{GroupAction, Service},
        },
        table::{ME, ORPHANED, Table, YOU},
    },
    util::dates::TIMESTAMP_FACTOR,
};
use mail::{Direction as MailDirection, MailAttachment, MailMessage, Participant};
use message_ir::{
    ConversationDocument, ConversationMeta, ExportMeta, HandleType, IrAttachment,
    IrConversationType, IrDirection, IrImessage, IrMessage, IrMessageKind, IrParticipant,
    IrService, SCHEMA_VERSION, owner_sender, parse_json_value,
};
use message_ir_format::{FormatSink, FormatSinkResult};
use message_vault_io_core::OutputFormat;
use sha2::{Digest, Sha256};

use crate::{
    attachments::load_attachment_bytes,
    body::{apply_body, referenced_attachment_indices},
    error::RuntimeError,
    fields::{
        PartRecord, TapbackCell, balloon_kind_label, balloon_summary, build_balloon_value,
        build_edit_records, build_part_records, expressive_label, parse_thread_part,
        shared_location_label, sticker_extras, transcription_for_attachment,
    },
    options::AttachmentEmbed,
    session::MailSession,
};

const EXPORT_SOURCE: &str = "imessage";
const EXPORT_TOOL: &str = "imessage-ir-exporter";
const DEFAULT_MESSAGE_PROGRESS_EVERY: u64 = 500;
/// JSONL still batches work, but report often enough that long attachment
/// decrypts between ticks do not look frozen on large backups.
const JSONL_MESSAGE_PROGRESS_EVERY: u64 = 1_000;
const CONVERSATION_PROGRESS_EVERY: u64 = 100;

const fn message_progress_every(format: OutputFormat) -> u64 {
    match format {
        OutputFormat::Jsonl => JSONL_MESSAGE_PROGRESS_EVERY,
        _ => DEFAULT_MESSAGE_PROGRESS_EVERY,
    }
}

/// Messages accumulated for one Apple `chat_identifier` before projection.
struct PendingConversation {
    conversation_type: IrConversationType,
    group_title: Option<String>,
    participants: Vec<Participant>,
    /// First non-empty `destination_caller_id` seen (used for `From`/`To` mapping).
    owner_handle: String,
    /// First non-empty owner display name (caller-id / Me).
    owner_display_name: Option<String>,
    messages: Vec<IrMessage>,
}

/// Stream chat.db into per-conversation CSV, EML, MBOX, JSON, or JSONL.
pub(crate) fn run_export(session: &MailSession) -> Result<FormatSinkResult, RuntimeError> {
    let format = session.options.output_format;
    session.options.emit_log("");
    session.options.emit_log(format!(
        "Preparing {} messages in {}",
        format.as_str(),
        session.options.export_path.display(),
    ));

    // Clean prior IR artifacts (including stale attachments/) before writing new
    // media, matching WhatsApp / SMS Backup & Restore `open_prepared` behavior.
    // Attachments are persisted during the message stream, so cleaning must
    // happen before that pass — not when the sink opens afterward.
    message_ir_format::clean_previous_ir_output(&session.options.export_path).map_err(|e| {
        RuntimeError::InvalidOptions(format!("clean previous export output: {e:#}"))
    })?;

    let copy_attachments = session.options.transforms.copies_attachments();
    let attachments_dir = session.options.export_path.join("attachments");
    if copy_attachments
        && matches!(
            format,
            OutputFormat::Csv | OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Xml
        )
        && session.options.attachment_embed == AttachmentEmbed::Embed
    {
        fs::create_dir_all(&attachments_dir)?;
    }

    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();
    let mut current_message_row = -1;
    let mut current_message = 0u64;
    let mut failures: u64 = 0;
    let total_messages =
        Message::get_count(session.data_source.db(), &session.options.query_context)?;

    let mut statement =
        Message::stream_rows(session.data_source.db(), &session.options.query_context)?;

    for message in Message::rows(&mut statement, [])? {
        // Cheap AtomicBool load; abort promptly when the user cancels.
        message_vault_io_core::check_cancel(session.options.cancel.as_ref())
            .map_err(|msg| RuntimeError::InvalidOptions(msg.to_string()))?;
        let mut msg = message?;

        if msg.rowid == current_message_row {
            current_message += 1;
            continue;
        }
        current_message_row = msg.rowid;

        // Poll vote/update noise — keep skipping (same as CSV/HTML export focus).
        if !msg.is_edited() && (msg.is_poll_vote() || msg.is_poll_update()) {
            current_message += 1;
            continue;
        }

        apply_body(&mut msg, session.data_source.db());

        if msg.is_poll_vote() || msg.is_poll_update() {
            current_message += 1;
            continue;
        }

        match collect_one(session, &mut conversations, &attachments_dir, &msg) {
            Ok(()) => {}
            Err(why) => {
                failures += 1;
                session.options.emit_log(format!(
                    "Skipping message (rowid={}, guid={}): {}",
                    msg.rowid, msg.guid, why
                ));
            }
        }
        current_message += 1;
        // `%` instead of `u64::is_multiple_of`: that method needs Rust 1.87,
        // but this crate's MSRV is 1.85.
        #[allow(clippy::manual_is_multiple_of)]
        if current_message % message_progress_every(format) == 0 {
            session
                .options
                .emit_log(format!("  …{current_message}/{total_messages}"));
        }
    }

    if failures > 0 {
        session.options.emit_log(format!(
            "{failures} messages skipped due to formatting errors."
        ));
    }

    let total_conversations = conversations.len() as u64;
    session.options.emit_log("");
    session.options.emit_log(format!(
        "Writing {total_conversations} conversation file(s)..."
    ));
    let mut sink = FormatSink::open(
        &session.options.export_path,
        format,
        session.options.transforms.clone(),
    )
    .map_err(|e| RuntimeError::InvalidOptions(format!("open export sink: {e:#}")))?;
    let mut written = 0u64;
    for (chat_identifier, convo) in conversations {
        // Cheap AtomicBool load; abort promptly when the user cancels.
        message_vault_io_core::check_cancel(session.options.cancel.as_ref())
            .map_err(|msg| RuntimeError::InvalidOptions(msg.to_string()))?;
        written += 1;
        if convo.messages.is_empty() {
            continue;
        }
        let export = ExportMeta {
            source: EXPORT_SOURCE.into(),
            tool: EXPORT_TOOL.into(),
            tool_version: env!("CARGO_PKG_VERSION").into(),
            owner_handle: (!convo.owner_handle.is_empty()).then(|| convo.owner_handle.clone()),
            owner_display_name: convo
                .owner_display_name
                .or_else(|| session.options.use_caller_id.then(|| ME.to_string())),
        };
        let (owner_handle, owner_display_name) = owner_sender(&export);
        let mut messages = convo.messages;
        for msg in &mut messages {
            if msg.direction == IrDirection::Outgoing {
                msg.sender_handle = owner_handle.clone();
                msg.sender_display_name = owner_display_name.clone();
            }
        }
        let doc = ConversationDocument {
            schema_version: SCHEMA_VERSION,
            export,
            conversation: ConversationMeta {
                chat_identifier,
                conversation_type: convo.conversation_type,
                group_title: convo.group_title,
                participants: convo
                    .participants
                    .into_iter()
                    .map(|p| {
                        let handle_type = handle_type_for(&p.handle);
                        IrParticipant {
                            handle: p.handle,
                            display_name: p.display_name,
                            handle_type: Some(handle_type),
                        }
                    })
                    .collect(),
                stats: Default::default(),
            },
            messages,
            packaging_stem_suffix: None,
        };
        let document_id = doc.conversation.chat_identifier.clone();
        sink.write_document(doc).map_err(|e| {
            RuntimeError::InvalidOptions(format!(
                "write {} for {}: {e:#}",
                format.as_str(),
                document_id
            ))
        })?;
        // `%` instead of `u64::is_multiple_of`: that method needs Rust 1.87,
        // but this crate's MSRV is 1.85.
        #[allow(clippy::manual_is_multiple_of)]
        if written % CONVERSATION_PROGRESS_EVERY == 0 || written == total_conversations {
            session.options.emit_log(format!(
                "  wrote {written}/{total_conversations} conversations"
            ));
        }
    }
    let sink_result = sink
        .finish()
        .map_err(|e| RuntimeError::InvalidOptions(format!("finish export sink: {e:#}")))?;

    Ok(sink_result)
}

fn collect_one(
    session: &MailSession,
    conversations: &mut BTreeMap<String, PendingConversation>,
    attachments_dir: &Path,
    message: &Message,
) -> Result<(), RuntimeError> {
    let mail = build_mail_message(session, message)?;
    let chat_identifier = if mail.chat_identifier.is_empty() {
        ORPHANED.to_string()
    } else {
        mail.chat_identifier.clone()
    };

    let ir_message = mail_message_to_ir(
        &mail,
        attachments_dir,
        session.options.output_format,
        session.options.attachment_embed,
        session.options.transforms.copies_attachments(),
    )?;

    let convo = conversations
        .entry(chat_identifier)
        .or_insert_with(|| PendingConversation {
            conversation_type: IrConversationType::parse(&mail.conversation_type),
            group_title: mail.group_title.clone(),
            participants: mail.participants.clone(),
            owner_handle: String::new(),
            owner_display_name: None,
            messages: Vec::new(),
        });
    if convo.owner_handle.is_empty() && !mail.owner_handle.is_empty() {
        convo.owner_handle = mail.owner_handle.clone();
    }
    if convo.owner_display_name.is_none() {
        convo.owner_display_name = mail.owner_display_name.clone();
    }
    convo.messages.push(ir_message);
    Ok(())
}

/// Convert a built [`MailMessage`] into [`IrMessage`] (core fields + `imessage` bag).
///
/// For CSV / JSON / JSONL / XML, non-empty attachment bytes are persisted under
/// `attachments/` and referenced by `path`. For EML / MBOX, bytes stay in
/// memory for [`message_ir_format::document_to_mail_messages`] to embed directly.
fn mail_message_to_ir(
    mail: &MailMessage,
    attachments_dir: &Path,
    format: OutputFormat,
    embed: AttachmentEmbed,
    copy_attachments: bool,
) -> Result<IrMessage, RuntimeError> {
    let persist_to_disk = copy_attachments
        && matches!(
            format,
            OutputFormat::Csv | OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Xml
        );

    let mut attachments = Vec::with_capacity(mail.attachments.len());
    for attachment in &mail.attachments {
        let has_bytes = embed == AttachmentEmbed::Embed && !attachment.bytes.is_empty();
        let missing_reason = if has_bytes {
            None
        } else if embed == AttachmentEmbed::Disabled {
            Some("embed_disabled".to_string())
        } else {
            Some("file_missing".to_string())
        };
        let (path, digest_sha256, file_size, bytes) = if persist_to_disk {
            if has_bytes {
                let (rel_path, digest, size) = persist_attachment(
                    attachments_dir,
                    mail.timestamp_unix_ms,
                    &attachment.bytes,
                    attachment.original_name.as_deref(),
                )?;
                (Some(rel_path), Some(digest), Some(size), None)
            } else {
                (None, attachment.digest_sha256.clone(), None, None)
            }
        } else {
            let bytes = has_bytes.then(|| attachment.bytes.clone());
            let size = bytes.as_ref().map(|b| b.len() as u64);
            (None, attachment.digest_sha256.clone(), size, bytes)
        };
        attachments.push(IrAttachment {
            path,
            original_name: attachment.original_name.clone(),
            mime_type: attachment.mime_type.clone(),
            digest_sha256,
            is_sticker: attachment.is_sticker,
            transcription: attachment.transcription.clone(),
            sticker_effect: attachment.sticker_effect.clone(),
            size_bytes: file_size,
            missing_reason,
            bytes,
        });
    }

    let direction = match mail.direction {
        MailDirection::Incoming => IrDirection::Incoming,
        MailDirection::Outgoing => IrDirection::Outgoing,
    };
    let (sender_handle, sender_display_name) = match direction {
        IrDirection::Outgoing => {
            let export = ExportMeta {
                source: mail.export_source.clone(),
                tool: mail.export_tool.clone(),
                tool_version: mail.export_tool_version.clone(),
                owner_handle: (!mail.owner_handle.is_empty()).then(|| mail.owner_handle.clone()),
                owner_display_name: mail.owner_display_name.clone(),
            };
            owner_sender(&export)
        }
        IrDirection::Incoming => (mail.sender_handle.clone(), mail.sender_display_name.clone()),
    };

    Ok(IrMessage {
        guid: mail.guid.clone(),
        timestamp_unix_ms: mail.timestamp_unix_ms,
        direction,
        service: IrService::parse(&mail.service),
        message_kind: IrMessageKind::parse(&mail.message_kind),
        sender_handle,
        sender_display_name,
        subject: mail.subject.clone(),
        text: mail.text.clone(),
        attachments,
        imessage: imessage_bag(mail),
        source: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonl_progress_is_less_frequent_than_other_formats() {
        assert_eq!(
            message_progress_every(OutputFormat::Jsonl),
            JSONL_MESSAGE_PROGRESS_EVERY
        );
        assert_eq!(
            message_progress_every(OutputFormat::Json),
            DEFAULT_MESSAGE_PROGRESS_EVERY
        );
    }

    #[test]
    fn persist_attachment_uses_temp_then_rename() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = b"hello-attachment-bytes";
        let (rel, digest, len) =
            persist_attachment(dir.path(), 1_609_459_200_000, bytes, Some("photo.jpg")).unwrap();
        assert_eq!(len, bytes.len() as u64);
        assert_eq!(digest, hex::encode(Sha256::digest(bytes)));
        let name = rel.strip_prefix("attachments/").expect("rel path prefix");
        let dest = dir.path().join(name);
        assert!(dest.is_file());
        assert_eq!(fs::read(&dest).unwrap(), bytes);
        assert!(!dir.path().join(format!("{name}.tmp")).exists());

        // Incomplete dest (wrong length) must be rewritten.
        fs::write(&dest, b"x").unwrap();
        assert_ne!(fs::metadata(&dest).unwrap().len(), bytes.len() as u64);
        let (rel2, digest2, _) =
            persist_attachment(dir.path(), 1_609_459_200_000, bytes, Some("photo.jpg")).unwrap();
        assert_eq!(rel2, rel);
        assert_eq!(digest2, digest);
        assert_eq!(fs::read(&dest).unwrap(), bytes);
        assert!(!dir.path().join(format!("{name}.tmp")).exists());
    }
}

/// Build typed [`IrImessage`] from `MailMessage` extension fields.
///
/// Nested Apple blobs (`parts` / `edits` / `tapbacks` / `app`) are parsed from
/// JSON strings into [`serde_json::Value`]s. Owner display name lives on
/// [`ExportMeta`], not here.
fn imessage_bag(mail: &MailMessage) -> Option<IrImessage> {
    fn nonempty(s: &Option<String>) -> Option<String> {
        s.as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    }

    IrImessage {
        is_reply: mail.is_reply,
        in_reply_to_guid: nonempty(&mail.in_reply_to_guid),
        thread_originator_part: mail.thread_originator_part,
        num_replies: mail.num_replies,
        is_deleted: mail.is_deleted,
        send_effect: nonempty(&mail.send_effect),
        shared_location: nonempty(&mail.shared_location),
        announcement: nonempty(&mail.announcement),
        read_receipt_rfc3339: nonempty(&mail.read_receipt_rfc3339),
        parts: mail
            .parts_json
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(parse_json_value),
        edits: mail
            .edits_json
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(parse_json_value),
        tapbacks: mail
            .tapbacks_json
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(parse_json_value),
        app: mail
            .app_json
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(parse_json_value),
        balloon_bundle_id: nonempty(&mail.balloon_bundle_id),
        balloon_kind: nonempty(&mail.balloon_kind),
        associated_guid: nonempty(&mail.associated_guid),
        associated_part: mail.associated_part,
        tapback_kind: nonempty(&mail.tapback_kind),
        tapback_emoji: nonempty(&mail.tapback_emoji),
        tapback_action: nonempty(&mail.tapback_action),
    }
    .into_option()
}

/// Destination file name for a persisted attachment: `<local-date>-<digest16><ext>`.
fn attachment_dest_name(
    timestamp_unix_ms: i64,
    digest_hex: &str,
    original_name: Option<&str>,
) -> String {
    let digest_prefix = &digest_hex[..16.min(digest_hex.len())];
    let secs = timestamp_unix_ms.div_euclid(1000);
    let date_prefix = Local
        .timestamp_opt(secs, 0)
        .single()
        .map(|t| t.format("%Y%m%d_%H%M%S").to_string())
        .unwrap_or_else(|| secs.to_string());
    let ext = original_name
        .and_then(|n| Path::new(n).extension())
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    format!("{date_prefix}-{digest_prefix}{ext}")
}

/// Write attachment bytes under `attachments_dir` (idempotent by digest name).
///
/// Writes via `{name}.tmp` then renames into place so a crash mid-write cannot
/// leave a short final file that later runs treat as complete. Hashes once.
///
/// Returns the export-relative path (`attachments/<name>`), the sha256 digest,
/// and the byte length of the persisted file.
fn persist_attachment(
    attachments_dir: &Path,
    timestamp_unix_ms: i64,
    bytes: &[u8],
    original_name: Option<&str>,
) -> Result<(String, String, u64), RuntimeError> {
    let digest_hex = hex::encode(Sha256::digest(bytes));
    let name = attachment_dest_name(timestamp_unix_ms, &digest_hex, original_name);
    let dest = attachments_dir.join(&name);
    let byte_len = bytes.len() as u64;
    let needs_write = match fs::metadata(&dest) {
        Ok(meta) => meta.len() != byte_len,
        Err(_) => true,
    };
    if needs_write {
        let tmp = attachments_dir.join(format!("{name}.tmp"));
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, &dest)?;
    }
    Ok((format!("attachments/{name}"), digest_hex, byte_len))
}

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

/// iMessage stores handles as phone numbers or email addresses without
/// recording which; infer the type from the handle shape.
fn handle_type_for(handle: &str) -> HandleType {
    if handle.contains('@') {
        HandleType::Email
    } else {
        HandleType::Phone
    }
}

fn raw_handle(session: &MailSession, handle_id: i32) -> Option<String> {
    session
        .resolve_participant(handle_id)
        .map(|name| name.details.clone())
}

fn display_name_for(session: &MailSession, handle_id: i32) -> Option<String> {
    session.resolve_participant(handle_id).map(|name| {
        if name.full.is_empty() {
            name.details.clone()
        } else {
            name.full.clone()
        }
    })
}

fn participants_for(session: &MailSession, chatroom: &Chat) -> (Vec<Participant>, &'static str) {
    let mut records = Vec::new();
    // Only non-empty handles are emitted, so only count those; a raw handle
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

fn announcement_text(session: &MailSession, msg: &Message) -> Option<String> {
    let announcement = msg.get_announcement()?;
    let mut who = session.who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id);
    if who == ME {
        who = YOU;
    }
    let participant_name = match &announcement {
        Announcement::GroupAction(
            GroupAction::ParticipantAdded(handle) | GroupAction::ParticipantRemoved(handle),
        ) => session.who(Some(*handle), false, &msg.destination_caller_id),
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

fn build_parent_tapbacks_json(session: &MailSession, message: &Message) -> Option<String> {
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
            let (kind, emoji) = match kind {
                Tapback::Loved => ("loved", None),
                Tapback::Liked => ("liked", None),
                Tapback::Disliked => ("disliked", None),
                Tapback::Laughed => ("laughed", None),
                Tapback::Emphasized => ("emphasized", None),
                Tapback::Questioned => ("questioned", None),
                Tapback::Emoji(e) => ("emoji", e.map(str::to_string)),
                Tapback::Sticker => ("sticker", None),
            };
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
    serde_json::to_string(&cells).ok()
}

fn try_handwriting_svg(session: &MailSession, message: &Message) -> Option<MailAttachment> {
    if !message.is_handwriting() {
        return None;
    }
    let payload = message.raw_payload_data(session.data_source.db())?;
    let hw = HandwrittenMessage::from_payload(&payload).ok()?;
    let svg = hw.render_svg();
    Some(MailAttachment {
        bytes: svg.into_bytes(),
        original_name: Some(format!("{}.svg", message.guid)),
        mime_type: Some("image/svg+xml".into()),
        digest_sha256: None,
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
    })
}

struct MailConversationContext {
    chat_identifier: String,
    conversation_type: String,
    group_title: Option<String>,
    participants: Vec<Participant>,
    is_from_me: bool,
    sender_handle: Option<String>,
    sender_display_name: Option<String>,
    service: String,
}

fn resolve_mail_conversation_context(
    session: &MailSession,
    message: &Message,
) -> MailConversationContext {
    let (chat_identifier, conversation_type, group_title, participants) =
        match session.conversation(message) {
            Some((chatroom, _)) => {
                let (participants, conversation_type) = participants_for(session, chatroom);
                (
                    chatroom.chat_identifier.clone(),
                    conversation_type.to_string(),
                    chatroom
                        .display_name()
                        .map(str::trim)
                        .filter(|n| !n.is_empty())
                        .map(str::to_string),
                    participants,
                )
            }
            None => (String::new(), "individual".to_string(), None, Vec::new()),
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

    MailConversationContext {
        chat_identifier,
        conversation_type,
        group_title,
        participants,
        is_from_me,
        sender_handle,
        sender_display_name,
        service,
    }
}

fn collect_mail_parts_and_attachments(
    session: &MailSession,
    message: &Message,
) -> Result<(Vec<PartRecord>, Vec<MailAttachment>), RuntimeError> {
    let mut attachments = Attachment::from_message(session.data_source.db(), message)?;
    let referenced = referenced_attachment_indices(message, &attachments);
    let emitted_index: std::collections::HashMap<usize, usize> = referenced
        .iter()
        .enumerate()
        .map(|(emitted, &full)| (full, emitted))
        .collect();

    let mut parts = build_part_records(message, &attachments);
    for part in &mut parts {
        part.attachment_indices = part
            .attachment_indices
            .iter()
            .filter_map(|full| emitted_index.get(full).copied())
            .collect();
    }

    let mut mail_attachments = Vec::new();
    for &idx in &referenced {
        let attachment = &mut attachments[idx];
        let bytes = load_attachment_bytes(session, attachment)?;
        let transcription = transcription_for_attachment(message, attachment);
        let (_prompt, sticker_effect) = sticker_extras(
            attachment,
            &session.options.platform,
            session.options.db_path.as_path(),
            session.options.attachment_root.as_deref(),
        );
        mail_attachments.push(MailAttachment {
            bytes,
            original_name: attachment.transfer_name.clone(),
            mime_type: attachment.mime_type.clone(),
            digest_sha256: None,
            is_sticker: attachment.is_sticker,
            transcription,
            sticker_effect,
        });
    }

    if let Some(svg) = try_handwriting_svg(session, message) {
        mail_attachments.push(svg);
    }

    Ok((parts, mail_attachments))
}

fn build_mail_message(
    session: &MailSession,
    message: &Message,
) -> Result<MailMessage, RuntimeError> {
    let MailConversationContext {
        chat_identifier,
        conversation_type,
        group_title,
        participants,
        is_from_me,
        sender_handle,
        sender_display_name,
        service,
    } = resolve_mail_conversation_context(session, message);

    let (parts, mail_attachments) = collect_mail_parts_and_attachments(session, message)?;

    let send_effect = expressive_label(message.get_expressive());
    let shared_location = message
        .shared_location_kind()
        .map(shared_location_label)
        .map(str::to_string);

    let app_value = build_balloon_value(session.data_source.db(), message);
    let balloon_kind = app_value.as_ref().and_then(balloon_kind_label);
    let balloon_bundle_id = message.balloon_bundle_id.clone();

    let edits = message
        .edited_parts
        .as_ref()
        .map(|edited| build_edit_records(edited, &session.offset))
        .unwrap_or_default();

    // --- Tapback path ---
    let message_kind;
    let mut text;
    let mut announcement = None;
    let mut associated_guid = None;
    let mut associated_part = None;
    let mut tapback_kind = None;
    let mut tapback_emoji = None;
    let mut tapback_action = None;
    let mut in_reply_to_guid = None;
    let mut is_reply = false;
    let mut thread_originator_part = None;

    if let Variant::Tapback(_, action, kind) = message.variant() {
        let (kind_s, emoji) = match kind {
            Tapback::Loved => ("loved", None),
            Tapback::Liked => ("liked", None),
            Tapback::Disliked => ("disliked", None),
            Tapback::Laughed => ("laughed", None),
            Tapback::Emphasized => ("emphasized", None),
            Tapback::Questioned => ("questioned", None),
            Tapback::Emoji(e) => ("emoji", e.map(str::to_string)),
            Tapback::Sticker => ("sticker", None),
        };
        let action_s = match action {
            TapbackAction::Added => "add",
            TapbackAction::Removed => "remove",
        };
        message_kind = if matches!(kind, Tapback::Sticker) {
            "sticker_tapback".to_string()
        } else {
            "tapback".to_string()
        };
        if let Some((part, guid)) = message.clean_associated_guid() {
            associated_guid = Some(guid.to_string());
            associated_part = Some(part as u32);
            in_reply_to_guid = Some(guid.to_string());
        }
        tapback_kind = Some(kind_s.to_string());
        tapback_emoji = emoji;
        tapback_action = Some(action_s.to_string());
        text = tapback_human_line(kind_s, tapback_emoji.as_deref(), action_s);
    } else if message.is_shareplay() {
        message_kind = "announcement".to_string();
        text = "SharePlay Message Ended".to_string();
        announcement = Some(text.clone());
    } else if message.is_announcement() {
        message_kind = "announcement".to_string();
        text = announcement_text(session, message).unwrap_or_default();
        announcement = Some(text.clone());
    } else if shared_location.is_some() {
        message_kind = "location_share".to_string();
        text = message.text.clone().unwrap_or_else(|| {
            format!(
                "Shared location {}",
                shared_location.as_deref().unwrap_or("started")
            )
        });
    } else if app_value.is_some() {
        message_kind = "balloon".to_string();
        text = app_value
            .as_ref()
            .map(|v| balloon_summary(v, message.text.as_deref()))
            .unwrap_or_default();
    } else if service.eq_ignore_ascii_case("imessage") {
        message_kind = "imessage".to_string();
        text = message.text.clone().unwrap_or_default();
    } else if !mail_attachments.is_empty() {
        message_kind = "mms".to_string();
        text = message.text.clone().unwrap_or_default();
    } else {
        message_kind = "sms".to_string();
        text = message.text.clone().unwrap_or_default();
    }

    // Replies (non-tapback): own message + thread headers.
    if !message.is_tapback() && message.is_reply() {
        is_reply = true;
        if let Some(guid) = message.thread_originator_guid.clone() {
            in_reply_to_guid = Some(guid);
        }
        thread_originator_part = message
            .thread_originator_part
            .as_deref()
            .and_then(parse_thread_part);
    }

    if let Some(effect) = send_effect.as_deref() {
        if text.is_empty() {
            text = effect.to_string();
        } else if !text.contains(effect) {
            text = format!("{text}\n\n{effect}");
        }
    }

    let read_receipt_rfc3339 = message
        .date_read(session.offset)
        .ok()
        .map(|d| d.to_rfc3339());

    let num_replies = if message.num_replies > 0 {
        Some(message.num_replies as u32)
    } else {
        None
    };

    let parts_json = if parts.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&parts).unwrap_or_else(|_| "[]".into()))
    };
    let edits_json = if edits.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&edits).unwrap_or_else(|_| "[]".into()))
    };
    let app_json = app_value.map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "null".into()));
    let tapbacks_json = if message.is_tapback() {
        None
    } else {
        build_parent_tapbacks_json(session, message)
    };

    let owner_handle = message.destination_caller_id.clone().unwrap_or_default();

    Ok(MailMessage {
        chat_identifier,
        conversation_type,
        group_title,
        participants,
        guid: message.guid.clone(),
        timestamp_unix_ms: timestamp_unix_ms(message, session.offset),
        direction: if is_from_me {
            MailDirection::Outgoing
        } else {
            MailDirection::Incoming
        },
        service,
        message_kind,
        sender_handle,
        sender_display_name,
        owner_handle,
        owner_display_name: owner_display_name(session, message),
        subject: message.subject.clone().filter(|s| !s.is_empty()),
        text,
        android_type: None,
        source_fields_json: None,
        export_source: EXPORT_SOURCE.into(),
        export_tool: EXPORT_TOOL.into(),
        export_tool_version: env!("CARGO_PKG_VERSION").into(),
        attachments: mail_attachments,
        filename_suffix: None,
        is_reply,
        in_reply_to_guid,
        thread_originator_part,
        num_replies,
        is_deleted: message.is_deleted(),
        send_effect,
        shared_location,
        announcement,
        read_receipt_rfc3339,
        parts_json,
        edits_json,
        app_json,
        balloon_bundle_id,
        balloon_kind,
        tapbacks_json,
        associated_guid,
        associated_part,
        tapback_kind,
        tapback_emoji,
        tapback_action,
    })
}
