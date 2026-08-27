//! Write [`ConversationDocument`] as JSON, JSON Lines, CSV, or mail.

use crate::util;
use crate::write_sbr;
use anyhow::{Context, Result, bail};
use mail::{
    Direction as MailDirection, MailAttachment, MailMessage, MailPackage, Participant,
    SmsMailFields, write_mail_package,
};
use message_csv::{AttachmentCell, conversation_filename, format_local_ts, json_cell};
use message_ir::{
    ConversationDocument, ConversationHeader, HandleType, IrDirection, IrImessage, IrMessageKind,
};
use message_vault_io_core::OutputFormat;
use serde::Serialize;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Unified CSV columns for every exporter (IR v3 projection).
///
/// Apple-only cells are empty for non-iMessage sources. Legacy names
/// (`date_ms`, `contact_name`, `xml_fields_json`) are gone — use
/// `timestamp_unix_ms`, `sender_display_name`, and `source_fields_json`.
pub const CSV_HEADERS: &[&str] = &[
    "chat_identifier",
    "conversation_type",
    "group_title",
    "participants_json",
    "guid",
    "timestamp",
    "timestamp_utc",
    "timestamp_display",
    "timestamp_unix_ms",
    "direction",
    "service",
    "sender_handle",
    "sender_display_name",
    "handle_type",
    "subject",
    "text",
    "attachments_json",
    "message_kind",
    "export_source",
    "export_tool",
    "export_tool_version",
    "owner_handle",
    "owner_display_name",
    "android_type",
    "source_fields_json",
    "read_receipt",
    "is_deleted",
    "send_effect",
    "shared_location",
    "is_announcement",
    "announcement",
    "is_reply",
    "thread_originator_guid",
    "thread_originator_part",
    "num_replies",
    "parts_json",
    "edits_json",
    "tapbacks_json",
    "app_json",
    "balloon_bundle_id",
    "balloon_kind",
    "associated_guid",
    "associated_part",
    "tapback_kind",
    "tapback_emoji",
    "tapback_action",
];

/// Write one conversation in a per-chat format.
///
/// For multi-chat exports (including XML `smses.xml`), use [`FormatSink`] instead.
/// [`OutputFormat::Xml`] returns an error here.
///
/// # Errors
///
/// Returns an error when the directory cannot be created, a file cannot be
/// written, or `format` is XML.
pub(crate) fn write_format(
    output_dir: &Path,
    format: OutputFormat,
    mut doc: ConversationDocument,
) -> Result<PathBuf> {
    doc.finalize_stats();
    match format {
        OutputFormat::Csv => write_conversation_csv(output_dir, &doc),
        OutputFormat::Json => write_conversation_json(output_dir, &doc),
        OutputFormat::Jsonl => write_conversation_jsonl(output_dir, &doc),
        OutputFormat::Eml => write_conversation_mail(output_dir, &doc, MailPackage::EmlFolders),
        OutputFormat::Mbox => write_conversation_mail(output_dir, &doc, MailPackage::Mbox),
        OutputFormat::Xml => write_sbr::write_format_xml_unsupported(),
    }
}

/// Per-conversation JSON artifact (`<stem>.json`).
fn write_conversation_json(output_dir: &Path, doc: &ConversationDocument) -> Result<PathBuf> {
    fs::create_dir_all(output_dir).with_context(|| format!("create {}", output_dir.display()))?;
    let path = output_dir.join(format!("{}.json", doc.filename_stem()));
    let mut tmp = path.clone();
    tmp.set_extension("json.tmp");
    let json = serde_json::to_vec_pretty(doc).context("serialize ConversationDocument")?;
    {
        let mut file = File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        file.write_all(&json)
            .with_context(|| format!("write {}", tmp.display()))?;
        file.write_all(b"\n")?;
    }
    fs::rename(&tmp, &path)
        .with_context(|| format!("rename {} → {}", tmp.display(), path.display()))?;
    Ok(path)
}

/// First JSON Lines line: schema, export, and conversation metadata (no messages).
fn write_conversation_jsonl(output_dir: &Path, doc: &ConversationDocument) -> Result<PathBuf> {
    fs::create_dir_all(output_dir).with_context(|| format!("create {}", output_dir.display()))?;
    let path = output_dir.join(format!("{}.jsonl", doc.filename_stem()));
    let mut tmp = path.clone();
    tmp.set_extension("jsonl.tmp");
    {
        let file = File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        let mut file = BufWriter::new(file);
        let header = ConversationHeader::from_document(doc);
        serde_json::to_writer(&mut file, &header).context("serialize JSONL header")?;
        file.write_all(b"\n")?;
        for msg in &doc.messages {
            serde_json::to_writer(&mut file, msg).context("serialize JSONL message")?;
            file.write_all(b"\n")?;
        }
        file.flush()
            .with_context(|| format!("flush {}", tmp.display()))?;
    }
    fs::rename(&tmp, &path)
        .with_context(|| format!("rename {} → {}", tmp.display(), path.display()))?;
    Ok(path)
}

