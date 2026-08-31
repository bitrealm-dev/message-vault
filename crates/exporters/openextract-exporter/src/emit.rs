//! Convert OpenExtract rows into the shared conversation structure, then write
//! the chosen output format via [`FormatSink`].

use crate::parse::{RawRow, SourceKind, discover_csv_files, parse_csv_file};
use anyhow::Result;
use chrono::DateTime;
use contacts::ContactsBook;
use message_csv::{DateRange, format_local_ts, stable_guid};
use message_ir::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, HandleType,
    IrConversationType, IrDirection, IrMessage, IrMessageKind, IrParticipant, IrService, IrSource,
    PendingConversation, PendingMessage, SCHEMA_VERSION, owner_sender,
};
use message_ir_format::{
    AttachmentSource, ConversationUnit, ExportTransforms, FormatSink, FormatSinkResult,
    WriteQueueOptions,
};
use message_vault_io_core::{CancelFlag, ExportReport, OutputFormat, prepare_outputs};
use phone::sanitize_number;
use serde_json::{Map, json};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

const EXPORT_SOURCE: &str = "openextract";
const EXPORT_TOOL: &str = "OpenExtract";
const EXPORT_TOOL_VERSION: &str = "0.5.1";

/// Read a per-exporter counter from the report's `extra` map (test assertions).
#[cfg(test)]
fn count(report: &ExportReport, key: &str) -> u64 {
    report.extra.get(key).copied().unwrap_or(0)
}

/// Inputs for [`convert_export`].
pub(crate) struct ConvertExportArgs<'a> {
    pub input: &'a Path,
    pub output: &'a Path,
    pub book: &'a ContactsBook,
    pub date_range: &'a DateRange,
    pub transforms: ExportTransforms,
    pub output_format: OutputFormat,
    pub cancel: Option<&'a CancelFlag>,
    /// Continue an interrupted export: keep previous output and skip the
    /// conversations already written.
    pub resume: bool,
}

