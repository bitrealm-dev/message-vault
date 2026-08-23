//! Convert iMazing Messages / WhatsApp rows into the shared conversation
//! structure, then write the chosen output format via [`FormatSink`].

use crate::attachments::{AttachmentIndex, ResolveAttachmentArgs, resolve_attachment_cell};
use crate::parse::{RawRow, SourceKind, discover_csv_files, parse_csv_file};
use anyhow::Result;
use chrono::{FixedOffset, Local, LocalResult, NaiveDateTime, TimeZone};
use contacts::ContactsBook;
use message_csv::{DateRange, format_local_ts, parse_utc_offset, stable_guid};
use message_ir::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, HandleType,
    IrAttachment, IrConversationType, IrDirection, IrMessage, IrMessageKind, IrParticipant,
    IrService, IrSource, PendingAttachment, PendingConversation, PendingMessage, SCHEMA_VERSION,
    owner_sender,
};
use message_ir_format::{ExportTransforms, FormatSink, FormatSinkResult};
use message_vault_io_core::{CancelFlag, ExportReport, OutputFormat, prepare_outputs};
use phone::sanitize_number;
use serde_json::Map;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

const EXPORT_SOURCE: &str = "imazing";
const EXPORT_TOOL: &str = "iMazing";
const EXPORT_TOOL_VERSION: &str = "3.5.5";

/// Read a per-exporter counter from the report's `extra` map (test assertions).
#[cfg(test)]
fn count(report: &ExportReport, key: &str) -> u64 {
    report.extra.get(key).copied().unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportFamily {
    Messages,
    WhatsApp,
}

impl TransportFamily {
    fn from_kind(kind: SourceKind) -> Self {
        match kind {
            SourceKind::Messages => Self::Messages,
            SourceKind::WhatsApp => Self::WhatsApp,
        }
    }

    fn key_prefix(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::WhatsApp => "whatsapp",
        }
    }
}

/// Inputs for [`convert_export`].
pub(crate) struct ConvertExportArgs<'a> {
    pub input: &'a Path,
    pub output: &'a Path,
    pub book: &'a ContactsBook,
    pub timezone: Option<&'a str>,
    pub date_range: &'a DateRange,
    pub transforms: ExportTransforms,
    pub output_format: OutputFormat,
    pub cancel: Option<&'a CancelFlag>,
}

