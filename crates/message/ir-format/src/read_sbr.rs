//! Read SMS Backup & Restore XML into [`ConversationDocument`] values.

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
use anyhow::{Result, bail};
use message_csv::{DateRange, format_local_ts, stable_guid};
use message_vault_io_core::{CancelFlag, check_cancel};
use phone::{OwnerPhoneSet, to_e164};
use sbr::{
    AttachmentBlob, ConversationKind, ParseStats, Record, infer_owner_phones, parse_file,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const EXPORT_SOURCE: &str = "sms-backup-restore";
const EXPORT_TOOL: &str = "SMS Backup & Restore";
const EXPORT_TOOL_VERSION: &str = "10.26.003";

#[derive(Debug, Default)]
pub struct SbrReadReport {
    pub conversations: u64,
    pub sms_seen: u64,
    pub mms_seen: u64,
    pub attachments_saved: u64,
    pub sent: u64,
    pub received: u64,
    pub skipped_invalid_date: u64,
    pub skipped_out_of_range: u64,
    pub skipped_unknown_address: u64,
    pub skipped_unknown_type: u64,
    pub skipped_draft_or_outbox: u64,
    pub skipped_empty_participants: u64,
    pub skipped_bad_attachment: u64,
    pub errors: Vec<String>,
}

pub struct SbrReadOptions<'a> {
    pub owner_phones: &'a [String],
    pub date_range: &'a DateRange,
    pub attachments_dir: Option<&'a Path>,
    pub copy_attachments: bool,
    pub keep_attachment_bytes: bool,
    pub cancel: Option<&'a CancelFlag>,
}

#[derive(Debug, Clone)]
struct PendingAttachment {
    rel_path: String,
    original_name: Option<String>,
    mime_type: Option<String>,
    digest: String,
    bytes: Option<Arc<[u8]>>,
}

#[derive(Debug, Clone)]
struct PendingMessage {
    sort_key: f64,
    is_from_me: bool,
    sender_digits: Option<String>,
    sender_display_name: Option<String>,
    text: String,
    subject: String,
    attachments: Vec<PendingAttachment>,
    dedupe_key: String,
    message_kind: &'static str,
    date_ms: String,
    contact_name: String,
    android_type: String,
    source_fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Default)]
struct PendingConversation {
    kind: ConversationKind,
    group_title: Option<String>,
    participant_e164s: Vec<String>,
    messages: Vec<PendingMessage>,
}

fn collect_xml_paths(input: &Path) -> Result<Vec<PathBuf>> {
    if input.is_file() {
        return Ok(vec![input.to_path_buf()]);
    }
    if !input.is_dir() {
        bail!("input is not a file or directory: {}", input.display());
    }
    let mut paths: Vec<_> = fs::read_dir(input)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
        })
        .collect();
    paths.sort();
    if paths.is_empty() {
        bail!("no .xml files found in {}", input.display());
    }
    Ok(paths)
}

fn merge_stats(report: &mut SbrReadReport, stats: ParseStats) {
    report.sms_seen += stats.sms_seen;
    report.mms_seen += stats.mms_seen;
    report.skipped_invalid_date += stats.skipped_invalid_date;
    report.skipped_unknown_address += stats.skipped_unknown_address;
    report.skipped_unknown_type += stats.skipped_unknown_type;
    report.skipped_draft_or_outbox += stats.skipped_draft_or_outbox;
    report.skipped_empty_participants += stats.skipped_empty_participants;
    report.skipped_bad_attachment += stats.skipped_bad_attachment;
}

