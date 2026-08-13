//! Convert an existing Message Vault export directory into another format.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use media::{MaxResolution, MediaMode, compress_options_from_cli};
use message_reexport::run;
use message_vault_io_core::{
    ExporterConfig, FormatConfig, MediaConfig, ObfuscateConfig, OutputFormat, SourceConfig,
};

#[derive(Parser, Debug)]
#[command(name = "message-reexporter")]
#[command(about = "Convert an existing Message Vault output to another format")]
struct Cli {
    /// Directory containing a prior Message Vault output (auto-detected)
    #[arg(long)]
    input: PathBuf,

    /// Output directory for the converted export
    #[arg(long)]
    output: PathBuf,

    /// Output format: `json` (default), `jsonl`, `csv`, `eml`, `mbox`, or `xml`
    #[arg(long = "format", default_value = "json", value_name = "FORMAT")]
    format: String,

    /// Rewrite output with stable fake names/numbers/text and placeholder media
    #[arg(long)]
    obfuscate: bool,

    /// Optional 8-hex seed for reproducible obfuscation (implies --obfuscate)
    #[arg(long = "obfuscate-seed")]
    obfuscate_seed: Option<String>,

    /// Attachment media: disabled, clone (default), convert, or compress
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
    let output_format = OutputFormat::parse(&cli.format).map_err(anyhow::Error::msg)?;
    let compress = compress_options_from_cli(
        cli.media_max_resolution,
        cli.media_max_fps,
        &cli.media_min_size,
        cli.media_skip_efficient,
    )?;
    let config = ExporterConfig {
        inputs: vec![cli.input],
        output: cli.output,
        date_range: Default::default(),
        timezone: None,
        contacts: None,
        obfuscate: ObfuscateConfig {
            enabled: cli.obfuscate || cli.obfuscate_seed.is_some(),
            seed: cli.obfuscate_seed,
        },
        media: MediaConfig {
            mode: cli.media_mode,
            compress,
        },
        cancel: None,
        log: None,
        output_format,
        source: SourceConfig::Format(FormatConfig {}),
    };
    for line in run(&config)?.messages {
        println!("{line}");
    }
    Ok(())
}
