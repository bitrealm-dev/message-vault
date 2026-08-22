//! Full export pipeline for CLI and in-process GUI.

use crate::emit::{ConvertExportArgs, convert_export};
use anyhow::{Result, bail};
use contacts::resolve_contacts_cli;
use message_ir_format::ExportTransforms;
use message_vault_io_core::{ExporterConfig, RunResult, SourceConfig};

/// Resolve contacts, convert, then apply media transforms and obfuscation.
///
/// # Errors
///
/// Returns an error when the source is not GO SMS Pro, conversion fails, media
/// processing fails for every candidate file, or the user cancels.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let SourceConfig::GoSmsPro(source) = &config.source else {
        bail!("go-sms-pro-exporter requires SourceConfig::GoSmsPro");
    };
    message_vault_io_core::check_cancel(config.cancel.as_ref()).map_err(anyhow::Error::msg)?;
    let input = config.require_input().map_err(anyhow::Error::msg)?;
    let mut messages = Vec::new();
    let (contacts_path, vcf) = config.contacts_csv_vcf();
    let log_fn = |line: &str| config.emit_log(line);
    let (contacts, _) = resolve_contacts_cli(contacts_path, vcf, Some(&log_fn))?;
    let mut transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
    transforms.log = config.log.clone();
    let (report, sink) = convert_export(ConvertExportArgs {
        input_dir: input,
        output_dir: &config.output,
        owner_phones: &source.owner_phones,
        contacts: &contacts,
        date_range: &config.date_range,
        transforms,
        output_format: config.output_format,
        cancel: config.cancel.as_ref(),
    })?;
    if !sink.media.errors.is_empty() && sink.media.processed == 0 && config.media.mode.needs_tools()
    {
        anyhow::bail!("media processing failed for all candidate files");
    }
    messages.extend(sink.log_lines());
    report.summary_lines(&config.output, &mut messages);
    Ok(RunResult { messages })
}
