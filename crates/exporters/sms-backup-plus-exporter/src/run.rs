//! Full export pipeline for CLI and in-process GUI.

use crate::emit::convert_export;
use anyhow::{Context, Result, bail};
use contacts::{NameMapping, resolve_contacts_cli};
use message_ir_format::ExportTransforms;
use message_vault_io_core::{ExporterConfig, RunResult, SourceConfig};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerConfig {
    #[serde(default)]
    phones: Vec<String>,
    #[serde(default)]
    emails: Vec<String>,
    #[serde(default)]
    source_dirs: Vec<PathBuf>,
}

fn crate_config(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("config")
        .join(name)
}

fn resolve_optional_config(explicit: Option<PathBuf>, default_name: &str) -> Option<PathBuf> {
    match explicit {
        Some(path) => Some(path),
        None => {
            let path = crate_config(default_name);
            path.is_file().then_some(path)
        }
    }
}

fn find_owner_config_path() -> Option<PathBuf> {
    let path = crate_config("owner.toml");
    path.is_file().then_some(path)
}

fn load_owner_config() -> Result<OwnerConfig> {
    let Some(path) = find_owner_config_path() else {
        return Ok(OwnerConfig::default());
    };
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read owner config {}", path.display()))?;
    toml::from_str(&text)
        .with_context(|| format!("failed to parse owner config {}", path.display()))
}

/// Resolve owner phones/emails from CLI values or `config/owner.toml`.
fn resolve_owner(
    cli_phones: Vec<String>,
    cli_emails: Vec<String>,
) -> Result<(Vec<String>, Vec<String>, Vec<PathBuf>)> {
    let defaults = load_owner_config()?;
    let phones = if !cli_phones.is_empty() {
        cli_phones
    } else if !defaults.phones.is_empty() {
        defaults.phones
    } else {
        anyhow::bail!(
            "owner phone required: pass --owner-phone or set phones in config/owner.toml"
        );
    };
    let emails = if !cli_emails.is_empty() {
        cli_emails
    } else if !defaults.emails.is_empty() {
        defaults.emails
    } else {
        anyhow::bail!(
            "owner email required: pass --owner-email or set emails in config/owner.toml"
        );
    };
    Ok((phones, emails, defaults.source_dirs))
}

/// Resolve input roots from CLI values or `source_dirs` in owner.toml.
fn resolve_inputs(cli_inputs: Vec<PathBuf>, defaults: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let inputs = if !cli_inputs.is_empty() {
        cli_inputs
    } else {
        defaults
    };
    if inputs.is_empty() {
        anyhow::bail!(
            "no --input given and config/owner.toml has no source_dirs; \
             pass --input PATH or set source_dirs in owner.toml"
        );
    }
    Ok(inputs)
}

/// Resolve owner/contacts/name-mapping, convert, apply media/obfuscate via FormatSink.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let SourceConfig::SmsBackupPlus(source) = &config.source else {
        bail!("sms-backup-plus-exporter requires SourceConfig::SmsBackupPlus");
    };
    message_vault_io_core::check_cancel(config.cancel.as_ref()).map_err(anyhow::Error::msg)?;
    let mut messages = Vec::new();

    let (owner_phones, owner_emails, default_inputs) =
        resolve_owner(source.owner_phones.clone(), source.owner_emails.clone())?;
    let inputs = resolve_inputs(config.inputs.clone(), default_inputs)?;

    let (contacts_path, vcf) = config.contacts_csv_vcf();
    let log_fn = |line: &str| config.emit_log(line);
    let (contacts_book, contacts_resolved) =
        resolve_contacts_cli(contacts_path, vcf, Some(&log_fn))?;
    let name_mapping_path =
        resolve_optional_config(source.name_mapping.clone(), "name-mapping.csv");
    let (name_mapping, _) = NameMapping::load_optional(name_mapping_path.as_deref())?;

    if source.verbose {
        match contacts_resolved.as_ref() {
            Some(path) => config.emit_log(format!("contacts: {}", path.display())),
            None => config.emit_log("contacts: (none)"),
        }
    }

    let mut transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
    transforms.log = config.log.clone();
    let (report, sink) = convert_export(
        &inputs,
        &config.output,
        &owner_phones,
        &owner_emails,
        &contacts_book,
        &name_mapping,
        &config.date_range,
        source.verbose,
        transforms,
        config.output_format,
        config.cancel.as_ref(),
        config.log.as_ref(),
    )?;
    if !sink.media.errors.is_empty() && sink.media.processed == 0 && config.media.mode.needs_tools()
    {
        anyhow::bail!("media processing failed for all candidate files");
    }
    messages.extend(sink.log_lines());

    if source.include_summary {
        report.summary_lines(&config.output, &mut messages);
    }
    Ok(RunResult { messages })
}
