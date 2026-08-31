//! Read SMS Backup & Restore XML into the shared conversation structure, then
//! write the chosen output format via [`FormatSink`].

use anyhow::Result;
use contacts::ContactsBook;
use media::MediaMode;
use message_csv::DateRange;
use message_ir::{ConversationDocument, HandleType};
use message_ir_format::{
    AttachmentSource, ConversationUnit, ExportTransforms, FormatSink, FormatSinkResult,
    SbrReadOptions, SbrReadReport, WriteQueueOptions, read_sbr_documents,
};
use message_vault_io_core::{CancelFlag, ExportReport, OutputFormat};
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
    /// Continue an interrupted export: keep previous output and skip the
    /// conversations already written.
    pub resume: bool,
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
    // Captured before `transforms` moves into the sink: the queue path is for
    // the import, which is JSONL and never obfuscated.
    let use_queue = args.output_format == OutputFormat::Jsonl && !args.transforms.obfuscate;
    let (sink, attachments_dir) = if args.resume {
        FormatSink::open_resume(args.output_dir, args.output_format, args.transforms)
    } else {
        FormatSink::open_prepared(args.output_dir, args.output_format, args.transforms)
    }?;
    let (mut documents, report) = read_sbr_documents(
        args.input,
        SbrReadOptions {
            owner_phones: args.owner_phones,
            date_range: args.date_range,
            attachments_dir: Some(&attachments_dir),
            copy_attachments,
            // On the queue path the bytes ride into the engine, which stages
            // them a conversation at a time; otherwise FormatSink reloads
            // staged bytes after media transforms.
            keep_attachment_bytes: use_queue,
            stage_attachments: !use_queue,
            media,
            compress: compress.clone(),
            log: log.as_ref(),
            cancel: args.cancel,
        },
    )?;
    enrich_contacts(args.contacts, &mut documents);

    if use_queue {
        let units: Vec<ConversationUnit> = documents
            .into_iter()
            .map(|doc| {
                ConversationUnit::from_doc(doc, |_, att| {
                    let hint = att
                        .size_bytes
                        .or_else(|| att.bytes.as_ref().map(|b| b.len() as u64));
                    match att.bytes.take() {
                        Some(bytes) => (AttachmentSource::Bytes(bytes), hint),
                        None => (AttachmentSource::Missing, hint),
                    }
                })
            })
            .collect();
        let options = WriteQueueOptions {
            media,
            compress,
            resume: args.resume,
            writer_count: 0,
        };
        let queue_report = message_ir_format::drain_write_queue(
            args.output_dir,
            units,
            &options,
            log.as_ref(),
            args.cancel,
        )?;
        let mut core = to_core_report(report);
        core.attachments_saved = queue_report.attachments_saved as u64;
        return Ok((
            core,
            FormatSinkResult {
                xml_path: None,
                media: queue_report.media,
                obfuscated_docs: 0,
            },
        ));
    }

    // The reader already counted conversations; zero the counter so the shared
    // write tail counts only the documents it actually writes.
    let mut core = to_core_report(report);
    core.conversations = 0;
    let sink_result = message_ir_format::write_documents_through_sink(
        documents,
        sink,
        log.as_ref(),
        args.cancel,
        &mut core,
    )?;
    Ok((core, sink_result))
}
