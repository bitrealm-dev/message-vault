//! Convert SMS Backup+ `.eml` trees into the shared conversation structure,
//! then write the chosen output format via [`FormatSink`].

use crate::archive::parse_archive_eml_mail;
use crate::contacts::{apply_name_mapping, enrich_display_names, fill_unknown_phone};
use crate::flat_eml::{MailHeaders, is_archive_eml, is_flat_sms_eml, parse_flat_eml_mail};
use crate::identity::{chat_id_for, cover_identity, timestamp_ms};
use crate::types::{AttachmentBlob, ParsedMessage};
use anyhow::{Context, Result, bail};
use contacts::{ContactsBook, NameMapping};
use message_csv::{DateRange, format_local_ts, stable_guid};
use message_ir::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, HandleType,
    IrAttachment, IrConversationType, IrDirection, IrMessage, IrMessageKind, IrParticipant,
    IrService, IrSource, PendingAttachment, PendingConversation, PendingMessage, SCHEMA_VERSION,
    owner_sender, parse_android_type,
};
use message_ir_format::{ExportTransforms, FormatSink, FormatSinkResult};
use message_vault_io_core::{CancelFlag, ExportReport, LogSink, OutputFormat, emit_log};
use phone::OwnerHandleSet;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const EXPORT_SOURCE: &str = "sms-backup-plus";
const EXPORT_TOOL: &str = "SMS Backup+";
const EXPORT_TOOL_VERSION: &str = "1.5.11";

/// Read a per-exporter counter from the report's `extra` map.
fn count(report: &ExportReport, key: &str) -> u64 {
    report.extra.get(key).copied().unwrap_or(0)
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
            content_type: blob.mime_type.clone().unwrap_or_default(),
            extension: Path::new(&blob.filename)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string(),
            digest_sha256: Some(blob.digest_hex.clone()),
            name_hint: blob.original_name.clone(),
        });
    }
    out
}

