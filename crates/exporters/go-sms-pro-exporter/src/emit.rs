//! Convert a GO SMS Pro backup into the shared conversation structure
//! ([`ConversationDocument`]) every exporter writes, then write the chosen
//! output format via [`FormatSink`].

use crate::attachments_emit::{
    pending_attachment_to_ir, queue_pdu_attachments, stage_conversation_attachments,
};
use crate::chat_id::{chat_id_group, chat_id_individual, guarded_phone};
use crate::xml::{SkippedBadAddrDetail, XmlMessage, parse_xml_file};
use anyhow::{Context, Result, bail};
use contacts::ContactsBook;
use go_sms_mms::{ParsedPdu, parse_pdu_file};
use media::MediaMode;
use message_csv::{DateRange, format_local_ts, stable_guid};
use message_ir::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, HandleType,
    IrAttachment, IrConversationType, IrDirection, IrMessage, IrMessageKind, IrParticipant,
    IrService, IrSource, PendingAttachment, PendingConversation, PendingMessage, SCHEMA_VERSION,
    owner_sender, parse_android_type,
};
use message_ir_format::{ExportTransforms, FormatSink, FormatSinkResult};
use message_vault_io_core::{CancelFlag, ExportReport, OutputFormat, emit_log, prepare_outputs};
use phone::OwnerHandleSet;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

const EXPORT_SOURCE: &str = "go-sms-pro";
const EXPORT_TOOL: &str = "GO SMS Pro";
/// Upstream app version not pinned yet (empty in CSV).
const EXPORT_TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Cap on retained skip-detail rows; overflow is counted and reported.
pub(crate) const MAX_SKIP_DETAILS: usize = 20;

/// Push one diagnostic row, keeping at most [`MAX_SKIP_DETAILS`] entries so
/// huge backups cannot grow the detail vectors without bound.
fn push_skip_detail<T>(details: &mut Vec<T>, more: &mut u64, item: T) {
    if details.len() < MAX_SKIP_DETAILS {
        details.push(item);
    } else {
        *more += 1;
    }
}

/// Skipped-row diagnostics kept out of the shared [`ExportReport`]: only used
/// to write the `skipped_*` CSV files at the end of [`convert_export`].
#[derive(Default)]
struct SkipDetails {
    invalid_address: Vec<SkippedBadAddrDetail>,
    invalid_address_more: u64,
    empty_pdu: Vec<SkippedEmptyPduDetail>,
    empty_pdu_more: u64,
    no_party: Vec<SkippedNoPartyDetail>,
    no_party_more: u64,
}

/// Diagnostic row for an empty/stub PDU file.
#[derive(Debug, Clone)]
pub(crate) struct SkippedEmptyPduDetail {
    pub pdu_filename: String,
}

/// Diagnostic row when a non-group PDU has no non-owner peer.
#[derive(Debug, Clone)]
pub(crate) struct SkippedNoPartyDetail {
    pub pdu_filename: String,
    pub participants: String,
    pub is_sent: bool,
    pub has_from: bool,
    pub has_to: bool,
}

/// Get or create the pending conversation for `chat_id`.
fn ensure_convo<'a>(
    map: &'a mut BTreeMap<String, PendingConversation>,
    chat_id: &str,
    is_group: bool,
    display_name: Option<String>,
    participant_e164s: Vec<String>,
) -> &'a mut PendingConversation {
    map.entry(chat_id.to_string())
        .or_insert_with(|| PendingConversation {
            chat_id: chat_id.to_string(),
            display_name,
            participant_e164s,
            messages: Vec::new(),
            is_group,
            has_attachments: false,
            extra: BTreeMap::new(),
        })
}

