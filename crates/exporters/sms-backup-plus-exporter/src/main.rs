use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use message_vault_io_core::{
    ContactsConfig, ContactsKind, ExporterConfig, MediaConfig, ObfuscateConfig, OutputFormat,
    SmsBackupPlusConfig, SourceConfig,
};
use media::{MaxResolution, MediaMode, compress_options_from_cli};
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

        /// Output directory for packaging + attachments/
        #[arg(long)]
        output: PathBuf,

        /// Output format: `json` (default), `jsonl`, `csv`, `eml`, `mbox`, or `xml`
        #[arg(long = "format", default_value = "json", value_name = "FORMAT")]
        format: String,

        /// Owner phone (E.164 or digits). Repeat for multiple owner numbers.
        /// Default: `phones` in config/owner.toml
        #[arg(long = "owner-phone")]
        owner_phones: Vec<String>,

        /// Owner email addresses used to detect sent messages when X-smssync-type is missing.
        /// Default: `emails` in config/owner.toml
        #[arg(long = "owner-email", value_name = "EMAIL")]
        owner_emails: Vec<String>,

        /// Contacts file for name↔phone lookup (VCF or vCard CSV; same as contacts-validate).
        /// Optional; without it (or `--vcf`) phone numbers are not resolved to names.
        #[arg(long)]
        contacts: Option<PathBuf>,

        /// Contacts VCF (alternate to `--contacts`).
        #[arg(long)]
        vcf: Option<PathBuf>,

        /// Name mapping CSV (`Phone,Incorrect Name`) for EML export aliases.
        /// Default: config/name-mapping.csv when that file exists.
        #[arg(long = "name-mapping")]
        name_mapping: Option<PathBuf>,

        /// Rewrite output with stable, non-reversible fake names/numbers/text and placeholder media
        #[arg(long)]
        obfuscate: bool,

        /// Optional 8-hex seed for reproducible obfuscation (implies --obfuscate)
        #[arg(long = "obfuscate-seed")]
        obfuscate_seed: Option<String>,

        /// Only messages on or after this date (YYYY-MM-DD, local midnight, inclusive)
        #[arg(long = "start-date", value_name = "YYYY-MM-DD")]
        start_date: Option<String>,

        /// Only messages before this date (YYYY-MM-DD, local midnight, exclusive)
        #[arg(long = "end-date", value_name = "YYYY-MM-DD")]
        end_date: Option<String>,

        /// Attachment media: disabled (no files), clone (default), convert, or compress
        #[arg(long = "media-mode", default_value = "clone", value_name = "MODE")]
        media_mode: MediaMode,

        /// Compress only: max long edge (720p, 1080p, 4k)
        #[arg(
            long = "media-max-resolution",
            default_value = "1080p",
            value_name = "RES"
        )]
        media_max_resolution: MaxResolution,

        /// Compress only: max frame rate
        #[arg(long = "media-max-fps", default_value_t = 30.0)]
        media_max_fps: f32,

        /// Compress only: only re-encode videos at/above this size (e.g. 20M)
        #[arg(long = "media-min-size", default_value = "20M")]
        media_min_size: String,

        /// Compress only: skip already-efficient HEVC under max resolution (default on)
        #[arg(long = "media-skip-efficient", default_value_t = true, action = clap::ArgAction::Set)]
        media_skip_efficient: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Convert {
            input,
            output,
            format,
            owner_phones,
            owner_emails,
            contacts,
            vcf,
            name_mapping,
            obfuscate,
            obfuscate_seed,
            start_date,
            end_date,
            media_mode,
            media_max_resolution,
            media_max_fps,
            media_min_size,
            media_skip_efficient,
        } => {
            let date_range = parse_date_range(start_date.as_deref(), end_date.as_deref())
                .map_err(anyhow::Error::msg)?;
            let output_format = OutputFormat::parse(&format).map_err(anyhow::Error::msg)?;
            let compress = compress_options_from_cli(
                media_max_resolution,
                media_max_fps,
                &media_min_size,
                media_skip_efficient,
            )?;
            let contacts = match (contacts, vcf) {
                (Some(path), _) => Some(ContactsConfig {
                    path,
                    kind: ContactsKind::Csv,
                }),
                (None, Some(path)) => Some(ContactsConfig {
                    path,
                    kind: ContactsKind::Vcf,
                }),
                (None, None) => None,
            };
            let result = run(&ExporterConfig {
                inputs: input,
                output,
                date_range,
                contacts,
                obfuscate: ObfuscateConfig {
                    enabled: obfuscate,
                    seed: obfuscate_seed,
                },
                media: MediaConfig {
                    mode: media_mode,
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

            for line in &result.messages {
                if line.starts_with("Media:")
                    || line.starts_with("  media ")
                    || line.starts_with("Obfuscated ")
                {
                    eprintln!("{line}");
                } else {
                    println!("{line}");
                }
            }
        }
    }
    Ok(())
}
