use anyhow::Result;
use clap::Parser;
use imazing_exporter::cli::Cli;
use imazing_exporter::{parse_date_range, run};
use message_vault_io_core::{ImazingConfig, SourceConfig};

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
        |date_range, output_format, compress| {
            let mut config = common.exporter_config(
                date_range,
                output_format,
                compress,
                vec![cli.input],
                SourceConfig::Imazing(ImazingConfig {}),
            );
            // iMazing is the one exporter with a timezone flag; the shared
            // constructor leaves timezone as None.
            config.timezone = cli.timezone.clone();
            config
        },
        run,
    )
}
