//! Convert SMS Backup+ `.eml` trees into the shared conversation structure,
//! then write the chosen output format via [`ExportWriter`].

use crate::attachments_emit::{merge_attachments, pending_attachment_to_ir, queue_attachments};
use crate::identity::{chat_id_for, cover_identity, timestamp_ms};
use crate::parse_emit::{ParsedEmlKind, collect_eml_paths, parse_one_eml};
use crate::types::ParsedMessage;
use anyhow::{Result, bail};
use contacts::{ContactsBook, NameMapping};
use message_csv::DateRange;
use message_ir::{
    ExportMeta, IrAttachment, IrService, IrSource, PendingAttachment, PendingConversation,
    PendingMessage, ProjectionHooks, parse_android_type, pending_to_document, prepare_conversation,
};
use message_ir_format::{AttachmentSource, ExportTransforms, ExportWriter, FormatSinkResult};
use message_vault_io_core::{
    CancelFlag, ExportReport, LogSink, OutputFormat, emit_log, prepare_outputs,
};
use phone::OwnerHandleSet;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

const EXPORT_SOURCE: &str = "sms-backup-plus";
const EXPORT_TOOL: &str = "SMS Backup+";
const EXPORT_TOOL_VERSION: &str = "1.5.11";

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

/// Get or create the pending conversation for `chat_id`, unioning rosters.
///
/// Unlike the shared `ensure_conversation` (which seeds a new entry only),
/// group membership changes over time here, and a later message's smaller
/// roster must not shrink the participant list. (A roster change that yields a
/// different chat_key still splits the conversation into fragments; this keeps
/// each fragment's participant list complete within that key.)
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
            PendingConversation::new(chat_id, is_group, display_name, Vec::new()),
        );
    }
    let convo = map
        .get_mut(chat_id)
        .expect("just inserted or already present");
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

/// Format each participant's digits as E.164 when unambiguous, dropping empties.
fn peer_handles_from_digits(participant_digits: &[(String, Option<String>)]) -> Vec<String> {
    participant_digits
        .iter()
        .map(|(d, _)| phone::normalize_lenient(d))
        .filter(|d| !d.is_empty())
        .collect()
}

/// True when the path has a `.eml` extension (any case).
pub(super) fn is_eml_file(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("eml"))
}

/// SMS Backup+ deltas of the shared [`pending_to_document`] projection.
struct SbpProjection<'a> {
    export: ExportMeta,
    blob_bytes: &'a HashMap<String, Vec<u8>>,
}

