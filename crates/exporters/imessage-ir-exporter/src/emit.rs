//! Convert Apple Messages rows into the shared conversation structure
//! ([`ConversationDocument`]) every exporter writes.
//!
//! Each row becomes a [`MailMessage`], then an [`IrMessage`] (core fields plus
//! an `imessage` extension bag). After the database stream ends,
//! [`message_ir_format::FormatSink`] writes the chosen output format.

use std::collections::BTreeMap;
use std::path::Path;

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
use mail::{MailMessage, Participant};
use message_ir::{
    ConversationDocument, ConversationMeta, ExportMeta, HandleType, IrAttachment,
    IrConversationType, IrDirection, IrImessage, IrMessage, IrMessageKind, IrParticipant,
    IrService, SCHEMA_VERSION, owner_sender, parse_json_value,
};
use message_ir_format::{
    AttachmentSource, ConversationUnit, FormatSink, FormatSinkResult, WriteQueueOptions,
};
use message_vault_io_core::{AttachmentJob, OutputFormat, run_attachment_jobs};

use crate::{
    attachments::read_resolved_attachment,
    attachments_emit::{AttachmentLoad, collect_mail_parts_and_attachments},
    body::apply_body,
    error::RuntimeError,
    fields::{
        TapbackCell, balloon_kind_label, balloon_summary, build_balloon_value, build_edit_records,
        expressive_label, parse_thread_part, shared_location_label,
    },
    options::AttachmentEmbed,
    session::MailSession,
};

const EXPORT_SOURCE: &str = "imessage";
const EXPORT_TOOL: &str = "imessage-ir-exporter";
const DEFAULT_MESSAGE_PROGRESS_EVERY: u64 = 500;
/// JSON Lines still batches work, but report often enough that long attachment
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
    /// Load keys in the same order as flattened `messages[].attachments`.
    attachment_loads: Vec<AttachmentLoad>,
}

/// Stream `chat.db` into the shared conversation structure, then write the
/// chosen output format (JSON Lines, JSON, CSV, EML, MBOX, or XML).
///
/// # Errors
///
/// Returns an error when the Messages database cannot be read, a conversation
/// cannot be written, or the user cancels.
pub(crate) fn run_export(session: &MailSession) -> Result<FormatSinkResult, RuntimeError> {
    let format = session.options.output_format;
    session.options.emit_log("");
    session.options.emit_log(format!(
        "Preparing {} messages in {}",
        format.as_str(),
        session.options.export_path.display(),
    ));

    // Prepare the sink before the message stream. Attachment files are written
    // after parse by the shared runner, so prior IR artifacts (including stale
    // attachments/) must be cleaned first — same pattern as WhatsApp / SMS
    // Backup & Restore. A resumed run is the exception: what the interrupted
    // run wrote is exactly the work this one gets to skip.
    let (mut sink, attachments_dir) = if session.options.resume {
        FormatSink::open_resume(
            &session.options.export_path,
            format,
            session.options.transforms.clone(),
        )
    } else {
        FormatSink::open_prepared(
            &session.options.export_path,
            format,
            session.options.transforms.clone(),
        )
    }
    .map_err(|e| RuntimeError::InvalidOptions(format!("open export sink: {e:#}")))?;

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

    // JSONL without obfuscation is the import path: it goes on the write
    // queue, which writes each conversation's attachments before the
    // conversation file and can therefore skip what a previous run finished.
    // Obfuscation is stateful across documents and the other formats merge or
    // embed at finish, so they keep the sink path.
    let use_queue = format == OutputFormat::Jsonl && !session.options.transforms.obfuscate;
    if use_queue {
        return drain_conversations(session, conversations);
    }

    if matches!(
        format,
        OutputFormat::Csv | OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Xml
    ) {
        stage_conversation_attachments(session, &mut conversations, &attachments_dir)?;
    }

    let total_conversations = conversations.len() as u64;
    session.options.emit_log("");
    session.options.emit_log(format!(
        "Preparing {total_conversations} conversation file(s)..."
    ));
    let mut written = 0u64;
    for (chat_identifier, convo) in conversations {
        // Cheap AtomicBool load; abort promptly when the user cancels.
        message_vault_io_core::check_cancel(session.options.cancel.as_ref())
            .map_err(|msg| RuntimeError::InvalidOptions(msg.to_string()))?;
        written += 1;
        if convo.messages.is_empty() {
            continue;
        }
        let doc = pending_to_document(chat_identifier, convo, session.options.use_caller_id);
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
            session
                .options
                .emit_log(format!("  preparing {written}/{total_conversations}"));
        }
    }
    let sink_result = sink
        .finish()
        .map_err(|e| RuntimeError::InvalidOptions(format!("finish export sink: {e:#}")))?;

    Ok(sink_result)
}

