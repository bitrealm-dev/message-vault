//! Convert wtsexporter JSON into the shared conversation structure, then write
//! the chosen output format via [`FormatSink`].

use crate::jid::{chat_id_from_jid, is_group_jid, jid_to_e164};
use crate::parse::{
    ChatJson, MessageJson, load_chat_store, media_path, message_text, timestamp_ms, timestamp_secs,
};
use anyhow::{Context, Result};
use media::{CompressOptions, MediaMode};
use message_csv::{DateRange, format_local_ts, json_cell, stable_guid};
use message_ir::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, HandleType,
    IrAttachment, IrConversationType, IrDirection, IrMessage, IrMessageKind, IrParticipant,
    IrService, IrSource, PendingAttachment, PendingConversation, PendingMessage, SCHEMA_VERSION,
    owner_sender,
};
use message_ir_format::{
    AttachmentSource, ConversationUnit, ExportTransforms, FormatSink, FormatSinkResult,
    WriteQueueOptions,
};
use message_vault_io_core::{
    AttachmentJob, CancelFlag, ExportReport, LogSink, OutputFormat, emit_log, run_attachment_jobs,
};
use serde_json::Map;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const EXPORT_SOURCE: &str = "whatsapp";
const EXPORT_TOOL: &str = "WhatsApp Chat Exporter";
/// Pinned documented upstream version (JSON convert path; shell-out may differ).
pub(crate) const EXPORT_TOOL_VERSION: &str = "0.13.0";

/// File extension without the leading dot, e.g. `"jpg"` for `"photo.jpg"`.
fn ext_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string()
}

/// Convert a wtsexporter `result.json` into the shared conversation structure,
/// then write the chosen output format.
///
/// `media_search_roots` are directories tried when resolving relative media paths
/// (typically the wtsexporter working directory / process cwd).
///
/// When `cancel` is set, it is checked between chats (cooperative cancellation).
///
/// # Errors
///
/// Returns an error when the JSON cannot be read, a conversation cannot be
/// written, or the user cancels.
pub(crate) fn convert_json(
    json_path: &Path,
    output: &Path,
    date_range: &DateRange,
    transforms: ExportTransforms,
    media_search_roots: &[PathBuf],
    output_format: OutputFormat,
    cancel: Option<&CancelFlag>,
    resume: bool,
) -> Result<(ExportReport, FormatSinkResult)> {
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    // Load the chat store BEFORE cleaning the output directory. The JSON may live
    // inside the output dir (e.g. wtsexporter_result.json) and cleaning
    // deletes all *.json files.
    let store = load_chat_store(json_path)?;
    let copy_attachments = transforms.copies_attachments();
    let media_mode = if copy_attachments {
        transforms.media
    } else {
        MediaMode::Disabled
    };
    let compress = transforms.compress.clone();
    let log = transforms.log.clone();
    // Captured before `transforms` moves into the sink: the queue path is for
    // the import, which is JSONL and never obfuscated.
    let use_queue = output_format == OutputFormat::Jsonl && !transforms.obfuscate;
    let (sink, attachments_dir) = if resume {
        FormatSink::open_resume(output, output_format, transforms)
    } else {
        FormatSink::open_prepared(output, output_format, transforms)
    }?;
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

    let mut documents = Vec::new();
    let mut media_sources = Vec::new();
    let mut units: Vec<ConversationUnit> = Vec::new();
    for (chat_id, mut convo) in conversations {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        if !prepare_conversation(&mut convo, &mut report) {
            continue;
        }
        if use_queue {
            // Same positional collection as the flat path, kept per
            // conversation so each unit carries its own sources.
            let mut convo_sources = Vec::new();
            collect_media_sources(&convo, &mut convo_sources);
            let doc = pending_to_document(&chat_id, &convo, &mut report)?;
            let mut source_iter = convo_sources.into_iter();
            units.push(ConversationUnit::from_doc(doc, |_, att| {
                let hint = att.size_bytes;
                match source_iter.next().flatten() {
                    Some(path) => (AttachmentSource::Path(path), hint),
                    None => (AttachmentSource::Missing, hint),
                }
            }));
            continue;
        }
        collect_media_sources(&convo, &mut media_sources);
        documents.push(pending_to_document(&chat_id, &convo, &mut report)?);
    }

    if use_queue {
        let options = WriteQueueOptions {
            media: media_mode,
            compress: compress.clone(),
            resume,
            writer_count: 0,
        };
        let sink_result = message_ir_format::drain_units(
            output,
            units,
            &options,
            log.as_ref(),
            cancel,
            &mut report,
        )?;
        return Ok((report, sink_result));
    }

    stage_conversation_attachments(
        &mut documents,
        &attachments_dir,
        media_mode,
        &compress,
        &media_sources,
        log.as_ref(),
        cancel,
        &mut report,
    )?;

    let sink_result = message_ir_format::write_documents_through_sink(
        documents,
        sink,
        log.as_ref(),
        cancel,
        &mut report,
    )?;

    Ok((report, sink_result))
}