/// Append parsed XML SMS rows to pending conversations.
fn add_xml_messages(
    conversations: &mut BTreeMap<String, PendingConversation>,
    msgs: Vec<XmlMessage>,
) {
    for msg in msgs {
        let chat_id = chat_id_individual(&msg.other_digits);
        let convo = ensure_convo(conversations, &chat_id, false, None, Vec::new());
        let dedupe_key = format!(
            "{}|{}|{}|",
            msg.timestamp_secs as i64,
            if msg.is_from_me { "1" } else { "0" },
            msg.text
        );
        convo.messages.push(PendingMessage {
            sort_key: msg.timestamp_secs as i64,
            is_from_me: msg.is_from_me,
            sender_handle: msg.sender_digits.unwrap_or_default(),
            sender_display_name: msg.name_alias.clone(),
            text: msg.text,
            attachments: Vec::new(),
            extra: {
                let mut e = BTreeMap::new();
                e.insert("dedupe_key".into(), dedupe_key);
                e.insert("source_kind".into(), "xml".to_string());
                e.insert("android_type".into(), msg.android_type);
                e.insert("date_ms".into(), msg.date_ms);
                e.insert("contact_name".into(), msg.contact_name);
                // XML rows carry no PDU diagnostics; absent keys read back empty.
                for (k, v) in msg.xml_fields {
                    e.insert(format!("xml:{k}"), v);
                }
                e
            },
        });
    }
}

/// File name of the PDU on disk (for skip-detail rows).
fn pdu_basename(parsed: &ParsedPdu) -> String {
    parsed
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}

/// GO SMS Pro stub / hollow PDU: no peers, no text, no media, no From/To headers.
fn is_empty_pdu(parsed: &ParsedPdu) -> bool {
    parsed.participants.is_empty()
        && parsed.body.trim().is_empty()
        && parsed.attachments.is_empty()
        && !parsed.has_from
        && !parsed.has_to
}

/// Append one parsed PDU (binary SMS/MMS) to the matching conversation(s).
fn add_pdu_message(
    conversations: &mut BTreeMap<String, PendingConversation>,
    parsed: ParsedPdu,
    attachments: Vec<PendingAttachment>,
    owners: &OwnerHandleSet,
    report: &mut ExportReport,
    skips: &mut SkipDetails,
) {
    if is_empty_pdu(&parsed) {
        report.bump("skipped_empty_pdu", 1);
        push_skip_detail(
            &mut skips.empty_pdu,
            &mut skips.empty_pdu_more,
            SkippedEmptyPduDetail {
                pdu_filename: pdu_basename(&parsed),
            },
        );
        return;
    }

    let targets: Vec<(String, bool, Option<String>, Vec<String>)> = {
        let others: Vec<_> = parsed
            .participants
            .iter()
            .filter(|p| !p.is_empty() && !owners.is_owner(p, HandleType::Phone))
            .cloned()
            .collect();
        if others.is_empty() {
            report.bump("skipped_no_other_party", 1);
            push_skip_detail(
                &mut skips.no_party,
                &mut skips.no_party_more,
                SkippedNoPartyDetail {
                    pdu_filename: pdu_basename(&parsed),
                    participants: parsed.participants.join(";"),
                    is_sent: parsed.is_sent,
                    has_from: parsed.has_from,
                    has_to: parsed.has_to,
                },
            );
            return;
        }
        // Treat multi-peer MMS as a group even when the PDU flag is unset.
        if parsed.is_group || others.len() >= 2 {
            let (id, title) = chat_id_group(&parsed.participants, owners);
            let peers: Vec<String> = others.iter().map(|d| guarded_phone(d)).collect();
            vec![(id, true, Some(title), peers)]
        } else {
            let other = &others[0];
            vec![(chat_id_individual(other), false, None, Vec::new())]
        }
    };

    report.bump("pdu_messages", 1);
    if targets.iter().any(|(_, is_group, _, _)| *is_group) {
        report.bump("pdu_group_messages", 1);
    }

    let att_names: Vec<String> = attachments
        .iter()
        .map(|a| a.digest_sha256.clone().unwrap_or_default())
        .collect();
    let dedupe_key = format!(
        "{}|{}|{}|{}",
        parsed.timestamp,
        if parsed.is_sent { "1" } else { "0" },
        parsed.body,
        att_names.join(",")
    );

    let pdu_filename = parsed
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let sender_digits = if parsed.is_sent || parsed.sender_number.is_empty() {
        None
    } else {
        Some(parsed.sender_number.clone())
    };

    let pending = PendingMessage {
        sort_key: parsed.timestamp,
        is_from_me: parsed.is_sent,
        sender_handle: sender_digits.unwrap_or_default(),
        sender_display_name: None,
        text: parsed.body.clone(),
        attachments,
        extra: {
            let mut e = BTreeMap::new();
            e.insert("dedupe_key".into(), dedupe_key);
            e.insert("source_kind".into(), "pdu".to_string());
            e.insert("android_type".into(), String::new());
            e.insert("date_ms".into(), String::new());
            e.insert("contact_name".into(), String::new());
            e.insert("pdu_filename".into(), pdu_filename);
            e.insert("pdu_decode".into(), parsed.decode_quality.to_string());
            if !parsed.pdu_fields.is_empty() {
                e.insert(
                    "pdu_fields".into(),
                    serde_json::to_string(&parsed.pdu_fields).unwrap_or_default(),
                );
            }
            e
        },
    };

    for (chat_id, is_group, group_title, peers) in targets {
        let convo = ensure_convo(conversations, &chat_id, is_group, group_title, peers);
        convo.messages.push(pending.clone());
    }
}

