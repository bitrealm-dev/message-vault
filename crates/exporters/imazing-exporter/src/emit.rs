//! Convert iMazing Messages / WhatsApp rows → common message → packaging via FormatSink.

use crate::attachments::{AttachmentIndex, resolve_attachment_cell};
use crate::parse::{RawRow, SourceKind, discover_csv_files, parse_csv_file};
use anyhow::{Context, Result, bail};
use chrono::{FixedOffset, Local, LocalResult, NaiveDateTime, TimeZone};
use contacts::ContactsBook;
use message_csv::{AttachmentCell, DateRange, format_local_ts, parse_utc_offset, stable_guid};
use message_vault_io_core::{CancelFlag, ExportReport, OutputFormat};
use message_ir::{
    ConversationDocument,
    ConversationMeta,
    ConversationStats,
    ExportMeta,
    IrAttachment,
    IrConversationType,
    IrDirection,
    IrMessage,
    IrMessageKind,
    IrParticipant,
    IrService,
    IrSource,
    SCHEMA_VERSION,
    owner_sender,
};
use message_ir_format::{ExportTransforms, FormatSink, FormatSinkResult};
use phone::{sanitize_number, to_e164};
use serde_json::Map;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

const EXPORT_SOURCE: &str = "imazing";
const EXPORT_TOOL: &str = "iMazing";
const EXPORT_TOOL_VERSION: &str = "3.5.5";

/// Bump a per-exporter counter in the report's `extra` map.
fn bump(report: &mut ExportReport, key: &str, by: u64) {
    *report.extra.entry(key.to_string()).or_insert(0) += by;
}

/// Read a per-exporter counter from the report's `extra` map (test assertions).
#[cfg(test)]
fn count(report: &ExportReport, key: &str) -> u64 {
    report.extra.get(key).copied().unwrap_or(0)
}

#[derive(Debug)]
struct PendingMessage {
    sort_key: i64,
    is_from_me: bool,
    is_notification: bool,
    sender_handle: String,
    sender_display_name: String,
    subject: String,
    text: String,
    contact_name: String,
    date_ms: String,
    service: String,
    status: String,
    msg_type: String,
    reactions: String,
    replying_to: String,
    forwarded: String,
    attachment_info: String,
    delivered_date: String,
    read_date: String,
    edited_date: String,
    deleted_date: String,
    sent_date: String,
    attachments: Vec<AttachmentCell>,
}

