//! Convert GO SMS Pro export → common message → packaging via FormatSink.

use crate::xml::{SkippedBadAddrDetail, XmlMessage, parse_xml_file};
use go_sms_mms::{ParsedPdu, parse_pdu_file};
use anyhow::{Context, Result, bail};
use chrono::{Local, TimeZone};
use contacts::ContactsBook;
use message_csv::{DateRange, format_local_ts, stable_guid};
use message_vault_io_core::{CancelFlag, OutputFormat};
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
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const EXPORT_SOURCE: &str = "go-sms-pro";
const EXPORT_TOOL: &str = "GO SMS Pro";
/// Upstream app version not pinned yet (empty in CSV).
const EXPORT_TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Default)]
pub(crate) struct ExportReport {
    pub conversations: u64,
    /// XML `<SMS>` rows seen while parsing (before write / dedupe).
    pub xml_messages_seen: u64,
    /// PDU files that produced a pending message (before write / dedupe).
    pub pdu_messages: u64,
    pub pdu_group_messages: u64,
    pub attachments_saved: u64,
    /// Rows written to CSV after dedupe (outgoing).
    pub sent: u64,
    /// Rows written to CSV after dedupe (incoming).
    pub received: u64,
    pub skipped_invalid_date: u64,
    pub skipped_out_of_range: u64,
    pub skipped_unknown_type: u64,
    pub skipped_unknown_address: u64,
    /// One row per invalid-address XML SMS (`skipped_invalid_address.csv` / stderr sample).
    pub skipped_unknown_address_details: Vec<SkippedBadAddrDetail>,
    pub skipped_unparseable_pdu: u64,
    /// Hollow PDU stub (no addresses, body, or attachments) — e.g. `application/smil\0` only.
    pub skipped_empty_pdu: u64,
    pub skipped_empty_pdu_details: Vec<SkippedEmptyPduDetail>,
    /// PDU parsed but no non-owner participant (self-only / empty PLMN set).
    pub skipped_no_other_party: u64,
    /// One row per `skipped_no_other_party` (for `skipped_no_party.csv` / stderr sample).
    pub skipped_no_other_party_details: Vec<SkippedNoPartyDetail>,
    pub errors: Vec<String>,
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

#[derive(Debug, Clone)]
struct PendingAttachment {
    /// Relative path under export dir, e.g. `attachments/20200101_000000-I_…_1.jpg`
    rel_path: String,
    original_name: Option<String>,
    mime_type: Option<String>,
    /// Bytes already written (for guid fingerprint).
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
    /// For within-thread dedupe.
    dedupe_key: String,
    source_kind: &'static str,
    android_type: String,
    date_ms: String,
    contact_name: String,
    pdu_filename: String,
    xml_fields: BTreeMap<String, String>,
    pdu_fields: BTreeMap<String, String>,
    pdu_decode: String,
}

#[derive(Debug, Default)]
struct PendingConversation {
    conversation_type: String,
    group_title: Option<String>,
    /// Non-owner peer E.164s for untitled group filenames.
    participant_e164s: Vec<String>,
    messages: Vec<PendingMessage>,
}

fn mime_for_ext(ext: &str) -> Option<&'static str> {
    match ext {
        ".jpg" | ".jpeg" => Some("image/jpeg"),
        ".png" => Some("image/png"),
        ".gif" => Some("image/gif"),
        ".3gp" => Some("video/3gpp"),
        ".mp4" => Some("video/mp4"),
        ".amr" => Some("audio/amr"),
        ".wav" => Some("audio/wav"),
        _ => None,
    }
}

fn chat_id_individual(digits: &str) -> String {
    to_e164(digits)
}