/// Ingest one WhatsApp chat JSON into a pending conversation (messages + media).
fn ingest_chat(
    jid: &str,
    chat: &ChatJson,
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
    if !group && let Some(e164) = jid_to_e164(jid) {
        peer_phones.insert(e164);
    }

    let mut pending = PendingConversation {
        chat_id: chat_id.clone(),
        display_name: group_title,
        participant_e164s: Vec::new(),
        messages: Vec::new(),
        is_group: group,
        has_attachments: false,
        extra: {
            let mut e = BTreeMap::new();
            e.insert("whatsapp_jid".into(), jid.to_string());
            e
        },
    };

    let display_fallback = chat.name.clone().unwrap_or_default();

    for msg in chat.messages.values() {
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
        if group
            && let Some(e164) = jid_to_e164(sender_handle.as_str())
                .or_else(|| msg.sender.as_deref().and_then(jid_to_e164))
        {
            peer_phones.insert(e164);
        }

        let text = message_text(msg);
        let (attachments, media_source) = match media_path(msg) {
            Some(src) => queue_media(
                src,
                chat.media_base.as_deref(),
                media_search_roots,
                copy_attachments,
                msg,
                report,
            ),
            None => (Vec::new(), None),
        };

        pending.messages.push(PendingMessage {
            sort_key: timestamp_ms(ts_raw),
            is_from_me,
            sender_handle,
            sender_display_name: if sender_display_name.is_empty() {
                None
            } else {
                Some(sender_display_name)
            },
            text,
            attachments,
            extra: {
                let mut e = BTreeMap::new();
                e.insert("key_id".into(), key_id_string(msg));
                e.insert("reply_json".into(), optional_json(&msg.reply));
                e.insert("reactions_json".into(), reactions_json(&msg.reactions));
                e.insert(
                    "is_sticker".into(),
                    if msg.sticker { "true" } else { "false" }.into(),
                );
                if let Some(path) = media_source {
                    e.insert("media_source".into(), path.to_string_lossy().into_owned());
                }
                e
            },
        });
    }

    if pending.messages.is_empty() {
        return Ok(None);
    }

    pending.participant_e164s = peer_phones.into_iter().collect();
    Ok(Some((chat_id, pending)))
}

/// Sender handle and display name for a WhatsApp message (empty when from me).
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

/// Resolve a media path during parse. Do not copy; the runner writes later.
fn queue_media(
    src: &str,
    media_base: Option<&str>,
    media_search_roots: &[PathBuf],
    copy_attachments: bool,
    msg: &MessageJson,
    report: &mut ExportReport,
) -> (Vec<PendingAttachment>, Option<PathBuf>) {
    let name = Path::new(src)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    let pending = PendingAttachment {
        rel_path: String::new(),
        content_type: msg.mime.clone().unwrap_or_default(),
        extension: name.as_deref().map(ext_of).unwrap_or_default(),
        digest_sha256: None,
        name_hint: name,
    };
    if !copy_attachments {
        return (vec![pending], None);
    }
    match resolve_media_file(src, media_base, media_search_roots) {
        Some(src_path) => (vec![pending], Some(src_path)),
        None => {
            report.bump("attachments_missing", 1);
            (Vec::new(), None)
        }
    }
}

/// Collect source paths in the same order attachments will appear on documents.
fn collect_media_sources(convo: &PendingConversation, out: &mut Vec<Option<PathBuf>>) {
    for msg in &convo.messages {
        if msg.attachments.is_empty() {
            continue;
        }
        let source = msg.extra_str("media_source").to_string();
        for _ in &msg.attachments {
            out.push((!source.is_empty()).then(|| PathBuf::from(&source)));
        }
    }
}

