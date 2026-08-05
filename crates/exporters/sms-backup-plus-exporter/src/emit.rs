//! Convert SMS Backup+ `.eml` trees into the common message → packaging via FormatSink.

use crate::archive::parse_archive_eml_mail;
use crate::contacts::{apply_name_mapping, enrich_display_names, fill_unknown_phone};
use crate::flat_eml::{MailHeaders, is_archive_eml, is_flat_sms_eml, parse_flat_eml_mail};
use crate::identity::{chat_id_for, cover_identity, timestamp_ms};
use crate::types::{AttachmentBlob, ParsedMessage};
use anyhow::{Result, bail};
use contacts::{ContactsBook, NameMapping};
use message_csv::{DateRange, format_local_ts, stable_guid};
use message_vault_io_core::{CancelFlag, LogSink, OutputFormat, emit_log};
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
    parse_android_type,
};
use message_ir_format::{
    ExportTransforms,
    FormatSink,
    FormatSinkResult,
    clean_previous_ir_output,
};
use phone::{OwnerPhoneSet, to_e164};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const EXPORT_SOURCE: &str = "sms-backup-plus";
const EXPORT_TOOL: &str = "SMS Backup+";
const EXPORT_TOOL_VERSION: &str = "1.5.11";

#[derive(Debug, Default)]
pub(crate) struct ExportReport {
    pub conversations: u64,
    pub flat_eml: u64,
    pub archive_eml: u64,
    pub messages: u64,
    pub messages_before_dedupe: u64,
    pub duplicates_dropped: u64,
    pub attachments_saved: u64,
    /// Rows written with outgoing direction (after dedupe).
    pub sent: u64,
    /// Rows written with incoming direction (after dedupe).
    pub received: u64,
    /// `.eml` files that are not SMS Backup+ shaped.
    pub skipped_out_of_range: u64,
    pub skipped_not_sms_backup_plus: u64,
    /// SMS Backup+-looking files that failed to parse.
    pub skipped_parse_error: u64,
    /// Messages kept under the `unknown` chat stem (no usable peer phone).
    pub unknown_chat_messages: u64,
    pub skipped_invalid_date: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
struct PendingAttachment {
    rel_path: String,
    original_name: Option<String>,
    mime_type: Option<String>,
    digest_hex: String,
}

#[derive(Debug, Clone)]
struct PendingMessage {
    sort_key: f64,
    is_from_me: bool,
    sender_digits: Option<String>,
    sender_display_name: Option<String>,
    text: String,
    attachments: Vec<PendingAttachment>,
    source_kind: String,
    smssync_id: String,
    date_ms: String,
    contact_name: String,
    android_type: String,
    eml_path: String,
}

#[derive(Debug, Default)]
struct PendingConversation {
    conversation_type: String,
    group_title: Option<String>,
    participant_e164s: Vec<String>,
    messages: Vec<PendingMessage>,
    /// Fingerprint → index in `messages` (online dedupe; keep earliest `sort_key`).
    by_identity: HashMap<String, usize>,
}

fn relative_eml_path(
    eml_path: &Path,
    inputs: &[PathBuf],
    file_inputs: &HashSet<PathBuf>,
) -> String {
    for root in inputs {
        if let Ok(rel) = eml_path.strip_prefix(root) {
            return rel.display().to_string();
        }
        if file_inputs.contains(root) && eml_path == root {
            return root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_else(|| eml_path.to_str().unwrap_or(""))
                .to_string();
        }
    }
    eml_path.display().to_string()
}

/// Write attachment blobs, returning the ones that succeeded.
///
/// A single failing attachment (disk full, permissions, ENAMETOOLONG) must not
/// drop the whole message: the failure is recorded in `report.errors` and the
/// message is kept without that attachment.
fn write_attachments(
    blobs: &[AttachmentBlob],
    attachments_dir: &Path,
    report: &mut ExportReport,
    copy_attachments: bool,
    path_display: &str,
) -> Vec<PendingAttachment> {
    if !copy_attachments {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(blobs.len());
    for blob in blobs {
        let path = attachments_dir.join(&blob.filename);
        if !path.exists() {
            if let Err(err) = fs::write(&path, &blob.data) {
                report.errors.push(format!(
                    "{path_display}: failed to write attachment {}: {err}",
                    blob.filename
                ));
                continue;
            }
            report.attachments_saved += 1;
        }
        out.push(PendingAttachment {
            rel_path: format!("attachments/{}", blob.filename),
            original_name: blob.original_name.clone(),
            mime_type: blob.mime_type.clone(),
            digest_hex: blob.digest_hex.clone(),
        });
    }
    out
}

fn ensure_convo<'a>(
    map: &'a mut HashMap<String, PendingConversation>,
    chat_id: &str,
    conversation_type: &str,
    group_title: Option<String>,
    participant_e164s: Vec<String>,
) -> &'a mut PendingConversation {
    // Avoid allocating a new String on every message for an existing chat.
    if !map.contains_key(chat_id) {
        map.insert(
            chat_id.to_string(),
            PendingConversation {
                conversation_type: conversation_type.to_string(),
                group_title,
                participant_e164s: Vec::new(),
                messages: Vec::new(),
                by_identity: HashMap::new(),
            },
        );
    }
    let convo = map
        .get_mut(chat_id)
        .expect("just inserted or already present");
    // Union rosters across messages: group membership changes over time, and a
    // later message's smaller roster must not shrink the participant list. (A
    // roster change that yields a different chat_key still splits the
    // conversation into fragments; this keeps each fragment's participant list
    // complete within that key.)
    convo.participant_e164s.extend(participant_e164s);
    convo.participant_e164s.sort();
    convo.participant_e164s.dedup();
    convo
}