/// Convert OpenExtract CSV(s) under `input` using `book` (from VCF/contacts).
///
/// When `cancel` is set, cooperative cancellation is checked between CSV files
/// and before writing. Cancelled runs return an error with message `cancelled`.
///
/// # Errors
///
/// Returns an error when output overlaps input, a CSV cannot be parsed, or the
/// user cancels.
pub(crate) fn convert_export(
    args: ConvertExportArgs<'_>,
) -> Result<(ExportReport, FormatSinkResult)> {
    let ConvertExportArgs {
        input,
        output,
        book,
        date_range,
        transforms,
        output_format,
        cancel,
        resume,
    } = args;
    let (inputs, output) = prepare_outputs(&[input.to_path_buf()], output)?;
    let input = &inputs[0];

    let log = transforms.log.clone();
    let transforms_media = transforms.media;
    let compress = transforms.compress.clone();
    // Captured before `transforms` moves into the sink: the queue path is for
    // the import, which is JSONL and never obfuscated.
    let use_queue = output_format == OutputFormat::Jsonl && !transforms.obfuscate;
    let (sink, _attachments_dir) = if resume {
        FormatSink::open_resume(&output, output_format, transforms)
    } else {
        FormatSink::open_prepared(&output, output_format, transforms)
    }?;

    let files = discover_csv_files(input)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();
    // Dedupe duplicate CSV rows: same chat + second + direction + text.
    let mut seen_keys: HashSet<String> = HashSet::new();

    // For per-chat files, infer peer once from all rows in that file.
    for path in &files {
        message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;
        let rows = match parse_csv_file(path) {
            Ok(r) => r,
            Err(e) => {
                report.errors.push(format!("{}: {e:#}", path.display()));
                continue;
            }
        };
        if rows.is_empty() {
            continue;
        }

        let per_chat_peer = if rows[0].source_kind == SourceKind::PerChat {
            Some(infer_peer_label(&rows))
        } else {
            None
        };

        for row in rows {
            let peer_label = row
                .conversation
                .as_deref()
                .filter(|s| !s.is_empty() && !is_me(s))
                .map(|s| s.to_string())
                .or_else(|| {
                    if !row.is_from_me && !is_me(&row.sender) {
                        Some(row.sender.clone())
                    } else {
                        per_chat_peer.clone()
                    }
                })
                .unwrap_or_else(|| "unknown".to_string());

            let (chat_id, contact_name, unresolved) = resolve_chat(book, &peer_label);
            if unresolved {
                report.bump("unresolved_chat_phone", 1);
            }

            let Some((secs, date_ms)) = parse_timestamp(&row.date) else {
                report.skipped_invalid_date += 1;
                continue;
            };
            if !date_range.contains_secs(secs) {
                report.skipped_out_of_range += 1;
                continue;
            }

            let is_from_me = resolve_is_from_me(&row);
            let (sender_handle, sender_display_name) =
                resolve_sender(book, &row, is_from_me, &chat_id, &contact_name);

            let dedupe_key = format!(
                "{}|{}|{}|{}",
                chat_id,
                secs,
                if is_from_me { "1" } else { "0" },
                row.text
            );
            if !seen_keys.insert(dedupe_key) {
                report.duplicates_dropped += 1;
                continue;
            }

            let convo =
                conversations
                    .entry(chat_id.clone())
                    .or_insert_with(|| PendingConversation {
                        chat_id: chat_id.clone(),
                        display_name: None,
                        participant_e164s: Vec::new(),
                        messages: Vec::new(),
                        is_group: false,
                        has_attachments: false,
                        extra: BTreeMap::new(),
                    });
            convo.messages.push(PendingMessage {
                sort_key: secs,
                is_from_me,
                sender_handle,
                sender_display_name: if sender_display_name.is_empty() {
                    None
                } else {
                    Some(sender_display_name)
                },
                text: row.text,
                attachments: Vec::new(),
                extra: {
                    let mut e = BTreeMap::new();
                    e.insert("contact_name".into(), contact_name);
                    e.insert("date_ms".into(), date_ms);
                    e.insert(
                        "has_attachments".into(),
                        if row.has_attachments { "true" } else { "false" }.into(),
                    );
                    e.insert("source_kind".into(), row.source_kind.as_str().to_string());
                    e
                },
            });
        }
    }

    message_vault_io_core::check_cancel(cancel).map_err(anyhow::Error::msg)?;

    let mut documents = Vec::new();
    for (chat_id, mut convo) in conversations {
        if !prepare_conversation(&mut convo, &mut report) {
            continue;
        }
        documents.push(pending_to_document(&chat_id, &convo, &mut report)?);
    }

    let sink_result = if use_queue {
        // OpenExtract carries no attachments; the queue is still worth taking
        // for its parallel conversation writes and its resume skip.
        let units: Vec<ConversationUnit> = documents
            .into_iter()
            .map(|doc| {
                ConversationUnit::from_doc(doc, |_, att| {
                    (AttachmentSource::Missing, att.size_bytes)
                })
            })
            .collect();
        let options = WriteQueueOptions {
            media: transforms_media,
            compress: compress.clone(),
            resume,
            writer_count: 0,
        };
        message_ir_format::drain_units(&output, units, &options, log.as_ref(), cancel, &mut report)?
    } else {
        message_ir_format::write_documents_through_sink(
            documents,
            sink,
            log.as_ref(),
            cancel,
            &mut report,
        )?
    };

    Ok((report, sink_result))
}

fn prepare_conversation(convo: &mut PendingConversation, report: &mut ExportReport) -> bool {
    if convo.messages.is_empty() {
        return false;
    }
    convo.messages.sort_by_key(|m| m.sort_key);
    message_vault_io_core::prune_and_finish_conversation(convo, report, |k| k)
}

fn is_me(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("me")
}

fn infer_peer_label(rows: &[RawRow]) -> String {
    let mut phone_peer = None;
    let mut name_peer = None;
    for row in rows {
        if row.is_from_me || is_me(&row.sender) {
            continue;
        }
        if sanitize_number(&row.sender).is_some() {
            phone_peer.get_or_insert_with(|| row.sender.clone());
        } else if name_peer.is_none() {
            name_peer = Some(row.sender.clone());
        }
    }
    phone_peer
        .or(name_peer)
        .unwrap_or_else(|| "unknown".to_string())
}

/// Returns `(chat_identifier, contact_name, unresolved_phone)`.
fn resolve_chat(book: &ContactsBook, peer: &str) -> (String, String, bool) {
    let peer = peer.trim();
    if peer.is_empty() || peer.eq_ignore_ascii_case("unknown") {
        return ("unknown".to_string(), String::new(), true);
    }
    if let Some(digits) = sanitize_number(peer) {
        // Format as E.164 when unambiguous. Otherwise keep digits as-is. Never invent `+0…`.
        let handle = phone::normalize_guarded(peer, phone::PhoneRegion::for_raw(peer)).normalized;
        // The contacts book keys entries by its own US-digit form; look up
        // with that form so +-prefixed raws still resolve names.
        let book_form = phone::normalize_guarded(&digits, phone::PhoneRegion::Usa).normalized;
        let name = book
            .lookup_name_by_handle(&book_form, HandleType::Phone)
            .unwrap_or("")
            .to_string();
        return (handle, name, false);
    }
    if let Some((e164, _)) = book.lookup_handle_by_name(peer) {
        return (e164, peer.to_string(), false);
    }
    // Name-only chat id — not fatal; vault may struggle later.
    (
        message_vault_io_core::name_stem(peer),
        peer.to_string(),
        true,
    )
}