/// Project one accumulated conversation into the shared document shape.
fn pending_to_document(
    chat_identifier: String,
    convo: PendingConversation,
    use_caller_id: bool,
) -> ConversationDocument {
    let export = ExportMeta {
        source: EXPORT_SOURCE.into(),
        tool: EXPORT_TOOL.into(),
        tool_version: env!("CARGO_PKG_VERSION").into(),
        owner_handle: (!convo.owner_handle.is_empty()).then(|| convo.owner_handle.clone()),
        owner_display_name: convo
            .owner_display_name
            .or_else(|| use_caller_id.then(|| ME.to_string())),
    };
    let (owner_handle, owner_display_name) = owner_sender(&export);
    let mut messages = convo.messages;
    for msg in &mut messages {
        if msg.direction == IrDirection::Outgoing {
            msg.sender_handle = owner_handle.clone();
            msg.sender_display_name = owner_display_name.clone();
        }
    }
    ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export,
        conversation: ConversationMeta {
            chat_identifier,
            conversation_type: convo.conversation_type,
            group_title: convo.group_title,
            participants: mail_participants_to_ir(convo.participants),
            stats: Default::default(),
        },
        messages,
        packaging_stem_suffix: None,
    }
}

/// Pair a conversation's document with its attachment sources.
///
/// `attachment_loads` is positional: it runs in the same order as the
/// conversation's flattened `messages[].attachments`, so the sources are
/// consumed in that order and land on the attachment each was collected for.
fn pending_to_unit(
    chat_identifier: String,
    mut convo: PendingConversation,
    use_caller_id: bool,
) -> ConversationUnit {
    let loads = std::mem::take(&mut convo.attachment_loads);
    let doc = pending_to_document(chat_identifier, convo, use_caller_id);
    let mut loads = loads.into_iter();
    ConversationUnit::from_doc(doc, |_, att| match loads.next() {
        Some(AttachmentLoad::Path { path, size_hint }) => (AttachmentSource::Path(path), size_hint),
        Some(AttachmentLoad::Bytes(bytes)) => {
            let hint = Some(bytes.len() as u64);
            (AttachmentSource::Bytes(bytes), hint)
        }
        Some(AttachmentLoad::Missing) | None => (AttachmentSource::Missing, att.size_bytes),
    })
}

