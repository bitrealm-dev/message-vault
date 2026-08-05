use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use imazing_exporter::{parse_date_range, run};
use message_vault_io_core::{
    CommonCli, ExporterConfig, ImazingConfig, MediaConfig, OutputFormat, SourceConfig,
};
use media::compress_options_from_cli;

#[derive(Parser, Debug)]
#[command(name = "imazing-exporter")]
#[command(
    about = "Convert iMazing Messages / WhatsApp CSV exports via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
struct Cli {
    /// Messages/WhatsApp export directory (or a single CSV for CLI convenience)
    #[arg(long)]
    input: PathBuf,

    /// UTC offset for naive Message Date values (e.g. UTC-05:00). Default: host local.
    #[arg(long)]
    timezone: Option<String>,

    #[command(flatten)]
    common: CommonCli,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let common = &cli.common;
    let date_range = parse_date_range(
        common.start_date.as_deref(),
        common.end_date.as_deref(),
        cli.timezone.as_deref(),
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
        inputs: vec![cli.input],
        output: common.output.clone(),
        date_range,
        timezone: cli.timezone.clone(),
        contacts: common.contacts_config(),
        obfuscate: common.obfuscate_config(),
        media: MediaConfig {
            mode: common.media_mode,
            compress,
        },
        cancel: None,
        log: None,
        output_format,
        source: SourceConfig::Imazing(ImazingConfig {}),
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