fn resolve_is_from_me(row: &RawRow) -> bool {
    if let Some(dir) = row.direction.as_deref() {
        let d = dir.trim().to_ascii_lowercase();
        if d == "sent" || d == "outgoing" {
            return true;
        }
        if d == "received" || d == "incoming" {
            return false;
        }
    }
    row.is_from_me
}

fn resolve_sender(
    book: &ContactsBook,
    row: &RawRow,
    is_from_me: bool,
    chat_id: &str,
    contact_name: &str,
) -> (String, String) {
    if is_from_me {
        return (String::new(), String::new());
    }
    // Prefer phone on chat_id when it looks like E.164.
    let handle = if chat_id.starts_with('+') || sanitize_number(chat_id).is_some() {
        if chat_id.starts_with('+') {
            // Only unambiguous +-prefixed values pass through. A fabricated
            // `+0…` stays digits-as-is so the vault can flag it.
            phone::normalize_guarded(chat_id, phone::PhoneRegion::for_raw(chat_id)).normalized
        } else {
            sanitize_number(chat_id)
                .map(|d| phone::normalize_guarded(&d, phone::PhoneRegion::Usa).normalized)
                .unwrap_or_default()
        }
    } else if let Some(digits) = sanitize_number(&row.sender) {
        phone::normalize_guarded(&digits, phone::PhoneRegion::Usa).normalized
    } else {
        String::new()
    };

    let display = if !contact_name.is_empty() {
        contact_name.to_string()
    } else if let Some(digits) = sanitize_number(&row.sender) {
        book.lookup_name_by_handle(
            &phone::normalize_guarded(&digits, phone::PhoneRegion::Usa).normalized,
            HandleType::Phone,
        )
        .unwrap_or("")
        .to_string()
    } else if !is_me(&row.sender) {
        row.sender.clone()
    } else {
        String::new()
    };

    (handle, display)
}

fn parse_timestamp(raw: &str) -> Option<(i64, String)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    // RFC3339 / ISO-8601 with offset (OpenExtract style).
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        let secs = dt.timestamp();
        return Some((secs, (secs * 1000).to_string()));
    }
    // Fallback without fractional seconds.
    if let Ok(dt) = DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%z") {
        let secs = dt.timestamp();
        return Some((secs, (secs * 1000).to_string()));
    }
    None
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

