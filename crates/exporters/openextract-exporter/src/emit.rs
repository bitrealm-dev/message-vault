//! Convert OpenExtract rows into the shared conversation structure, then write
//! the chosen output format via [`FormatSink`].

use crate::parse::{RawRow, SourceKind, discover_csv_files, parse_csv_file};
use anyhow::Result;
use chrono::DateTime;
use contacts::ContactsBook;
use message_csv::DateRange;
use message_ir::{
    ExportMeta, HandleType, IrParticipant, IrService, IrSource, PendingConversation,
    PendingMessage, ProjectionHooks, ensure_conversation, pending_to_document,
    prepare_conversation,
};
use message_ir_format::{AttachmentSource, ExportTransforms, ExportWriter, FormatSinkResult};
use message_vault_io_core::{CancelFlag, ExportReport, OutputFormat, prepare_outputs};
use phone::sanitize_number;
use serde_json::{Map, json};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

const EXPORT_SOURCE: &str = "openextract";
const EXPORT_TOOL: &str = "OpenExtract";
const EXPORT_TOOL_VERSION: &str = "0.5.1";

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

    let writer = ExportWriter::open(&output, output_format, transforms, resume)?;

    let files = discover_csv_files(input)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();
    // Dedupe duplicate CSV rows: same chat + second + direction + text.
    let mut seen_keys: HashSet<String> = HashSet::new();

    // For per-chat files, infer peer once from all rows in that file.
    for path in &files {
        message_vault_io_core::check_cancel(cancel)?;
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

            let convo = ensure_conversation(&mut conversations, &chat_id, false, None, Vec::new());
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

    message_vault_io_core::check_cancel(cancel)?;

    let hooks = OpenExtractProjection {
        export: message_vault_io_core::export_meta(
            EXPORT_SOURCE,
            EXPORT_TOOL,
            EXPORT_TOOL_VERSION,
            None,
            None,
        ),
    };
    let mut documents = Vec::new();
    for (chat_id, mut convo) in conversations {
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

    // OpenExtract carries no attachments; every attachment source is Missing.
    let sink_result = writer.finish(
        documents,
        &mut |att| (AttachmentSource::Missing, att.size_bytes),
        cancel,
        &mut report,
    )?;

    Ok((report, sink_result))
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
    if sanitize_number(peer).is_some() {
        // Format as E.164 when unambiguous. Otherwise keep digits as-is. Never invent `+0…`.
        let handle = phone::normalize_guarded(peer, phone::PhoneRegion::for_raw(peer)).normalized;
        // The contacts book keys entries by the same guarded policy.
        let name = book
            .lookup_name_by_handle(&handle, HandleType::Phone)
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
    } else if sanitize_number(&row.sender).is_some() {
        book.lookup_name_by_handle(
            &phone::normalize_typed_handle(&row.sender, HandleType::Phone).0,
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

/// OpenExtract deltas of the shared [`pending_to_document`] projection.
struct OpenExtractProjection {
    export: ExportMeta,
}

impl ProjectionHooks for OpenExtractProjection {
    fn export(&self) -> ExportMeta {
        self.export.clone()
    }

    fn service(&self, _msg: &PendingMessage) -> IrService {
        IrService::Sms
    }

    fn source(&self, _convo: &PendingConversation, msg: &PendingMessage) -> IrSource {
        let mut fields = Map::new();
        fields.insert("source_kind".into(), json!(msg.extra_str("source_kind")));
        fields.insert(
            "has_attachments".into(),
            json!(msg.extra_flag("has_attachments")),
        );
        IrSource {
            android_type: None,
            fields,
        }
    }

    /// The roster is the single peer named by the chat id; an unresolved
    /// `unknown` chat has no roster at all.
    fn participants(&self, chat_id: &str, convo: &PendingConversation) -> Vec<IrParticipant> {
        if chat_id.is_empty() || chat_id.eq_ignore_ascii_case("unknown") {
            return Vec::new();
        }
        vec![IrParticipant {
            handle: chat_id.to_string(),
            display_name: convo.first_contact_name(),
            handle_type: Some(HandleType::Phone),
        }]
    }
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
        assert_eq!(report.extra("unresolved_chat_phone"), 0);
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
        assert!(report.extra("unresolved_chat_phone") >= 1);
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
