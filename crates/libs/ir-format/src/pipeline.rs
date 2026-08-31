//! The shared exporter run skeleton and tail.

use crate::{ExportTransforms, FormatSinkResult};
use contacts::{ContactsBook, resolve_contacts_cli};
use message_vault_io_core::{ExportReport, ExporterConfig, RunResult, check_cancel};

/// [`run_pipeline_with_contacts`] with the default contacts step: resolve
/// the config's `--contacts` / `--vcf` through [`resolve_contacts_cli`]
/// (which warns when neither is set).
///
/// # Errors
///
/// Returns an error when the user cancels, contacts cannot be loaded,
/// conversion fails, or media processing fails for every candidate file.
pub fn run_pipeline(
    config: &ExporterConfig,
    convert: impl FnOnce(
        &ContactsBook,
        ExportTransforms,
    ) -> anyhow::Result<(ExportReport, FormatSinkResult)>,
) -> anyhow::Result<RunResult> {
    run_pipeline_with_contacts(
        config,
        |config, log_fn| {
            let (contacts_path, vcf) = config.contacts_csv_vcf();
            resolve_contacts_cli(contacts_path, vcf, Some(log_fn)).map(|(book, _)| book)
        },
        convert,
    )
}

/// The shared exporter run skeleton: cancel check, contacts resolution,
/// transforms, conversion, media-failure bail, and result assembly.
///
/// `load_contacts` resolves the contacts book (exporters with custom
/// loading — iMazing — pass their own closure; the rest use
/// [`run_pipeline`]); `convert` runs the source-specific conversion and
/// returns the report and finished sink.
///
/// # Errors
///
/// Returns an error when the user cancels, contacts cannot be loaded,
/// conversion fails, or media processing fails for every candidate file.
pub fn run_pipeline_with_contacts(
    config: &ExporterConfig,
    load_contacts: impl FnOnce(&ExporterConfig, &dyn Fn(&str)) -> anyhow::Result<ContactsBook>,
    convert: impl FnOnce(
        &ContactsBook,
        ExportTransforms,
    ) -> anyhow::Result<(ExportReport, FormatSinkResult)>,
) -> anyhow::Result<RunResult> {
    check_cancel(config.cancel.as_ref())?;
    let mut messages = Vec::new();
    let log_fn = |line: &str| config.emit_log(line);
    let contacts = load_contacts(config, &log_fn)?;
    let mut transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
    transforms.log = config.log.clone();
    let (report, sink) = convert(&contacts, transforms)?;
    if !sink.media.errors.is_empty() && sink.media.processed == 0 && config.media.mode.needs_tools()
    {
        anyhow::bail!("media processing failed for all candidate files");
    }
    messages.extend(sink.log_lines());
    report.summary_lines(&config.output, &mut messages);
    Ok(RunResult { messages })
}

/// The run tail shared by exporters whose middle diverges (WhatsApp):
/// media-failure bail plus log-line and summary assembly.
///
/// # Errors
///
/// Returns an error when media processing fails for every candidate file.
pub fn finish_run(
    config: &ExporterConfig,
    report: &ExportReport,
    sink: &FormatSinkResult,
    needs_tools: bool,
) -> anyhow::Result<RunResult> {
    if !sink.media.errors.is_empty() && sink.media.processed == 0 && needs_tools {
        anyhow::bail!("media processing failed for all candidate files");
    }
    let mut messages = sink.log_lines();
    report.summary_lines(&config.output, &mut messages);
    Ok(RunResult { messages })
}