/// Prefer flat over archive (richer metadata); otherwise keep the earlier timestamp.
fn should_replace_kept(existing: &PendingMessage, incoming: &ParsedMessage) -> bool {
    let existing_flat = existing.source_kind == "flat";
    let incoming_flat = incoming.source_kind == "flat";
    if incoming_flat && !existing_flat {
        return true;
    }
    if !incoming_flat && existing_flat {
        return false;
    }
    if incoming_flat
        && existing_flat
        && incoming
            .smssync_id
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty())
        && existing.smssync_id.trim().is_empty()
    {
        return true;
    }
    incoming.timestamp_secs < existing.sort_key
}

/// Union attachment lists by content digest so flat↔archive dedupe does not drop media.
fn merge_attachments(into: &mut Vec<PendingAttachment>, from: Vec<PendingAttachment>) {
    let mut seen: HashSet<String> = into.iter().map(|a| a.digest_hex.clone()).collect();
    for att in from {
        if seen.insert(att.digest_hex.clone()) {
            into.push(att);
        }
    }
}

fn pending_from_parsed(msg: ParsedMessage, pending_atts: Vec<PendingAttachment>) -> PendingMessage {
    let date_ms = timestamp_ms(msg.timestamp_secs).to_string();
    let name = msg.name_hint.clone().unwrap_or_default();
    PendingMessage {
        sort_key: msg.timestamp_secs,
        is_from_me: msg.is_from_me,
        sender_digits: msg.sender_digits,
        sender_display_name: msg.name_hint,
        text: msg.text,
        attachments: pending_atts,
        source_kind: msg.source_kind,
        smssync_id: msg.smssync_id.unwrap_or_default(),
        date_ms,
        contact_name: name,
        android_type: msg.android_type,
        eml_path: msg.eml_path,
    }
}