/// Convert iMazing Messages / WhatsApp CSV(s) under `input` using `book` from a contacts VCF/vCard CSV.
///
/// `timezone`: fixed UTC offset (e.g. `UTC-05:00`). When `None`, use the host local zone.
/// When `transforms` copies attachments, media files are copied into `output/attachments/`.
/// When `cancel` is set, cooperative cancellation is checked between CSV files.
///
/// # Errors
///
/// Returns an error when output overlaps input, a CSV cannot be parsed, or the
/// user cancels.
pub(crate) fn convert_export(
    args: ConvertExportArgs<'_>,
) -> Result<(ExportReport, FormatSinkResult)> {
    let ConvertExportArgs {
        input,
        output,
        book,
        timezone,
        date_range,
        transforms,
        output_format,
        cancel,
    } = args;
    let tz = resolve_tz(timezone)?;
    let (inputs, output) = prepare_outputs(&[input.to_path_buf()], output)?;
    let input = &inputs[0];
    let copy_attachments = transforms.copies_attachments();
    let (mut sink, attachments_dir) =
        FormatSink::open_prepared(&output, output_format, transforms)?;
    // Walk the input tree once; per-attachment lookups hit this index.
    let attachment_index = copy_attachments.then(|| AttachmentIndex::build(input));

    let files = discover_csv_files(input)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();
    // Parse-time dedupe state keyed by conversation key (the shared
    // PendingConversation carries document data only).
    let mut seen_keys: BTreeMap<String, HashSet<String>> = BTreeMap::new();

    for discovered in &files {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        match discovered.kind {
            SourceKind::Messages => report.bump("messages_files", 1),
            SourceKind::WhatsApp => report.bump("whatsapp_files", 1),
        }
        let rows = match parse_csv_file(&discovered.path, discovered.kind) {
            Ok(r) => r,
            Err(e) => {
                report
                    .errors
                    .push(format!("{}: {e:#}", discovered.path.display()));
                continue;
            }
        };
        if rows.is_empty() {
            continue;
        }

        let family = TransportFamily::from_kind(discovered.kind);
        let mut by_session: BTreeMap<String, Vec<&RawRow>> = BTreeMap::new();
        for row in &rows {
            by_session
                .entry(row.chat_session.clone())
                .or_default()
                .push(row);
        }

        for (session, session_rows) in by_session {
            let peer = collect_peer_info(book, discovered.kind, &session, &session_rows);
            if peer.unresolved_chat {
                report.bump("unresolved_chat_phone", 1);
            }
            report.bump(
                "unresolved_group_participants",
                peer.unresolved_roster_labels,
            );

            let convo_key = format!("{}|{}", family.key_prefix(), peer.chat_id);
            let convo =
                conversations
                    .entry(convo_key.clone())
                    .or_insert_with(|| PendingConversation {
                        chat_id: peer.chat_id.clone(),
                        display_name: if peer.group {
                            Some(session.clone())
                        } else {
                            None
                        },
                        participant_e164s: Vec::new(),
                        messages: Vec::new(),
                        is_group: peer.group,
                        has_attachments: false,
                        extra: {
                            let mut e = BTreeMap::new();
                            e.insert("source_kind".into(), discovered.kind.as_str().to_string());
                            e
                        },
                    });

            for row in session_rows {
                let Some((secs, date_ms)) = parse_message_date(&row.message_date, &tz) else {
                    report.skipped_invalid_date += 1;
                    continue;
                };
                if !date_range.contains_secs(secs) {
                    report.skipped_out_of_range += 1;
                    continue;
                }
                let is_notification = is_notification(&row.msg_type);
                let is_from_me = !is_notification && is_outgoing(&row.msg_type);
                let (sender_handle, sender_display_name) = resolve_sender(
                    book,
                    row,
                    is_from_me,
                    is_notification,
                    &peer.chat_id,
                    &peer.contact_name,
                );

                let mut attachments = Vec::new();
                let mut attachment_extra: BTreeMap<String, String> = BTreeMap::new();
                if !row.attachment.is_empty() {
                    let csv_parent = discovered.path.parent().unwrap_or_else(|| Path::new("."));
                    let mut copy_failures = 0u64;
                    let cell = resolve_attachment_cell(ResolveAttachmentArgs {
                        csv_name: &row.attachment,
                        attachment_type: &row.attachment_type,
                        csv_parent,
                        index: attachment_index.as_ref(),
                        attachments_dir: &attachments_dir,
                        copy_attachments,
                        message_secs: secs,
                        attachments_saved: &mut report.attachments_saved,
                        copy_failures: &mut copy_failures,
                    });
                    if copy_failures > 0 {
                        report.bump("attachment-copy-failures", copy_failures);
                    }
                    let rel_path = cell.meta.path.clone().unwrap_or_default();
                    attachments.push(PendingAttachment {
                        rel_path: rel_path.clone(),
                        content_type: cell.meta.mime_type.clone().unwrap_or_default(),
                        extension: Path::new(&rel_path)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_string(),
                        digest_sha256: cell.meta.digest_sha256.clone(),
                        name_hint: cell.meta.original_name.clone(),
                    });
                    // iMazing rows carry at most one attachment, so sticker
                    // metadata fits on the message.
                    attachment_extra.insert(
                        "is_sticker".into(),
                        if cell.is_sticker { "true" } else { "false" }.into(),
                    );
                    attachment_extra.insert(
                        "transcription".into(),
                        cell.transcription.unwrap_or_default(),
                    );
                    attachment_extra.insert(
                        "sticker_effect".into(),
                        cell.sticker_effect.unwrap_or_default(),
                    );
                }

                // sender_id distinguishes same-second same-text rows from
                // different senders in group chats.
                let dedupe_key = format!(
                    "{}|{}|{}|{}|{}|{}",
                    peer.chat_id,
                    secs,
                    if is_from_me { "1" } else { "0" },
                    row.sender_id,
                    row.text,
                    row.attachment
                );
                if !seen_keys
                    .entry(convo_key.clone())
                    .or_default()
                    .insert(dedupe_key)
                {
                    report.duplicates_dropped += 1;
                    continue;
                }

                let service = if row.service.trim().is_empty() {
                    match discovered.kind {
                        SourceKind::WhatsApp => "WhatsApp".to_string(),
                        SourceKind::Messages => "SMS".to_string(),
                    }
                } else {
                    row.service.clone()
                };

                convo.messages.push(PendingMessage {
                    sort_key: secs,
                    is_from_me,
                    sender_handle,
                    sender_display_name: if sender_display_name.is_empty() {
                        None
                    } else {
                        Some(sender_display_name)
                    },
                    text: row.text.clone(),
                    attachments,
                    extra: {
                        let mut e = BTreeMap::new();
                        e.insert(
                            "is_notification".into(),
                            if is_notification { "true" } else { "false" }.into(),
                        );
                        e.insert("subject".into(), row.subject.clone());
                        e.insert("contact_name".into(), peer.contact_name.clone());
                        e.insert("date_ms".into(), date_ms);
                        e.insert("service".into(), service);
                        e.insert("imazing_status".into(), row.status.clone());
                        e.insert("imazing_type".into(), row.msg_type.clone());
                        e.insert("reactions".into(), row.reactions.clone());
                        e.insert("replying_to".into(), row.replying_to.clone());
                        e.insert("forwarded".into(), row.forwarded.clone());
                        e.insert("attachment_info".into(), row.attachment_info.clone());
                        e.insert("delivered_date".into(), row.delivered_date.clone());
                        e.insert("read_date".into(), row.read_date.clone());
                        e.insert("edited_date".into(), row.edited_date.clone());
                        e.insert("deleted_date".into(), row.deleted_date.clone());
                        e.insert("sent_date".into(), row.sent_date.clone());
                        e.extend(attachment_extra);
                        e
                    },
                });
            }
        }
    }

    for (key, mut convo) in conversations {
        let chat_id = key
            .split_once('|')
            .map(|(_, id)| id.to_string())
            .unwrap_or_else(|| key.clone());
        if !prepare_conversation(&mut convo, &mut report) {
            continue;
        }
        let doc = pending_to_document(&chat_id, &convo, &mut report)?;
        sink.write_document(doc)?;
        report.conversations += 1;
    }
    let sink_result = sink.finish()?;

    Ok((report, sink_result))
}

