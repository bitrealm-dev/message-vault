use anyhow::Result;
use clap::Parser;
use go_sms_pro_exporter::cli::Cli;
use go_sms_pro_exporter::{parse_date_range, run};
use message_vault_io_core::{GoSmsProConfig, SourceConfig};

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
                SourceConfig::GoSmsPro(GoSmsProConfig {
                    owner_phones: cli.owner_phones,
                }),
            )
        },
        run,
    )
}
