//! Full export pipeline for CLI and in-process GUI.

use crate::emit::{ConvertExportArgs, convert_export};
use anyhow::{Result, bail};
use contacts::ContactsBook;
use message_vault_io_core::{ExporterConfig, RunResult, SourceConfig};

/// Load contacts, convert, then apply media transforms and obfuscation.
///
/// # Errors
///
/// Returns an error when the source is not iMazing, conversion fails, media
/// processing fails for every candidate file, or the user cancels.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let SourceConfig::Imazing(_) = &config.source else {
        bail!("imazing-exporter requires SourceConfig::Imazing");
    };
    message_vault_io_core::check_cancel(config.cancel.as_ref())?;
    let input = config.require_input().map_err(anyhow::Error::msg)?;
    let mut messages = Vec::new();
    let result = message_ir_format::run_pipeline_with_contacts(
        config,
        |config, _log_fn| {
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
            Ok(book)
        },
        |book, transforms| {
            convert_export(ConvertExportArgs {
                input,
                output: &config.output,
                book,
                timezone: config.timezone.as_deref(),
                date_range: &config.date_range,
                transforms,
                output_format: config.output_format,
                cancel: config.cancel.as_ref(),
                resume: config.resume,
            })
        },
    )?;
    messages.extend(result.messages);
    Ok(RunResult { messages })
}
