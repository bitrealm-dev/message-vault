//! Full export pipeline for CLI and in-process GUI.

use crate::emit::{ConvertExportArgs, convert_export};
use anyhow::{Result, bail};
use message_vault_io_core::{ExporterConfig, RunResult, SourceConfig};

/// Resolve contacts, convert, then apply media transforms and obfuscation.
///
/// # Errors
///
/// Returns an error when the source is not OpenExtract, conversion fails, media
/// processing fails for every candidate file, or the user cancels.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let SourceConfig::OpenExtract(_) = &config.source else {
        bail!("openextract-exporter requires SourceConfig::OpenExtract");
    };
    message_vault_io_core::check_cancel(config.cancel.as_ref())?;
    let input = config.require_input().map_err(anyhow::Error::msg)?;
    message_ir_format::run_pipeline(config, |book, transforms| {
        convert_export(ConvertExportArgs {
            input,
            output: &config.output,
            book,
            date_range: &config.date_range,
            transforms,
            output_format: config.output_format,
            cancel: config.cancel.as_ref(),
            resume: config.resume,
        })
    })
}