fn chat_id_group(participant_digits: &[String], owners: &OwnerPhoneSet) -> (String, String) {
    let mut others: Vec<String> = participant_digits
        .iter()
        .filter(|d| !d.is_empty() && !owners.is_owner(d))
        .cloned()
        .collect();
    others.sort();
    others.dedup();
    let title = if others.is_empty() {
        "Group".to_string()
    } else if others.len() <= 4 {
        format!(
            "Group: {}",
            others
                .iter()
                .map(|d| to_e164(d))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!(
            "Group: {}, and {} others",
            others[..4]
                .iter()
                .map(|d| to_e164(d))
                .collect::<Vec<_>>()
                .join(", "),
            others.len() - 4
        )
    };
    let slug = others
        .iter()
        .map(|d| d.as_str())
        .collect::<Vec<_>>()
        .join("_");
    let id = if slug.is_empty() {
        "chat-group-unknown".to_string()
    } else {
        format!("chat-group-{slug}")
    };
    // Keep filesystem-safe length.
    let id = if id.len() > 180 {
        let digest = hex::encode(Sha256::digest(id.as_bytes()));
        format!("chat-group-{}", &digest[..16])
    } else {
        id
    };
    (id, title)
}

fn ensure_convo<'a>(
    map: &'a mut BTreeMap<String, PendingConversation>,
    chat_id: &str,
    conversation_type: &str,
    group_title: Option<String>,
    participant_e164s: Vec<String>,
) -> &'a mut PendingConversation {
    map.entry(chat_id.to_string())
        .or_insert_with(|| PendingConversation {
            conversation_type: conversation_type.to_string(),
            group_title,
            participant_e164s,
            messages: Vec::new(),
        })
}

fn add_xml_messages(
    conversations: &mut BTreeMap<String, PendingConversation>,
    msgs: Vec<XmlMessage>,
) {
    for msg in msgs {
        let chat_id = chat_id_individual(&msg.other_digits);
        let convo = ensure_convo(conversations, &chat_id, "individual", None, Vec::new());
        let dedupe_key = format!(
            "{}|{}|{}|",
            msg.timestamp_secs as i64,
            if msg.is_from_me { "1" } else { "0" },
            msg.text
        );
        convo.messages.push(PendingMessage {
            sort_key: msg.timestamp_secs,
            is_from_me: msg.is_from_me,
            sender_digits: msg.sender_digits,
            sender_display_name: msg.name_hint.clone(),
            text: msg.text,
            attachments: Vec::new(),
            dedupe_key,
            source_kind: "xml",
            android_type: msg.android_type,
            date_ms: msg.date_ms,
            contact_name: msg.contact_name,
            pdu_filename: String::new(),
            xml_fields: msg.xml_fields,
            pdu_fields: BTreeMap::new(),
            pdu_decode: String::new(),
        });
    }
}

fn save_pdu_attachments(
    parsed: &ParsedPdu,
    attachments_dir: &Path,
    report: &mut ExportReport,
    copy_attachments: bool,
) -> Result<Vec<PendingAttachment>> {
    if !copy_attachments {
        return Ok(Vec::new());
    }
    fs::create_dir_all(attachments_dir)?;
    let date_prefix = Local
        .timestamp_opt(parsed.timestamp, 0)
        .single()
        .map(|t| t.format("%Y%m%d_%H%M%S").to_string())
        .unwrap_or_else(|| parsed.timestamp.to_string());

    let mut out = Vec::new();
    for (idx, att) in parsed.attachments.iter().enumerate() {
        let digest_hex = hex::encode(Sha256::digest(&att.data));
        let digest_prefix = &digest_hex[..16.min(digest_hex.len())];
        let name = format!(
            "{}-I_{}_{}_{}{}",
            date_prefix,
            parsed.timestamp,
            digest_prefix,
            idx + 1,
            att.ext
        );
        let path = attachments_dir.join(&name);
        // Content-addressed name: rewrite only when missing (same bytes → same path).
        if !path.exists() {
            fs::write(&path, &att.data)?;
            report.attachments_saved += 1;
        }
        out.push(PendingAttachment {
            rel_path: format!("attachments/{name}"),
            original_name: att.smil_name.clone().or(Some(name)),
            mime_type: mime_for_ext(&att.ext).map(|s| s.to_string()),
            digest_hex,
        });
    }
    Ok(out)
}

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

