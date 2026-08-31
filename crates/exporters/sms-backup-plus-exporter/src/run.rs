//! Full export pipeline for CLI and in-process GUI.

use crate::emit::{ConvertExportArgs, convert_export};
use anyhow::{Result, bail};
use contacts::{NameMapping, resolve_contacts_cli};
use message_ir_format::ExportTransforms;
use message_vault_io_core::{ExporterConfig, RunResult, SourceConfig};

/// Check the required inputs, resolve contacts and name mapping, then convert.
///
/// The shared `run_pipeline` cannot host this exporter's `--no-summary` flag
/// (it appends the summary lines unconditionally), so the SMS Backup+ specifics
/// stay here and only the shared `finish_run` tail is reused.
///
/// # Errors
///
/// Returns an error when the source is not SMS Backup+, an input, owner phone,
/// or owner email is missing, conversion fails, media processing fails for
/// every candidate file, or the user cancels.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let SourceConfig::SmsBackupPlus(source) = &config.source else {
        bail!("sms-backup-plus-exporter requires SourceConfig::SmsBackupPlus");
    };
    message_vault_io_core::check_cancel(config.cancel.as_ref())?;

    if source.owner_phones.is_empty() {
        bail!("owner phone required: pass --owner-phone");
    }
    if source.owner_emails.is_empty() {
        bail!("owner email required: pass --owner-email");
    }
    if config.inputs.is_empty() {
        bail!("no input given: pass --input PATH");
    }

    let (contacts_path, vcf) = config.contacts_csv_vcf();
    let log_fn = |line: &str| config.emit_log(line);
    let (contacts_book, contacts_resolved) =
        resolve_contacts_cli(contacts_path, vcf, Some(&log_fn))?;
    let (name_mapping, _) = NameMapping::load_optional(source.name_mapping.as_deref())?;

    if source.verbose {
        match contacts_resolved.as_ref() {
            Some(path) => config.emit_log(format!("contacts: {}", path.display())),
            None => config.emit_log("contacts: (none)"),
        }
    }

    let mut transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
    transforms.log = config.log.clone();
    let (report, sink) = convert_export(ConvertExportArgs {
        inputs: &config.inputs,
        output_dir: &config.output,
        owner_phones: &source.owner_phones,
        owner_emails: &source.owner_emails,
        contacts: &contacts_book,
        name_mapping: &name_mapping,
        date_range: &config.date_range,
        verbose: source.verbose,
        transforms,
        output_format: config.output_format,
        cancel: config.cancel.as_ref(),
        log: config.log.as_ref(),
        resume: config.resume,
    })?;
    if source.include_summary {
        return message_ir_format::finish_run(
            config,
            &report,
            &sink,
            config.media.mode.needs_tools(),
        );
    }
    // --no-summary: the shared tail appends the summary unconditionally, so
    // repeat its media-failure bail and keep only the sink log lines.
    if !sink.media.errors.is_empty() && sink.media.processed == 0 && config.media.mode.needs_tools()
    {
        anyhow::bail!("media processing failed for all candidate files");
    }
    Ok(RunResult {
        messages: sink.log_lines(),
    })
}
