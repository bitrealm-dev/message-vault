use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use message_vault_io_core::{
    CommonCli, ExporterConfig, MediaConfig, OutputFormat, SmsBackupPlusConfig, SourceConfig,
};
use media::compress_options_from_cli;
use sms_backup_plus_exporter::{parse_date_range, run};

#[derive(Parser, Debug)]
#[command(name = "sms-backup-plus-exporter")]
#[command(
    about = "Convert SMS Backup+ EML exports via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
struct Cli {
    /// Log progress to stderr (inputs, scan/write progress, dedupe summary)
    #[arg(short = 'v', long, global = true)]
    verbose: bool,

    /// Skip the end-of-run summary on stdout
    #[arg(long, global = true)]
    no_summary: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Convert EML tree via common message to the chosen packaging format
    Convert {
        /// Path to a .eml file or directory tree of EMLs (Archive/, Sent/, …).
        /// Repeat for multiple roots; trees are merged and path-deduped.
        /// Default: source_dirs from config/owner.toml when set.
        #[arg(long = "input")]
        input: Vec<PathBuf>,

        /// Owner phone (E.164 or digits). Repeat for multiple owner numbers.
        /// Default: `phones` in config/owner.toml
        #[arg(long = "owner-phone")]
        owner_phones: Vec<String>,

        /// Owner email addresses used to detect sent messages when X-smssync-type is missing.
        /// Default: `emails` in config/owner.toml
        #[arg(long = "owner-email", value_name = "EMAIL")]
        owner_emails: Vec<String>,

        /// Name mapping CSV (`Phone,Incorrect Name`) for EML export aliases.
        /// Default: config/name-mapping.csv when that file exists.
        #[arg(long = "name-mapping")]
        name_mapping: Option<PathBuf>,

        #[command(flatten)]
        common: CommonCli,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Convert {
            input,
            owner_phones,
            owner_emails,
            name_mapping,
            common,
        } => {
            let common = &common;
            let date_range = parse_date_range(common.start_date.as_deref(), common.end_date.as_deref())
                .map_err(anyhow::Error::msg)?;
            let output_format = OutputFormat::parse(&common.format).map_err(anyhow::Error::msg)?;
            let compress = compress_options_from_cli(
                common.media_max_resolution,
                common.media_max_fps,
                &common.media_min_size,
                common.media_skip_efficient,
            )?;
            let result = run(&ExporterConfig {
                inputs: input,
                output: common.output.clone(),
                date_range,
                timezone: None,
                contacts: common.contacts_config(),
                obfuscate: common.obfuscate_config(),
                media: MediaConfig {
                    mode: common.media_mode,
                    compress,
                },
                cancel: None,
                log: None,
                output_format,
                source: SourceConfig::SmsBackupPlus(SmsBackupPlusConfig {
                    owner_phones,
                    owner_emails,
                    name_mapping,
                    verbose: cli.verbose,
                    include_summary: !cli.no_summary,
                }),
            })?;

            message_vault_io_core::print_result(&result);
        }
    }
    Ok(())
}