fn stage_attachments(
    blobs: &[AttachmentBlob],
    options: &SbrReadOptions<'_>,
    report: &mut SbrReadReport,
) -> Result<Vec<PendingAttachment>> {
    if !options.copy_attachments && !options.keep_attachment_bytes {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(blobs.len());
    for blob in blobs {
        if options.copy_attachments {
            if let Some(dir) = options.attachments_dir {
                let path = dir.join(&blob.filename);
                if !path.exists() {
                    fs::write(&path, blob.data.as_ref())?;
                    report.attachments_saved += 1;
                }
            }
        }
        out.push(PendingAttachment {
            rel_path: format!("attachments/{}", blob.filename),
            original_name: blob.original_name.clone(),
            mime_type: blob.mime_type.clone(),
            digest: blob.digest_hex.clone(),
            bytes: options
                .keep_attachment_bytes
                .then(|| Arc::clone(&blob.data)),
        });
    }
    Ok(out)
}

fn chat_id(record: &Record) -> String {
    match record.conversation_kind {
        ConversationKind::Group => format!("chat-{}", record.chat_key),
        ConversationKind::Individual => to_e164(&record.chat_key),
    }
}

fn add_record(
    conversations: &mut BTreeMap<String, PendingConversation>,
    record: Record,
    attachments: Vec<PendingAttachment>,
) -> Result<()> {
    let id = chat_id(&record);
    let peers = record
        .participant_digits
        .iter()
        .map(|(d, _)| to_e164(d))
        .filter(|d| !d.is_empty())
        .collect();
    let conversation = conversations
        .entry(id)
        .or_insert_with(|| PendingConversation {
            kind: record.conversation_kind,
            group_title: record.group_title.clone(),
            participant_e164s: peers,
            messages: Vec::new(),
        });
    let names: Vec<_> = attachments.iter().map(|a| a.rel_path.as_str()).collect();
    let dedupe_key = format!(
        "{}|{}|{}|{}",
        record.timestamp_secs as i64,
        u8::from(record.is_from_me),
        record.text,
        names.join(",")
    );
    let source_fields = serde_json::to_value(&record.source_fields)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    conversation.messages.push(PendingMessage {
        sort_key: record.timestamp_secs,
        is_from_me: record.is_from_me,
        sender_digits: record.sender_digits,
        sender_display_name: record.sender_display_name,
        text: record.text,
        subject: record.subject,
        attachments,
        dedupe_key,
        message_kind: record.message_kind,
        date_ms: record.date_ms,
        contact_name: record.contact_name,
        android_type: record.android_type,
        source_fields,
    });
    Ok(())
}

fn dedupe(messages: &mut Vec<PendingMessage>) {
    messages.sort_by(|a, b| a.sort_key.total_cmp(&b.sort_key));
    let mut seen = HashSet::new();
    messages.retain(|m| seen.insert(m.dedupe_key.clone()));
}

fn names_by_handle(conversation: &PendingConversation) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for message in &conversation.messages {
        if let (Some(digits), Some(name)) = (
            &message.sender_digits,
            message
                .sender_display_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        ) {
            names
                .entry(to_e164(digits))
                .or_insert_with(|| name.to_string());
        }
        if conversation.kind == ConversationKind::Individual {
            let name = message.contact_name.trim();
            if !name.is_empty() {
                for peer in &conversation.participant_e164s {
                    names
                        .entry(peer.clone())
                        .or_insert_with(|| name.to_string());
                }
            }
        }
    }
    names
}

fn to_document(
    id: &str,
    conversation: &PendingConversation,
    owner_handle: Option<&str>,
    report: &mut SbrReadReport,
) -> ConversationDocument {
    let names = names_by_handle(conversation);
    let export = ExportMeta {
        source: EXPORT_SOURCE.into(),
        tool: EXPORT_TOOL.into(),
        tool_version: EXPORT_TOOL_VERSION.into(),
        owner_handle: owner_handle.map(str::to_string),
        owner_display_name: None,
    };
    let owner = owner_sender(&export);
    let mut messages = Vec::with_capacity(conversation.messages.len());
    for message in &conversation.messages {
        if message.is_from_me {
            report.sent += 1
        } else {
            report.received += 1
        }
        let timestamp_unix_ms = message
            .date_ms
            .parse()
            .unwrap_or_else(|_| (message.sort_key as i64).saturating_mul(1000));
        let timestamp = format_local_ts(message.sort_key as i64).expect("timestamps validated");
        let digests: Vec<_> = message
            .attachments
            .iter()
            .map(|a| a.digest.clone())
            .collect();
        let sender = if message.is_from_me {
            owner.clone()
        } else {
            (
                message.sender_digits.as_deref().map(to_e164),
                message.sender_display_name.clone(),
            )
        };
        messages.push(IrMessage {
            guid: stable_guid(
                id,
                &timestamp.0,
                message.is_from_me,
                &message.text,
                &digests,
            ),
            timestamp_unix_ms,
            direction: if message.is_from_me {
                IrDirection::Outgoing
            } else {
                IrDirection::Incoming
            },
            service: IrService::Sms,
            message_kind: IrMessageKind::parse(message.message_kind),
            sender_handle: sender.0,
            sender_display_name: sender.1,
            subject: (!message.subject.is_empty()).then(|| message.subject.clone()),
            text: message.text.clone(),
            attachments: message
                .attachments
                .iter()
                .map(|a| IrAttachment {
                    path: Some(a.rel_path.clone()),
                    original_name: a.original_name.clone(),
                    mime_type: a.mime_type.clone(),
                    digest_sha256: Some(a.digest.clone()),
                    is_sticker: false,
                    transcription: None,
                    sticker_effect: None,
                    bytes: a.bytes.as_ref().map(|b| b.as_ref().to_vec()),
                })
                .collect(),
            imessage: None,
            source: IrSource {
                android_type: message.android_type.trim().parse().ok(),
                fields: message.source_fields.clone(),
            }
            .into_option(),
        });
    }
    let participants = conversation
        .participant_e164s
        .iter()
        .filter(|h| !h.is_empty())
        .map(|handle| IrParticipant {
            handle: handle.clone(),
            display_name: names.get(handle).cloned(),
        })
        .collect();
    let mut document = ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export,
        conversation: ConversationMeta {
            chat_identifier: id.into(),
            conversation_type: match conversation.kind {
                ConversationKind::Individual => IrConversationType::Individual,
                ConversationKind::Group => IrConversationType::Group,
            },
            group_title: conversation.group_title.clone(),
            participants,
            stats: ConversationStats::default(),
        },
        messages,
        packaging_stem_suffix: None,
    };
    document.finalize_stats();
    document
}

