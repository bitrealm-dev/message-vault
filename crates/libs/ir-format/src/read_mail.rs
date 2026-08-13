//! Read an EML folder or mboxrd mailbox back into a [`ConversationDocument`].

use crate::normalize::{imessage_from_parts, source_from_parts};
use anyhow::{Context, Result, bail};
use mail::{
    Direction as MailDirection, MailMessage, mail_message_from_eml_bytes, mail_messages_from_mbox,
};
use message_ir::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, IrAttachment,
    IrConversationType, IrDirection, IrImessage, IrMessage, IrMessageKind, IrParticipant,
    IrService, SCHEMA_VERSION, parse_android_type,
};
use message_vault_io_core::discover_files;
use std::fs;
use std::path::Path;

/// Scan a conversation directory of `.eml` files into a conversation document.
///
/// # Errors
///
/// Returns an error when `dir` is not a directory, no `.eml` files are found,
/// or a file cannot be read or parsed.
pub fn read_conversation_eml_dir(dir: &Path) -> Result<ConversationDocument> {
    if !dir.is_dir() {
        bail!("not a directory: {}", dir.display());
    }
    let mut paths = discover_files(dir, &|p| {
        p.extension()
            .and_then(|x| x.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("eml"))
    })
    .with_context(|| format!("read {}", dir.display()))?;
    paths.sort();
    if paths.is_empty() {
        bail!("no .eml files in {}", dir.display());
    }

    let mut mail_messages = Vec::with_capacity(paths.len());
    for path in &paths {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let msg = mail_message_from_eml_bytes(&bytes)
            .with_context(|| format!("parse {}", path.display()))?;
        mail_messages.push(msg);
    }
    mail_messages.sort_by(cmp_mail_messages);

    let packaging = crate::util::packaging_suffix_from_stem(
        dir.file_name().and_then(|n| n.to_str()).unwrap_or_default(),
    );
    document_from_mail_messages(&mail_messages, packaging)
}

/// Read a conversation `.mbox` (mboxrd) into a conversation document.
///
/// # Errors
///
/// Returns an error when the mailbox cannot be read or contains no messages.
pub fn read_conversation_mbox(path: &Path) -> Result<ConversationDocument> {
    let mut mail_messages = mail_messages_from_mbox(path)?;
    if mail_messages.is_empty() {
        bail!("mbox has no messages: {}", path.display());
    }
    mail_messages.sort_by(cmp_mail_messages);
    let packaging = crate::util::packaging_suffix_from_stem(
        path.file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or_default(),
    );
    document_from_mail_messages(&mail_messages, packaging)
}

/// Order mail messages by timestamp, then GUID, so both readers agree.
fn cmp_mail_messages(a: &MailMessage, b: &MailMessage) -> std::cmp::Ordering {
    a.timestamp_unix_ms
        .cmp(&b.timestamp_unix_ms)
        .then_with(|| a.guid.cmp(&b.guid))
}

/// Map a list of [`MailMessage`] values from one conversation into a
/// [`ConversationDocument`].
///
/// # Errors
///
/// Returns an error when `messages` is empty.
fn document_from_mail_messages(
    messages: &[MailMessage],
    packaging_stem_suffix: Option<String>,
) -> Result<ConversationDocument> {
    if messages.is_empty() {
        bail!("document_from_mail_messages requires at least one message");
    }
    let first = &messages[0];
    let export = ExportMeta {
        source: first.export_source.clone(),
        tool: first.export_tool.clone(),
        tool_version: first.export_tool_version.clone(),
        owner_handle: nonempty_owned(&first.owner_handle),
        owner_display_name: first
            .owner_display_name
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    };

    let participants: Vec<IrParticipant> = first
        .participants
        .iter()
        .map(participant_from_mail)
        .collect();

    let ir_messages: Vec<IrMessage> = messages.iter().map(ir_message_from_mail).collect();

    let mut doc = ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export,
        conversation: ConversationMeta {
            chat_identifier: first.chat_identifier.clone(),
            conversation_type: IrConversationType::parse(&first.conversation_type),
            group_title: first
                .group_title
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            participants,
            stats: ConversationStats::default(),
        },
        messages: ir_messages,
        packaging_stem_suffix,
    };
    doc.finalize_stats();
    Ok(doc)
}