fn add_message(
    conversations: &mut HashMap<String, PendingConversation>,
    msg: ParsedMessage,
    pending_atts: Vec<PendingAttachment>,
    report: &mut ExportReport,
) {
    let chat_id = chat_id_for(&msg);
    let dedupe_key = cover_identity(&msg);

    let peers: Vec<String> = msg
        .participant_digits
        .iter()
        .map(|(d, _)| to_e164(d))
        .filter(|d| !d.is_empty())
        .collect();
    let convo = ensure_convo(
        conversations,
        &chat_id,
        &msg.conversation_type,
        msg.group_title.clone(),
        peers,
    );

    report.messages_before_dedupe += 1;

    if let Some(&idx) = convo.by_identity.get(&dedupe_key) {
        report.duplicates_dropped += 1;
        if should_replace_kept(&convo.messages[idx], &msg) {
            let kept_atts = std::mem::take(&mut convo.messages[idx].attachments);
            let mut pending = pending_from_parsed(msg, pending_atts);
            merge_attachments(&mut pending.attachments, kept_atts);
            convo.messages[idx] = pending;
        } else {
            merge_attachments(&mut convo.messages[idx].attachments, pending_atts);
        }
        return;
    }

    let idx = convo.messages.len();
    convo.by_identity.insert(dedupe_key, idx);
    convo.messages.push(pending_from_parsed(msg, pending_atts));
}

