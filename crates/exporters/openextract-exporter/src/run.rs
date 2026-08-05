//! Full export pipeline for CLI and in-process GUI.

use crate::emit::{ExportReport, convert_export};
use anyhow::{Result, bail};
use contacts::resolve_contacts_cli;
use message_vault_io_core::{RunResult, ExporterConfig, SourceConfig};
use message_ir_format::ExportTransforms;
use std::path::Path;

/// Resolve contacts, convert, apply media/obfuscate via FormatSink.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let SourceConfig::OpenExtract(_) = &config.source else {
        bail!("openextract-exporter requires SourceConfig::OpenExtract");
    };
    message_vault_io_core::check_cancel(config.cancel.as_ref()).map_err(anyhow::Error::msg)?;
    let input = config.require_input().map_err(anyhow::Error::msg)?;
    let mut messages = Vec::new();
    let (contacts_path, vcf) = config.contacts_csv_vcf();
    let log_fn = |line: &str| config.emit_log(line);
    let (book, book_path) = resolve_contacts_cli(contacts_path, vcf, Some(&log_fn))?;
    let mut transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
    transforms.log = config.log.clone();
    let (report, sink) = convert_export(
        input,
        &config.output,
        &book,
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
        book_path.as_deref(),
    ));
    Ok(RunResult { messages })
}

/// Format the convert summary the same way the CLI prints it.
fn report_summary_lines(
    report: &ExportReport,
    output: &Path,
    contacts_path: Option<&Path>,
) -> Vec<String> {
    let mut lines = vec![
        format!("Wrote {}", output.display()),
        match contacts_path {
            Some(path) => format!("  contacts from:       {}", path.display()),
            None => "  contacts from:       (none)".to_string(),
        },
        format!("  conversations:       {}", report.conversations),
        format!("  messages:            {}", report.messages),
        format!(
            "  sent / received:     {} / {}",
            report.sent, report.received
        ),
    ];
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
    if !report.errors.is_empty() {
        lines.push(format!("  errors:              {}", report.errors.len()));
        for err in report.errors.iter().take(10) {
            lines.push(format!("    {err}"));
        }
    }
    lines
}