fn stem_suffix(doc: &ConversationDocument) -> Option<&str> {
    doc.packaging_stem_suffix.as_deref()
}

/// Compact JSON for nested bags; empty string when absent (never the literal `null`).
fn value_cell(v: Option<&Value>) -> String {
    v.filter(|v| !v.is_null())
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

/// CSV `parts_json` cell: only from the iMessage bag, and omit a single plain
/// text/run part that merely duplicates [`IrMessage::text`].
fn parts_cell_for_csv(text: &str, parts: Option<&Value>) -> String {
    if parts_are_trivial_text_duplicate(text, parts) {
        return String::new();
    }
    value_cell(parts)
}

/// True when `parts` is a one-element array whose text equals `message_text`
/// and kind is absent, `run`, or `text`.
pub(crate) fn parts_are_trivial_text_duplicate(message_text: &str, parts: Option<&Value>) -> bool {
    let Some(Value::Array(items)) = parts else {
        return false;
    };
    if items.len() != 1 {
        return false;
    }
    let Some(obj) = items[0].as_object() else {
        return false;
    };
    let Some(part_text) = obj.get("text").and_then(|v| v.as_str()) else {
        return false;
    };
    if part_text != message_text {
        return false;
    }
    matches!(
        obj.get("kind").and_then(|v| v.as_str()),
        None | Some("run") | Some("text")
    )
}

fn value_as_string(v: Option<&Value>) -> Option<String> {
    let v = v?;
    if v.is_null() {
        return None;
    }
    Some(serde_json::to_string(v).unwrap_or_default()).filter(|s| !s.is_empty())
}

#[derive(Serialize)]
struct ParticipantCell {
    handle: String,
    display_name: String,
    handle_type: Option<HandleType>,
}

/// CSV `handle_type` cell: the sender's handle type, inferred from the sender
/// handle with the same rules the EML/mbox reader uses on re-import. Empty
/// when the message has no sender handle.
fn sender_handle_type_cell(sender_handle: Option<&str>) -> &'static str {
    match sender_handle {
        Some(handle) => crate::util::infer_handle_type(handle).as_str(),
        None => "",
    }
}

