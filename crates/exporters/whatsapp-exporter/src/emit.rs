//! Convert wtsexporter JSON → common message → packaging via FormatSink.

use crate::jid::{chat_id_from_jid, is_group_jid, jid_to_e164};
use crate::parse::{
    ChatJson, MessageJson, load_chat_store, media_path, message_text, timestamp_ms, timestamp_secs,
};
use anyhow::{Context, Result};
use message_csv::{DateRange, format_local_ts, json_cell, stable_guid};
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
use serde_json::Map;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const EXPORT_SOURCE: &str = "whatsapp";
const EXPORT_TOOL: &str = "WhatsApp Chat Exporter";
/// Pinned documented upstream version (JSON convert path; shell-out may differ).
pub(crate) const EXPORT_TOOL_VERSION: &str = "0.13.0";

/// Bump a per-exporter counter in the report's `extra` map.
fn bump(report: &mut ExportReport, key: &str, by: u64) {
    *report.extra.entry(key.to_string()).or_insert(0) += by;
}

#[derive(Debug)]
struct PendingAttachment {
    rel_path: String,
    original_name: Option<String>,
    mime_type: Option<String>,
    is_sticker: bool,
    digest_hex: String,
}

#[derive(Debug)]
struct PendingMessage {
    /// Unix milliseconds; same-second ties are broken by `key_id` during sort.
    sort_key: i64,
    is_from_me: bool,
    sender_handle: String,
    sender_display_name: String,
    text: String,
    key_id: String,
    reply_json: String,
    reactions_json: String,
    attachments: Vec<PendingAttachment>,
}

#[derive(Debug, Default)]
struct PendingConversation {
    conversation_type: String,
    group_title: Option<String>,
    whatsapp_jid: String,
    participant_e164s: Vec<String>,
    messages: Vec<PendingMessage>,
}