fn add_pdu_message(
    conversations: &mut BTreeMap<String, PendingConversation>,
    parsed: ParsedPdu,
    attachments: Vec<PendingAttachment>,
    owners: &OwnerPhoneSet,
    report: &mut ExportReport,
) {
    if is_empty_pdu(&parsed) {
        report.skipped_empty_pdu += 1;
        report
            .skipped_empty_pdu_details
            .push(SkippedEmptyPduDetail {
                pdu_filename: pdu_basename(&parsed),
            });
        return;
    }

    let targets: Vec<(String, String, Option<String>, Vec<String>)> = if parsed.is_group {
        let (id, title) = chat_id_group(&parsed.participants, owners);
        let peers: Vec<String> = parsed
            .participants
            .iter()
            .filter(|p| !p.is_empty() && !owners.is_owner(p))
            .map(|d| to_e164(d))
            .collect();
        vec![(id, "group".to_string(), Some(title), peers)]
    } else {
        let others: Vec<_> = parsed
            .participants
            .iter()
            .filter(|p| !p.is_empty() && !owners.is_owner(p))
            .cloned()
            .collect();
        if others.is_empty() {
            report.skipped_no_other_party += 1;
            report
                .skipped_no_other_party_details
                .push(SkippedNoPartyDetail {
                    pdu_filename: pdu_basename(&parsed),
                    participants: parsed.participants.join(";"),
                    is_sent: parsed.is_sent,
                    has_from: parsed.has_from,
                    has_to: parsed.has_to,
                });
            return;
        }
        let other = &others[0];
        vec![(
            chat_id_individual(other),
            "individual".to_string(),
            None,
            Vec::new(),
        )]
    };

    report.pdu_messages += 1;
    if parsed.is_group {
        report.pdu_group_messages += 1;
    }

    let att_names: Vec<String> = attachments.iter().map(|a| a.rel_path.clone()).collect();
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
        sort_key: parsed.timestamp as f64,
        is_from_me: parsed.is_sent,
        sender_digits,
        sender_display_name: None,
        text: parsed.body.clone(),
        attachments,
        dedupe_key,
        source_kind: "pdu",
        android_type: String::new(),
        date_ms: String::new(),
        contact_name: String::new(),
        pdu_filename,
        xml_fields: BTreeMap::new(),
        pdu_fields: parsed.pdu_fields.clone(),
        pdu_decode: parsed.decode_quality.to_string(),
    };

    for (chat_id, conversation_type, group_title, peers) in targets {
        let convo = ensure_convo(
            conversations,
            &chat_id,
            &conversation_type,
            group_title,
            peers,
        );
        convo.messages.push(pending.clone());
    }
}