/// Key prefix shared by XML and PDU rows: `secs|direction|text` up to the
/// trailing attachment section. The XML key (`…|text|`) is a strict prefix of
/// the PDU key (`…|body|att_names`) for the same message, so exact-key dedupe
/// alone would let both rows through and export the MMS twice.
fn dedupe_base_key(key: &str) -> &str {
    key.rsplit_once('|').map(|(base, _)| base).unwrap_or(key)
}

/// Drop duplicate pending messages, keeping the row with more attachments.
fn dedupe_messages(messages: &mut Vec<PendingMessage>) {
    messages.sort_by(|a, b| {
        a.sort_key
            .partial_cmp(&b.sort_key)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen_base: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<PendingMessage> = Vec::with_capacity(messages.len());
    for m in messages.drain(..) {
        let base = dedupe_base_key(m.extra_str("dedupe_key")).to_string();
        match seen_base.get(&base).copied() {
            None => {
                seen_base.insert(base, out.len());
                out.push(m);
            }
            Some(idx) => {
                let existing = &out[idx];
                if existing.attachments.is_empty() && !m.attachments.is_empty() {
                    // Same message in the XML backup (no attachments) and its
                    // PDU file (with media): keep the row that carries them.
                    out[idx] = m;
                } else if existing.attachments.is_empty() || m.attachments.is_empty() {
                    // Exact duplicate or an attachment-less row shadowed by a
                    // richer one already kept: drop it.
                } else {
                    // Two attachment-bearing rows with the same prefix are
                    // distinct MMS (same second, direction, and caption but
                    // different media): keep both.
                    seen_base.insert(base, out.len());
                    out.push(m);
                }
            }
        }
    }
    *messages = out;
}

/// Dedupe, drop invalid dates, and return false when nothing remains.
fn prepare_conversation(convo: &mut PendingConversation, report: &mut ExportReport) -> bool {
    dedupe_messages(&mut convo.messages);
    message_vault_io_core::prune_and_finish_conversation(convo, report, |k| k)
}

/// Map of handle → display name from message extras and sender fields.
fn display_names_for_handles(convo: &PendingConversation) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for msg in &convo.messages {
        if !msg.sender_handle.is_empty() {
            let handle = guarded_phone(&msg.sender_handle);
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

/// First non-empty `contact_name` extra on a message in this conversation.
fn first_contact_name(convo: &PendingConversation) -> Option<String> {
    convo
        .messages
        .iter()
        .map(|m| m.extra_str("contact_name").trim())
        .find(|n| !n.is_empty())
        .map(str::to_string)
}

/// True when the path has a `.xml` extension (any case).
fn is_xml_file(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
}

/// True for GO SMS Pro PDU files named `I_*.pdu` (the binary encoding of an
/// SMS/MMS on the phone).
fn is_pdu_file(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("I_") && n.ends_with(".pdu"))
}

/// Build a [`ConversationDocument`] from one pending conversation.
///
/// Currently always returns `Ok`. The `Result` matches the other exporters.
fn pending_to_document(
    chat_id: &str,
    convo: &PendingConversation,
    owner_handle: &str,
    report: &mut ExportReport,
    blob_bytes: &std::collections::HashMap<String, Vec<u8>>,
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

    let owner_meta = ExportMeta {
        source: String::new(),
        tool: String::new(),
        tool_version: String::new(),
        owner_handle: Some(owner_handle.to_string()),
        owner_display_name: None,
    };
    let export = message_vault_io_core::export_meta(
        EXPORT_SOURCE,
        EXPORT_TOOL,
        EXPORT_TOOL_VERSION,
        &owner_meta,
    );
    let (owner_sender_handle, owner_sender_display) = owner_sender(&export);

    let mut messages = Vec::with_capacity(convo.messages.len());
    for msg in &convo.messages {
        if msg.is_from_me {
            report.sent += 1;
        } else {
            report.received += 1;
        }
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
                    Some(guarded_phone(&msg.sender_handle))
                },
                msg.sender_display_name.clone(),
            )
        };
        let attachments: Vec<IrAttachment> = msg
            .attachments
            .iter()
            .map(|a| pending_attachment_to_ir(a, blob_bytes))
            .collect();
        let message_kind = if msg.attachments.is_empty() {
            IrMessageKind::Sms
        } else {
            IrMessageKind::Mms
        };

        let mut fields = serde_json::Map::new();
        fields.insert(
            "source_kind".into(),
            serde_json::Value::String(msg.extra_str("source_kind").to_string()),
        );
        let pdu_filename = msg.extra_str("pdu_filename");
        if !pdu_filename.is_empty() {
            fields.insert(
                "pdu_filename".into(),
                serde_json::Value::String(pdu_filename.to_string()),
            );
        }
        let pdu_decode = msg.extra_str("pdu_decode");
        if !pdu_decode.is_empty() {
            fields.insert(
                "pdu_decode".into(),
                serde_json::Value::String(pdu_decode.to_string()),
            );
        }
        let pdu_fields = msg.extra_str("pdu_fields");
        if !pdu_fields.is_empty() {
            fields.insert(
                "pdu_fields".into(),
                serde_json::from_str(pdu_fields).unwrap_or(serde_json::Value::Null),
            );
        }
        for (k, v) in &msg.extra {
            if let Some(k) = k.strip_prefix("xml:") {
                fields
                    .entry(k.to_string())
                    .or_insert_with(|| serde_json::Value::String(v.clone()));
            }
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

/// Fill `contact_name` and sender display name from the contacts book.
fn enrich_pending_names(book: &ContactsBook, chat_id: &str, msg: &mut PendingMessage) {
    let phones: Vec<&str> = if msg.sender_handle.is_empty() {
        vec![chat_id]
    } else {
        vec![msg.sender_handle.as_str(), chat_id]
    };
    for phone in phones {
        let contact_name = msg.extra_str("contact_name").to_string();
        if let Some(name) = book.enrich_display_name(phone, HandleType::Phone, &contact_name) {
            msg.extra.insert("contact_name".into(), name);
        }
        let cur = msg.sender_display_name.as_deref().unwrap_or("");
        if let Some(name) = book.enrich_display_name(phone, HandleType::Phone, cur) {
            msg.sender_display_name = Some(name);
        }
    }
}

/// Inputs for [`convert_export`].
pub(crate) struct ConvertExportArgs<'a> {
    pub input_dir: &'a Path,
    pub output_dir: &'a Path,
    pub owner_phones: &'a [String],
    pub contacts: &'a ContactsBook,
    pub date_range: &'a DateRange,
    pub transforms: ExportTransforms,
    pub output_format: OutputFormat,
    pub cancel: Option<&'a CancelFlag>,
}

/// Convert a GO SMS Pro export directory into the shared conversation structure
/// ([`ConversationDocument`]), then write the chosen output format.
///
/// When `cancel` is set, cooperative cancellation is checked between XML files
/// and between PDU files. Cancelled runs return an error with message `cancelled`.
///
/// # Errors
///
/// Returns an error when the input is not a directory, output overlaps input,
/// a file cannot be read or written, or the user cancels.
pub(crate) fn convert_export(
    args: ConvertExportArgs<'_>,
) -> Result<(ExportReport, FormatSinkResult)> {
    let ConvertExportArgs {
        input_dir,
        output_dir,
        owner_phones,
        contacts,
        date_range,
        transforms,
        output_format,
        cancel,
    } = args;
    if !input_dir.is_dir() {
        bail!("input is not a directory: {}", input_dir.display());
    }

    let (inputs, output_dir) = prepare_outputs(&[input_dir.to_path_buf()], output_dir)?;
    let input_dir = &inputs[0];

    let owners = OwnerHandleSet::from_phones(owner_phones)?;
    let owner_handle = guarded_phone(
        owners
            .primary_phone_digit()
            .context("owner phone has no usable digits")?,
    );
    let mut report = ExportReport::default();
    let mut skips = SkipDetails::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    // Clean previous CSV / mail artifacts (keep attachments if re-run; rewrite as needed).
    let copy_attachments = transforms.copies_attachments();
    let media_mode = if copy_attachments {
        transforms.media
    } else {
        MediaMode::Disabled
    };
    let compress = transforms.compress.clone();
    let log = transforms.log.clone();
    let (mut sink, attachments_dir) =
        FormatSink::open_prepared(&output_dir, output_format, transforms)?;
    let mut blob_bytes: std::collections::HashMap<String, Vec<u8>> =
        std::collections::HashMap::new();

    let mut xml_paths = message_vault_io_core::discover_files(input_dir, &is_xml_file)?;
    xml_paths.sort();

    for xml_path in xml_paths {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        match parse_xml_file(&xml_path) {
            Ok((msgs, stats)) => {
                report.bump("xml_messages_seen", stats.messages);
                report.skipped_invalid_date += stats.skipped_invalid_date;
                report.bump("skipped_unknown_type", stats.skipped_unknown_type);
                report.bump("skipped_unknown_address", stats.skipped_unknown_address);
                skips.invalid_address_more += stats.skipped_unknown_address_details_more;
                for d in stats.skipped_unknown_address_details {
                    push_skip_detail(
                        &mut skips.invalid_address,
                        &mut skips.invalid_address_more,
                        d,
                    );
                }
                let msgs: Vec<_> = msgs
                    .into_iter()
                    .filter(|msg| {
                        if date_range.contains_secs_f64(msg.timestamp_secs) {
                            true
                        } else {
                            report.skipped_out_of_range += 1;
                            false
                        }
                    })
                    .collect();
                add_xml_messages(&mut conversations, msgs);
            }
            Err(err) => report
                .errors
                .push(format!("{}: {err:#}", xml_path.display())),
        }
    }

    let mut pdu_paths = message_vault_io_core::discover_files(input_dir, &is_pdu_file)?;
    pdu_paths.sort();

    for pdu_path in pdu_paths {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        let all_digits = owners.all_phone_digits();
        match parse_pdu_file(
            &pdu_path,
            &all_digits,
            owners.primary_phone_digit().unwrap_or(""),
        ) {
            Ok(None) => {
                report.bump("skipped_unparseable_pdu", 1);
                if report.errors.len() < 20 {
                    report
                        .errors
                        .push(format!("{}: unparseable PDU", pdu_path.display()));
                }
            }
            Ok(Some(parsed)) => {
                if !date_range.contains_secs(parsed.timestamp) {
                    report.skipped_out_of_range += 1;
                    continue;
                }
                match queue_pdu_attachments(&parsed, copy_attachments, &mut blob_bytes) {
                    Ok(atts) => add_pdu_message(
                        &mut conversations,
                        parsed,
                        atts,
                        &owners,
                        &mut report,
                        &mut skips,
                    ),
                    Err(err) => report
                        .errors
                        .push(format!("{}: {err:#}", pdu_path.display())),
                }
            }
            Err(err) => report
                .errors
                .push(format!("{}: {err:#}", pdu_path.display())),
        }
    }

    message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;

    let mut documents = Vec::new();
    for (chat_id, mut convo) in conversations {
        for msg in &mut convo.messages {
            enrich_pending_names(contacts, &chat_id, msg);
        }
        if !prepare_conversation(&mut convo, &mut report) {
            continue;
        }
        documents.push(pending_to_document(
            &chat_id,
            &convo,
            &owner_handle,
            &mut report,
            &blob_bytes,
        )?);
    }

    stage_conversation_attachments(
        &mut documents,
        &attachments_dir,
        media_mode,
        &compress,
        log.as_ref(),
        cancel,
        &mut report,
    )
    .map_err(anyhow::Error::msg)?;

    let total_conversations = documents.len() as u64;
    emit_log(log.as_ref(), "");
    emit_log(
        log.as_ref(),
        format!("Preparing {total_conversations} conversation file(s)..."),
    );
    let mut written = 0u64;
    for doc in documents {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        written += 1;
        sink.write_document(doc)?;
        report.conversations += 1;
        #[allow(clippy::manual_is_multiple_of)]
        if written % 100 == 0 || written == total_conversations {
            emit_log(
                log.as_ref(),
                format!("  preparing {written}/{total_conversations}"),
            );
        }
    }

    let sink_result = sink.finish()?;

    write_skipped_invalid_address_csv(
        &output_dir,
        &skips.invalid_address,
        skips.invalid_address_more,
    )?;
    write_skipped_empty_pdu_csv(&output_dir, &skips.empty_pdu, skips.empty_pdu_more)?;
    write_skipped_no_party_csv(&output_dir, &skips.no_party, skips.no_party_more)?;

    Ok((report, sink_result))
}

fn remove_if_exists(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn write_skipped_invalid_address_csv(
    output_dir: &Path,
    details: &[SkippedBadAddrDetail],
    more: u64,
) -> Result<()> {
    let path = output_dir.join("skipped_invalid_address.csv");
    // Remove legacy filename from earlier builds.
    remove_if_exists(&output_dir.join("skipped_bad_addr.csv"));
    if details.is_empty() && more == 0 {
        remove_if_exists(&path);
        return Ok(());
    }
    let mut wtr =
        csv::Writer::from_path(&path).with_context(|| format!("create {}", path.display()))?;
    wtr.write_record([
        "xml_file",
        "address",
        "contact_name",
        "android_type",
        "date_ms",
        "body",
    ])?;
    for d in details {
        wtr.write_record([
            d.xml_file.as_str(),
            d.address.as_str(),
            d.contact_name.as_str(),
            d.android_type.as_str(),
            d.date_ms.as_str(),
            d.body.as_str(),
        ])?;
    }
    if more > 0 {
        wtr.write_record([
            "",
            "",
            "",
            "",
            "",
            &format!("...and {more} more entries not shown"),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

fn write_skipped_empty_pdu_csv(
    output_dir: &Path,
    details: &[SkippedEmptyPduDetail],
    more: u64,
) -> Result<()> {
    let path = output_dir.join("skipped_empty_pdu.csv");
    if details.is_empty() && more == 0 {
        remove_if_exists(&path);
        return Ok(());
    }
    let mut wtr =
        csv::Writer::from_path(&path).with_context(|| format!("create {}", path.display()))?;
    wtr.write_record(["pdu_filename"])?;
    for d in details {
        wtr.write_record([d.pdu_filename.as_str()])?;
    }
    if more > 0 {
        wtr.write_record([&format!("...and {more} more entries not shown")])?;
    }
    wtr.flush()?;
    Ok(())
}

fn write_skipped_no_party_csv(
    output_dir: &Path,
    details: &[SkippedNoPartyDetail],
    more: u64,
) -> Result<()> {
    let path = output_dir.join("skipped_no_party.csv");
    if details.is_empty() && more == 0 {
        remove_if_exists(&path);
        return Ok(());
    }
    let mut wtr =
        csv::Writer::from_path(&path).with_context(|| format!("create {}", path.display()))?;
    wtr.write_record([
        "pdu_filename",
        "participants",
        "is_sent",
        "has_from",
        "has_to",
    ])?;
    for d in details {
        wtr.write_record([
            d.pdu_filename.as_str(),
            d.participants.as_str(),
            if d.is_sent { "1" } else { "0" },
            if d.has_from { "1" } else { "0" },
            if d.has_to { "1" } else { "0" },
        ])?;
    }
    if more > 0 {
        wtr.write_record([
            "",
            "",
            "",
            "",
            &format!("...and {more} more entries not shown"),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_chat_ids_do_not_collide_on_digit_boundaries() {
        let owners = OwnerHandleSet::from_phones(&["+15555550100".into()]).unwrap();
        let (a, _) = chat_id_group(&["12".into(), "34".into()], &owners);
        let (b, _) = chat_id_group(&["123".into(), "4".into()], &owners);
        assert_ne!(a, b);
        assert!(a.contains("2:12"));
        assert!(b.contains("3:123"));
    }

    #[test]
    fn multi_peer_pdu_without_group_flag_uses_group_chat_id() {
        let owners = OwnerHandleSet::from_phones(&["+15555550100".into()]).unwrap();
        let parsed = ParsedPdu {
            path: std::path::PathBuf::from("I_1609459200_x.pdu"),
            timestamp: 1_609_459_200,
            participants: vec![
                "15555550100".into(),
                "15555550122".into(),
                "15555550133".into(),
            ],
            body: "hi".into(),
            attachments: Vec::new(),
            is_sent: true,
            is_group: false,
            sender_number: String::new(),
            has_from: false,
            has_to: true,
            pdu_fields: BTreeMap::new(),
            decode_quality: "structured",
        };
        let mut conversations = BTreeMap::new();
        let mut report = ExportReport::default();
        let mut skips = SkipDetails::default();
        add_pdu_message(
            &mut conversations,
            parsed,
            Vec::new(),
            &owners,
            &mut report,
            &mut skips,
        );
        assert_eq!(conversations.len(), 1);
        let convo = conversations.values().next().unwrap();
        assert!(convo.is_group);
        assert!(convo.chat_id.starts_with("chat-group-"));
        assert_eq!(
            report.extra.get("pdu_group_messages").copied().unwrap_or(0),
            1
        );
    }

    fn test_msg(key: &str, attachments: usize) -> PendingMessage {
        PendingMessage {
            sort_key: 1609459200,
            is_from_me: false,
            sender_handle: String::new(),
            sender_display_name: None,
            text: String::new(),
            attachments: (0..attachments)
                .map(|i| PendingAttachment {
                    rel_path: format!("attachments/a{i}.jpg"),
                    content_type: String::new(),
                    extension: "jpg".into(),
                    digest_sha256: None,
                    name_hint: None,
                })
                .collect(),
            extra: {
                let mut e = BTreeMap::new();
                e.insert("dedupe_key".into(), key.to_string());
                e.insert("source_kind".into(), "xml".to_string());
                e
            },
        }
    }

    #[test]
    fn dedupe_base_key_prefix() {
        assert_eq!(dedupe_base_key("1609459200|1|hello|"), "1609459200|1|hello");
        assert_eq!(
            dedupe_base_key("1609459200|1|hello|attachments/a1.jpg"),
            "1609459200|1|hello"
        );
        // Pipes inside the text must not split the base key.
        assert_eq!(
            dedupe_base_key("1609459200|1|he|llo|attachments/a1.jpg"),
            "1609459200|1|he|llo"
        );
    }

    #[test]
    fn xml_and_pdu_mms_rows_collapse_keeping_attachments() {
        // The same MMS appears in the XML backup (no attachments) and as a PDU
        // row with media. Exact-key dedupe would export it twice.
        let mut pdu_row = test_msg("1609459200|1|hello|attachments/a1.jpg", 1);
        pdu_row.extra.insert("source_kind".into(), "pdu".into());
        let mut msgs = vec![test_msg("1609459200|1|hello|", 0), pdu_row];
        dedupe_messages(&mut msgs);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].attachments.len(), 1);
        assert_eq!(msgs[0].extra_str("source_kind"), "pdu");
    }

    #[test]
    fn distinct_mms_with_same_prefix_both_kept() {
        // Two MMS sharing second, direction, and caption but with different
        // media are distinct messages: both rows survive.
        let mut msgs = vec![
            test_msg("1609459200|1|photo|attachments/a1.jpg", 1),
            test_msg("1609459200|1|photo|attachments/a2.jpg", 1),
        ];
        dedupe_messages(&mut msgs);
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn plain_sms_duplicates_dropped() {
        let mut msgs = vec![
            test_msg("1609459200|1|hi|", 0),
            test_msg("1609459200|1|hi|", 0),
        ];
        dedupe_messages(&mut msgs);
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn pdu_row_shadowed_by_xml_row_keeps_attachments() {
        // Defensive: XML pass runs first, but if a PDU row with media ever
        // precedes its XML twin, the attachment row still wins.
        let mut pdu_row = test_msg("1609459200|1|hello|attachments/a1.jpg", 1);
        pdu_row.extra.insert("source_kind".into(), "pdu".into());
        let mut msgs = vec![pdu_row, test_msg("1609459200|1|hello|", 0)];
        dedupe_messages(&mut msgs);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].attachments.len(), 1);
        assert_eq!(msgs[0].extra_str("source_kind"), "pdu");
    }
}
