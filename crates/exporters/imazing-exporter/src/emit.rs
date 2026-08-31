//! Convert iMazing Messages / WhatsApp rows into the shared conversation
//! structure, then write the chosen output format via [`ExportWriter`].

use crate::attachments::{AttachmentIndex, ResolveAttachmentArgs, resolve_attachment_cell};
use crate::attachments_emit::{attachment_guid_materials, pending_attachment_to_ir};
use crate::parse::{RawRow, SourceKind, discover_csv_files, parse_csv_file};
use crate::parse_emit::{
    collect_peer_info, is_notification, is_outgoing, parse_message_date, resolve_sender, resolve_tz,
};
use anyhow::Result;
use contacts::ContactsBook;
use message_csv::DateRange;
use message_ir::{
    ExportMeta, HandleType, IrAttachment, IrParticipant, IrService, IrSource, PendingAttachment,
    PendingConversation, PendingMessage, ProjectedRole, ProjectionHooks, pending_to_document,
    prepare_conversation,
};
use message_ir_format::{AttachmentSource, ExportTransforms, ExportWriter, FormatSinkResult};
use message_vault_io_core::{CancelFlag, ExportReport, OutputFormat, prepare_outputs};
use serde_json::Map;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

const EXPORT_SOURCE: &str = "imazing";
const EXPORT_TOOL: &str = "iMazing";
const EXPORT_TOOL_VERSION: &str = "3.5.5";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransportFamily {
    Messages,
    WhatsApp,
}

impl TransportFamily {
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
    /// Continue an interrupted export: keep previous output and skip the
    /// conversations already written.
    pub resume: bool,
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
        resume,
    } = args;
    let tz = resolve_tz(timezone)?;
    let (inputs, output) = prepare_outputs(&[input.to_path_buf()], output)?;
    let input = &inputs[0];
    let writer = ExportWriter::open(&output, output_format, transforms, resume)?;
    let copy_attachments = writer.copies_attachments();
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
            let convo = conversations.entry(convo_key.clone()).or_insert_with(|| {
                let mut convo = PendingConversation::new(
                    peer.chat_id.clone(),
                    peer.group,
                    peer.group.then(|| session.clone()),
                    Vec::new(),
                );
                convo
                    .extra
                    .insert("source_kind".into(), discovered.kind.as_str().to_string());
                convo
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
                    let (cell, source) = resolve_attachment_cell(ResolveAttachmentArgs {
                        csv_name: &row.attachment,
                        attachment_type: &row.attachment_type,
                        csv_parent,
                        index: attachment_index.as_ref(),
                        copy_attachments,
                    });
                    attachments.push(PendingAttachment {
                        rel_path: row.attachment.clone(),
                        content_type: cell.meta.mime_type.clone().unwrap_or_default(),
                        extension: Path::new(&row.attachment)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_string(),
                        digest_sha256: None,
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
                    if let Some(src) = source {
                        attachment_extra.insert(
                            "attachment_source".into(),
                            src.to_string_lossy().into_owned(),
                        );
                    }
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

    let hooks = ImazingProjection {
        export: message_vault_io_core::export_meta(
            EXPORT_SOURCE,
            EXPORT_TOOL,
            EXPORT_TOOL_VERSION,
            None,
            None,
        ),
    };
    let mut documents = Vec::new();
    let mut sources = Vec::new();
    for (_key, mut convo) in conversations {
        let chat_id = convo.chat_id.clone();
        let (keep, skipped) =
            prepare_conversation(&mut convo, |a, b| a.sort_key.cmp(&b.sort_key), |k| k);
        report.skipped_invalid_date += skipped;
        if !keep {
            continue;
        }
        collect_attachment_sources(&convo, &mut sources);
        let (doc, tally) = pending_to_document(&chat_id, &convo, &hooks);
        report.messages += tally.messages;
        report.sent += tally.sent;
        report.received += tally.received;
        if tally.notifications > 0 {
            report.bump("notifications", tally.notifications);
        }
        documents.push(doc);
    }

    let mut source_iter = sources.into_iter();
    let sink_result = writer.finish(
        documents,
        &mut |att| match source_iter.next().flatten() {
            Some(path) => {
                // iMazing's rows carry no size; stat the source so the byte
                // counters and the headroom check see it.
                let hint = att
                    .size_bytes
                    .or_else(|| std::fs::metadata(&path).ok().map(|m| m.len()));
                (AttachmentSource::Path(path), hint)
            }
            None => (AttachmentSource::Missing, att.size_bytes),
        },
        cancel,
        &mut report,
    )?;

    Ok((report, sink_result))
}

fn collect_attachment_sources(
    convo: &PendingConversation,
    out: &mut Vec<Option<std::path::PathBuf>>,
) {
    for msg in &convo.messages {
        if msg.attachments.is_empty() {
            continue;
        }
        let source = msg.extra_str("attachment_source").to_string();
        for _ in &msg.attachments {
            out.push((!source.is_empty()).then(|| std::path::PathBuf::from(&source)));
        }
    }
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

/// iMazing deltas of the shared [`pending_to_document`] projection.
struct ImazingProjection {
    export: ExportMeta,
}

impl ProjectionHooks for ImazingProjection {
    fn export(&self) -> ExportMeta {
        self.export.clone()
    }

    fn service(&self, msg: &PendingMessage) -> IrService {
        IrService::parse(msg.extra_str("service"))
    }

    fn role(&self, msg: &PendingMessage) -> ProjectedRole {
        if msg.extra_flag("is_notification") {
            ProjectedRole::Notification
        } else if msg.is_from_me {
            ProjectedRole::Outgoing
        } else {
            ProjectedRole::Incoming
        }
    }

    fn subject(&self, msg: &PendingMessage) -> Option<String> {
        msg.extra_opt("subject")
    }

    fn guid_materials(&self, msg: &PendingMessage) -> Vec<String> {
        attachment_guid_materials(&msg.attachments)
    }

    fn attachment_to_ir(&self, att: &PendingAttachment, msg: &PendingMessage) -> IrAttachment {
        pending_attachment_to_ir(att, msg)
    }

    fn participants(&self, chat_id: &str, convo: &PendingConversation) -> Vec<IrParticipant> {
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
                display_name: convo.first_contact_name(),
                handle_type: Some(handle_type_for(chat_id)),
            });
        }
        participants
    }

    fn packaging_stem_suffix(&self, convo: &PendingConversation) -> Option<String> {
        imazing_packaging_stem_suffix(convo.extra_str("source_kind"))
    }

    fn source(&self, convo: &PendingConversation, msg: &PendingMessage) -> IrSource {
        let mut fields = Map::new();
        // Session string is not a real group title: stored as data only
        // (the document's `group_title` stays `None`, matching the previous
        // CSV/mail stem).
        let session_title = convo.display_name.as_deref().unwrap_or("");
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
        IrSource {
            android_type: None,
            fields,
        }
    }
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
            resume: false,
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
        assert_eq!(report.extra("unresolved_chat_phone"), 0);
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
        assert!(report.extra("unresolved_chat_phone") >= 1);
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
        assert_eq!(report.extra("unresolved_group_participants"), 0);
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
        assert_eq!(report.extra("unresolved_group_participants"), 1);
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
        assert_eq!(report.extra("messages_files"), 1);
        assert_eq!(report.extra("whatsapp_files"), 1);
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