#[derive(Debug)]
struct PeerInfo {
    chat_id: String,
    contact_name: String,
    group: bool,
    unresolved_chat: bool,
    unresolved_roster_labels: u64,
}

fn collect_peer_info(
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
            handles
                .insert(phone::normalize_guarded(sid, phone::PhoneRegion::for_raw(sid)).normalized);
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
                handles.insert(
                    phone::normalize_guarded(label, phone::PhoneRegion::for_raw(label)).normalized,
                );
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
enum TzMode {
    Local,
    Fixed(FixedOffset),
}

fn resolve_tz(timezone: Option<&str>) -> Result<TzMode> {
    match timezone.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(TzMode::Local),
        Some(name) => {
            let offset = parse_utc_offset(name).map_err(anyhow::Error::msg)?;
            Ok(TzMode::Fixed(offset))
        }
    }
}

fn parse_message_date(raw: &str, tz: &TzMode) -> Option<(i64, String)> {
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

fn is_outgoing(msg_type: &str) -> bool {
    matches!(
        msg_type.trim().to_ascii_lowercase().as_str(),
        "outgoing" | "sent"
    )
}

fn is_notification(msg_type: &str) -> bool {
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
                let e164 = phone::normalize_guarded(
                    &text[start..i],
                    phone::PhoneRegion::for_raw(&text[start..i]),
                )
                .normalized;
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

/// Returns `(chat_identifier, contact_name, unresolved_phone)`.
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
        let contact_name = if let Some(digits) = sanitize_number(handle) {
            // The book keys entries by its own US-digit form.
            book.lookup_name_by_handle(
                &phone::normalize_guarded(&digits, phone::PhoneRegion::Usa).normalized,
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
    if let Some(digits) = sanitize_number(session) {
        // Format as E.164 when unambiguous. Otherwise keep digits as-is. Never
        // invent `+0…`. The contacts book looks up its own US-digit form.
        let handle =
            phone::normalize_guarded(session, phone::PhoneRegion::for_raw(session)).normalized;
        let book_form = phone::normalize_guarded(&digits, phone::PhoneRegion::Usa).normalized;
        let name = book
            .lookup_name_by_handle(&book_form, HandleType::Phone)
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

fn resolve_sender(
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
            phone::normalize_guarded(&row.sender_id, phone::PhoneRegion::for_raw(&row.sender_id))
                .normalized
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
        handle =
            phone::normalize_guarded(&row.sender_id, phone::PhoneRegion::for_raw(&row.sender_id))
                .normalized;
    } else if !chat_id.contains('@')
        && (chat_id.starts_with('+') || sanitize_number(chat_id).is_some())
    {
        handle = phone::normalize_guarded(chat_id, phone::PhoneRegion::for_raw(chat_id)).normalized;
    } else if !row.sender_name.is_empty()
        && let Some((e164, _)) = book.lookup_handle_by_name(&row.sender_name)
    {
        handle = e164;
    }

    let mut display = row.sender_name.trim().to_string();
    if display.is_empty()
        && let Some(digits) = sanitize_number(&handle)
    {
        // The book keys entries by its own US-digit form.
        display = book
            .lookup_name_by_handle(
                &phone::normalize_guarded(&digits, phone::PhoneRegion::Usa).normalized,
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

fn prepare_conversation(convo: &mut PendingConversation, report: &mut ExportReport) -> bool {
    if convo.messages.is_empty() {
        return false;
    }
    convo.messages.sort_by_key(|m| m.sort_key);
    message_vault_io_core::prune_and_finish_conversation(convo, report, |k| k)
}

/// iMazing identifiers are E.164 phones, emails, or (rarely) name stems;
/// infer the type from the handle shape.
fn handle_type_for(handle: &str) -> HandleType {
    if handle.contains('@') {
        HandleType::Email
    } else {
        HandleType::Phone
    }
}

fn imazing_peers(is_group: bool, chat_id: &str) -> Vec<String> {
    if is_group {
        chat_id
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    }
}

fn imazing_packaging_stem_suffix(source_kind: &str) -> Option<String> {
    if source_kind == "whatsapp" {
        Some("__whatsapp".into())
    } else {
        None
    }
}

/// Materials for [`stable_guid`]: prefer content digests so a later run that
/// finds and copies a previously missing file does not change the message id.
fn attachment_guid_materials(attachments: &[PendingAttachment]) -> Vec<String> {
    let mut digests: Vec<String> = attachments
        .iter()
        .map(|a| {
            a.digest_sha256
                .clone()
                .unwrap_or_else(|| a.rel_path.clone())
        })
        .collect();
    digests.sort();
    digests
}

/// First non-empty `contact_name` extra on a message in this conversation.
fn first_contact_name(convo: &PendingConversation) -> Option<String> {
    convo
        .messages
        .iter()
        .map(|m| m.extra_str("contact_name").trim())
        .find(|n| !n.is_empty())
        .map(str::to_string)
}

/// Map a staged attachment onto the shared [`IrAttachment`] shape.
fn pending_attachment_to_ir(a: &PendingAttachment, msg: &PendingMessage) -> IrAttachment {
    IrAttachment {
        path: Some(a.rel_path.clone()),
        original_name: a.name_hint.clone(),
        mime_type: a.mime_type(),
        digest_sha256: a.digest_sha256.clone(),
        is_sticker: msg.extra_flag("is_sticker"),
        transcription: msg.extra_opt("transcription"),
        sticker_effect: msg.extra_opt("sticker_effect"),
        size_bytes: None,
        missing_reason: None,
        bytes: None,
    }
}

/// Build a [`ConversationDocument`] from one pending conversation.
///
/// Currently always returns `Ok`. The `Result` matches the other exporters.
fn pending_to_document(
    chat_id: &str,
    convo: &PendingConversation,
    report: &mut ExportReport,
) -> Result<ConversationDocument> {
    let peers = imazing_peers(convo.is_group, chat_id);
    let mut participants: Vec<IrParticipant> = peers
        .iter()
        .map(|h| IrParticipant {
            handle: h.clone(),
            display_name: None,
            handle_type: Some(handle_type_for(h)),
        })
        .collect();
    if participants.is_empty() && !convo.is_group && !chat_id.is_empty() {
        participants.push(IrParticipant {
            handle: chat_id.to_string(),
            display_name: first_contact_name(convo),
            handle_type: Some(handle_type_for(chat_id)),
        });
    }
    let packaging_stem_suffix = imazing_packaging_stem_suffix(convo.extra_str("source_kind"));
    // Match previous CSV/mail stem: conversation_filename gets None for title
    // (session string is not a real group title).
    let session_title = convo.display_name.as_deref().unwrap_or("");

    let owner_meta = ExportMeta {
        source: String::new(),
        tool: String::new(),
        tool_version: String::new(),
        owner_handle: None,
        owner_display_name: None,
    };
    let export = message_vault_io_core::export_meta(
        EXPORT_SOURCE,
        EXPORT_TOOL,
        EXPORT_TOOL_VERSION,
        &owner_meta,
    );
    let (owner_handle, owner_display) = owner_sender(&export);

    let mut messages = Vec::with_capacity(convo.messages.len());
    for msg in &convo.messages {
        let is_notification = msg.extra_flag("is_notification");
        if is_notification {
            report.bump("notifications", 1);
        } else if msg.is_from_me {
            report.sent += 1;
        } else {
            report.received += 1;
        }
        report.messages += 1;

        let (ts_local, _, _) = format_local_ts(msg.sort_key).expect("timestamp validated above");
        let digests = attachment_guid_materials(&msg.attachments);
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &digests);
        let timestamp_unix_ms = msg
            .extra_str("date_ms")
            .parse::<i64>()
            .unwrap_or_else(|_| msg.sort_key.saturating_mul(1000));
        let attachments: Vec<IrAttachment> = msg
            .attachments
            .iter()
            .map(|a| pending_attachment_to_ir(a, msg))
            .collect();
        let message_kind = if msg.attachments.is_empty() {
            IrMessageKind::Sms
        } else {
            IrMessageKind::Mms
        };

        let mut fields = Map::new();
        if !session_title.is_empty() {
            fields.insert(
                "group_title".into(),
                serde_json::Value::String(session_title.to_string()),
            );
        }
        for key in [
            "imazing_status",
            "imazing_type",
            "reactions",
            "replying_to",
            "forwarded",
            "attachment_info",
            "delivered_date",
            "read_date",
            "edited_date",
            "deleted_date",
            "sent_date",
        ] {
            let val = msg.extra_str(key);
            if !val.is_empty() {
                fields.insert(key.into(), serde_json::Value::String(val.to_string()));
            }
        }
        let source = IrSource {
            android_type: None,
            fields,
        }
        .into_option();

        let is_outgoing = msg.is_from_me && !is_notification;
        let (sender_handle, sender_display_name) = if is_outgoing {
            (owner_handle.clone(), owner_display.clone())
        } else {
            (
                if msg.sender_handle.is_empty() {
                    None
                } else {
                    Some(msg.sender_handle.clone())
                },
                msg.sender_display_name.clone(),
            )
        };

        messages.push(IrMessage {
            guid,
            timestamp_unix_ms,
            direction: if is_outgoing {
                IrDirection::Outgoing
            } else {
                IrDirection::Incoming
            },
            service: IrService::parse(msg.extra_str("service")),
            message_kind,
            sender_handle,
            sender_display_name,
            subject: msg.extra_opt("subject"),
            text: msg.text.clone(),
            attachments,
            imessage: None,
            source,
        });
    }

    Ok(ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export,
        conversation: ConversationMeta {
            chat_identifier: chat_id.to_string(),
            conversation_type: if convo.is_group {
                IrConversationType::Group
            } else {
                IrConversationType::Individual
            },
            // None matches previous CSV/mail stem (session string is not a real group title).
            group_title: None,
            participants,
            stats: ConversationStats::default(),
        },
        messages,
        packaging_stem_suffix,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;

    fn write(dir: &tempfile::TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = File::create(&path).unwrap();
        write!(f, "{body}").unwrap();
        path
    }

    fn convert(
        input: &std::path::Path,
        output: &std::path::Path,
        book: &ContactsBook,
    ) -> Result<(ExportReport, FormatSinkResult)> {
        convert_export(ConvertExportArgs {
            input,
            output,
            book,
            timezone: Some("UTC"),
            date_range: &DateRange::default(),
            transforms: ExportTransforms::none(),
            output_format: OutputFormat::Csv,
            cancel: None,
        })
    }

    fn pending_att(rel_path: &str, digest: Option<&str>) -> PendingAttachment {
        PendingAttachment {
            rel_path: rel_path.into(),
            content_type: String::new(),
            extension: "jpg".into(),
            digest_sha256: digest.map(str::to_string),
            name_hint: None,
        }
    }

    #[test]
    fn message_guid_prefers_digest_over_rel_path() {
        // Same digest, different relative paths → same GUID material.
        let a = pending_att("attachments/old_name.jpg", Some("abc123"));
        let b = pending_att("attachments/new_name.jpg", Some("abc123"));
        assert_eq!(
            attachment_guid_materials(&[a]),
            attachment_guid_materials(&[b])
        );

        // Digest present wins over path; path alone differs from digest.
        let with_digest = pending_att("attachments/x.jpg", Some("deadbeef"));
        let path_only = pending_att("attachments/x.jpg", None);
        assert_ne!(
            attachment_guid_materials(&[with_digest]),
            attachment_guid_materials(&[path_only])
        );

        // Order of attachments must not change the sorted material list.
        let mixed = [
            pending_att("a.jpg", Some("bb")),
            pending_att("b.jpg", Some("aa")),
        ];
        assert_eq!(
            attachment_guid_materials(&mixed),
            vec!["aa".to_string(), "bb".to_string()]
        );
    }

    #[test]
    fn name_session_resolves_via_contacts() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages - Bob.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob McRoy,2020-01-01 12:00:00,SMS,Incoming,+13212462167,Bob McRoy,Read,,,Hello,,,\n\
Bob McRoy,2020-01-01 12:01:00,SMS,Outgoing,,,Read,,,Hi,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Bob,,McRoy,+13212462167,\n",
        );
        let book = ContactsBook::load_vcard_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book).unwrap();
        assert_eq!(report.conversations, 1);
        assert_eq!(count(&report, "unresolved_chat_phone"), 0);
        assert_eq!(report.messages, 2);
        let csv_path = out.join("+13212462167.csv");
        let body = fs::read_to_string(&csv_path).unwrap();
        assert!(body.contains("Bob McRoy"));
        assert!(body.contains("imazing"));
        assert!(body.contains("iMazing"));
        assert!(body.contains("imazing_type"));
    }

    #[test]
    fn name_without_phone_still_writes() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages - Mystery.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Mystery Person,2020-01-01 12:00:00,SMS,Incoming,,,Read,,,Hello,,,\n\
Mystery Person,2020-01-01 12:01:00,SMS,Outgoing,,,Read,,,Hi,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Other,,Person,+15555550999,\n",
        );
        let book = ContactsBook::load_vcard_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book).unwrap();
        assert!(count(&report, "unresolved_chat_phone") >= 1);
        assert_eq!(report.conversations, 1);
        assert!(out.join("Mystery_Person.csv").is_file());
    }

    #[test]
    fn drops_exact_duplicate_rows() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob,2020-01-01 12:00:00,SMS,Outgoing,,,Read,,,Same,,,\n\
Bob,2020-01-01 12:00:00,SMS,Outgoing,,,Read,,,Same,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Bob,,,+15555550100,\n",
        );
        let book = ContactsBook::load_vcard_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book).unwrap();
        assert_eq!(report.messages, 1);
        assert_eq!(report.duplicates_dropped, 1);
    }

    #[test]
    fn keeps_same_text_different_attachment() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob,2020-01-01 12:00:00,SMS,Incoming,+15555550100,Bob,Read,,,Photo,,a.jpg,Image\n\
