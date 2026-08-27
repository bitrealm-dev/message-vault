//! Read SMS Backup & Restore XML into the shared conversation structure, then
//! write the chosen output format via [`FormatSink`].

use anyhow::Result;
use contacts::ContactsBook;
use media::MediaMode;
use message_csv::DateRange;
use message_ir::{ConversationDocument, HandleType};
use message_ir_format::{
    ExportTransforms, FormatSink, FormatSinkResult, SbrReadOptions, SbrReadReport,
    read_sbr_documents,
};
use message_vault_io_core::{CancelFlag, ExportReport, OutputFormat, emit_log};
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
    out.extra.insert(
        "skipped_unknown_address".into(),
        report.skipped_unknown_address,
    );
    out.extra
        .insert("skipped_unknown_type".into(), report.skipped_unknown_type);
    out.extra.insert(
        "skipped_draft_or_outbox".into(),
        report.skipped_draft_or_outbox,
    );
    out.extra.insert(
        "skipped_empty_participants".into(),
        report.skipped_empty_participants,
    );
    out.extra.insert(
        "skipped_bad_attachment".into(),
        report.skipped_bad_attachment,
    );
    out
}

/// Fill participant and sender display names from the contacts book.
fn enrich_contacts(book: &ContactsBook, documents: &mut [ConversationDocument]) {
    for document in documents {
        for participant in &mut document.conversation.participants {
            let current = participant.display_name.as_deref().unwrap_or("");
            if let Some(name) =
                book.enrich_display_name(&participant.handle, HandleType::Phone, current)
            {
                participant.display_name = Some(name);
            }
        }
        for message in &mut document.messages {
            let Some(handle) = message.sender_handle.as_deref() else {
                continue;
            };
            let current = message.sender_display_name.as_deref().unwrap_or("");
            if let Some(name) = book.enrich_display_name(handle, HandleType::Phone, current) {
                message.sender_display_name = Some(name);
            }
        }
    }
}

/// Inputs for [`convert_export`].
pub(crate) struct ConvertExportArgs<'a> {
    pub input: &'a Path,
    pub output_dir: &'a Path,
    pub owner_phones: &'a [String],
    pub contacts: &'a ContactsBook,
    pub date_range: &'a DateRange,
    pub transforms: ExportTransforms,
    pub output_format: OutputFormat,
    pub cancel: Option<&'a CancelFlag>,
}

/// Convert SMS Backup & Restore XML into the shared conversation structure,
/// then write the chosen output format.
///
/// # Errors
///
/// Returns an error when the XML cannot be read, a conversation cannot be
/// written, or the user cancels.
pub(crate) fn convert_export(
    args: ConvertExportArgs<'_>,
) -> Result<(ExportReport, FormatSinkResult)> {
    let copy_attachments = args.transforms.copies_attachments();
    let media = if copy_attachments {
        args.transforms.media
    } else {
        MediaMode::Disabled
    };
    let compress = args.transforms.compress.clone();
    let log = args.transforms.log.clone();
    let (mut sink, attachments_dir) =
        FormatSink::open_prepared(args.output_dir, args.output_format, args.transforms)?;
    let (mut documents, report) = read_sbr_documents(
        args.input,
        SbrReadOptions {
            owner_phones: args.owner_phones,
            date_range: args.date_range,
            attachments_dir: Some(&attachments_dir),
            copy_attachments,
            // FormatSink reloads staged bytes after media transforms.
            keep_attachment_bytes: false,
            media,
            compress,
            log: log.as_ref(),
            cancel: args.cancel,
        },
    )?;
    enrich_contacts(args.contacts, &mut documents);

    let total_conversations = documents.len() as u64;
    emit_log(log.as_ref(), "");
    emit_log(
        log.as_ref(),
        format!("Preparing {total_conversations} conversation file(s)..."),
    );
    let mut written = 0u64;
    for document in documents {
        written += 1;
        sink.write_document(document)?;
        #[allow(clippy::manual_is_multiple_of)]
        if written % 100 == 0 || written == total_conversations {
            emit_log(
                log.as_ref(),
                format!("  preparing {written}/{total_conversations}"),
            );
        }
    }
    Ok((to_core_report(report), sink.finish()?))
}