/// Write staged attachment bytes after parse and before conversation files.
fn stage_conversation_attachments(
    documents: &mut [ConversationDocument],
    attachments_dir: &Path,
    mode: MediaMode,
    compress: &CompressOptions,
    media_sources: &[Option<PathBuf>],
    log: Option<&LogSink>,
    cancel: Option<&CancelFlag>,
    report: &mut ExportReport,
) -> Result<()> {
    let mut jobs = Vec::new();
    for doc in documents.iter_mut() {
        for msg in &mut doc.messages {
            let ts = msg.timestamp_unix_ms;
            for att in &mut msg.attachments {
                let hint = att.size_bytes;
                jobs.push(AttachmentJob {
                    attachment: att,
                    timestamp_unix_ms: ts,
                    size_hint: hint,
                });
            }
        }
    }
    let cancel_flag = cancel.map(|flag| flag.as_ref());
    run_attachment_jobs(
        &mut jobs,
        attachments_dir,
        mode,
        compress,
        |i| {
            let Some(path) = media_sources.get(i).and_then(|p| p.as_ref()) else {
                return Ok(None);
            };
            std::fs::read(path).map(Some).or(Ok(None))
        },
        |progress| {
            emit_log(
                log,
                format!(
                    "  attachments {}/{} {}/{}",
                    progress.done, progress.total, progress.bytes_done, progress.bytes_total
                ),
            );
        },
        log,
        cancel_flag,
    )
    .map_err(anyhow::Error::msg)?;
    for job in &jobs {
        if job.attachment.path.is_some() && job.attachment.digest_sha256.is_some() {
            report.attachments_saved += 1;
        }
    }
    Ok(())
}

/// Resolve a wtsexporter media path against `media_base` and search roots.
///
/// Only paths that resolve inside an allowed root (search roots, or an
/// absolute `media_base`) are accepted. Absolute hints and `..` segments
/// cannot escape those roots.
fn resolve_media_file(
    src: &str,
    media_base: Option<&str>,
    media_search_roots: &[PathBuf],
) -> Option<PathBuf> {
    let allowed = allowed_media_roots(media_base, media_search_roots);
    if allowed.is_empty() {
        return None;
    }

    let hint = Path::new(src);
    let mut candidates: Vec<PathBuf> = Vec::new();
    if hint.is_absolute() {
        candidates.push(hint.to_path_buf());
    } else if let Some(base) = media_base.map(str::trim).filter(|s| !s.is_empty()) {
        let base_path = Path::new(base);
        if base_path.is_absolute() {
            candidates.push(base_path.join(hint));
        }
        for root in media_search_roots {
            candidates.push(root.join(base_path).join(hint));
        }
        for root in media_search_roots {
            candidates.push(root.join(hint));
        }
    } else {
        for root in media_search_roots {
            candidates.push(root.join(hint));
        }
    }

    candidates
        .into_iter()
        .find(|p| p.is_file() && path_within_any(p, &allowed))
}

/// Allowed roots for media path checks (search roots plus an absolute `media_base`).
fn allowed_media_roots(media_base: Option<&str>, media_search_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = media_search_roots.to_vec();
    if let Some(base) = media_base.map(str::trim).filter(|s| !s.is_empty()) {
        let base_path = Path::new(base);
        if base_path.is_absolute() {
            roots.push(base_path.to_path_buf());
        }
    }
    roots
}

/// True when `path` resolves to a location under any of `roots`.
fn path_within_any(path: &Path, roots: &[PathBuf]) -> bool {
    let Ok(canon) = fs::canonicalize(path) else {
        return false;
    };
    roots.iter().any(|root| {
        fs::canonicalize(root)
            .ok()
            .is_some_and(|root_canon| canon.starts_with(root_canon))
    })
}