impl ProjectionHooks for SbpProjection<'_> {
    fn export(&self) -> ExportMeta {
        self.export.clone()
    }

    fn service(&self, _msg: &PendingMessage) -> IrService {
        IrService::Sms
    }

    fn normalize_handle(&self, raw: &str) -> String {
        phone::normalize_lenient(raw)
    }

    fn attachment_to_ir(&self, att: &PendingAttachment, _msg: &PendingMessage) -> IrAttachment {
        pending_attachment_to_ir(att, self.blob_bytes)
    }

    fn source(&self, convo: &PendingConversation, msg: &PendingMessage) -> IrSource {
        let mut fields = serde_json::Map::new();
        for key in ["source_kind", "smssync_id", "eml_path"] {
            let value = msg.extra_str(key);
            if !value.is_empty() {
                fields.insert(key.into(), serde_json::Value::String(value.to_string()));
            }
        }
        if let Some(title) = convo.display_name.as_deref().filter(|t| !t.is_empty()) {
            // Android group title stored as data only. Filenames do not use it.
            fields.insert(
                "android_group_title".into(),
                serde_json::Value::String(title.to_string()),
            );
        }
        IrSource {
            android_type: parse_android_type(msg.extra_str("android_type")),
            fields,
        }
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
    /// Continue an interrupted export: keep previous output and skip the
    /// conversations already written.
    pub resume: bool,
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
        resume,
    } = args;
    let owners = OwnerHandleSet::from_phones(owner_phones)?;
    let owner_handle = owners
        .primary_owner_handle()
        .expect("from_phones guarantees a phone owner handle");
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

    let input_paths: Vec<PathBuf> = inputs.iter().map(|p| p.as_ref().to_path_buf()).collect();
    let (inputs, output_dir) = prepare_outputs(&input_paths, output_dir)?;

    let writer = ExportWriter::open(&output_dir, output_format, transforms, resume)?;
    let copy_attachments = writer.copies_attachments();
    let mut blob_bytes: HashMap<String, Vec<u8>> = HashMap::new();

    let input_roots = inputs.clone();
    let file_inputs: HashSet<PathBuf> = input_roots
        .iter()
        .filter(|p| p.is_file())
        .cloned()
        .collect();

    let eml_paths = collect_eml_paths(&inputs, cancel)?;
    let total = eml_paths.len() as u64;
    vlog(
        verbose,
        log,
        format!("scanning {total} .eml files (parallel parse)"),
    );
    // Pre-size for typical 1:1 chat counts; grows as needed.
    conversations.reserve((total / 4).min(50_000) as usize);

    message_vault_io_core::check_cancel(cancel)?;

    let owner_all_digits = owners.all_phone_digits();

    // Parallel parse in chunks so attachment payloads are not all held at once.
    // Each worker checks cancel before reading an EML.
    const EML_PARSE_CHUNK: usize = 256;
    let mut scanned: u64 = 0;
    for chunk in eml_paths.chunks(EML_PARSE_CHUNK) {
        message_vault_io_core::check_cancel(cancel)?;
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
            message_vault_io_core::check_cancel(cancel)?;
            scanned += 1;
            report_progress(verbose, log, "scanned", scanned, total);
            match outcome {
                ParsedEmlKind::Cancelled => {
                    bail!("cancelled");
                }
                ParsedEmlKind::Archive {
                    msgs,
                    skipped_dates,
                    _path_display: _,
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
                        let atts =
                            queue_attachments(&msg.attachments, copy_attachments, &mut blob_bytes);
                        add_message(&mut conversations, &mut by_identity, msg, atts, &mut report);
                    }
                }
                ParsedEmlKind::Flat {
                    msg,
                    _path_display: _,
                } => {
                    report.bump("flat_eml", 1);
                    if !date_range.contains_secs_f64(msg.timestamp_secs) {
                        report.skipped_out_of_range += 1;
                        continue;
                    }
                    if msg.chat_key.is_empty() {
                        report.bump("unknown_chat_messages", 1);
                    }
                    let atts =
                        queue_attachments(&msg.attachments, copy_attachments, &mut blob_bytes);
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
            report.extra("flat_eml"),
            report.extra("archive_eml"),
            report.extra("messages_before_dedupe"),
            report.extra("unknown_chat_messages"),
            report.extra("skipped_not_sms_backup_plus"),
            report.extra("skipped_parse_error"),
            report.skipped_invalid_date
        ),
    );

    let hooks = SbpProjection {
        export: message_vault_io_core::export_meta(
            EXPORT_SOURCE,
            EXPORT_TOOL,
            EXPORT_TOOL_VERSION,
            Some(owner_handle),
            None,
        ),
        blob_bytes: &blob_bytes,
    };
    let mut documents = Vec::new();
    for (chat_id, mut convo) in conversations {
        message_vault_io_core::check_cancel(cancel)?;
        let (keep, skipped) =
            prepare_conversation(&mut convo, |a, b| a.sort_key.cmp(&b.sort_key), |k| k);
        report.skipped_invalid_date += skipped;
        if !keep {
            continue;
        }
        let (doc, tally) = pending_to_document(&chat_id, &convo, &hooks);
        report.messages += tally.messages;
        report.sent += tally.sent;
        report.received += tally.received;
        documents.push(doc);
    }

    if !writer.use_queue() {
        vlog(
            verbose,
            log,
            format!(
                "writing {} conversation files (duplicates dropped so far: {})",
                documents.len(),
                report.duplicates_dropped
            ),
        );
    }
    let sink_result = writer.finish(
        documents,
        &mut |att| {
            let hint = att
                .size_bytes
                .or_else(|| att.bytes.as_ref().map(|b| b.len() as u64));
            match att.bytes.take() {
                Some(bytes) => (AttachmentSource::Bytes(bytes), hint),
                None => (AttachmentSource::Missing, hint),
            }
        },
        cancel,
        &mut report,
    )?;

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
    use crate::types::AttachmentBlob;
    use media::{CompressOptions, MediaMode};
    use message_ir::{
        ConversationDocument, ConversationMeta, ConversationStats, IrConversationType, IrDirection,
        IrMessage, IrMessageKind, SCHEMA_VERSION,
    };

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
    fn queue_attachments_keeps_message_on_single_failure() {
        let dir = tempfile::tempdir().unwrap();
        let att_dir = dir.path().join("attachments");
        std::fs::create_dir_all(&att_dir).unwrap();
        // Empty bytes: the runner records file_missing and continues.
        let blobs = vec![
            AttachmentBlob {
                filename: "missing.jpg".into(),
                original_name: None,
                mime_type: Some("image/jpeg".into()),
                digest_hex: "aaa".into(),
                data: vec![],
            },
            AttachmentBlob {
                filename: "ok.jpg".into(),
                original_name: None,
                mime_type: Some("image/jpeg".into()),
                digest_hex: "bbb".into(),
                data: vec![4, 5, 6],
            },
        ];
        let mut blob_bytes = std::collections::HashMap::new();
        let queued = queue_attachments(&blobs, true, &mut blob_bytes);
        assert_eq!(queued.len(), 2);

        let mut atts: Vec<_> = queued
            .iter()
            .map(|a| pending_attachment_to_ir(a, &blob_bytes))
            .collect();
        let mut report = ExportReport::default();
        let mut doc = ConversationDocument {
            schema_version: SCHEMA_VERSION,
            export: ExportMeta {
                source: String::new(),
                tool: String::new(),
                tool_version: String::new(),
                owner_handle: None,
                owner_display_name: None,
            },
            conversation: ConversationMeta {
                chat_identifier: "test".into(),
                conversation_type: IrConversationType::Individual,
                group_title: None,
                participants: Vec::new(),
                stats: ConversationStats::default(),
            },
            messages: vec![IrMessage {
                guid: "g".into(),
                timestamp_unix_ms: 0,
                direction: IrDirection::Incoming,
                service: IrService::Sms,
                message_kind: IrMessageKind::Mms,
                sender_handle: None,
                sender_display_name: None,
                subject: None,
                text: "hi".into(),
                attachments: std::mem::take(&mut atts),
                imessage: None,
                source: None,
            }],
            packaging_stem_suffix: None,
        };
        let payloads: Vec<Option<Vec<u8>>> = doc
            .messages
            .iter()
            .flat_map(|msg| msg.attachments.iter().map(|att| att.bytes.clone()))
            .collect();
        message_vault_io_core::stage_conversation_attachments(
            std::slice::from_mut(&mut doc),
            &att_dir,
            MediaMode::Clone,
            &CompressOptions::default(),
            |i| Ok(payloads.get(i).cloned().flatten()),
            None,
            None,
            &mut report,
        )
        .unwrap();
        // The missing source stays on the message; the good one is staged.
        assert_eq!(doc.messages[0].attachments.len(), 2);
        assert_eq!(
            doc.messages[0].attachments[0].missing_reason.as_deref(),
            Some("file_missing")
        );
        assert!(doc.messages[0].attachments[1].path.is_some());
        assert_eq!(report.attachments_saved, 1);
        assert_eq!(std::fs::read_dir(&att_dir).unwrap().count(), 1);
    }
}
