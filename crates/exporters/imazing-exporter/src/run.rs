//! Full export pipeline for CLI and in-process GUI.

use crate::emit::{ExportReport, convert_export};
use anyhow::{Result, bail};
use contacts::ContactsBook;
use message_vault_io_core::{RunResult, ExporterConfig, SourceConfig};
use message_ir_format::ExportTransforms;
use std::path::Path;

/// Load contacts, convert, apply media/obfuscate via FormatSink.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let SourceConfig::Imazing(source) = &config.source else {
        bail!("imazing-exporter requires SourceConfig::Imazing");
    };
    let input = config.require_input().map_err(anyhow::Error::msg)?;
    let mut messages = Vec::new();
    let (contacts_csv, contacts_vcf) = config.contacts_csv_vcf();
    let (book, contacts_path) = match (contacts_csv, contacts_vcf) {
        (Some(path), None) | (None, Some(path)) => {
            if !path.is_file() {
                bail!("contacts file not found: {}", path.display());
            }
            let book = ContactsBook::load_contacts_file(&path)?;
            (book, Some(path))
        }
        (Some(_), Some(_)) => bail!("contacts config must be CSV or VCF, not both"),
        (None, None) => {
            messages.push(
                "warning: no contacts file provided (--contacts); \
                 phone numbers will not be resolved to names"
                    .to_string(),
            );
            (ContactsBook::empty(), None)
        }
    };

    let mut transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
    transforms.log = config.log.clone();
    let (report, sink) = convert_export(
        input,
        &config.output,
        &book,
        source.timezone.as_deref(),
        &config.date_range,
        transforms,
        config.output_format,
        config.cancel.as_ref(),
    )?;
    if !sink.media.errors.is_empty() && sink.media.processed == 0 && config.media.mode.needs_tools()
    {
        anyhow::bail!("media processing failed for all candidate files");
    }
    messages.extend(sink.log_lines());

    messages.extend(report_summary_lines(
        &report,
        &config.output,
        contacts_path.as_deref(),
    ));
    Ok(RunResult { messages })
}

/// Format the convert summary the same way the CLI prints it.
fn report_summary_lines(
    report: &ExportReport,
    output: &Path,
    contacts: Option<&Path>,
) -> Vec<String> {
    let mut lines = vec![
        format!("Wrote {}", output.display()),
        match contacts {
            Some(path) => format!("  contacts from:       {}", path.display()),
            None => "  contacts from:       (none)".to_string(),
        },
        format!("  messages CSVs:       {}", report.messages_files),
        format!("  whatsapp CSVs:       {}", report.whatsapp_files),
        format!("  conversations:       {}", report.conversations),
        format!("  messages:            {}", report.messages),
        format!("  attachments:         {}", report.attachments_saved),
        format!(
            "  sent / received:     {} / {}",
            report.sent, report.received
        ),
    ];
    if report.notifications > 0 {
        lines.push(format!("  notifications:       {}", report.notifications));
    }
    if report.duplicates_dropped > 0 {
        lines.push(format!(
            "  duplicates dropped:  {}",
            report.duplicates_dropped
        ));
    }
    if report.skipped_invalid_date > 0 {
        lines.push(format!(
            "  skipped bad date:    {}",
            report.skipped_invalid_date
        ));
    }
    if report.skipped_out_of_range > 0 {
        lines.push(format!(
            "  skipped date range:  {}",
            report.skipped_out_of_range
        ));
    }
    if report.unresolved_chat_phone > 0 {
        lines.push(format!(
            "  unresolved phone:    {} (name-only chat ids; vault import may struggle)",
            report.unresolved_chat_phone
        ));
    }
    if report.unresolved_group_participants > 0 {
        lines.push(format!(
            "  unresolved members:  {} (group roster names with no phone in contacts)",
            report.unresolved_group_participants
        ));
    }
    if !report.errors.is_empty() {
        lines.push(format!("  errors:              {}", report.errors.len()));
        for err in report.errors.iter().take(10) {
            lines.push(format!("  - {err}"));
        }
    }
    lines
}