Bob,2020-01-01 12:00:00,SMS,Incoming,+15555550100,Bob,Read,,,Photo,,b.jpg,Image\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Bob,,,+15555550100,\n",
        );
        let book = ContactsBook::load_vcard_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book).unwrap();
        assert_eq!(report.messages, 2);
        assert_eq!(report.duplicates_dropped, 0);
    }

    #[test]
    fn silent_group_member_resolved_via_contacts() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Alice Example & Bob Example & Carol Silent,2020-01-01 12:00:00,iMessage,Incoming,+15555550111,Alice Example,Read,,,Hi,,,\n\
Alice Example & Bob Example & Carol Silent,2020-01-01 12:01:00,iMessage,Incoming,+15555550122,Bob Example,Read,,,Hey,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Alice,,Example,+15555550111,\n\
Bob,,Example,+15555550122,\n\
Carol,,Silent,+15555550133,\n",
        );
        let book = ContactsBook::load_vcard_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book).unwrap();
        assert_eq!(report.conversations, 1);
        assert_eq!(count(&report, "unresolved_group_participants"), 0);
        let body = fs::read_to_string(out.join("group_+15555550111_+15555550122_+15555550133.csv"))
            .unwrap();
        assert!(body.contains("+15555550133") || body.contains("15555550133"));
        assert!(body.contains("group"));
    }

    #[test]
    fn silent_group_member_without_contacts_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Alice Example & Bob Example & Carol Silent,2020-01-01 12:00:00,iMessage,Incoming,+15555550111,Alice Example,Read,,,Hi,,,\n\