fn dedupe_messages(messages: &mut Vec<PendingMessage>) {
    messages.sort_by(|a, b| {
        a.sort_key
            .partial_cmp(&b.sort_key)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen = HashSet::new();
    messages.retain(|m| seen.insert(m.dedupe_key.clone()));
}

fn prepare_conversation(convo: &mut PendingConversation, report: &mut ExportReport) -> bool {
    dedupe_messages(&mut convo.messages);
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
        fields.insert(
            "source_kind".into(),
            serde_json::Value::String(msg.source_kind.to_string()),
        );
        if !msg.pdu_filename.is_empty() {
            fields.insert(
                "pdu_filename".into(),
                serde_json::Value::String(msg.pdu_filename.clone()),
            );
        }
        if !msg.pdu_decode.is_empty() {
            fields.insert(
                "pdu_decode".into(),
                serde_json::Value::String(msg.pdu_decode.clone()),
            );
        }
        if !msg.pdu_fields.is_empty() {
            fields.insert(
                "pdu_fields".into(),
                serde_json::to_value(&msg.pdu_fields).unwrap_or(serde_json::Value::Null),
            );
        }
        for (k, v) in &msg.xml_fields {
            fields
                .entry(k.clone())
                .or_insert_with(|| serde_json::Value::String(v.clone()));
        }
        if let Some(title) = convo.group_title.as_deref().filter(|t| !t.is_empty()) {
            // Synthetic Android group label; kept as data, not used for filenames.
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

fn enrich_pending_names(book: &ContactsBook, chat_id: &str, msg: &mut PendingMessage) {
    let phones: Vec<&str> = msg
        .sender_digits
        .as_deref()
        .into_iter()
        .chain(std::iter::once(chat_id))
        .collect();
    for phone in phones {
        if let Some(name) = book.enrich_display_name(phone, &msg.contact_name) {
            msg.contact_name = name;
        }
        let cur = msg.sender_display_name.as_deref().unwrap_or("");
        if let Some(name) = book.enrich_display_name(phone, cur) {
            msg.sender_display_name = Some(name);
        }
    }
}

/// Convert a GO SMS Pro export directory into per-conversation CSV, EML, or MBOX.
///
/// When `cancel` is set, cooperative cancellation is checked between XML files
/// and between PDU files. Cancelled runs return an error with message `cancelled`.
pub(crate) fn convert_export(
    input_dir: &Path,
    output_dir: &Path,
    owner_phones: &[String],
    contacts: &ContactsBook,
    date_range: &DateRange,
    transforms: ExportTransforms,
    output_format: OutputFormat,
    cancel: Option<&CancelFlag>,
) -> Result<(ExportReport, FormatSinkResult)> {
    if !input_dir.is_dir() {
        bail!("input is not a directory: {}", input_dir.display());
    }

    let owners = OwnerPhoneSet::new(owner_phones)?;
    let owner_handle = to_e164(&owners.primary_digits);
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    // Clean previous CSV / mail artifacts (keep attachments if re-run; rewrite as needed).
    fs::create_dir_all(output_dir)?;
    clean_previous_ir_output(output_dir)?;
    let copy_attachments = transforms.copies_attachments();
    let attachments_dir = output_dir.join("attachments");
    if copy_attachments {
        fs::create_dir_all(&attachments_dir)?;
    }
    let mut sink = FormatSink::open(output_dir, output_format, transforms)?;

    let mut xml_paths: Vec<PathBuf> = fs::read_dir(input_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
        })
        .collect();
    xml_paths.sort();

    for xml_path in xml_paths {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        match parse_xml_file(&xml_path) {
            Ok((msgs, stats)) => {
                report.xml_messages_seen += stats.messages;
                report.skipped_invalid_date += stats.skipped_invalid_date;
                report.skipped_unknown_type += stats.skipped_unknown_type;
                report.skipped_unknown_address += stats.skipped_unknown_address;
                report
                    .skipped_unknown_address_details
                    .extend(stats.skipped_unknown_address_details);
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

    let mut pdu_paths: Vec<PathBuf> = fs::read_dir(input_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("I_") && n.ends_with(".pdu"))
        })
        .collect();
    pdu_paths.sort();

    for pdu_path in pdu_paths {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        match parse_pdu_file(&pdu_path, &owners.all_digits, &owners.primary_digits) {
            Ok(None) => {
                report.skipped_unparseable_pdu += 1;
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
                match save_pdu_attachments(&parsed, &attachments_dir, &mut report, copy_attachments)
                {
                    Ok(atts) => {
                        add_pdu_message(&mut conversations, parsed, atts, &owners, &mut report)
                    }
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

    for (chat_id, mut convo) in conversations {
        for msg in &mut convo.messages {
            enrich_pending_names(contacts, &chat_id, msg);
        }
        if !prepare_conversation(&mut convo, &mut report) {
            continue;
        }
        let doc = pending_to_document(&chat_id, &convo, &owner_handle, &mut report)?;
        sink.write_document(doc)?;
        report.conversations += 1;
    }

    let sink_result = sink.finish()?;

    write_skipped_invalid_address_csv(output_dir, &report.skipped_unknown_address_details)?;
    write_skipped_empty_pdu_csv(output_dir, &report.skipped_empty_pdu_details)?;
    write_skipped_no_party_csv(output_dir, &report.skipped_no_other_party_details)?;

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
) -> Result<()> {
    let path = output_dir.join("skipped_invalid_address.csv");
    // Remove legacy filename from earlier builds.
    remove_if_exists(&output_dir.join("skipped_bad_addr.csv"));
    if details.is_empty() {
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
    wtr.flush()?;
    Ok(())
}

fn write_skipped_empty_pdu_csv(output_dir: &Path, details: &[SkippedEmptyPduDetail]) -> Result<()> {
    let path = output_dir.join("skipped_empty_pdu.csv");
    if details.is_empty() {
        remove_if_exists(&path);
        return Ok(());
    }
    let mut wtr =
        csv::Writer::from_path(&path).with_context(|| format!("create {}", path.display()))?;
    wtr.write_record(["pdu_filename"])?;
    for d in details {
        wtr.write_record([d.pdu_filename.as_str()])?;
    }
    wtr.flush()?;
    Ok(())
}

fn write_skipped_no_party_csv(output_dir: &Path, details: &[SkippedNoPartyDetail]) -> Result<()> {
    let path = output_dir.join("skipped_no_party.csv");
    if details.is_empty() {
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
    wtr.flush()?;
    Ok(())
}
