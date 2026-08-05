//! Thin orchestration from the message-ir SBR reader to packaging.

use anyhow::Result;
use contacts::ContactsBook;
use message_csv::DateRange;
use message_vault_io_core::{CancelFlag, ExportReport, OutputFormat};
use message_ir::{
    ConversationDocument,
};
use message_ir_format::{
    ExportTransforms,
    FormatSink,
    FormatSinkResult,
    SbrReadOptions,
    SbrReadReport,
    read_sbr_documents,
};
use std::path::Path;

/// Map the ir-format read report onto the shared [`ExportReport`] shape,
/// moving reader-specific counters into `extra`.
fn to_core_report(report: SbrReadReport) -> ExportReport {
    let mut out = ExportReport {
        conversations: report.conversations,
        sent: report.sent,
        received: report.received,
        attachments_saved: report.attachments_saved,
        skipped_invalid_date: report.skipped_invalid_date,
        skipped_out_of_range: report.skipped_out_of_range,
        errors: report.errors,
        ..ExportReport::default()
    };
    out.extra.insert("sms_seen".into(), report.sms_seen);
    out.extra.insert("mms_seen".into(), report.mms_seen);
    out.extra
        .insert("skipped_unknown_address".into(), report.skipped_unknown_address);
    out.extra
        .insert("skipped_unknown_type".into(), report.skipped_unknown_type);
    out.extra
        .insert("skipped_draft_or_outbox".into(), report.skipped_draft_or_outbox);
    out.extra.insert(
        "skipped_empty_participants".into(),
        report.skipped_empty_participants,
    );
    out.extra
        .insert("skipped_bad_attachment".into(), report.skipped_bad_attachment);
    out
}

fn enrich_contacts(book: &ContactsBook, documents: &mut [ConversationDocument]) {
    for document in documents {
        for participant in &mut document.conversation.participants {
            let current = participant.display_name.as_deref().unwrap_or("");
            if let Some(name) = book.enrich_display_name(&participant.handle, current) {
                participant.display_name = Some(name);
            }
        }
        for message in &mut document.messages {
            let Some(handle) = message.sender_handle.as_deref() else {
                continue;
            };
            let current = message.sender_display_name.as_deref().unwrap_or("");
            if let Some(name) = book.enrich_display_name(handle, current) {
                message.sender_display_name = Some(name);
            }
        }
    }
}

/// Convert SMS Backup & Restore XML into IR, then package it through FormatSink.
pub(crate) fn convert_export(
    input: &Path,
    output_dir: &Path,
    owner_phones: &[String],
    contacts: &ContactsBook,
    date_range: &DateRange,
    transforms: ExportTransforms,
    output_format: OutputFormat,
    cancel: Option<&CancelFlag>,
) -> Result<(ExportReport, FormatSinkResult)> {
    let copy_attachments = transforms.copies_attachments();
    let (mut sink, attachments_dir) =
        FormatSink::open_prepared(output_dir, output_format, transforms)?;
    let (mut documents, report) = read_sbr_documents(
        input,
        SbrReadOptions {
            owner_phones,
            date_range,
            attachments_dir: Some(&attachments_dir),
            copy_attachments,
            // FormatSink reloads staged bytes after media transforms.
            keep_attachment_bytes: false,
            cancel,
        },
    )?;
    enrich_contacts(contacts, &mut documents);

    for document in documents {
        sink.write_document(document)?;
    }
    Ok((to_core_report(report), sink.finish()?))
}
