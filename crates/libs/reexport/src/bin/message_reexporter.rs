//! Convert an existing Message Vault export directory into another format.

use anyhow::Result;
use clap::Parser;
use media::compress_options_from_cli;
use message_reexport::cli::Cli;
use message_reexport::run;
use message_vault_io_core::{
    ExporterConfig, FormatConfig, MediaConfig, ObfuscateConfig, OutputFormat, SourceConfig,
};

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
        resume: false,
        source: SourceConfig::Format(FormatConfig {}),
    };
    for line in run(&config)?.messages {
        println!("{line}");
    }
    Ok(())
}