/// Convert a wtsexporter `result.json` into per-chat CSV under `output`.
///
/// `media_search_roots` are directories tried when resolving relative media paths
/// (typically the wtsexporter working directory / process cwd).
///
/// When `cancel` is set, it is checked between chats (cooperative cancellation).
pub(crate) fn convert_json(
    json_path: &Path,
    output: &Path,
    date_range: &DateRange,
    transforms: ExportTransforms,
    media_search_roots: &[PathBuf],
    output_format: OutputFormat,
    cancel: Option<&CancelFlag>,
) -> Result<(ExportReport, FormatSinkResult)> {
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    // Load the chat store BEFORE cleaning the output directory. The JSON may live
    // inside the output dir (e.g. wtsexporter_result.json) and cleaning
    // deletes all *.json files.
    let store = load_chat_store(json_path)?;
    let copy_attachments = transforms.copies_attachments();
    let (mut sink, _attachments_dir) =
        FormatSink::open_prepared(output, output_format, transforms)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    for (jid, chat) in store {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        if jid.starts_with('_') {
            // Reserved / system keys if any.
            continue;
        }
        match ingest_chat(
            &jid,
            &chat,
            output,
            date_range,
            copy_attachments,
            media_search_roots,
            &mut report,
        ) {
            Ok(Some((chat_id, convo))) => {
                conversations.insert(chat_id, convo);
            }
            Ok(None) => {}
            Err(e) => report.errors.push(format!("{jid}: {e:#}")),
        }
    }

    for (chat_id, mut convo) in conversations {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
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

fn ingest_chat(
    jid: &str,
    chat: &ChatJson,
    output: &Path,
    date_range: &DateRange,
    copy_attachments: bool,
    media_search_roots: &[PathBuf],
    report: &mut ExportReport,
) -> Result<Option<(String, PendingConversation)>> {
    let group = is_group_jid(jid);
    let chat_id = chat_id_from_jid(jid);
    let group_title = if group {
        chat.name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    } else {
        None
    };

    let mut peer_phones: BTreeSet<String> = BTreeSet::new();
    if !group {
        if let Some(e164) = jid_to_e164(jid) {
            peer_phones.insert(e164);
        }
    }

    let mut pending = PendingConversation {
        conversation_type: if group {
            "group".into()
        } else {
            "individual".into()
        },
        group_title,
        whatsapp_jid: jid.to_string(),
        participant_e164s: Vec::new(),
        messages: Vec::new(),
    };

    let display_fallback = chat.name.clone().unwrap_or_default();

    for (_id, msg) in &chat.messages {
        let Some(ts_raw) = msg.timestamp else {
            report.skipped_invalid_date += 1;
            continue;
        };
        let secs = timestamp_secs(ts_raw);
        if format_local_ts(secs).is_none() {
            report.skipped_invalid_date += 1;
            continue;
        }
        if !date_range.contains_secs(secs) {
            report.skipped_out_of_range += 1;
            continue;
        }

        let is_from_me = msg.from_me;
        let (sender_handle, sender_display_name) =
            resolve_sender(msg, is_from_me, &chat_id, &display_fallback, group);
        if group {
            if let Some(e164) = jid_to_e164(sender_handle.as_str())
                .or_else(|| msg.sender.as_deref().and_then(jid_to_e164))
            {
                peer_phones.insert(e164);
            }
        }

        let text = message_text(msg);
        let attachments = match media_path(msg) {
            Some(src) if copy_attachments => {
                match copy_media(
                    src,
                    chat.media_base.as_deref(),
                    media_search_roots,
                    output,
                    &chat_id,
                    msg,
                ) {
                    Ok(Some(att)) => {
                        report.attachments_saved += 1;
                        vec![att]
                    }
                    Ok(None) => {
                        bump(&mut *report, "attachments_missing", 1);
                        Vec::new()
                    }
                    Err(e) => {
                        report.errors.push(format!("{jid} media: {e:#}"));
                        Vec::new()
                    }
                }
            }
            Some(src) => vec![PendingAttachment {
                rel_path: src.to_string(),
                original_name: Path::new(src)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned()),
                mime_type: msg.mime.clone(),
                is_sticker: msg.sticker,
                digest_hex: digest_path_label(src),
            }],
            None => Vec::new(),
        };

        pending.messages.push(PendingMessage {
            sort_key: timestamp_ms(ts_raw),
            is_from_me,
            sender_handle,
            sender_display_name,
            text,
            key_id: key_id_string(msg),
            reply_json: optional_json(&msg.reply),
            reactions_json: reactions_json(&msg.reactions),
            attachments,
        });
    }

    if pending.messages.is_empty() {
        return Ok(None);
    }

    pending.participant_e164s = peer_phones.into_iter().collect();
    Ok(Some((chat_id, pending)))
}

fn resolve_sender(
    msg: &MessageJson,
    is_from_me: bool,
    chat_id: &str,
    chat_name: &str,
    group: bool,
) -> (String, String) {
    if is_from_me {
        return (String::new(), String::new());
    }
    if group {
        // Real JID / phone sender → E.164 handle. Display-name senders (e.g. a
        // group member's name) leave the handle empty; only the display name is set.
        let sender = msg.sender.as_deref().unwrap_or_default();
        match jid_to_e164(sender) {
            Some(e164) => (e164, String::new()),
            None => (String::new(), sender.to_string()),
        }
    } else {
        let handle = if chat_id.starts_with('+') {
            chat_id.to_string()
        } else {
            msg.sender
                .as_deref()
                .and_then(jid_to_e164)
                .unwrap_or_else(|| chat_id.to_string())
        };
        (handle, chat_name.to_string())
    }
}

fn copy_media(
    src: &str,
    media_base: Option<&str>,
    media_search_roots: &[PathBuf],
    output: &Path,
    chat_id: &str,
    msg: &MessageJson,
) -> Result<Option<PendingAttachment>> {
    let Some(src_path) = resolve_media_file(src, media_base, media_search_roots) else {
        return Ok(None);
    };
    let original = src_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "attachment.bin".into());
    let stem = sanitize_att_stem(chat_id);
    let dest_name = unique_name(output, &stem, &original, msg);
    let rel = format!("attachments/{dest_name}");
    let dest = output.join(&rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&src_path, &dest)
        .with_context(|| format!("copy {} → {}", src_path.display(), dest.display()))?;
    let digest = file_sha256(&dest)?;
    Ok(Some(PendingAttachment {
        rel_path: rel,
        original_name: Some(original),
        mime_type: msg.mime.clone(),
        is_sticker: msg.sticker,
        digest_hex: digest,
    }))
}

/// Resolve a wtsexporter media path against `media_base` and search roots.
fn resolve_media_file(
    src: &str,
    media_base: Option<&str>,
    media_search_roots: &[PathBuf],
) -> Option<PathBuf> {
    let hint = Path::new(src);
    if hint.is_file() {
        return Some(hint.to_path_buf());
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(base) = media_base.map(str::trim).filter(|s| !s.is_empty()) {
        let base_path = Path::new(base);
        candidates.push(base_path.join(hint));
        for root in media_search_roots {
            candidates.push(root.join(base_path).join(hint));
            if base_path.is_absolute() {
                candidates.push(base_path.join(hint));
            }
        }
    }
    for root in media_search_roots {
        candidates.push(root.join(hint));
    }

    candidates.into_iter().find(|p| p.is_file())
}

fn unique_name(output: &Path, chat_stem: &str, original: &str, msg: &MessageJson) -> String {
    let base = format!("{chat_stem}_{original}");
    let candidate = output.join("attachments").join(&base);
    if !candidate.exists() {
        return base;
    }
    let suffix = key_id_string(msg);
    // Truncate at a UTF-8 char boundary (never byte-slice a String).
    let short: String = suffix.chars().take(12).collect();
    format!("{chat_stem}_{short}_{original}")
}

fn sanitize_att_stem(chat_id: &str) -> String {
    let s: String = chat_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "chat".into() } else { s }
}

