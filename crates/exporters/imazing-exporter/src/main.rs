use anyhow::Result;
use clap::Parser;
use imazing_exporter::cli::Cli;
use imazing_exporter::{parse_date_range, run};
use message_vault_io_core::{ExporterConfig, ImazingConfig, MediaConfig, SourceConfig};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let common = &cli.common;
    message_vault_io_core::run_cli(
        common,
        |c| {
            parse_date_range(
                c.start_date.as_deref(),
                c.end_date.as_deref(),
                cli.timezone.as_deref(),
            )
        },
        |date_range, output_format, compress| ExporterConfig {
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
        },
        run,
    )
}