Alice Example & Bob Example & Carol Silent,2020-01-01 12:01:00,iMessage,Incoming,+15555550122,Bob Example,Read,,,Hey,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Alice,,Example,+15555550111,\n\
Bob,,Example,+15555550122,\n",
        );
        let book = ContactsBook::load_vcard_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book).unwrap();
        assert_eq!(report.conversations, 1);
        assert_eq!(count(&report, "unresolved_group_participants"), 1);
    }

    #[test]
    fn whatsapp_and_messages_same_peer_stay_separate() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages/chat/Messages - Bob.csv",
            "Chat Session,Message Date,Delivered Date,Read Date,Edited Date,Deleted Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob,2020-01-01 12:00:00,,,,,SMS,Incoming,+15555550100,Bob,Read,,,SMS hi,,,\n",
        );
        write(
            &dir,
            "WhatsApp/chat/WhatsApp - Bob.csv",
            "Chat Session,Message Date,Sent Date,Type,Sender ID,Sender Name,Status,Forwarded,Replying to,Text,Reactions,Attachment,Attachment type,Attachment info\n\
Bob,2020-01-01 12:05:00,,Incoming,+15555550100,Bob,Read,,,WA hi,,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts/All/Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Bob,,,+15555550100,\n",
        );
        let book = ContactsBook::load_vcard_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book).unwrap();
        assert_eq!(report.conversations, 2);
        assert_eq!(count(&report, "messages_files"), 1);
        assert_eq!(count(&report, "whatsapp_files"), 1);
        assert!(out.join("+15555550100.csv").is_file());
        assert!(out.join("+15555550100__whatsapp.csv").is_file());
        let wa = fs::read_to_string(out.join("+15555550100__whatsapp.csv")).unwrap();
        assert!(wa.contains("whatsapp"));
    }

    #[test]
    fn rejects_unknown_timezone() {
        let err = resolve_tz(Some("America/New_York")).unwrap_err();
        assert!(err.to_string().contains("UTC"));
    }

    #[test]
    fn copies_attachment_by_suffix_match() {
        let dir = tempfile::tempdir().unwrap();
        let chat = dir.path().join("chat");
        fs::create_dir_all(&chat).unwrap();
        let csv = chat.join("Messages - Bob.csv");
        fs::write(
            &csv,
            "Chat Session,Message Date,Delivered Date,Read Date,Edited Date,Deleted Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob McRoy,2020-01-01 12:00:00,,,,,SMS,Incoming,+15555550100,Bob,Read,,,Hi,,image000000.jpg,Image\n",
        )
        .unwrap();
        fs::write(chat.join("ABC123_image000000.jpg"), b"fake-jpeg-bytes").unwrap();
        let book = ContactsBook::empty();
        let out = dir.path().join("out");
        let (report, _) = convert(&chat, &out, &book).unwrap();
        assert_eq!(report.attachments_saved, 1);
        assert_eq!(report.messages, 1);
        let att_dir = out.join("attachments");
        assert!(att_dir.is_dir());
        let count = fs::read_dir(&att_dir).unwrap().count();
        assert_eq!(count, 1);
        let body = fs::read_to_string(out.join("+15555550100.csv")).unwrap();
        assert!(body.contains("attachments/"));
    }

    #[test]
    fn email_sender_with_digits_stays_email() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages - Bob.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob McRoy,2020-01-01 12:00:00,iMessage,Incoming,bob2024@gmail.com,Bob McRoy,Read,,,Hello,,,\n\