/// Per-conversation CSV using the unified [`CSV_HEADERS`] contract.
pub(crate) fn write_conversation_csv(
    output_dir: &Path,
    doc: &ConversationDocument,
) -> Result<PathBuf> {
    fs::create_dir_all(output_dir).with_context(|| format!("create {}", output_dir.display()))?;
    let filename = conversation_filename(
        doc.conversation.conversation_type.as_str(),
        &doc.conversation.chat_identifier,
        doc.conversation.group_title.as_deref(),
        &doc.conversation
            .participants
            .iter()
            .map(|p| p.handle.clone())
            .collect::<Vec<_>>(),
        stem_suffix(doc),
    );
    let path = output_dir.join(filename);
    let mut tmp_name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| "chat.csv".into());
    tmp_name.push(".tmp");
    let tmp_path = path.with_file_name(tmp_name);
    let file = File::create(&tmp_path).with_context(|| format!("create {}", tmp_path.display()))?;
    let mut wtr = csv::Writer::from_writer(file);
    wtr.write_record(CSV_HEADERS)
        .with_context(|| format!("write header {}", path.display()))?;

    let participants_json = json_cell(
        &doc.conversation
            .participants
            .iter()
            .map(|p| ParticipantCell {
                handle: p.handle.clone(),
                display_name: p.display_name.clone().unwrap_or_default(),
                handle_type: p.handle_type,
            })
            .collect::<Vec<_>>(),
    );

    for msg in &doc.messages {
        let secs = msg.timestamp_unix_ms.div_euclid(1000);
        let (ts_local, ts_utc, ts_display) = format_local_ts(secs).ok_or_else(|| {
            anyhow::anyhow!("invalid timestamp_unix_ms {}", msg.timestamp_unix_ms)
        })?;
        let attachment_cells: Vec<AttachmentCell> = msg
            .attachments
            .iter()
            .map(|a| AttachmentCell {
                meta: a.into(),
                is_sticker: a.is_sticker,
                transcription: a.transcription.clone(),
                sticker_effect: a.sticker_effect.clone(),
            })
            .collect();
        let attachments_json = json_cell(&attachment_cells);
        let timestamp_unix_ms = msg.timestamp_unix_ms.to_string();
        let android_type = msg
            .source
            .as_ref()
            .and_then(|s| s.android_type)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let source_fields_json = msg
            .source
            .as_ref()
            .filter(|s| !s.fields.is_empty())
            .map(|s| serde_json::to_string(&s.fields).unwrap_or_default())
            .unwrap_or_default();

        let im = msg.imessage.as_ref();
        let read_receipt = im
            .and_then(|i| i.read_receipt_rfc3339.as_deref())
            .unwrap_or("");
        let is_deleted = im.map(|i| i.is_deleted).unwrap_or(false);
        let send_effect = im.and_then(|i| i.send_effect.as_deref()).unwrap_or("");
        let shared_location = im.and_then(|i| i.shared_location.as_deref()).unwrap_or("");
        let is_announcement = msg.message_kind == IrMessageKind::Announcement;
        let announcement = im.and_then(|i| i.announcement.as_deref()).unwrap_or("");
        let is_reply = im.map(|i| i.is_reply).unwrap_or(false);
        let thread_originator_guid = im.and_then(|i| i.in_reply_to_guid.as_deref()).unwrap_or("");
        let thread_originator_part = im
            .and_then(|i| i.thread_originator_part)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let num_replies = im
            .and_then(|i| i.num_replies)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let parts_json = parts_cell_for_csv(msg.text.as_str(), im.and_then(|i| i.parts.as_ref()));
        let edits_json = value_cell(im.and_then(|i| i.edits.as_ref()));
        let tapbacks_json = value_cell(im.and_then(|i| i.tapbacks.as_ref()));
        let app_json = value_cell(im.and_then(|i| i.app.as_ref()));
        let balloon_bundle_id = im
            .and_then(|i| i.balloon_bundle_id.as_deref())
            .unwrap_or("");
        let balloon_kind = im.and_then(|i| i.balloon_kind.as_deref()).unwrap_or("");
        let associated_guid = im.and_then(|i| i.associated_guid.as_deref()).unwrap_or("");
        let associated_part = im
            .and_then(|i| i.associated_part)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let tapback_kind = im.and_then(|i| i.tapback_kind.as_deref()).unwrap_or("");
        let tapback_emoji = im.and_then(|i| i.tapback_emoji.as_deref()).unwrap_or("");
        let tapback_action = im.and_then(|i| i.tapback_action.as_deref()).unwrap_or("");

        wtr.write_record([
            doc.conversation.chat_identifier.as_str(),
            doc.conversation.conversation_type.as_str(),
            doc.conversation.group_title.as_deref().unwrap_or(""),
            participants_json.as_str(),
            msg.guid.as_str(),
            ts_local.as_str(),
            ts_utc.as_str(),
            ts_display.as_str(),
            timestamp_unix_ms.as_str(),
            msg.direction.as_str(),
            msg.service.as_str(),
            msg.sender_handle.as_deref().unwrap_or(""),
            msg.sender_display_name.as_deref().unwrap_or(""),
            sender_handle_type_cell(msg.sender_handle.as_deref()),
            msg.subject.as_deref().unwrap_or(""),
            msg.text.as_str(),
            attachments_json.as_str(),
            msg.message_kind.as_str(),
            doc.export.source.as_str(),
            doc.export.tool.as_str(),
            doc.export.tool_version.as_str(),
            doc.export.owner_handle.as_deref().unwrap_or(""),
            doc.export.owner_display_name.as_deref().unwrap_or(""),
            android_type.as_str(),
            source_fields_json.as_str(),
            read_receipt,
            if is_deleted { "true" } else { "false" },
            send_effect,
            shared_location,
            if is_announcement { "true" } else { "false" },
            announcement,
            if is_reply { "true" } else { "false" },
            thread_originator_guid,
            thread_originator_part.as_str(),
            num_replies.as_str(),
            parts_json.as_str(),
            edits_json.as_str(),
            tapbacks_json.as_str(),
            app_json.as_str(),
            balloon_bundle_id,
            balloon_kind,
            associated_guid,
            associated_part.as_str(),
            tapback_kind,
            tapback_emoji,
            tapback_action,
        ])
        .with_context(|| format!("write row {}", path.display()))?;
    }

    wtr.flush()?;
    drop(wtr);
    fs::rename(&tmp_path, &path)
        .with_context(|| format!("rename {} → {}", tmp_path.display(), path.display()))?;

    Ok(path)
}

