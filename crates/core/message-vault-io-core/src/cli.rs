//! Shared CLI argument groups for exporter binaries.
//!
//! Every exporter command-line tool flattens [`CommonCli`] plus its own
//! source-specific args.

use std::path::PathBuf;

use media::{CompressOptions, MaxResolution, MediaMode, compress_options_from_cli};
use message_csv::DateRange;

use crate::pipeline::{RunResult, print_result};
use crate::{
    ContactsConfig, ContactsKind, ExporterConfig, MediaConfig, ObfuscateConfig, OutputFormat,
    SourceConfig, contacts_kind_from_path,
};

/// CLI arguments common to (nearly) every exporter.
///
/// Use with `#[command(flatten)]` in the exporter's `Cli` struct.
#[derive(Debug, Clone, clap::Args)]
pub struct CommonCli {
    /// Output directory for packaging + `attachments/`
    #[arg(long)]
    pub output: PathBuf,

    /// Output format: `json` (default), `jsonl`, `csv`, `eml`, `mbox`, or `xml`
    #[arg(long = "format", default_value = "json", value_name = "FORMAT")]
    pub format: String,

    /// vCard CSV or VCF contacts file for phone→name resolution.
    /// Optional; without it phone numbers are not resolved to names.
    #[arg(long, conflicts_with = "vcf")]
    pub contacts: Option<PathBuf>,

    /// Contacts VCF (alternate to `--contacts`).
    #[arg(long)]
    pub vcf: Option<PathBuf>,

    /// Rewrite output with stable, non-reversible fake names/numbers/text and placeholder media
    #[arg(long)]
    pub obfuscate: bool,

    /// Optional seed for reproducible obfuscation (implies --obfuscate)
    #[arg(long = "obfuscate-seed")]
    pub obfuscate_seed: Option<String>,

    /// Only messages on or after this date (YYYY-MM-DD, local midnight, inclusive)
    #[arg(long = "start-date", value_name = "YYYY-MM-DD")]
    pub start_date: Option<String>,

    /// Only messages before this date (YYYY-MM-DD, local midnight, exclusive)
    #[arg(long = "end-date", value_name = "YYYY-MM-DD")]
    pub end_date: Option<String>,

    /// Attachment media: disabled (no files), clone (default), convert, or compress
    #[arg(long = "media-mode", default_value = "clone", value_name = "MODE")]
    pub media_mode: MediaMode,

    /// Compress only: max long edge (720p, 1080p, 4k)
    #[arg(
        long = "media-max-resolution",
        default_value = "1080p",
        value_name = "RES"
    )]
    pub media_max_resolution: MaxResolution,

    /// Compress only: max frame rate
    #[arg(long = "media-max-fps", default_value_t = 30.0)]
    pub media_max_fps: f32,

    /// Compress only: only re-encode videos at/above this size (e.g. 20M)
    #[arg(long = "media-min-size", default_value = "20M")]
    pub media_min_size: String,

    /// Compress only: skip already-efficient HEVC under max resolution (default on)
    #[arg(long = "media-skip-efficient", default_value_t = true, action = clap::ArgAction::Set)]
    pub media_skip_efficient: bool,
}

impl CommonCli {
    /// Build the `ContactsConfig` from `--contacts` / `--vcf`.
    ///
    /// When `--contacts` has a known extension (`.vcf`), the kind is inferred;
    /// otherwise it is treated as CSV. This matches iMazing's historical
    /// behavior and works for all exporters.
    pub fn contacts_config(&self) -> Option<ContactsConfig> {
        match (&self.contacts, &self.vcf) {
            (Some(path), _) => {
                let kind = contacts_kind_from_path(&path.to_string_lossy());
                Some(ContactsConfig {
                    path: path.clone(),
                    kind,
                })
            }
            (None, Some(path)) => Some(ContactsConfig {
                path: path.clone(),
                kind: ContactsKind::Vcf,
            }),
            (None, None) => None,
        }
    }

    /// Build the `ObfuscateConfig` from `--obfuscate` / `--obfuscate-seed`.
    pub fn obfuscate_config(&self) -> ObfuscateConfig {
        ObfuscateConfig {
            enabled: self.obfuscate,
            seed: self.obfuscate_seed.clone(),
        }
    }

    /// Build the `ExporterConfig` an exporter binary hands to its run
    /// function: the shared fields come from these common flags
    /// (`timezone: None`, `cancel: None`, `log: None`, `resume: false` —
    /// the CLI has no flags for those), the caller supplies what differs
    /// per exporter.
    ///
    /// `date_range`, `output_format`, and `compress` are the values
    /// [`run_cli`] parsed from the flags. A binary that deviates on a
    /// shared field (iMazing's `--timezone`) overrides it on the returned
    /// value.
    pub fn exporter_config(
        &self,
        date_range: DateRange,
        output_format: OutputFormat,
        compress: CompressOptions,
        inputs: Vec<PathBuf>,
        source: SourceConfig,
    ) -> ExporterConfig {
        ExporterConfig {
            inputs,
            output: self.output.clone(),
            date_range,
            timezone: None,
            contacts: self.contacts_config(),
            obfuscate: self.obfuscate_config(),
            media: MediaConfig {
                mode: self.media_mode,
                compress,
            },
            cancel: None,
            log: None,
            output_format,
            resume: false,
            source,
        }
    }
}

/// The clap `Command` for an exporter binary (for embedding `--help` output
/// into GUI docs).
pub fn clap_command<C: clap::CommandFactory>() -> clap::Command {
    C::command()
}

/// The shared exporter main: parse the common CLI flags, build the source
/// config, run, and print the result with the standard stdout/stderr split.
///
/// `parse_dates` supplies the exporter's date parsing (local or
/// timezone-aware); `build` builds the exporter's `ExporterConfig` from the
/// parsed common values; `run` is the exporter's run function.
///
/// # Errors
///
/// Returns an error when a flag value cannot be parsed or the run fails.
pub fn run_cli(
    common: &CommonCli,
    parse_dates: impl FnOnce(&CommonCli) -> Result<DateRange, String>,
    build: impl FnOnce(DateRange, OutputFormat, CompressOptions) -> ExporterConfig,
    run: impl FnOnce(&ExporterConfig) -> anyhow::Result<RunResult>,
) -> anyhow::Result<()> {
    let date_range = parse_dates(common).map_err(anyhow::Error::msg)?;
    let output_format = OutputFormat::parse(&common.format).map_err(anyhow::Error::msg)?;
    let compress = compress_options_from_cli(
        common.media_max_resolution,
        common.media_max_fps,
        &common.media_min_size,
        common.media_skip_efficient,
    )?;
    let result = run(&build(date_range, output_format, compress))?;
    print_result(&result);
    Ok(())
}

/// Declare the standard test that a crate's `clap_command()` reports its
/// binary name.
///
/// Usage: `message_vault_io_core::clap_command_uses_binary_name_test!("go-sms-pro-exporter");`
#[cfg(feature = "cli")]
// `crate` here is deliberate: it resolves to the exporter crate that invokes
// this macro, whose own `cli::clap_command()` is what the test asserts on.
#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! clap_command_uses_binary_name_test {
    ($bin:literal) => {
        #[cfg(test)]
        mod clap_command_tests {
            #[test]
            fn clap_command_uses_binary_name() {
                let cmd = crate::cli::clap_command();
                assert_eq!(cmd.get_name(), $bin);
            }
        }
    };
}