Bob McRoy,2020-01-01 12:01:00,iMessage,Outgoing,,,Read,,,Hi,,,\n",
        );
        let book = ContactsBook::empty();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book).unwrap();
        assert_eq!(report.conversations, 1);
        assert_eq!(report.messages, 2);
        // Chat id stays the full email; the CSV filename stems `@` to `_`.
        let csv_path = out.join("bob2024_gmail_com.csv");
        assert!(
            csv_path.is_file(),
            "expected email chat file; got {}",
            out.read_dir()
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let body = fs::read_to_string(csv_path).unwrap();
        assert!(body.contains("bob2024@gmail.com"));
        assert!(!body.contains("12024"));
    }

    #[test]
    fn same_text_same_second_different_senders_kept() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Group Chat,2020-01-01 12:00:00,iMessage,Incoming,+15555550111,Alice,Read,,,Same,,,\n\
Group Chat,2020-01-01 12:00:00,iMessage,Incoming,+15555550122,Bob,Read,,,Same,,,\n",
        );
        let book = ContactsBook::empty();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book).unwrap();
        assert_eq!(report.messages, 2);
        assert_eq!(report.duplicates_dropped, 0);
    }

    #[test]
    fn output_equals_input_bails_before_cleaning() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages - Bob.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob,2020-01-01 12:00:00,SMS,Incoming,+13212462167,Bob,Read,,,Hello,,,\n",
        );
        let book = ContactsBook::empty();
        let err = convert(dir.path(), dir.path(), &book).unwrap_err();
        assert!(err.to_string().contains("must not be the same as"), "{err}");
        // Source CSV must survive the refused run.
        assert!(dir.path().join("Messages - Bob.csv").is_file());
    }
}
