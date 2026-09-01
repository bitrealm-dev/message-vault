//! Convert a GO SMS Pro backup into the shared conversation structure
//! ([`ConversationDocument`]) every exporter writes, then write the chosen
//! output format via [`FormatSink`].

use crate::attachments_emit::{pending_attachment_to_ir, queue_pdu_attachments};
use crate::chat_id::{chat_id_group, chat_id_individual, guarded_phone};
use crate::xml::{SkippedBadAddrDetail, XmlMessage, parse_xml_file};
use anyhow::{Context, Result, bail};
use go_sms_mms::{ParsedPdu, parse_pdu_file};
use message_ir::{
    ExportMeta, HandleType, IrAttachment, IrService, IrSource, PendingAttachment,
    PendingConversation, PendingMessage, ProjectionHooks, ensure_conversation, parse_android_type,
    pending_to_document, prepare_conversation,
};
use message_ir_format::{AttachmentSource, ExportTransforms, ExportWriter, FormatSinkResult};
use message_vault_io_core::{CancelFlag, ExportReport, OutputFormat, prepare_outputs};
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

/// Append parsed XML SMS rows to pending conversations.
fn add_xml_messages(
    conversations: &mut BTreeMap<String, PendingConversation>,
    msgs: Vec<XmlMessage>,
) {
    for msg in msgs {
        let chat_id = chat_id_individual(&msg.other_digits);
        let convo = ensure_conversation(conversations, &chat_id, false, None, Vec::new());
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
        let convo = ensure_conversation(conversations, &chat_id, is_group, group_title, peers);
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

/// GO SMS Pro deltas of the shared [`pending_to_document`] projection.
struct GoSmsProjection<'a> {
    export: ExportMeta,
    blob_bytes: &'a HashMap<String, Vec<u8>>,
}

impl ProjectionHooks for GoSmsProjection<'_> {
    fn export(&self) -> ExportMeta {
        self.export.clone()
    }

    fn service(&self, _msg: &PendingMessage) -> IrService {
        IrService::Sms
    }

    fn normalize_handle(&self, raw: &str) -> String {
        guarded_phone(raw)
    }

    fn attachment_to_ir(&self, att: &PendingAttachment, _msg: &PendingMessage) -> IrAttachment {
        pending_attachment_to_ir(att, self.blob_bytes)
    }

    fn source(&self, convo: &PendingConversation, msg: &PendingMessage) -> IrSource {
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
        IrSource {
            android_type: parse_android_type(msg.extra_str("android_type")),
            fields,
        }
    }
}

/// Inputs for [`convert_export`].
pub(crate) struct ConvertExportArgs<'a> {
    pub input_dir: &'a Path,
    pub output_dir: &'a Path,
    pub owner_phones: &'a [String],
    pub transforms: ExportTransforms,
    pub output_format: OutputFormat,
    pub cancel: Option<&'a CancelFlag>,
    /// Continue an interrupted export: keep previous output and skip the
    /// conversations already written.
    pub resume: bool,
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
        transforms,
        output_format,
        cancel,
        resume,
    } = args;
    if !input_dir.is_dir() {
        bail!("input is not a directory: {}", input_dir.display());
    }

    let (inputs, output_dir) = prepare_outputs(&[input_dir.to_path_buf()], output_dir)?;
    let input_dir = &inputs[0];

    let owners = OwnerHandleSet::from_phones(owner_phones)?;
    let owner_handle = owners
        .primary_owner_handle()
        .expect("from_phones guarantees a phone owner handle");
    let mut report = ExportReport::default();
    let mut skips = SkipDetails::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    // Clean previous CSV / mail artifacts (keep attachments if re-run; rewrite as needed).
    let writer = ExportWriter::open(&output_dir, output_format, transforms, resume)?;
    let copy_attachments = writer.copies_attachments();
    let mut blob_bytes: HashMap<String, Vec<u8>> = HashMap::new();

    let mut xml_paths = message_vault_io_core::discover_files(input_dir, &is_xml_file)?;
    xml_paths.sort();

    for xml_path in xml_paths {
        message_vault_io_core::check_cancel(cancel)?;
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
                let msgs: Vec<_> = msgs.into_iter().collect();
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
        message_vault_io_core::check_cancel(cancel)?;
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

    message_vault_io_core::check_cancel(cancel)?;

    let hooks = GoSmsProjection {
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
        dedupe_messages(&mut convo.messages);
        let (keep, skipped) =
            prepare_conversation(&mut convo, |a, b| a.sort_key.cmp(&b.sort_key), |k| k);
        report.skipped_invalid_date += skipped;
        if !keep {
            continue;
        }
        let (doc, tally) = pending_to_document(&chat_id, &convo, &hooks);
        report.sent += tally.sent;
        report.received += tally.received;
        documents.push(doc);
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
        assert_eq!(report.extra("pdu_group_messages"), 1);
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
