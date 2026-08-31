use anyhow::Result;
use clap::Parser;
use message_vault_io_core::{OpenExtractConfig, SourceConfig};
use openextract_exporter::cli::Cli;
use openextract_exporter::{parse_date_range, run};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let common = &cli.common;
    message_vault_io_core::run_cli(
        common,
        |c| parse_date_range(c.start_date.as_deref(), c.end_date.as_deref()),
        |date_range, output_format, compress| {
            common.exporter_config(
                date_range,
                output_format,
                compress,
                vec![cli.input],
                SourceConfig::OpenExtract(OpenExtractConfig {}),
            )
        },
        run,
    )
}