fn prepare_conversation(convo: &mut PendingConversation, report: &mut ExportReport) -> bool {
    if convo.messages.is_empty() {
        return false;
    }
    convo.messages.sort_by(|a, b| {
        a.sort_key
            .partial_cmp(&b.sort_key)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    convo.messages.retain(|m| {
        if format_local_ts(m.sort_key as i64).is_some() {
            true
        } else {
            report.skipped_invalid_date += 1;
            false
        }
    });
    !convo.messages.is_empty()
}

fn display_names_for_handles(convo: &PendingConversation) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for msg in &convo.messages {
        if let Some(digits) = &msg.sender_digits {
            let handle = to_e164(digits);
            if let Some(name) = msg
                .sender_display_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
            {
                names.entry(handle).or_insert_with(|| name.to_string());
            }
        }
        if convo.conversation_type == "individual" {
            let name = msg.contact_name.trim();
            if !name.is_empty() {
                for peer in &convo.participant_e164s {
                    names
                        .entry(peer.clone())
                        .or_insert_with(|| name.to_string());
                }
            }
        }
    }
    names
}

fn pending_to_document(
    chat_id: &str,
    convo: &PendingConversation,
    owner_handle: &str,
    report: &mut ExportReport,
) -> Result<ConversationDocument> {
    let name_by_handle = display_names_for_handles(convo);
    let mut participants: Vec<IrParticipant> = convo
        .participant_e164s
        .iter()
        .filter(|h| !h.is_empty())
        .map(|h| IrParticipant {
            handle: h.clone(),
            display_name: name_by_handle.get(h).cloned(),
        })
        .collect();
    if participants.is_empty() && convo.conversation_type == "individual" && !chat_id.is_empty() {
        participants.push(IrParticipant {
            handle: chat_id.to_string(),
            display_name: name_by_handle.get(chat_id).cloned().or_else(|| {
                convo
                    .messages
                    .iter()
                    .map(|m| m.contact_name.trim())
                    .find(|n| !n.is_empty())
                    .map(str::to_string)
            }),
        });
    }

    let export = ExportMeta {
        source: EXPORT_SOURCE.into(),
        tool: EXPORT_TOOL.into(),
        tool_version: EXPORT_TOOL_VERSION.into(),
        owner_handle: Some(owner_handle.to_string()),
        owner_display_name: None,
    };
    let (owner_sender_handle, owner_sender_display) = owner_sender(&export);

    let mut messages = Vec::with_capacity(convo.messages.len());
    for msg in &convo.messages {
        if msg.is_from_me {
            report.sent += 1;
        } else {
            report.received += 1;
        }
        report.messages += 1;
        let secs = msg.sort_key as i64;
        let (ts_local, _, _) = format_local_ts(secs).expect("timestamp validated above");
        let digests: Vec<String> = msg
            .attachments
            .iter()
            .map(|a| a.digest_hex.clone())
            .collect();
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &digests);
        let timestamp_unix_ms = msg
            .date_ms
            .parse::<i64>()
            .unwrap_or_else(|_| secs.saturating_mul(1000));
        let (sender_handle, sender_display_name) = if msg.is_from_me {
            (owner_sender_handle.clone(), owner_sender_display.clone())
        } else {
            (
                msg.sender_digits.as_ref().map(|d| to_e164(d)),
                msg.sender_display_name.clone(),
            )
        };
        let attachments: Vec<IrAttachment> = msg
            .attachments
            .iter()
            .map(|a| IrAttachment {
                path: Some(a.rel_path.clone()),
                original_name: a.original_name.clone(),
                mime_type: a.mime_type.clone(),
                digest_sha256: Some(a.digest_hex.clone()),
                is_sticker: false,
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

        let mut fields = serde_json::Map::new();
        if !msg.source_kind.is_empty() {
            fields.insert(
                "source_kind".into(),
                serde_json::Value::String(msg.source_kind.clone()),
            );
        }
        if !msg.smssync_id.is_empty() {
            fields.insert(
                "smssync_id".into(),
                serde_json::Value::String(msg.smssync_id.clone()),
            );
        }
        if !msg.eml_path.is_empty() {
            fields.insert(
                "eml_path".into(),
                serde_json::Value::String(msg.eml_path.clone()),
            );
        }
        if let Some(title) = convo.group_title.as_deref().filter(|t| !t.is_empty()) {
            // Synthetic group label; kept as data, not used for filenames.
            fields.insert(
                "android_group_title".into(),
                serde_json::Value::String(title.to_string()),
            );
        }
        let source = IrSource {
            android_type: parse_android_type(&msg.android_type),
            fields,
        }
        .into_option();

        messages.push(IrMessage {
            guid,
            timestamp_unix_ms,
            direction: if msg.is_from_me {
                IrDirection::Outgoing
            } else {
                IrDirection::Incoming
            },
            service: IrService::Sms,
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
            // Synthetic Android group titles are not used for filenames.
            group_title: None,
            participants,
            stats: ConversationStats::default(),
        },
        messages,
        packaging_stem_suffix: None,
    })
}

fn collect_eml_paths<P: AsRef<Path>>(
    inputs: &[P],
    cancel: Option<&CancelFlag>,
) -> Result<Vec<PathBuf>> {
    if inputs.is_empty() {
        bail!("at least one --input path is required");
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>, cancel: Option<&CancelFlag>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
            let entry = entry?;
            let ft = entry.file_type()?;
            let path = entry.path();
            if ft.is_dir() {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(name.as_str(), "duplicate" | "exclude" | ".git") {
                    continue;
                }
                walk(&path, out, cancel)?;
            } else if ft.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("eml"))
            {
                out.push(path);
            }
        }
        Ok(())
    }

    let mut paths = Vec::new();
    for input in inputs {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        let input = input.as_ref();
        if input.is_file() {
            if input
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("eml"))
            {
                paths.push(input.to_path_buf());
            } else {
                bail!("input file is not .eml: {}", input.display());
            }
            continue;
        }
        if !input.is_dir() {
            bail!("input is not a file or directory: {}", input.display());
        }
        walk(input, &mut paths, cancel)?;
    }

    // Stable order for deterministic CSV dedupe winners when timestamps tie.
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        let listed = inputs
            .iter()
            .map(|p| p.as_ref().display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        bail!("no .eml files under: {listed}");
    }
    Ok(paths)
}

/// Per-file parse result produced in parallel; merged serially into conversations.
enum ParsedEmlKind {
    Archive {
        msgs: Vec<ParsedMessage>,
        skipped_dates: u64,
        path_display: String,
    },
    Flat {
        msg: ParsedMessage,
        path_display: String,
    },
    FlatNone,
    NotSms,
    IoError(String),
    ParseError(String),
}