/// Write every conversation through the shared write queue.
fn drain_conversations(
    session: &MailSession,
    conversations: BTreeMap<String, PendingConversation>,
) -> Result<FormatSinkResult, RuntimeError> {
    let use_caller_id = session.options.use_caller_id;
    let units: Vec<ConversationUnit> = conversations
        .into_iter()
        .filter(|(_, convo)| !convo.messages.is_empty())
        .map(|(chat_identifier, convo)| pending_to_unit(chat_identifier, convo, use_caller_id))
        .collect();

    let options = WriteQueueOptions {
        media: session.options.transforms.media,
        compress: session.options.transforms.compress.clone(),
        resume: session.options.resume,
        writer_count: 0,
    };
    let log = session.options.log.clone();
    let cancel = session.options.cancel.as_ref();

    let encrypted = session
        .data_source
        .backup
        .as_ref()
        .is_some_and(|b| b.is_encrypted());

    let report = if encrypted {
        // crabapple's Backup holds a SQLite connection, which is not Sync, so
        // the decrypt loader cannot cross threads. One writer keeps every
        // invariant; decrypt-bound throughput would not have parallelized well
        // anyway.
        let mut load = |source: &mut AttachmentSource| match source {
            AttachmentSource::Path(path) => {
                let bytes = read_resolved_attachment(session, path).map_err(|e| {
                    // Say why before it becomes a chip: a systemic failure
                    // otherwise reads as a run's worth of unexplained gaps.
                    session.options.emit_log(format!(
                        "warning: attachment {} could not be read: {e}",
                        path.display()
                    ));
                    e.to_string()
                })?;
                Ok((!bytes.is_empty()).then_some(bytes))
            }
            other => message_ir_format::load_attachment_source(other),
        };
        message_ir_format::drain_write_queue_with_loader(
            &session.options.export_path,
            units,
            &options,
            &mut load,
            log.as_ref(),
            cancel,
        )
    } else {
        message_ir_format::drain_write_queue(
            &session.options.export_path,
            units,
            &options,
            log.as_ref(),
            cancel,
        )
    }
    .map_err(|e| RuntimeError::InvalidOptions(format!("write conversations: {e:#}")))?;

    Ok(FormatSinkResult {
        xml_path: None,
        media: report.media,
        obfuscated_docs: 0,
    })
}

/// Map mail-crate participants onto the shared [`IrParticipant`] shape.
fn mail_participants_to_ir(participants: Vec<Participant>) -> Vec<IrParticipant> {
    participants
        .into_iter()
        .map(|p| {
            let handle_type = handle_type_for(&p.handle);
            IrParticipant {
                handle: p.handle,
                display_name: p.display_name,
                handle_type: Some(handle_type),
            }
        })
        .collect()
}

/// Convert one Apple message into an [`IrMessage`] and append it to its conversation.
///
/// # Errors
///
/// Returns an error when the message cannot be converted or an attachment
/// cannot be written.
/// Write staged attachment bytes after parse and before conversation files.
fn stage_conversation_attachments(
    session: &MailSession,
    conversations: &mut BTreeMap<String, PendingConversation>,
    attachments_dir: &Path,
) -> Result<(), RuntimeError> {
    let mode = session.options.transforms.media;
    let compress = session.options.transforms.compress.clone();
    let cancel = session.options.cancel.as_ref().map(|flag| flag.as_ref());

    let mut loads = Vec::new();
    let mut jobs = Vec::new();
    for convo in conversations.values_mut() {
        loads.append(&mut convo.attachment_loads);
        for msg in &mut convo.messages {
            let ts = msg.timestamp_unix_ms;
            for att in &mut msg.attachments {
                let hint = match loads.get(jobs.len()) {
                    Some(AttachmentLoad::Path { size_hint, .. }) => *size_hint,
                    Some(AttachmentLoad::Bytes(bytes)) => Some(bytes.len() as u64),
                    _ => att.size_bytes,
                };
                jobs.push(AttachmentJob {
                    attachment: att,
                    timestamp_unix_ms: ts,
                    size_hint: hint,
                });
            }
        }
    }

    run_attachment_jobs(
        &mut jobs,
        attachments_dir,
        mode,
        &compress,
        |i| match loads.get(i) {
            Some(AttachmentLoad::Path { path, .. }) => {
                let bytes = read_resolved_attachment(session, path).map_err(|e| {
                    // run_attachment_jobs turns any Err other than "canceled" into a
                    // file_missing attachment and moves on. Log the real reason here
                    // first, or a systemic failure (a revoked Full Disk Access, a
                    // failing disk) degrades into a run's worth of unexplained chips.
                    session.options.emit_log(format!(
                        "warning: attachment {} could not be read: {e}",
                        path.display()
                    ));
                    e.to_string()
                })?;
                Ok((!bytes.is_empty()).then_some(bytes))
            }
            Some(AttachmentLoad::Bytes(bytes)) => Ok(Some(bytes.clone())),
            _ => Ok(None),
        },
        |progress| {
            session.options.emit_log(format!(
                "  attachments {}/{} {}/{}",
                progress.done, progress.total, progress.bytes_done, progress.bytes_total
            ));
        },
        session.options.log.as_ref(),
        cancel,
    )
    .map_err(RuntimeError::InvalidOptions)?;

    for convo in conversations.values_mut() {
        for msg in &mut convo.messages {
            for att in &mut msg.attachments {
                att.bytes = None;
            }
        }
    }
    Ok(())
}

