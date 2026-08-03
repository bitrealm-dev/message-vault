use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use imazing_exporter::{parse_date_range, run};
use message_vault_io_core::{
    ContactsConfig, ExporterConfig, ImazingConfig, MediaConfig, ObfuscateConfig, OutputFormat,
    SourceConfig, contacts_kind_from_path,
};
use media::{MaxResolution, MediaMode, compress_options_from_cli};

#[derive(Parser, Debug)]
#[command(name = "imazing-exporter")]
#[command(
    about = "Convert iMazing Messages / WhatsApp CSV exports via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
struct Cli {
    /// Messages/WhatsApp export directory (or a single CSV for CLI convenience)
    #[arg(long)]
    input: PathBuf,

    /// Output directory for packaging + attachments/
    #[arg(long)]
    output: PathBuf,

    /// Output format: `json` (default), `jsonl`, `csv`, `eml`, `mbox`, or `xml`
    #[arg(long = "format", default_value = "json", value_name = "FORMAT")]
    format: String,

    /// vCard CSV or VCF used to resolve phone numbers to names.
    /// Optional; without it phone numbers are not resolved to names.
    #[arg(long)]
    contacts: Option<PathBuf>,

    /// UTC offset for naive Message Date values (e.g. UTC-05:00). Default: host local.
    #[arg(long)]
    timezone: Option<String>,

    /// Only messages on or after this date (YYYY-MM-DD, timezone midnight, inclusive)
    #[arg(long = "start-date", value_name = "YYYY-MM-DD")]
    start_date: Option<String>,

    /// Only messages before this date (YYYY-MM-DD, timezone midnight, exclusive)
    #[arg(long = "end-date", value_name = "YYYY-MM-DD")]
    end_date: Option<String>,

    /// Rewrite output with stable, non-reversible fake names/numbers/text and placeholder media
    #[arg(long)]
    obfuscate: bool,

    /// Optional 8-hex seed for reproducible obfuscation (implies --obfuscate)
    #[arg(long = "obfuscate-seed")]
    obfuscate_seed: Option<String>,

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
    let date_range = parse_date_range(
        cli.start_date.as_deref(),
        cli.end_date.as_deref(),
        cli.timezone.as_deref(),
    )
    .map_err(anyhow::Error::msg)?;
    let output_format = OutputFormat::parse(&cli.format).map_err(anyhow::Error::msg)?;
    let compress = compress_options_from_cli(
        cli.media_max_resolution,
        cli.media_max_fps,
        &cli.media_min_size,
        cli.media_skip_efficient,
    )?;
    let contacts = cli.contacts.map(|path| {
        let kind = contacts_kind_from_path(&path.to_string_lossy());
        ContactsConfig { path, kind }
    });
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
        source: SourceConfig::Imazing(ImazingConfig {
            timezone: cli.timezone,
        }),
    })?;

    for line in &result.messages {
        if line.starts_with("Media:")
            || line.starts_with("  media ")
            || line.starts_with("Obfuscated ")
            || line.starts_with("warning:")
            || line.starts_with("  - ")
        {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
    Ok(())
}
