//! The shared exporter run skeleton and tail.

use crate::{ExportTransforms, FormatSinkResult};
use contacts::ContactsBook;
use message_vault_io_core::{ExportReport, ExporterConfig, RunResult, check_cancel};

/// The shared exporter run skeleton: cancel check, contacts resolution,
/// transforms, conversion, media-failure bail, and result assembly.
///
/// `load_contacts` resolves the contacts book (exporters with custom
/// loading pass their own closure); `convert` runs the source-specific
/// conversion and returns the report and finished sink.
///
/// # Errors
///
/// Returns an error when the user cancels, contacts cannot be loaded,
/// conversion fails, or media processing fails for every candidate file.
pub fn run_pipeline(
    config: &ExporterConfig,
    load_contacts: impl FnOnce(&ExporterConfig, &dyn Fn(&str)) -> anyhow::Result<ContactsBook>,
    convert: impl FnOnce(
        &ContactsBook,
        ExportTransforms,
    ) -> anyhow::Result<(ExportReport, FormatSinkResult)>,
) -> anyhow::Result<RunResult> {
    check_cancel(config.cancel.as_ref()).map_err(anyhow::Error::msg)?;
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