/// Build an [`IrParticipant`] from a mail participant.
///
/// EML and mbox do not store a handle type, so the type is inferred from the
/// handle string (`@` → email, digit-heavy → phone, else other).
fn participant_from_mail(p: &mail::Participant) -> IrParticipant {
    IrParticipant {
        handle: p.handle.clone(),
        display_name: p
            .display_name
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        handle_type: Some(crate::util::infer_handle_type(&p.handle)),
    }
}

/// Map one mail message into the shared conversation message type.
fn ir_message_from_mail(msg: &MailMessage) -> IrMessage {
    let direction = match msg.direction {
        MailDirection::Incoming => IrDirection::Incoming,
        MailDirection::Outgoing => IrDirection::Outgoing,
    };
    let source = source_from_parts(
        msg.android_type.as_deref().and_then(parse_android_type),
        msg.source_fields_json.as_deref().unwrap_or(""),
    );
    let imessage = imessage_from_parts(IrImessage {
        is_reply: msg.is_reply,
        in_reply_to_guid: msg.in_reply_to_guid.clone(),
        thread_originator_part: msg.thread_originator_part,
        num_replies: msg.num_replies,
        is_deleted: msg.is_deleted,
        send_effect: msg.send_effect.clone(),
        shared_location: msg.shared_location.clone(),
        announcement: msg.announcement.clone(),
        read_receipt_rfc3339: msg.read_receipt_rfc3339.clone(),
        parts: parse_json_opt(msg.parts_json.as_deref()),
        edits: parse_json_opt(msg.edits_json.as_deref()),
        tapbacks: parse_json_opt(msg.tapbacks_json.as_deref()),
        app: parse_json_opt(msg.app_json.as_deref()),
        balloon_bundle_id: msg.balloon_bundle_id.clone(),
        balloon_kind: msg.balloon_kind.clone(),
        associated_guid: msg.associated_guid.clone(),
        associated_part: msg.associated_part,
        tapback_kind: msg.tapback_kind.clone(),
        tapback_emoji: msg.tapback_emoji.clone(),
        tapback_action: msg.tapback_action.clone(),
    });

    let attachments = msg.attachments.iter().map(attachment_from_mail).collect();

    IrMessage {
        guid: msg.guid.clone(),
        timestamp_unix_ms: msg.timestamp_unix_ms,
        direction,
        service: IrService::parse(&msg.service),
        message_kind: IrMessageKind::parse(&msg.message_kind),
        sender_handle: msg.sender_handle.clone().filter(|s| !s.is_empty()),
        sender_display_name: msg.sender_display_name.clone().filter(|s| !s.is_empty()),
        subject: msg.subject.clone().filter(|s| !s.is_empty()),
        text: msg.text.clone(),
        attachments,
        imessage,
        source,
    }
}

/// Map one mail attachment into [`IrAttachment`].
fn attachment_from_mail(a: &mail::MailAttachment) -> IrAttachment {
    IrAttachment {
        path: None,
        original_name: a.original_name.clone(),
        mime_type: a.mime_type.clone(),
        digest_sha256: a.digest_sha256.clone(),
        is_sticker: a.is_sticker,
        transcription: a.transcription.clone(),
        sticker_effect: a.sticker_effect.clone(),
        size_bytes: None,
        missing_reason: None,
        bytes: if a.bytes.is_empty() {
            None
        } else {
            Some(a.bytes.clone())
        },
    }
}

/// Parse an optional JSON cell, treating blank and `null` as `None`.
fn parse_json_opt(s: Option<&str>) -> Option<serde_json::Value> {
    let t = s?.trim();
    if t.is_empty() || t == "null" {
        return None;
    }
    serde_json::from_str(t).ok()
}

/// Trimmed owned string, or `None` when blank.
fn nonempty_owned(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}