/// WhatsApp `key_id` as a string (empty when missing).
fn key_id_string(msg: &MessageJson) -> String {
    match &msg.key_id {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

/// Compact JSON cell, or empty when `None` / null.
fn optional_json(v: &Option<serde_json::Value>) -> String {
    match v {
        Some(val) if !val.is_null() => json_cell(val),
        _ => String::new(),
    }
}

/// Compact JSON for reactions, or empty when null / empty object.
fn reactions_json(v: &serde_json::Value) -> String {
    if v.is_null() || (v.is_object() && v.as_object().is_some_and(|o| o.is_empty())) {
        String::new()
    } else {
        json_cell(v)
    }
}

/// Sort, drop invalid dates, and return false when nothing remains.
fn prepare_conversation(convo: &mut PendingConversation, report: &mut ExportReport) -> bool {
    convo.messages.sort_by(|a, b| {
        a.sort_key
            .cmp(&b.sort_key)
            .then_with(|| a.extra_str("key_id").cmp(b.extra_str("key_id")))
    });
    message_vault_io_core::prune_and_finish_conversation(convo, report, |k| k / 1000)
}

/// Map a staged attachment onto the shared [`IrAttachment`] shape.
fn pending_attachment_to_ir(a: &PendingAttachment, msg: &PendingMessage) -> IrAttachment {
    IrAttachment {
        path: (!a.rel_path.is_empty()).then(|| a.rel_path.clone()),
        original_name: a.name_hint.clone(),
        mime_type: a.mime_type(),
        digest_sha256: a.digest_sha256.clone(),
        is_sticker: msg.extra_flag("is_sticker"),
        transcription: None,
        sticker_effect: None,
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
    let participants: Vec<IrParticipant> = convo
        .participant_e164s
        .iter()
        .filter(|h| !h.is_empty())
        .map(|h| IrParticipant {
            handle: h.clone(),
            display_name: None,
            handle_type: Some(HandleType::Phone),
        })
        .collect();

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
            .map(|a| a.digest_sha256.clone().unwrap_or_default())
            .collect();
        // key_id uniquely identifies a WhatsApp message; feed it into the GUID
        // so same-second messages with identical text get distinct GUIDs.
        digests.push(msg.extra_str("key_id").to_string());
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &digests);
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
        let whatsapp_jid = convo.extra_str("whatsapp_jid");
        if !whatsapp_jid.is_empty() {
            fields.insert(
                "jid".into(),
                serde_json::Value::String(whatsapp_jid.to_string()),
            );
        }
        let key_id = msg.extra_str("key_id");
        if !key_id.is_empty() {
            fields.insert(
                "key_id".into(),
                serde_json::Value::String(key_id.to_string()),
            );
        }
        let reply_json = msg.extra_str("reply_json");
        if !reply_json.is_empty() {
            fields.insert(
                "reply".into(),
                serde_json::from_str(reply_json)
                    .unwrap_or_else(|_| serde_json::Value::String(reply_json.to_string())),
            );
        }
        let reactions_json = msg.extra_str("reactions_json");
        if !reactions_json.is_empty() {
            fields.insert(
                "reactions".into(),
                serde_json::from_str(reactions_json)
                    .unwrap_or_else(|_| serde_json::Value::String(reactions_json.to_string())),
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
                msg.sender_display_name.clone(),
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
            conversation_type: if convo.is_group {
                IrConversationType::Group
            } else {
                IrConversationType::Individual
            },
            group_title: convo.display_name.clone(),
            participants,
            stats: ConversationStats::default(),
        },
        messages,
        packaging_stem_suffix: Some("__whatsapp".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_media_rejects_absolute_paths_outside_roots() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.bin");
        fs::write(&secret, b"secret").unwrap();
        assert!(
            resolve_media_file(secret.to_str().unwrap(), None, &[root.path().to_path_buf()],)
                .is_none()
        );
    }

    #[test]
    fn resolve_media_rejects_file_only_under_cwd_like_path() {
        // Media roots passed to convert must be explicit (input / JSON parent /
        // work dir). A path that only exists under a separate "CWD-like" tree
        // must not resolve when that tree is omitted from the allowlist.
        let allowed = tempfile::tempdir().unwrap();
        let cwd_like = tempfile::tempdir().unwrap();
        let secret = cwd_like.path().join("media.jpg");
        fs::write(&secret, b"jpeg").unwrap();
        let roots = [allowed.path().to_path_buf()];
        assert!(!path_within_any(&secret, &roots));
        assert!(
            resolve_media_file(secret.to_str().unwrap(), None, &roots).is_none(),
            "file under a non-allowed tree must be rejected"
        );
    }

    #[test]
    fn resolve_media_rejects_dotdot_escape() {
        let root = tempfile::tempdir().unwrap();
        let sibling = root.path().parent().unwrap().join("escape_probe.bin");
        fs::write(&sibling, b"x").unwrap();
        let hint = "../escape_probe.bin";
        assert!(
            resolve_media_file(hint, None, &[root.path().to_path_buf()]).is_none(),
            "relative .. must not escape search roots"
        );
        let _ = fs::remove_file(&sibling);
    }

    #[test]
    fn resolve_media_accepts_path_under_search_root() {
        let root = tempfile::tempdir().unwrap();
        let media_base = "AppDomainGroup-group.net.whatsapp.WhatsApp.shared";
        let rel = "Message/Media/chat/a/b/photo.jpg";
        let src = root.path().join(media_base).join(rel);
        fs::create_dir_all(src.parent().unwrap()).unwrap();
        fs::write(&src, b"jpeg").unwrap();
        let found = resolve_media_file(rel, Some(media_base), &[root.path().to_path_buf()]);
        assert_eq!(found.as_deref(), Some(src.as_path()));
    }
}
