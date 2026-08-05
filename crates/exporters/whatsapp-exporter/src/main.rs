use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use message_vault_io_core::{
    CommonCli, ExporterConfig, MediaConfig, OutputFormat, SourceConfig, WhatsappConfig,
    WhatsappPlatform,
};
use media::compress_options_from_cli;
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

    #[command(flatten)]
    common: CommonCli,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let common = &cli.common;
    let date_range = parse_date_range(
        common.start_date.as_deref(),
        common.end_date.as_deref(),
    )
    .map_err(anyhow::Error::msg)?;
    let output_format = OutputFormat::parse(&common.format).map_err(anyhow::Error::msg)?;
    let compress = compress_options_from_cli(
        common.media_max_resolution,
        common.media_max_fps,
        &common.media_min_size,
        common.media_skip_efficient,
    )?;
    let result = run(&ExporterConfig {
        inputs: cli.input.into_iter().collect(),
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

    message_vault_io_core::print_result(&result);
    Ok(())
}