/// Parse SBR XML, project records to IR, stage attachments, filter, and dedupe.
pub fn read_sbr_documents(
    input: &Path,
    options: SbrReadOptions<'_>,
) -> Result<(Vec<ConversationDocument>, SbrReadReport)> {
    let paths = collect_xml_paths(input)?;
    let mut owner_phones = options.owner_phones.to_vec();
    if owner_phones.is_empty() {
        for path in &paths {
            owner_phones.extend(infer_owner_phones(path)?);
        }
        owner_phones.sort();
        owner_phones.dedup();
    }
    let (owners, owner_handle) = if owner_phones.is_empty() {
        (HashSet::new(), None)
    } else {
        let owners = OwnerPhoneSet::new(&owner_phones)?;
        let handle = to_e164(&owners.primary_digits);
        (owners.all_digits, Some(handle))
    };
    if options.copy_attachments {
        if let Some(dir) = options.attachments_dir {
            fs::create_dir_all(dir)?;
        }
    }

    let mut report = SbrReadReport::default();
    let mut conversations = BTreeMap::new();
    for path in paths {
        check_cancel(options.cancel).map_err(anyhow::Error::msg)?;
        match parse_file(&path, &owners) {
            Ok((records, stats)) => {
                merge_stats(&mut report, stats);
                for record in records {
                    if !options.date_range.contains_secs_f64(record.timestamp_secs) {
                        report.skipped_out_of_range += 1;
                        continue;
                    }
                    match stage_attachments(&record.attachments, &options, &mut report)
                        .and_then(|attachments| add_record(&mut conversations, record, attachments))
                    {
                        Ok(()) => {}
                        Err(error) => report.errors.push(format!("{}: {error:#}", path.display())),
                    }
                }
            }
            Err(error) => report.errors.push(format!("{}: {error:#}", path.display())),
        }
    }
    check_cancel(options.cancel).map_err(anyhow::Error::msg)?;
    let mut documents = Vec::new();
    for (id, mut conversation) in conversations {
        dedupe(&mut conversation.messages);
        conversation.messages.retain(|message| {
            let valid = format_local_ts(message.sort_key as i64).is_some();
            if !valid {
                report.skipped_invalid_date += 1;
            }
            valid
        });
        if conversation.messages.is_empty() {
            continue;
        }
        documents.push(to_document(
            &id,
            &conversation,
            owner_handle.as_deref(),
            &mut report,
        ));
        report.conversations += 1;
    }
    Ok((documents, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SbrBackupSession;

    #[test]
    fn reads_then_writes_source_fields_and_attachment() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.xml");
        fs::write(&input, r#"<smses><mms date="1400773400000" msg_box="2" address="+15555550101" extra="yes"><parts><part seq="0" ct="image/jpeg" name="pic.jpg" data="aGVsbG8="/></parts><addrs><addr address="+15555550100" type="137" charset="106"/><addr address="+15555550101" type="151"/></addrs></mms></smses>"#).unwrap();
        let output = dir.path().join("output");
        let stage = output.join("attachments");
        let (docs, report) = read_sbr_documents(
            &input,
            SbrReadOptions {
                owner_phones: &[],
                date_range: &DateRange::default(),
                attachments_dir: Some(&stage),
                copy_attachments: true,
                keep_attachment_bytes: false,
                cancel: None,
            },
        )
        .unwrap();
        assert_eq!(report.attachments_saved, 1);
        assert_eq!(docs[0].export.owner_handle.as_deref(), Some("+15555550100"));
        assert_eq!(
            docs[0].messages[0].source.as_ref().unwrap().fields["attrs"]["extra"],
            "yes"
        );
        let mut writer = SbrBackupSession::create(&output).unwrap();
        writer.append_document(&docs[0]).unwrap();
        let xml = fs::read_to_string(writer.finish().unwrap()).unwrap();
        assert!(xml.contains(r#"extra="yes""#));
        assert!(xml.contains(r#"data="aGVsbG8=""#));
        assert!(xml.contains(r#"charset="106""#));
    }
}