/// Build a [`ConversationDocument`] from one pending conversation.
///
/// Currently always returns `Ok`. The `Result` matches the other exporters.
fn pending_to_document(
    chat_id: &str,
    convo: &PendingConversation,
    report: &mut ExportReport,
) -> Result<ConversationDocument> {
    let contact_name = first_contact_name(convo);
    let participants = if chat_id.is_empty() || chat_id.eq_ignore_ascii_case("unknown") {
        Vec::new()
    } else {
        vec![IrParticipant {
            handle: chat_id.to_string(),
            display_name: contact_name,
            handle_type: Some(HandleType::Phone),
        }]
    };

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
        if msg.is_from_me {
            report.sent += 1;
        } else {
            report.received += 1;
        }
        report.messages += 1;
        let secs = msg.sort_key;
        let (ts_local, _, _) = format_local_ts(secs).expect("timestamp validated above");
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &[]);
        let timestamp_unix_ms = msg
            .extra_str("date_ms")
            .parse::<i64>()
            .unwrap_or_else(|_| secs.saturating_mul(1000));

        let mut fields = Map::new();
        fields.insert("source_kind".into(), json!(msg.extra_str("source_kind")));
        fields.insert(
            "has_attachments".into(),
            json!(msg.extra_flag("has_attachments")),
        );
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
            timestamp_unix_ms,
            direction: if msg.is_from_me {
                IrDirection::Outgoing
            } else {
                IrDirection::Incoming
            },
            service: IrService::Sms,
            message_kind: IrMessageKind::Sms,
            sender_handle,
            sender_display_name,
            subject: None,
            text: msg.text.clone(),
            attachments: Vec::new(),
            imessage: None,
            source,
        });
    }

    Ok(ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export,
        conversation: ConversationMeta {
            chat_identifier: chat_id.to_string(),
            conversation_type: IrConversationType::Individual,
            group_title: None,
            participants,
            stats: ConversationStats::default(),
        },
        messages,
        packaging_stem_suffix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use contacts::ContactsBook;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;

    fn write(dir: &tempfile::TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut f = File::create(&path).unwrap();
        write!(f, "{body}").unwrap();
        path
    }

    fn convert(
        input: &std::path::Path,
        output: &std::path::Path,
        book: &ContactsBook,
        date_range: &DateRange,
    ) -> Result<(ExportReport, FormatSinkResult)> {
        convert_export(ConvertExportArgs {
            input,
            output,
            book,
            date_range,
            transforms: ExportTransforms::none(),
            output_format: OutputFormat::Csv,
            cancel: None,
            resume: false,
        })
    }

    #[test]
    fn phone_peer_gets_vcf_name() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "conversation_1.csv",
            "Date,Sender,Text,Is From Me,Has Attachments\n\
2020-01-01T12:00:00+00:00,+15555550122,Hello,False,False\n\
2020-01-01T12:01:00+00:00,me,Hi,True,False\n",
        );
        let vcf = write(
            &dir,
            "contacts.vcf",
            "BEGIN:VCARD\nVERSION:3.0\nN:Example;Sam;;;\nFN:Sam Example\n\
TEL;TYPE=CELL:+1-555-555-0122\nEND:VCARD\n",
        );
        let book = ContactsBook::load_vcf(&vcf).unwrap();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book, &DateRange::default()).unwrap();
        assert_eq!(report.conversations, 1);
        assert_eq!(count(&report, "unresolved_chat_phone"), 0);
        let csv_path = out.join("+15555550122.csv");
        let body = fs::read_to_string(&csv_path).unwrap();
        assert!(body.contains("Sam Example"));
        assert!(body.contains("openextract"));
    }

    #[test]
    fn name_without_phone_still_writes() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "conversation_2.csv",
            "Date,Sender,Text,Is From Me,Has Attachments\n\
2020-01-01T12:00:00+00:00,Cathy Arp,Hi,False,False\n\
2020-01-01T12:01:00+00:00,me,Hello,True,False\n",
        );
        let vcf = write(
            &dir,
            "contacts.vcf",
            "BEGIN:VCARD\nVERSION:3.0\nN:Other;Person;;;\nFN:Other Person\n\
TEL:+15555550999\nEND:VCARD\n",
        );
        let book = ContactsBook::load_vcf(&vcf).unwrap();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book, &DateRange::default()).unwrap();
        assert!(count(&report, "unresolved_chat_phone") >= 1);
        assert_eq!(report.conversations, 1);
        let csv_path = out.join("Cathy_Arp.csv");
        assert!(csv_path.is_file(), "missing {}", csv_path.display());
    }

    #[test]
    fn date_range_skips_messages_outside_window() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "conversation_1.csv",
            "Date,Sender,Text,Is From Me,Has Attachments\n\
2019-12-31T23:00:00+00:00,+15555550122,Old,False,False\n\
2020-01-01T12:00:00+00:00,+15555550122,Keep,False,False\n\
2020-01-02T00:00:00+00:00,+15555550122,New,False,False\n",
        );
        let book = ContactsBook::empty();
        let out = dir.path().join("out");
        let range =
            DateRange::parse_optional_tz(Some("2020-01-01"), Some("2020-01-02"), Some("UTC"))
                .unwrap();
        let (report, _) = convert(dir.path(), &out, &book, &range).unwrap();
        assert_eq!(report.skipped_out_of_range, 2);
        assert_eq!(report.messages, 1);
        let body = fs::read_to_string(out.join("+15555550122.csv")).unwrap();
        assert!(body.contains("Keep"));
        assert!(!body.contains("Old"));
        assert!(!body.contains("New"));
    }

    #[test]
    fn duplicate_rows_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "conversation_1.csv",
            "Date,Sender,Text,Is From Me,Has Attachments\n\
2020-01-01T12:00:00+00:00,+15555550122,Hello,False,False\n\
2020-01-01T12:00:00+00:00,+15555550122,Hello,False,False\n\
2020-01-01T12:01:00+00:00,me,Hi,True,False\n",
        );
        let book = ContactsBook::empty();
        let out = dir.path().join("out");
        let (report, _) = convert(dir.path(), &out, &book, &DateRange::default()).unwrap();
        assert_eq!(report.duplicates_dropped, 1);
        assert_eq!(report.messages, 2);
        assert_eq!(report.conversations, 1);
    }
}