fn parse_one_eml(
    eml_path: &Path,
    rel_path: String,
    owner_digits: &HashSet<String>,
    owner_emails_lc: &[String],
    contacts: &ContactsBook,
    name_mapping: &NameMapping,
) -> ParsedEmlKind {
    let bytes = match std::fs::read(eml_path) {
        Ok(b) => b,
        Err(err) => {
            return ParsedEmlKind::IoError(format!("{}: {err}", eml_path.display()));
        }
    };
    let mail = match mailparse::parse_mail(&bytes) {
        Ok(m) => m,
        Err(err) => {
            return ParsedEmlKind::ParseError(format!("{}: parse EML: {err}", eml_path.display()));
        }
    };
    let headers = MailHeaders::from_mail(&mail);
    let path_display = eml_path.display().to_string();

    if is_archive_eml(&headers) {
        match parse_archive_eml_mail(eml_path, &mail, &headers) {
            Ok((mut msgs, skipped_dates)) => {
                for msg in &mut msgs {
                    msg.eml_path = rel_path.clone();
                    let _ = apply_name_mapping(msg, name_mapping, contacts);
                    let _ = fill_unknown_phone(msg, contacts);
                    enrich_display_names(msg, contacts);
                }
                ParsedEmlKind::Archive {
                    msgs,
                    skipped_dates,
                    path_display,
                }
            }
            Err(err) => ParsedEmlKind::ParseError(format!("{path_display}: {err:#}")),
        }
    } else if is_flat_sms_eml(&headers) {
        match parse_flat_eml_mail(eml_path, &mail, &headers, owner_digits, owner_emails_lc) {
            Ok(Some(mut msg)) => {
                msg.eml_path = rel_path;
                let _ = apply_name_mapping(&mut msg, name_mapping, contacts);
                let _ = fill_unknown_phone(&mut msg, contacts);
                enrich_display_names(&mut msg, contacts);
                ParsedEmlKind::Flat { msg, path_display }
            }
            Ok(None) => ParsedEmlKind::FlatNone,
            Err(err) => ParsedEmlKind::ParseError(format!("{path_display}: {err:#}")),
        }
    } else {
        ParsedEmlKind::NotSms
    }
}

const EML_PROGRESS_EVERY: u64 = 5000;

fn vlog(verbose: bool, log: Option<&LogSink>, msg: impl AsRef<str>) {
    if verbose {
        emit_log(log, msg);
    }
}

fn report_progress(
    verbose: bool,
    log: Option<&LogSink>,
    label: &str,
    processed: u64,
    total: u64,
) {
    if !verbose || total == 0 {
        return;
    }
    let every = EML_PROGRESS_EVERY;
    if processed == total || (every > 0 && processed.is_multiple_of(every)) {
        emit_log(log, format!("{label}: {processed} / {total}"));
    }
}