fn write_conversation_mail(
    output_dir: &Path,
    doc: &ConversationDocument,
    package: MailPackage,
) -> Result<PathBuf> {
    let messages = document_to_mail_messages(doc, output_dir)?;
    if messages.is_empty() {
        bail!("conversation has no messages");
    }
    write_mail_package(output_dir, package, &messages)
}

/// Build [`MailMessage`] list from IR (reads attachment bytes from disk when missing).
///
/// # Errors
///
/// Returns an error when an attachment file cannot be read from disk.
pub fn document_to_mail_messages(
    doc: &ConversationDocument,
    output_dir: &Path,
) -> Result<Vec<MailMessage>> {
    let owner = doc.export.owner_handle.clone().unwrap_or_default();
    let participants: Vec<Participant> = doc
        .conversation
        .participants
        .iter()
        .map(|p| Participant {
            handle: p.handle.clone(),
            display_name: p.display_name.clone(),
        })
        .collect();

    let mut out = Vec::with_capacity(doc.messages.len());
    for msg in &doc.messages {
        let mut attachments = Vec::with_capacity(msg.attachments.len());
        for a in &msg.attachments {
            let bytes = util::load_attachment_bytes_strict(a, output_dir)?;
            attachments.push(MailAttachment {
                bytes,
                meta: a.into(),
                is_sticker: a.is_sticker,
                transcription: a.transcription.clone(),
                sticker_effect: a.sticker_effect.clone(),
            });
        }

        let android_type = msg
            .source
            .as_ref()
            .and_then(|s| s.android_type)
            .map(|n| n.to_string());
        let source_fields_json = msg.source.as_ref().and_then(|s| {
            if s.fields.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&s.fields).unwrap_or_default())
            }
        });

        let mut mail = MailMessage::sms(SmsMailFields {
            chat_identifier: doc.conversation.chat_identifier.clone(),
            conversation_type: doc.conversation.conversation_type.as_str().to_string(),
            group_title: doc.conversation.group_title.clone(),
            participants: participants.clone(),
            guid: msg.guid.clone(),
            timestamp_unix_ms: msg.timestamp_unix_ms,
            direction: match msg.direction {
                IrDirection::Incoming => MailDirection::Incoming,
                IrDirection::Outgoing => MailDirection::Outgoing,
            },
            service: msg.service.as_str().to_string(),
            message_kind: msg.message_kind.as_str().to_string(),
            sender_handle: msg.sender_handle.clone(),
            sender_display_name: msg.sender_display_name.clone(),
            owner_handle: owner.clone(),
            subject: msg.subject.clone(),
            text: msg.text.clone(),
            android_type,
            source_fields_json,
            export_source: doc.export.source.clone(),
            export_tool: doc.export.tool.clone(),
            export_tool_version: doc.export.tool_version.clone(),
            attachments,
            filename_suffix: doc.packaging_stem_suffix.clone(),
        });
        if let Some(imessage) = &msg.imessage {
            apply_imessage_fields(&mut mail, imessage);
        }
        if mail.owner_display_name.is_none() {
            mail.owner_display_name = doc.export.owner_display_name.clone();
        }
        out.push(mail);
    }
    Ok(out)
}

/// Restore iMessage extension fields from [`IrImessage`] onto a [`MailMessage`].
fn apply_imessage_fields(mail: &mut MailMessage, imessage: &IrImessage) {
    mail.is_reply = imessage.is_reply;
    mail.in_reply_to_guid = imessage.in_reply_to_guid.clone();
    mail.thread_originator_part = imessage.thread_originator_part;
    mail.num_replies = imessage.num_replies;
    mail.is_deleted = imessage.is_deleted;
    mail.send_effect = imessage.send_effect.clone();
    mail.shared_location = imessage.shared_location.clone();
    mail.announcement = imessage.announcement.clone();
    mail.read_receipt_rfc3339 = imessage.read_receipt_rfc3339.clone();
    mail.parts_json = value_as_string(imessage.parts.as_ref());
    mail.edits_json = value_as_string(imessage.edits.as_ref());
    mail.app_json = value_as_string(imessage.app.as_ref());
    mail.balloon_bundle_id = imessage.balloon_bundle_id.clone();
    mail.balloon_kind = imessage.balloon_kind.clone();
    mail.tapbacks_json = value_as_string(imessage.tapbacks.as_ref());
    mail.associated_guid = imessage.associated_guid.clone();
    mail.associated_part = imessage.associated_part;
    mail.tapback_kind = imessage.tapback_kind.clone();
    mail.tapback_emoji = imessage.tapback_emoji.clone();
    mail.tapback_action = imessage.tapback_action.clone();
}
