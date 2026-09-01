//! The shared exporter run skeleton and tail.

use crate::{ExportTransforms, FormatSinkResult};
use message_vault_io_core::{ExportReport, ExporterConfig, RunResult, check_cancel};

/// The shared exporter run skeleton: cancel check, transforms, conversion,
/// media-failure bail, and result assembly.
///
/// Exporters no longer resolve names from a contacts file. A backup that
/// carries its own contact data (Apple's address book, WhatsApp's contacts
/// database) is read by that exporter directly; everything else arrives at the
/// vault as raw identities and is reconciled there.
///
/// # Errors
///
/// Returns an error when the user cancels, conversion fails, or media
/// processing fails for every candidate file.
pub fn run_pipeline(
    config: &ExporterConfig,
    convert: impl FnOnce(ExportTransforms) -> anyhow::Result<(ExportReport, FormatSinkResult)>,
) -> anyhow::Result<RunResult> {
    check_cancel(config.cancel.as_ref())?;
    let mut messages = Vec::new();
    let mut transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
    transforms.log = config.log.clone();
    let (report, sink) = convert(transforms)?;
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
