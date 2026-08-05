//! Full export pipeline for CLI and in-process GUI.

use crate::emit::convert_export;
use anyhow::{Result, bail};
use contacts::ContactsBook;
use message_vault_io_core::{RunResult, ExporterConfig, SourceConfig};
use message_ir_format::ExportTransforms;

/// Load contacts, convert, apply media/obfuscate via FormatSink.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let SourceConfig::Imazing(_) = &config.source else {
        bail!("imazing-exporter requires SourceConfig::Imazing");
    };
    message_vault_io_core::check_cancel(config.cancel.as_ref()).map_err(anyhow::Error::msg)?;
    let input = config.require_input().map_err(anyhow::Error::msg)?;
    let mut messages = Vec::new();
    let (contacts_csv, contacts_vcf) = config.contacts_csv_vcf();
    let (book, _contacts_path) = match (contacts_csv, contacts_vcf) {
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
        config.timezone.as_deref(),
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

    report.summary_lines(&config.output, &mut messages);
    Ok(RunResult { messages })
}