/// Convert SMS Backup+ EML tree(s) into per-conversation CSV, EML, or MBOX.
///
/// Deduplication runs while scanning, using [`cover_identity`] (second-floored
/// chat + direction + text) so archive and flat copies of the same SMS collapse.
/// When `cancel` is set, cooperative cancellation is checked during the EML walk
/// and while merging parse results.
pub(crate) fn convert_export<P: AsRef<Path>>(
    inputs: &[P],
    output_dir: &Path,
    owner_phones: &[String],
    owner_emails: &[String],
    contacts: &ContactsBook,
    name_mapping: &NameMapping,
    date_range: &DateRange,
    verbose: bool,
    transforms: ExportTransforms,
    output_format: OutputFormat,
    cancel: Option<&CancelFlag>,
    log: Option<&LogSink>,
) -> Result<(ExportReport, FormatSinkResult)> {
    let owners = OwnerPhoneSet::new(owner_phones)?;
    let owner_handle = to_e164(&owners.primary_digits);
    let owner_emails_lc: Vec<String> = owner_emails
        .iter()
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect();
    let mut report = ExportReport::default();
    let mut conversations: HashMap<String, PendingConversation> = HashMap::new();

    vlog(
        verbose,
        log,
        format!("owner phones: {}", owners.all_digits.len()),
    );
    vlog(
        verbose,
        log,
        format!("owner emails: {}", owner_emails_lc.len()),
    );
    vlog(
        verbose,
        log,
        format!("contacts entries (by phone): {}", contacts.len()),
    );
    vlog(verbose, log, format!("output: {}", output_dir.display()));

    fs::create_dir_all(output_dir)?;
    clean_previous_ir_output(output_dir)?;
    let copy_attachments = transforms.copies_attachments();
    let attachments_dir = output_dir.join("attachments");
    if copy_attachments {
        fs::create_dir_all(&attachments_dir)?;
    }

    let input_roots: Vec<PathBuf> = inputs.iter().map(|p| p.as_ref().to_path_buf()).collect();
    let file_inputs: HashSet<PathBuf> = input_roots
        .iter()
        .filter(|p| p.is_file())
        .cloned()
        .collect();

    let eml_paths = collect_eml_paths(inputs, cancel)?;
    let total = eml_paths.len() as u64;
    vlog(
        verbose,
        log,
        format!("scanning {total} .eml files (parallel parse)"),
    );
    // Pre-size for typical 1:1 chat counts; grows as needed.
    conversations.reserve((total / 4).min(50_000) as usize);

    message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;

    // Parallel: read + MIME parse + message build. Serial: attachment write + dedupe merge.
    let outcomes: Vec<ParsedEmlKind> = eml_paths
        .par_iter()
        .map(|eml_path| {
            let rel_path = relative_eml_path(eml_path, &input_roots, &file_inputs);
            parse_one_eml(
                eml_path,
                rel_path,
                &owners.all_digits,
                &owner_emails_lc,
                contacts,
                name_mapping,
            )
        })
        .collect();

    for (idx, outcome) in outcomes.into_iter().enumerate() {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        report_progress(verbose, log, "scanned", (idx + 1) as u64, total);
        match outcome {
            ParsedEmlKind::Archive {
                msgs,
                skipped_dates,
                path_display,
            } => {
                report.archive_eml += 1;
                report.skipped_invalid_date += skipped_dates;
                for msg in msgs {
                    if !date_range.contains_secs_f64(msg.timestamp_secs) {
                        report.skipped_out_of_range += 1;
                        continue;
                    }
                    if msg.chat_key.is_empty() {
                        report.unknown_chat_messages += 1;
                    }
                    // Keep the message even when some attachments fail to write.
                    let atts = write_attachments(
                        &msg.attachments,
                        &attachments_dir,
                        &mut report,
                        copy_attachments,
                        &path_display,
                    );
                    add_message(&mut conversations, msg, atts, &mut report);
                }
            }
            ParsedEmlKind::Flat { msg, path_display } => {
                report.flat_eml += 1;
                if !date_range.contains_secs_f64(msg.timestamp_secs) {
                    report.skipped_out_of_range += 1;
                    continue;
                }
                if msg.chat_key.is_empty() {
                    report.unknown_chat_messages += 1;
                }
                // Keep the message even when some attachments fail to write.
                let atts = write_attachments(
                    &msg.attachments,
                    &attachments_dir,
                    &mut report,
                    copy_attachments,
                    &path_display,
                );
                add_message(&mut conversations, msg, atts, &mut report);
            }
            ParsedEmlKind::FlatNone => {
                report.skipped_parse_error += 1;
            }
            ParsedEmlKind::NotSms => {
                report.skipped_not_sms_backup_plus += 1;
            }
            ParsedEmlKind::IoError(msg) => {
                report.errors.push(msg);
            }
            ParsedEmlKind::ParseError(msg) => {
                report.skipped_parse_error += 1;
                report.errors.push(msg);
            }
        }
    }

    vlog(
        verbose,
        log,
        format!(
            "parsed: flat_eml={} archive_eml={} messages={} unknown_chat={} skipped_not_sms_backup_plus={} skipped_parse_error={} skipped_bad_date={}",
            report.flat_eml,
            report.archive_eml,
            report.messages_before_dedupe,
            report.unknown_chat_messages,
            report.skipped_not_sms_backup_plus,
            report.skipped_parse_error,
            report.skipped_invalid_date
        ),
    );

    let convo_total = conversations.len() as u64;
    vlog(
        verbose,
        log,
        format!(
            "writing {convo_total} conversation files (duplicates dropped so far: {})",
            report.duplicates_dropped
        ),
    );
    let mut sink = FormatSink::open(output_dir, output_format, transforms)?;
    let mut written = 0u64;
    for (chat_id, mut convo) in conversations {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        if !prepare_conversation(&mut convo, &mut report) {
            written += 1;
            report_progress(verbose, log, "wrote", written, convo_total);
            continue;
        }
        let doc = pending_to_document(&chat_id, &convo, &owner_handle, &mut report)?;
        sink.write_document(doc)?;
        report.conversations += 1;
        written += 1;
        report_progress(verbose, log, "wrote", written, convo_total);
    }
    let sink_result = sink.finish()?;

    vlog(
        verbose,
        log,
        format!(
            "done: conversations={} messages={} duplicates_dropped={} attachments={}",
            report.conversations,
            report.messages,
            report.duplicates_dropped,
            report.attachments_saved
        ),
    );
    if verbose && !report.errors.is_empty() {
        emit_log(log, format!("errors: {}", report.errors.len()));
        for err in report.errors.iter().take(20) {
            emit_log(log, format!("  {err}"));
        }
        if report.errors.len() > 20 {
            emit_log(
                log,
                format!("  … and {} more", report.errors.len() - 20),
            );
        }
    }

    Ok((report, sink_result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_attachments_unions_by_digest() {
        let mut into = vec![PendingAttachment {
            rel_path: "attachments/a.jpg".into(),
            original_name: Some("a.jpg".into()),
            mime_type: Some("image/jpeg".into()),
            digest_hex: "aaa".into(),
        }];
        let from = vec![
            PendingAttachment {
                rel_path: "attachments/a.jpg".into(),
                original_name: Some("a.jpg".into()),
                mime_type: Some("image/jpeg".into()),
                digest_hex: "aaa".into(),
            },
            PendingAttachment {
                rel_path: "attachments/b.jpg".into(),
                original_name: Some("b.jpg".into()),
                mime_type: Some("image/jpeg".into()),
                digest_hex: "bbb".into(),
            },
        ];
        merge_attachments(&mut into, from);
        assert_eq!(into.len(), 2);
        assert_eq!(into[1].digest_hex, "bbb");
    }

    #[test]
    fn write_attachments_keeps_message_on_single_failure() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        // A NUL byte is never a valid path component, so this write always
        // fails (EINVAL on Unix, invalid name on Windows).
        let blobs = vec![
            AttachmentBlob {
                filename: "bad\u{0}name.jpg".into(),
                original_name: None,
                mime_type: None,
                digest_hex: "aaa".into(),
                data: vec![1, 2, 3],
            },
            AttachmentBlob {
                filename: "ok.jpg".into(),
                original_name: None,
                mime_type: None,
                digest_hex: "bbb".into(),
                data: vec![4, 5, 6],
            },
        ];
        let mut report = ExportReport::default();
        let out = write_attachments(&blobs, &att_dir, &mut report, true, "msg.eml");
        // The failing attachment is dropped, the good one survives, and the
        // failure is recorded instead of aborting the whole message.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].digest_hex, "bbb");
        assert!(att_dir.join("ok.jpg").exists());
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("failed to write attachment"));
        assert_eq!(report.attachments_saved, 1);
    }
}