fn file_sha256(path: &Path) -> Result<String> {
    // Stream in 64KB chunks so large media never loads fully into RAM.
    let file = fs::File::open(path)?;
    let mut reader = std::io::BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn digest_path_label(src: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(src.as_bytes());
    hex::encode(hasher.finalize())
}

fn key_id_string(msg: &MessageJson) -> String {
    match &msg.key_id {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

fn optional_json(v: &Option<serde_json::Value>) -> String {
    match v {
        Some(val) if !val.is_null() => json_cell(val),
        _ => String::new(),
    }
}

fn reactions_json(v: &serde_json::Value) -> String {
    if v.is_null() || (v.is_object() && v.as_object().is_some_and(|o| o.is_empty())) {
        String::new()
    } else {
        json_cell(v)
    }
}

fn prepare_conversation(convo: &mut PendingConversation, report: &mut ExportReport) -> bool {
    convo.messages.sort_by(|a, b| {
        a.sort_key
            .cmp(&b.sort_key)
            .then_with(|| a.key_id.cmp(&b.key_id))
    });
    convo.messages.retain(|m| {
        if format_local_ts(m.sort_key / 1000).is_some() {
            true
        } else {
            report.skipped_invalid_date += 1;
            false
        }
    });
    !convo.messages.is_empty()
}

fn pending_to_document(
    chat_id: &str,
    convo: &PendingConversation,
    report: &mut ExportReport,
) -> Result<ConversationDocument> {
    let participants: Vec<IrParticipant> = convo
        .participant_e164s
        .iter()
        .filter(|h| !h.is_empty())
        .map(|h| IrParticipant {
            handle: h.clone(),
            display_name: None,
        })
        .collect();

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
        report.messages += 1;
        if msg.is_from_me {
            report.sent += 1;
        } else {
            report.received += 1;
        }
        let secs = msg.sort_key / 1000;
        let (ts_local, _, _) = format_local_ts(secs).expect("timestamp validated above");
        let mut digests: Vec<String> = msg
            .attachments
            .iter()
            .map(|a| a.digest_hex.clone())
            .collect();
        // key_id uniquely identifies a WhatsApp message; feed it into the GUID
        // so same-second messages with identical text get distinct GUIDs.
        digests.push(msg.key_id.clone());
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &digests);
        let attachments: Vec<IrAttachment> = msg
            .attachments
            .iter()
            .map(|a| IrAttachment {
                path: Some(a.rel_path.clone()),
                original_name: a.original_name.clone(),
                mime_type: a.mime_type.clone(),
                digest_sha256: Some(a.digest_hex.clone()),
                is_sticker: a.is_sticker,
                transcription: None,
                sticker_effect: None,
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
        if !convo.whatsapp_jid.is_empty() {
            fields.insert(
                "jid".into(),
                serde_json::Value::String(convo.whatsapp_jid.clone()),
            );
        }
        if !msg.key_id.is_empty() {
            fields.insert(
                "key_id".into(),
                serde_json::Value::String(msg.key_id.clone()),
            );
        }
        if !msg.reply_json.is_empty() {
            fields.insert(
                "reply".into(),
                serde_json::from_str(&msg.reply_json)
                    .unwrap_or_else(|_| serde_json::Value::String(msg.reply_json.clone())),
            );
        }
        if !msg.reactions_json.is_empty() {
            fields.insert(
                "reactions".into(),
                serde_json::from_str(&msg.reactions_json)
                    .unwrap_or_else(|_| serde_json::Value::String(msg.reactions_json.clone())),
            );
        }
        let source = IrSource {
            android_type: None,
            fields,
        }
        .into_option();

        let (sender_handle, sender_display_name) = if msg.is_from_me {
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
            timestamp_unix_ms: msg.sort_key,
            direction: if msg.is_from_me {
                IrDirection::Outgoing
            } else {
                IrDirection::Incoming
            },
            service: IrService::Whatsapp,
            message_kind,
            sender_handle,
            sender_display_name,
            subject: None,
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
            group_title: convo.group_title.clone(),
            participants,
            stats: ConversationStats::default(),
        },
        messages,
        packaging_stem_suffix: Some("__whatsapp".into()),
    })
}