fn ensure_convo<'a>(
    map: &'a mut HashMap<String, PendingConversation>,
    chat_id: &str,
    is_group: bool,
    display_name: Option<String>,
    participant_e164s: Vec<String>,
) -> &'a mut PendingConversation {
    // Avoid allocating a new String on every message for an existing chat.
    if !map.contains_key(chat_id) {
        map.insert(
            chat_id.to_string(),
            PendingConversation {
                chat_id: chat_id.to_string(),
                display_name,
                participant_e164s: Vec::new(),
                messages: Vec::new(),
                is_group,
                has_attachments: false,
                extra: BTreeMap::new(),
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
    let existing_flat = existing.extra_str("source_kind") == "flat";
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
        && existing.extra_str("smssync_id").trim().is_empty()
    {
        return true;
    }
    (incoming.timestamp_secs as i64) < existing.sort_key
}

/// Union attachment lists by content digest so flat↔archive dedupe does not drop media.
fn merge_attachments(into: &mut Vec<PendingAttachment>, from: Vec<PendingAttachment>) {
    let mut seen: HashSet<String> = into
        .iter()
        .map(|a| a.digest_sha256.clone().unwrap_or_default())
        .collect();
    for att in from {
        if seen.insert(att.digest_sha256.clone().unwrap_or_default()) {
            into.push(att);
        }
    }
}

fn pending_from_parsed(msg: ParsedMessage, pending_atts: Vec<PendingAttachment>) -> PendingMessage {
    let date_ms = timestamp_ms(msg.timestamp_secs).to_string();
    let name = msg.name_alias.clone().unwrap_or_default();
    PendingMessage {
        sort_key: msg.timestamp_secs as i64,
        is_from_me: msg.is_from_me,
        sender_handle: msg.sender_digits.unwrap_or_default(),
        sender_display_name: msg.name_alias,
        text: msg.text,
        attachments: pending_atts,
        extra: {
            let mut e = BTreeMap::new();
            e.insert("source_kind".into(), msg.source_kind);
            e.insert("smssync_id".into(), msg.smssync_id.unwrap_or_default());
            e.insert("date_ms".into(), date_ms);
            e.insert("contact_name".into(), name);
            e.insert("android_type".into(), msg.android_type);
            e.insert("eml_path".into(), msg.eml_path);
            e
        },
    }
}

fn add_message(
    conversations: &mut HashMap<String, PendingConversation>,
    by_identity: &mut HashMap<String, HashMap<String, usize>>,
    msg: ParsedMessage,
    pending_atts: Vec<PendingAttachment>,
    report: &mut ExportReport,
) {
    let chat_id = chat_id_for(&msg);
    let dedupe_key = cover_identity(&msg);

    let peers: Vec<String> = peer_handles_from_digits(&msg.participant_digits);
    let convo = ensure_convo(
        conversations,
        &chat_id,
        msg.conversation_type == "group",
        msg.group_title.clone(),
        peers,
    );

    report.bump("messages_before_dedupe", 1);

    // Online dedupe state keyed by chat id: fingerprint → index in `messages`
    // (keep earliest `sort_key`). The shared PendingConversation carries
    // document data only.
    let idx_map = by_identity.entry(chat_id.clone()).or_default();

    if let Some(&idx) = idx_map.get(&dedupe_key) {
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
    idx_map.insert(dedupe_key, idx);
    convo.messages.push(pending_from_parsed(msg, pending_atts));
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
    convo.has_attachments = convo.messages.iter().any(|m| !m.attachments.is_empty());
    !convo.messages.is_empty()
}

fn display_names_for_handles(convo: &PendingConversation) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for msg in &convo.messages {
        if !msg.sender_handle.is_empty() {
            let handle = phone::normalize_guarded(
                &msg.sender_handle,
                phone::PhoneRegion::for_raw(&msg.sender_handle),
            )
            .normalized;
            if let Some(name) = msg
                .sender_display_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
            {
                names.entry(handle).or_insert_with(|| name.to_string());
            }
        }
        if !convo.is_group {
            let name = msg.extra_str("contact_name").trim();
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

/// Format each participant's digits as E.164 when unambiguous, dropping empties.
fn peer_handles_from_digits(participant_digits: &[(String, Option<String>)]) -> Vec<String> {
    participant_digits
        .iter()
        .map(|(d, _)| phone::normalize_guarded(d, phone::PhoneRegion::for_raw(d)).normalized)
        .filter(|d| !d.is_empty())
        .collect()
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
fn pending_attachment_to_ir(a: &PendingAttachment) -> IrAttachment {
    IrAttachment {
        path: Some(a.rel_path.clone()),
        original_name: a.name_hint.clone(),
        mime_type: a.mime_type(),
        digest_sha256: a.digest_sha256.clone(),
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        size_bytes: None,
        missing_reason: None,
        bytes: None,
    }
}

/// True when the path has a `.eml` extension (any case).
fn is_eml_file(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("eml"))
}

/// Build a [`ConversationDocument`] from one pending conversation.
///
/// Currently always returns `Ok`. The `Result` matches the other exporters.
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
            handle_type: Some(HandleType::Phone),
        })
        .collect();
    if participants.is_empty() && !convo.is_group && !chat_id.is_empty() {
        participants.push(IrParticipant {
            handle: chat_id.to_string(),
            display_name: name_by_handle
                .get(chat_id)
                .cloned()
                .or_else(|| first_contact_name(convo)),
            handle_type: Some(HandleType::Phone),
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
        let secs = msg.sort_key;
        let (ts_local, _, _) = format_local_ts(secs).expect("timestamp validated above");
        let digests: Vec<String> = msg
            .attachments
            .iter()
            .map(|a| a.digest_sha256.clone().unwrap_or_default())
            .collect();
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &digests);
        let timestamp_unix_ms = msg
            .extra_str("date_ms")
            .parse::<i64>()
            .unwrap_or_else(|_| secs.saturating_mul(1000));
        let (sender_handle, sender_display_name) = if msg.is_from_me {
            (owner_sender_handle.clone(), owner_sender_display.clone())
        } else {
            (
                if msg.sender_handle.is_empty() {
                    None
                } else {
                    Some(
                        phone::normalize_guarded(
                            &msg.sender_handle,
                            phone::PhoneRegion::for_raw(&msg.sender_handle),
                        )
                        .normalized,
                    )
                },
                msg.sender_display_name.clone(),
            )
        };
        let attachments: Vec<IrAttachment> = msg
            .attachments
            .iter()
            .map(pending_attachment_to_ir)
            .collect();
        let message_kind = if msg.attachments.is_empty() {
            IrMessageKind::Sms
        } else {
            IrMessageKind::Mms
        };

        let mut fields = serde_json::Map::new();
        let source_kind = msg.extra_str("source_kind");
        if !source_kind.is_empty() {
            fields.insert(
                "source_kind".into(),
                serde_json::Value::String(source_kind.to_string()),
            );
        }
        let smssync_id = msg.extra_str("smssync_id");
        if !smssync_id.is_empty() {
            fields.insert(
                "smssync_id".into(),
                serde_json::Value::String(smssync_id.to_string()),
            );
        }
        let eml_path = msg.extra_str("eml_path");
        if !eml_path.is_empty() {
            fields.insert(
                "eml_path".into(),
                serde_json::Value::String(eml_path.to_string()),
            );
        }
        if let Some(title) = convo.display_name.as_deref().filter(|t| !t.is_empty()) {
            // Android group title stored as data only. Filenames do not use it.
            fields.insert(
                "android_group_title".into(),
                serde_json::Value::String(title.to_string()),
            );
        }
        let source = IrSource {
            android_type: parse_android_type(msg.extra_str("android_type")),
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
            conversation_type: if convo.is_group {
                IrConversationType::Group
            } else {
                IrConversationType::Individual
            },
            // Android group titles are not used for filenames.
            group_title: None,
            participants,
            stats: ConversationStats::default(),
        },
        messages,
        packaging_stem_suffix: None,
    })
}

/// Collect `.eml` paths from files and directories, skipping `duplicate` /
/// `exclude` / `.git` folders.
///
/// # Errors
///
/// Returns an error when an input is neither a file nor a directory, a file is
/// not `.eml`, no `.eml` files are found, or the user cancels.
fn collect_eml_paths<P: AsRef<Path>>(
    inputs: &[P],
    cancel: Option<&CancelFlag>,
) -> Result<Vec<PathBuf>> {
    if inputs.is_empty() {
        bail!("at least one --input path is required");
    }

    // Preserve the previous behavior of never descending into these directories.
    fn in_skipped_dir(path: &Path) -> bool {
        path.components().any(|c| {
            matches!(
                c.as_os_str()
                    .to_str()
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("duplicate" | "exclude" | ".git")
            )
        })
    }

    let mut paths = Vec::new();
    for input in inputs {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        let input = input.as_ref();
        if input.is_file() {
            if is_eml_file(input) {
                paths.push(input.to_path_buf());
            } else {
                bail!("input file is not .eml: {}", input.display());
            }
            continue;
        }
        if !input.is_dir() {
            bail!("input is not a file or directory: {}", input.display());
        }
        let mut found = message_vault_io_core::discover_files(input, &is_eml_file)?;
        found.retain(|p| !in_skipped_dir(p));
        paths.extend(found);
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
        msg: Box<ParsedMessage>,
        path_display: String,
    },
    FlatNone,
    NotSms,
    IoError(String),
    ParseError(String),
    /// Cooperative cancel observed at the start of a parallel worker.
    Cancelled,
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
                ParsedEmlKind::Flat {
                    msg: Box::new(msg),
                    path_display,
                }
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

fn report_progress(verbose: bool, log: Option<&LogSink>, label: &str, processed: u64, total: u64) {
    if !verbose || total == 0 {
        return;
    }
    let every = EML_PROGRESS_EVERY;
    if processed == total || (every > 0 && processed.is_multiple_of(every)) {
        emit_log(log, format!("{label}: {processed} / {total}"));
    }
}

/// Inputs for [`convert_export`].
pub(crate) struct ConvertExportArgs<'a, P: AsRef<Path>> {
    pub inputs: &'a [P],
    pub output_dir: &'a Path,
    pub owner_phones: &'a [String],
    pub owner_emails: &'a [String],
    pub contacts: &'a ContactsBook,
    pub name_mapping: &'a NameMapping,
    pub date_range: &'a DateRange,
    pub verbose: bool,
    pub transforms: ExportTransforms,
    pub output_format: OutputFormat,
    pub cancel: Option<&'a CancelFlag>,
    pub log: Option<&'a LogSink>,
}

/// Convert SMS Backup+ EML tree(s) into the shared conversation structure, then
/// write the chosen output format.
///
/// Deduplication runs while scanning, using [`cover_identity`] (second-floored
/// chat + direction + text) so archive and flat copies of the same SMS collapse.
/// When `cancel` is set, cooperative cancellation is checked during the EML walk
/// and while merging parse results.
///
/// # Errors
///
/// Returns an error when no `.eml` files are found, output overlaps an input,
/// a file cannot be read or written, or the user cancels.
pub(crate) fn convert_export<P: AsRef<Path>>(
    args: ConvertExportArgs<'_, P>,
) -> Result<(ExportReport, FormatSinkResult)> {
    let ConvertExportArgs {
        inputs,
        output_dir,
        owner_phones,
        owner_emails,
        contacts,
        name_mapping,
        date_range,
        verbose,
        transforms,
        output_format,
        cancel,
        log,
    } = args;
    let owners = OwnerHandleSet::from_phones(owner_phones)?;
    let primary = owners
        .primary_phone_digit()
        .context("owner phone has no usable digits")?;
    let guarded = phone::normalize_guarded(primary, phone::PhoneRegion::Usa);
    let owner_handle = guarded.normalized;
    let owner_emails_lc: Vec<String> = owner_emails
        .iter()
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect();
    let mut report = ExportReport::default();
    let mut conversations: HashMap<String, PendingConversation> = HashMap::new();
    // Online dedupe state (fingerprint → message index) keyed by chat id;
    // the shared PendingConversation carries document data only.
    let mut by_identity: HashMap<String, HashMap<String, usize>> = HashMap::new();

    vlog(
        verbose,
        log,
        format!("owner phones: {}", owners.all_phone_digits().len()),
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

    fs::create_dir_all(output_dir).with_context(|| format!("create {}", output_dir.display()))?;
    // Resolve to absolute paths so relative inputs work and so the output/input
    // overlap check uses the same path form. Cleaning the output before reading
    // the input would otherwise delete leftover export CSVs/JSON inside a backup
    // tree when output points at (or contains) an input root.
    let output_dir = fs::canonicalize(output_dir)
        .with_context(|| format!("resolve {}", output_dir.display()))?;
    for input in inputs {
        let input = input.as_ref();
        let input =
            fs::canonicalize(input).with_context(|| format!("resolve {}", input.display()))?;
        if output_dir == input || input.starts_with(&output_dir) {
            bail!(
                "output {} must not be the same as, or contain, the input {}",
                output_dir.display(),
                input.display()
            );
        }
    }

    let copy_attachments = transforms.copies_attachments();
    let (mut sink, attachments_dir) =
        FormatSink::open_prepared(&output_dir, output_format, transforms)?;

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

    let owner_all_digits = owners.all_phone_digits();

    // Parallel parse in chunks so attachment payloads are not all held at once.
    // Each worker checks cancel before reading an EML.
    const EML_PARSE_CHUNK: usize = 256;
    let mut scanned: u64 = 0;
    for chunk in eml_paths.chunks(EML_PARSE_CHUNK) {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        let outcomes: Vec<ParsedEmlKind> = chunk
            .par_iter()
            .map(|eml_path| {
                if message_vault_io_core::is_cancelled(cancel) {
                    return ParsedEmlKind::Cancelled;
                }
                let rel_path = relative_eml_path(eml_path, &input_roots, &file_inputs);
                parse_one_eml(
                    eml_path,
                    rel_path,
                    &owner_all_digits,
                    &owner_emails_lc,
                    contacts,
                    name_mapping,
                )
            })
            .collect();

        for outcome in outcomes {
            message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
            scanned += 1;
            report_progress(verbose, log, "scanned", scanned, total);
            match outcome {
                ParsedEmlKind::Cancelled => {
                    bail!("cancelled");
                }
                ParsedEmlKind::Archive {
                    msgs,
                    skipped_dates,
                    path_display,
                } => {
                    report.bump("archive_eml", 1);
                    report.skipped_invalid_date += skipped_dates;
                    for msg in msgs {
                        if !date_range.contains_secs_f64(msg.timestamp_secs) {
                            report.skipped_out_of_range += 1;
                            continue;
                        }
                        if msg.chat_key.is_empty() {
                            report.bump("unknown_chat_messages", 1);
                        }
                        let atts = write_attachments(
                            &msg.attachments,
                            &attachments_dir,
                            &mut report,
                            copy_attachments,
                            &path_display,
                        );
                        add_message(&mut conversations, &mut by_identity, msg, atts, &mut report);
                    }
                }
                ParsedEmlKind::Flat { msg, path_display } => {
                    report.bump("flat_eml", 1);
                    if !date_range.contains_secs_f64(msg.timestamp_secs) {
                        report.skipped_out_of_range += 1;
                        continue;
                    }
                    if msg.chat_key.is_empty() {
                        report.bump("unknown_chat_messages", 1);
                    }
                    let atts = write_attachments(
                        &msg.attachments,
                        &attachments_dir,
                        &mut report,
                        copy_attachments,
                        &path_display,
                    );
                    add_message(
                        &mut conversations,
                        &mut by_identity,
                        *msg,
                        atts,
                        &mut report,
                    );
                }
                ParsedEmlKind::FlatNone => {
                    report.bump("skipped_parse_error", 1);
                }
                ParsedEmlKind::NotSms => {
                    report.bump("skipped_not_sms_backup_plus", 1);
                }
                ParsedEmlKind::IoError(msg) => {
                    report.errors.push(msg);
                }
                ParsedEmlKind::ParseError(msg) => {
                    report.bump("skipped_parse_error", 1);
                    report.errors.push(msg);
                }
            }
        }
    }

    vlog(
        verbose,
        log,
        format!(
            "parsed: flat_eml={} archive_eml={} messages={} unknown_chat={} skipped_not_sms_backup_plus={} skipped_parse_error={} skipped_bad_date={}",
            count(&report, "flat_eml"),
            count(&report, "archive_eml"),
            count(&report, "messages_before_dedupe"),
            count(&report, "unknown_chat_messages"),
            count(&report, "skipped_not_sms_backup_plus"),
            count(&report, "skipped_parse_error"),
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
            emit_log(log, format!("  … and {} more", report.errors.len() - 20));
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
            content_type: "image/jpeg".into(),
            extension: "jpg".into(),
            digest_sha256: Some("aaa".into()),
            name_hint: Some("a.jpg".into()),
        }];
        let from = vec![
            PendingAttachment {
                rel_path: "attachments/a.jpg".into(),
                content_type: "image/jpeg".into(),
                extension: "jpg".into(),
                digest_sha256: Some("aaa".into()),
                name_hint: Some("a.jpg".into()),
            },
            PendingAttachment {
                rel_path: "attachments/b.jpg".into(),
                content_type: "image/jpeg".into(),
                extension: "jpg".into(),
                digest_sha256: Some("bbb".into()),
                name_hint: Some("b.jpg".into()),
            },
        ];
        merge_attachments(&mut into, from);
        assert_eq!(into.len(), 2);
        assert_eq!(into[1].digest_sha256.as_deref(), Some("bbb"));
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
        assert_eq!(out[0].digest_sha256.as_deref(), Some("bbb"));
        assert!(att_dir.join("ok.jpg").exists());
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("failed to write attachment"));
        assert_eq!(report.attachments_saved, 1);
    }
}
