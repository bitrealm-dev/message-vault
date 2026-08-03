use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use message_vault_io_core::{
    ContactsConfig, ContactsKind, ExporterConfig, MediaConfig, ObfuscateConfig, OutputFormat,
    SmsBackupRestoreConfig, SourceConfig,
};
use media::{MaxResolution, MediaMode, compress_options_from_cli};
use sms_backup_restore_exporter::{parse_date_range, run};

#[derive(Parser, Debug)]
#[command(name = "sms-backup-restore-exporter")]
#[command(
    about = "Convert SMS Backup & Restore XML via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
struct Cli {
    /// Path to sms-*.xml file, or a directory of .xml files
    #[arg(long)]
    input: PathBuf,

    /// Output directory for packaging + attachments/
    #[arg(long)]
    output: PathBuf,

    /// Output format: `json` (default), `jsonl`, `csv`, `eml`, `mbox`, or `xml`
    #[arg(long = "format", default_value = "json", value_name = "FORMAT")]
    format: String,

    /// Owner phone (E.164 or digits). Repeat for multiple owner numbers.
    /// Required — there is no demo default (wrong owner flips MMS chat keys).
    #[arg(long = "owner-phone", required = true)]
    owner_phones: Vec<String>,

    /// Contacts file for phone→name fill (VCF or vCard CSV; same as contacts-validate).
    /// Optional; without it (or `--vcf`) phone numbers are not resolved to names.
    #[arg(long)]
    contacts: Option<PathBuf>,

    /// Contacts VCF (alternate to `--contacts`).
    #[arg(long)]
    vcf: Option<PathBuf>,

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let date_range = parse_date_range(cli.start_date.as_deref(), cli.end_date.as_deref())
        .map_err(anyhow::Error::msg)?;
    let output_format = OutputFormat::parse(&cli.format).map_err(anyhow::Error::msg)?;
    let compress = compress_options_from_cli(
        cli.media_max_resolution,
        cli.media_max_fps,
        &cli.media_min_size,
        cli.media_skip_efficient,
    )?;
    let contacts = match (cli.contacts, cli.vcf) {
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
        inputs: vec![cli.input],
        output: cli.output,
        date_range,
        contacts,
        obfuscate: ObfuscateConfig {
            enabled: cli.obfuscate,
            seed: cli.obfuscate_seed,
        },
        media: MediaConfig {
            mode: cli.media_mode,
            compress,
        },
        cancel: None,
        log: None,
        output_format,
        source: SourceConfig::SmsBackupRestore(SmsBackupRestoreConfig {
            owner_phones: cli.owner_phones,
        }),
    })?;

    for line in &result.messages {
        // Media / obfuscate notes historically went to stderr; summary to stdout.
        if line.starts_with("Media:")
            || line.starts_with("  media ")
            || line.starts_with("Obfuscated ")
        {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
    Ok(())
}