fn collect_one(
    session: &MailSession,
    conversations: &mut BTreeMap<String, PendingConversation>,
    attachments_dir: &Path,
    message: &Message,
) -> Result<(), RuntimeError> {
    let (mail, loads) = build_mail_message(session, message)?;
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
            attachment_loads: Vec::new(),
        });
    if convo.owner_handle.is_empty() && !mail.owner_handle.is_empty() {
        convo.owner_handle = mail.owner_handle.clone();
    }
    if convo.owner_display_name.is_none() {
        convo.owner_display_name = mail.owner_display_name.clone();
    }
    convo.attachment_loads.extend(loads);
    convo.messages.push(ir_message);
    Ok(())
}

/// Convert a built [`MailMessage`] into [`IrMessage`] (core fields + `imessage` bag).
///
/// For CSV / JSON / JSON Lines / XML, non-empty attachment bytes are written
/// under `attachments/` and referenced by `path`. For EML / MBOX, bytes stay in
/// memory for [`message_ir_format::document_to_mail_messages`] to embed directly.
///
/// # Errors
///
/// Returns an error when attachment bytes cannot be written to disk.
fn mail_message_to_ir(
    mail: &MailMessage,
    _attachments_dir: &Path,
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
        let deferred = persist_to_disk && embed == AttachmentEmbed::Embed;
        let missing_reason = if has_bytes || deferred {
            None
        } else if embed == AttachmentEmbed::Disabled {
            Some("not_copied".to_string())
        } else {
            Some("file_missing".to_string())
        };
        let (path, digest_sha256, file_size, bytes) = if persist_to_disk {
            if has_bytes {
                // Handwriting SVG stays in memory. File attachments are loaded
                // later by `run_attachment_jobs`.
                (
                    None,
                    None,
                    Some(attachment.bytes.len() as u64),
                    Some(attachment.bytes.clone()),
                )
            } else {
                (None, None, None, None)
            }
        } else {
            let bytes = has_bytes.then(|| attachment.bytes.clone());
            let size = bytes.as_ref().map(|b| b.len() as u64);
            (None, attachment.meta.digest_sha256.clone(), size, bytes)
        };
        attachments.push(IrAttachment {
            path,
            original_name: attachment.meta.original_name.clone(),
            mime_type: attachment.meta.mime_type.clone(),
            digest_sha256,
            is_sticker: attachment.is_sticker,
            transcription: attachment.transcription.clone(),
            sticker_effect: attachment.sticker_effect.clone(),
            size_bytes: file_size,
            missing_reason,
            bytes,
        });
    }

    let (sender_handle, sender_display_name) = match mail.message.direction {
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
        IrDirection::Incoming => (
            mail.message.sender_handle.clone(),
            mail.message.sender_display_name.clone(),
        ),
    };

    let mut out = mail.message.clone();
    out.sender_handle = sender_handle;
    out.sender_display_name = sender_display_name;
    out.attachments = attachments;
    Ok(out)
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