#[derive(Debug, Default)]
struct PendingConversation {
    conversation_type: String,
    group_title: String,
    source_kind: Option<SourceKind>,
    messages: Vec<PendingMessage>,
    seen: HashSet<String>,
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

/// Convert iMazing Messages / WhatsApp CSV(s) under `input` using `book` from a contacts VCF/vCard CSV.
///
/// `timezone`: fixed UTC offset (e.g. `UTC-05:00`). When `None`, use the host local zone.
/// When `transforms` copies attachments, media files are copied into `output/attachments/`.
/// When `cancel` is set, cooperative cancellation is checked between CSV files.
pub(crate) fn convert_export(
    input: &Path,
    output: &Path,
    book: &ContactsBook,
    timezone: Option<&str>,
    date_range: &DateRange,
    transforms: ExportTransforms,
    output_format: OutputFormat,
    cancel: Option<&CancelFlag>,
) -> Result<(ExportReport, FormatSinkResult)> {
    let tz = resolve_tz(timezone)?;
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    // Canonicalize so relative inputs resolve and `parent()` below is absolute,
    // and so output/input identity is checked on resolved paths.
    let input = fs::canonicalize(input).with_context(|| format!("resolve {}", input.display()))?;
    let output = fs::canonicalize(output).with_context(|| format!("resolve {}", output.display()))?;
    if output == input || input.starts_with(&output) {
        bail!(
            "output {} must not be the same as, or contain, the input {}",
            output.display(),
            input.display()
        );
    }
    let copy_attachments = transforms.copies_attachments();
    let (mut sink, attachments_dir) =
        FormatSink::open_prepared(&output, output_format, transforms)?;
    // Walk the input tree once; per-attachment lookups hit this index.
    let attachment_index = copy_attachments.then(|| AttachmentIndex::build(&input));

    let files = discover_csv_files(&input)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    for discovered in &files {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        match discovered.kind {
            SourceKind::Messages => bump(&mut report, "messages_files", 1),
            SourceKind::WhatsApp => bump(&mut report, "whatsapp_files", 1),
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
                bump(&mut report, "unresolved_chat_phone", 1);
            }
            bump(&mut report, "unresolved_group_participants", peer.unresolved_roster_labels);

            let convo_key = format!("{}|{}", family.key_prefix(), peer.chat_id);
            let convo = conversations
                .entry(convo_key)
                .or_insert_with(|| PendingConversation {
                    conversation_type: if peer.group {
                        "group".into()
                    } else {
                        "individual".into()
                    },
                    group_title: if peer.group {
                        session.clone()
                    } else {
                        String::new()
                    },
                    source_kind: Some(discovered.kind),
                    messages: Vec::new(),
                    seen: HashSet::new(),
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
                if !row.attachment.is_empty() {
                    let csv_parent = discovered.path.parent().unwrap_or_else(|| Path::new("."));
                    attachments.push(resolve_attachment_cell(
                        &row.attachment,
                        &row.attachment_type,
                        csv_parent,
                        attachment_index.as_ref(),
                        &attachments_dir,
                        copy_attachments,
                        secs,
                        &mut report.attachments_saved,
                    ));
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
                if !convo.seen.insert(dedupe_key) {
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
                    is_notification,
                    sender_handle,
                    sender_display_name,
                    subject: row.subject.clone(),
                    text: row.text.clone(),
                    contact_name: peer.contact_name.clone(),
                    date_ms,
                    service,
                    status: row.status.clone(),
                    msg_type: row.msg_type.clone(),
                    reactions: row.reactions.clone(),
                    replying_to: row.replying_to.clone(),
                    forwarded: row.forwarded.clone(),
                    attachment_info: row.attachment_info.clone(),
                    delivered_date: row.delivered_date.clone(),
                    read_date: row.read_date.clone(),
                    edited_date: row.edited_date.clone(),
                    deleted_date: row.deleted_date.clone(),
                    sent_date: row.sent_date.clone(),
                    attachments,
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
        } else if let Some(digits) = sanitize_number(sid) {
            handles.insert(to_e164(&digits));
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
            if let Some(digits) = sanitize_number(label) {
                handles.insert(to_e164(&digits));
                continue;
            }
            if let Some(e164) = book.lookup_e164_by_name(label) {
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
            if i > start + 1 {
                if let Some(digits) = sanitize_number(&text[start..i]) {
                    let e164 = to_e164(&digits);
                    if !out.contains(&e164) {
                        out.push(e164);
                    }
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
        return (message_vault_io_core::name_stem(session), session.trim().to_string(), true);
    }

    if let Some(handle) = peer_handles.first() {
        let contact_name = if let Some(digits) = sanitize_number(handle) {
            book.lookup_name_by_phone(&digits).unwrap_or("").to_string()
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
        let e164 = to_e164(&digits);
        let name = book.lookup_name_by_phone(&digits).unwrap_or("").to_string();
        return (e164, name, false);
    }
    if let Some(e164) = book.lookup_e164_by_name(session) {
        return (e164, session.to_string(), false);
    }
    (message_vault_io_core::name_stem(session), session.to_string(), true)
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
        } else if let Some(digits) = sanitize_number(&row.sender_id) {
            to_e164(&digits)
        } else {
            String::new()
        };
        return (handle, row.sender_name.trim().to_string());
    }

    let mut handle = String::new();
    if row.sender_id.contains('@') {
        handle = row.sender_id.trim().to_string();
    } else if let Some(digits) = sanitize_number(&row.sender_id) {
        handle = to_e164(&digits);
    } else if !chat_id.contains('@')
        && (chat_id.starts_with('+') || sanitize_number(chat_id).is_some())
    {
        handle = if chat_id.starts_with('+') {
            chat_id.to_string()
        } else {
            sanitize_number(chat_id)
                .map(|d| to_e164(&d))
                .unwrap_or_default()
        };
    } else if !row.sender_name.is_empty() {
        if let Some(e164) = book.lookup_e164_by_name(&row.sender_name) {
            handle = e164;
        }
    }

    let mut display = row.sender_name.trim().to_string();
    if display.is_empty() {
        if let Some(digits) = sanitize_number(&handle) {
            display = book.lookup_name_by_phone(&digits).unwrap_or("").to_string();
        }
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
    convo.messages.retain(|m| {
        if format_local_ts(m.sort_key).is_some() {
            true
        } else {
            report.skipped_invalid_date += 1;
            false
        }
    });
    !convo.messages.is_empty()
}

fn imazing_peers(conversation_type: &str, chat_id: &str) -> Vec<String> {
    if conversation_type.eq_ignore_ascii_case("group") {
        chat_id
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    }
}

fn imazing_packaging_stem_suffix(source_kind: Option<SourceKind>) -> Option<String> {
    if source_kind == Some(SourceKind::WhatsApp) {
        Some("__whatsapp".into())
    } else {
        None
    }
}

fn pending_to_document(
    chat_id: &str,
    convo: &PendingConversation,
    report: &mut ExportReport,
) -> Result<ConversationDocument> {
    let peers = imazing_peers(&convo.conversation_type, chat_id);
    let mut participants: Vec<IrParticipant> = peers
        .iter()
        .map(|h| IrParticipant {
            handle: h.clone(),
            display_name: None,
        })
        .collect();
    if participants.is_empty() && convo.conversation_type == "individual" && !chat_id.is_empty() {
        participants.push(IrParticipant {
            handle: chat_id.to_string(),
            display_name: convo
                .messages
                .iter()
                .map(|m| m.contact_name.trim())
                .find(|n| !n.is_empty())
                .map(str::to_string),
        });
    }
    let packaging_stem_suffix = imazing_packaging_stem_suffix(convo.source_kind);
    // Match previous CSV/mail stem: conversation_filename gets None for title
    // (session string is not a real group title).
    let session_title = convo.group_title.trim();

    let export = ExportMeta {
        source: EXPORT_SOURCE.into(),
        tool: EXPORT_TOOL.into(),
        tool_version: EXPORT_TOOL_VERSION.into(),
        owner_handle: None,
        owner_display_name: None,
    };
    let (owner_handle, owner_display) = owner_sender(&export);

    let mut messages = Vec::with_capacity(convo.messages.len());
    for msg in &convo.messages {
        if msg.is_notification {
            bump(report, "notifications", 1);
        } else if msg.is_from_me {
            report.sent += 1;
        } else {
            report.received += 1;
        }
        report.messages += 1;

        let (ts_local, _, _) = format_local_ts(msg.sort_key).expect("timestamp validated above");
        let digests: Vec<String> = msg
            .attachments
            .iter()
            .filter_map(|a| a.path.clone())
            .collect();
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &digests);
        let timestamp_unix_ms = msg
            .date_ms
            .parse::<i64>()
            .unwrap_or_else(|_| msg.sort_key.saturating_mul(1000));
        let attachments: Vec<IrAttachment> = msg
            .attachments
            .iter()
            .map(|a| IrAttachment {
                path: a.path.clone(),
                original_name: a.original_name.clone(),
                mime_type: a.mime_type.clone(),
                digest_sha256: None,
                is_sticker: a.is_sticker,
                transcription: a.transcription.clone(),
                sticker_effect: a.sticker_effect.clone(),
                size_bytes: None,
                bytes: None,
            })
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
        for (key, val) in [
            ("imazing_status", msg.status.as_str()),
            ("imazing_type", msg.msg_type.as_str()),
            ("reactions", msg.reactions.as_str()),
            ("replying_to", msg.replying_to.as_str()),
            ("forwarded", msg.forwarded.as_str()),
            ("attachment_info", msg.attachment_info.as_str()),
            ("delivered_date", msg.delivered_date.as_str()),
            ("read_date", msg.read_date.as_str()),
            ("edited_date", msg.edited_date.as_str()),
            ("deleted_date", msg.deleted_date.as_str()),
            ("sent_date", msg.sent_date.as_str()),
        ] {
            if !val.is_empty() {
                fields.insert(key.into(), serde_json::Value::String(val.to_string()));
            }
        }
        let source = IrSource {
            android_type: None,
            fields,
        }
        .into_option();

        let is_outgoing = msg.is_from_me && !msg.is_notification;
        let (sender_handle, sender_display_name) = if is_outgoing {
            (owner_handle.clone(), owner_display.clone())
        } else {
            (
                if msg.sender_handle.is_empty() {
                    None
                } else {
                    Some(msg.sender_handle.clone())
                },
                if msg.sender_display_name.is_empty() {
                    None
                } else {
                    Some(msg.sender_display_name.clone())
                },
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
            service: IrService::parse(&msg.service),
            message_kind,
            sender_handle,
            sender_display_name,
            subject: if msg.subject.is_empty() {
                None
            } else {
                Some(msg.subject.clone())
            },
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
            conversation_type: IrConversationType::parse(&convo.conversation_type),
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
    use std::fs::File;
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
        let (report, _) = convert_export(
            dir.path(),
            &out,
            &book,
            Some("UTC"),
            &DateRange::default(),
            ExportTransforms::none(),
            OutputFormat::Csv,
            None,
        )
        .unwrap();
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
        let (report, _) = convert_export(
            dir.path(),
            &out,
            &book,
            Some("UTC"),
            &DateRange::default(),
            ExportTransforms::none(),
            OutputFormat::Csv,
            None,
        )
        .unwrap();
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
        let (report, _) = convert_export(
            dir.path(),
            &out,
            &book,
            Some("UTC"),
            &DateRange::default(),
            ExportTransforms::none(),
            OutputFormat::Csv,
            None,
        )
        .unwrap();
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
        let (report, _) = convert_export(
            dir.path(),
            &out,
            &book,
            Some("UTC"),
            &DateRange::default(),
            ExportTransforms::none(),
            OutputFormat::Csv,
            None,
        )
        .unwrap();
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
        let (report, _) = convert_export(
            dir.path(),
            &out,
            &book,
            Some("UTC"),
            &DateRange::default(),
            ExportTransforms::none(),
            OutputFormat::Csv,
            None,
        )
        .unwrap();
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
        let (report, _) = convert_export(
            dir.path(),
            &out,
            &book,
            Some("UTC"),
            &DateRange::default(),
            ExportTransforms::none(),
            OutputFormat::Csv,
            None,
        )
        .unwrap();
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
        let (report, _) = convert_export(
            dir.path(),
            &out,
            &book,
            Some("UTC"),
            &DateRange::default(),
            ExportTransforms::none(),
            OutputFormat::Csv,
            None,
        )
        .unwrap();
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
        let (report, _) = convert_export(
            &chat,
            &out,
            &book,
            Some("UTC"),
            &DateRange::default(),
            ExportTransforms::none(),
            OutputFormat::Csv,
            None,
        )
        .unwrap();
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
        let (report, _) = convert_export(
            dir.path(),
            &out,
            &book,
            Some("UTC"),
            &DateRange::default(),
            ExportTransforms::none(),
            OutputFormat::Csv,
            None,
        )
        .unwrap();
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
        let (report, _) = convert_export(
            dir.path(),
            &out,
            &book,
            Some("UTC"),
            &DateRange::default(),
            ExportTransforms::none(),
            OutputFormat::Csv,
            None,
        )
        .unwrap();
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
        let err = convert_export(
            dir.path(),
            dir.path(),
            &book,
            Some("UTC"),
            &DateRange::default(),
            ExportTransforms::none(),
            OutputFormat::Csv,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("must not be the same as"), "{err}");
        // Source CSV must survive the refused run.
        assert!(dir.path().join("Messages - Bob.csv").is_file());
    }
}
