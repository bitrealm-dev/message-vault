//! Thin orchestration from the message-ir SBR reader to packaging.

use anyhow::Result;
use contacts::ContactsBook;
use message_csv::DateRange;
use message_vault_io_core::{CancelFlag, OutputFormat};
use message_ir::{
    ConversationDocument,
};
use message_ir_format::{
    ExportTransforms,
    FormatSink,
    FormatSinkResult,
    SbrReadOptions,
    SbrReadReport,
    clean_previous_ir_output,
    read_sbr_documents,
};
use std::fs;
use std::path::Path;

pub(crate) type ExportReport = SbrReadReport;

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
    fs::create_dir_all(output_dir)?;
    clean_previous_ir_output(output_dir)?;
    let attachments_dir = output_dir.join("attachments");
    let copy_attachments = transforms.copies_attachments();
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

    let mut sink = FormatSink::open(output_dir, output_format, transforms)?;
    for document in documents {
        sink.write_document(document)?;
    }
    Ok((report, sink.finish()?))
}