/// iMessage stores handles as phone numbers or email addresses without
/// recording which; infer the type from the handle shape.
fn handle_type_for(handle: &str) -> HandleType {
    if handle.contains('@') {
        HandleType::Email
    } else {
        HandleType::Phone
    }
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

/// Participants and conversation type (`dm` / `group`) for one chat room.
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

/// JSON array of tapbacks on this message, if any exist.
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

/// Chat id, participants, and sender fields used to build a [`MailMessage`].
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

/// Build a [`MailMessage`] from one Apple Messages row.
///
/// # Errors
///
/// Returns an error when body parts or attachments cannot be loaded.
fn build_mail_message(
    session: &MailSession,
    message: &Message,
) -> Result<(MailMessage, Vec<AttachmentLoad>), RuntimeError> {
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

    let defer_file_bytes = session.options.transforms.copies_attachments()
        && matches!(
            session.options.output_format,
            OutputFormat::Csv | OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Xml
        );
    let (parts, mail_attachments, loads) =
        collect_mail_parts_and_attachments(session, message, defer_file_bytes)?;

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

    let tapbacks_json = if message.is_tapback() {
        None
    } else {
        build_parent_tapbacks_json(session, message)
    };

    let owner_handle = message.destination_caller_id.clone().unwrap_or_default();

    fn nonempty(s: Option<String>) -> Option<String> {
        s.as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    }

    let bag = IrImessage {
        is_reply,
        in_reply_to_guid: nonempty(in_reply_to_guid),
        thread_originator_part,
        num_replies,
        is_deleted: message.is_deleted(),
        send_effect: nonempty(send_effect),
        shared_location: nonempty(shared_location),
        announcement: nonempty(announcement),
        read_receipt_rfc3339: nonempty(read_receipt_rfc3339),
        parts: if parts.is_empty() {
            None
        } else {
            serde_json::to_value(&parts).ok()
        },
        edits: if edits.is_empty() {
            None
        } else {
            serde_json::to_value(&edits).ok()
        },
        tapbacks: tapbacks_json.as_deref().map(parse_json_value),
        app: app_value,
        balloon_bundle_id: nonempty(balloon_bundle_id),
        balloon_kind: nonempty(balloon_kind),
        associated_guid: nonempty(associated_guid),
        associated_part,
        tapback_kind: nonempty(tapback_kind),
        tapback_emoji: nonempty(tapback_emoji),
        tapback_action: nonempty(tapback_action),
    };

    let mail = MailMessage {
        chat_identifier,
        conversation_type,
        group_title,
        participants,
        owner_handle,
        owner_display_name: owner_display_name(session, message),
        export_source: EXPORT_SOURCE.into(),
        export_tool: EXPORT_TOOL.into(),
        export_tool_version: env!("CARGO_PKG_VERSION").into(),
        filename_suffix: None,
        message: IrMessage {
            guid: message.guid.clone(),
            timestamp_unix_ms: timestamp_unix_ms(message, session.offset),
            direction: if is_from_me {
                IrDirection::Outgoing
            } else {
                IrDirection::Incoming
            },
            service: IrService::parse(&service),
            message_kind: IrMessageKind::parse(&message_kind),
            sender_handle,
            sender_display_name,
            subject: message.subject.clone().filter(|s| !s.is_empty()),
            text,
            attachments: Vec::new(),
            imessage: bag.into_option(),
            source: None,
        },
        attachments: mail_attachments,
    };
    Ok((mail, loads))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mail::MailAttachment;
    use std::fs;

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

    fn sample_mail_with_attachment(bytes: Vec<u8>) -> MailMessage {
        MailMessage {
            chat_identifier: "+15555550122".into(),
            conversation_type: "individual".into(),
            group_title: None,
            participants: vec![],
            owner_handle: "+15555550100".into(),
            owner_display_name: None,
            export_source: "imessage".into(),
            export_tool: "test".into(),
            export_tool_version: "0".into(),
            filename_suffix: None,
            message: IrMessage {
                guid: "guid-1".into(),
                timestamp_unix_ms: 1_609_459_200_000,
                direction: IrDirection::Incoming,
                service: IrService::Sms,
                message_kind: IrMessageKind::Sms,
                sender_handle: Some("+15555550122".into()),
                sender_display_name: None,
                subject: None,
                text: "hi".into(),
                attachments: Vec::new(),
                imessage: None,
                source: None,
            },
            attachments: vec![MailAttachment {
                bytes,
                meta: message_ir::AttachmentMeta {
                    path: None,
                    original_name: Some("a.jpg".into()),
                    mime_type: Some("image/jpeg".into()),
                    digest_sha256: None,
                },
                is_sticker: false,
                transcription: None,
                sticker_effect: None,
            }],
        }
    }

    #[test]
    fn missing_reason_reflects_embed_and_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let with_bytes = sample_mail_with_attachment(b"abc".to_vec());
        let ir = mail_message_to_ir(
            &with_bytes,
            dir.path(),
            OutputFormat::Jsonl,
            AttachmentEmbed::Embed,
            true,
        )
        .unwrap();
        assert_eq!(ir.attachments[0].missing_reason, None);
        assert!(ir.attachments[0].path.is_none());
        assert_eq!(ir.attachments[0].bytes.as_deref(), Some(b"abc".as_slice()));
        assert!(fs::read_dir(dir.path()).unwrap().next().is_none());

        let empty = sample_mail_with_attachment(Vec::new());
        let ir_deferred = mail_message_to_ir(
            &empty,
            dir.path(),
            OutputFormat::Jsonl,
            AttachmentEmbed::Embed,
            true,
        )
        .unwrap();
        assert_eq!(ir_deferred.attachments[0].missing_reason, None);
        assert!(ir_deferred.attachments[0].bytes.is_none());
        assert!(ir_deferred.attachments[0].path.is_none());
        assert!(fs::read_dir(dir.path()).unwrap().next().is_none());

        let ir_disabled = mail_message_to_ir(
            &with_bytes,
            dir.path(),
            OutputFormat::Jsonl,
            AttachmentEmbed::Disabled,
            true,
        )
        .unwrap();
        assert_eq!(
            ir_disabled.attachments[0].missing_reason.as_deref(),
            Some("not_copied")
        );
    }

    /// A bare message carrying `count` attachments, for pairing tests.
    fn msg_with_attachments(ts: i64, count: usize) -> IrMessage {
        IrMessage {
            guid: format!("guid-{ts}"),
            timestamp_unix_ms: ts,
            direction: IrDirection::Incoming,
            service: IrService::IMessage,
            message_kind: IrMessageKind::IMessage,
            sender_handle: Some("+15555550101".into()),
            sender_display_name: None,
            subject: None,
            text: "hi".into(),
            attachments: (0..count)
                .map(|i| IrAttachment {
                    path: None,
                    original_name: Some(format!("a{i}.jpg")),
                    mime_type: None,
                    digest_sha256: None,
                    is_sticker: false,
                    transcription: None,
                    sticker_effect: None,
                    size_bytes: None,
                    missing_reason: None,
                    bytes: None,
                })
                .collect(),
            imessage: None,
            source: None,
        }
    }

    #[test]
    fn unit_sources_land_on_the_attachment_each_was_collected_for() {
        // attachment_loads is positional against the conversation's flattened
        // attachments, so the first load belongs to the first message's
        // attachment and the second to the next message's.
        let first = std::path::PathBuf::from("first.jpg");
        let convo = PendingConversation {
            conversation_type: IrConversationType::Individual,
            group_title: None,
            participants: Vec::new(),
            owner_handle: String::new(),
            owner_display_name: None,
            messages: vec![msg_with_attachments(1000, 1), msg_with_attachments(2000, 1)],
            attachment_loads: vec![
                AttachmentLoad::Path {
                    path: first.clone(),
                    size_hint: Some(11),
                },
                AttachmentLoad::Bytes(b"second".to_vec()),
            ],
        };

        let unit = pending_to_unit("+15555550101".into(), convo, false);

        assert_eq!(unit.attachments.len(), 2);
        assert_eq!(unit.attachments[0].message_index, 0);
        assert_eq!(unit.attachments[0].attachment_index, 0);
        assert_eq!(unit.attachments[0].timestamp_unix_ms, 1000);
        assert_eq!(unit.attachments[0].size_hint, Some(11));
        match &unit.attachments[0].source {
            AttachmentSource::Path(p) => assert_eq!(p, &first),
            other => panic!("first attachment lost its path source: {other:?}"),
        }

        assert_eq!(unit.attachments[1].message_index, 1);
        assert_eq!(unit.attachments[1].timestamp_unix_ms, 2000);
        match &unit.attachments[1].source {
            AttachmentSource::Bytes(b) => assert_eq!(b, b"second"),
            other => panic!("second attachment lost its bytes source: {other:?}"),
        }
    }
}
