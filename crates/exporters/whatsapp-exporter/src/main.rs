use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use message_vault_io_core::{
    ExporterConfig, MediaConfig, ObfuscateConfig, OutputFormat, SourceConfig, WhatsappConfig,
    WhatsappPlatform,
};
use media::{MaxResolution, MediaMode, compress_options_from_cli};
use whatsapp_exporter::{parse_date_range, run};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliPlatform {
    Android,
    Ios,
}

impl From<CliPlatform> for WhatsappPlatform {
    fn from(value: CliPlatform) -> Self {
        match value {
            CliPlatform::Android => WhatsappPlatform::Android,
            CliPlatform::Ios => WhatsappPlatform::Ios,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "whatsapp-exporter")]
#[command(
    about = "Convert WhatsApp DB/backup (via wtsexporter) via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
struct Cli {
    /// Directory (or msgstore.db file) used to resolve relative defaults such as
    /// `msgstore.db` / `wa.db` / `WhatsApp/`. Defaults to the process cwd.
    /// Extraction always runs in a temporary directory (not this path).
    /// The GUI omits this flag.
    #[arg(long)]
    input: Option<PathBuf>,

    /// Output directory for packaging + attachments/
    #[arg(long)]
    output: PathBuf,

    /// Output format: `json` (default), `jsonl`, `csv`, `eml`, `mbox`, or `xml`
    #[arg(long = "format", default_value = "json", value_name = "FORMAT")]
    format: String,

    /// Android or iOS (required unless --json)
    #[arg(long, value_enum)]
    platform: Option<CliPlatform>,

    /// Skip wtsexporter; convert an existing result.json
    #[arg(long)]
    json: Option<PathBuf>,

    /// Decryption key file path or crypt15 hex key (forwarded as -k)
    #[arg(long)]
    key: Option<String>,

    /// Encrypted backup / iOS backup path (forwarded as -b)
    #[arg(long)]
    backup: Option<PathBuf>,

    /// Contacts database wa.db / ContactsV2.sqlite (forwarded as -w)
    #[arg(long)]
    wa: Option<PathBuf>,

    /// WhatsApp media folder (forwarded as -m)
    #[arg(long)]
    media: Option<PathBuf>,

    /// Explicit msgstore.db path (forwarded as -d)
    #[arg(long)]
    db: Option<PathBuf>,

    /// WhatsApp Business defaults
    #[arg(long)]
    business: bool,

    /// Rewrite output with stable fake names/numbers/text and placeholder media
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
    let result = run(&ExporterConfig {
        inputs: cli.input.into_iter().collect(),
        output: cli.output,
        date_range,
        contacts: None,
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
        source: SourceConfig::Whatsapp(WhatsappConfig {
            platform: cli.platform.map(Into::into),
            json: cli.json,
            key: cli.key,
            backup: cli.backup,
            wa: cli.wa,
            media: cli.media,
            db: cli.db,
            business: cli.business,
        }),
    })?;

    for line in &result.messages {
        // Media / obfuscate / wtsexporter log → stderr; convert summary → stdout.
        if line.starts_with("Media:")
            || line.starts_with("  media ")
            || line.starts_with("Obfuscated ")
            || !(line.starts_with("Wrote ") || line.starts_with("  "))
        {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
    Ok(())
}
