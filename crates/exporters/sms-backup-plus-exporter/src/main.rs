use anyhow::Result;
use clap::Parser;
use message_vault_io_core::{SmsBackupPlusConfig, SourceConfig};
use sms_backup_plus_exporter::cli::{Cli, Commands};
use sms_backup_plus_exporter::{parse_date_range, run};

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Convert {
            input,
            owner_phones,
            owner_emails,
            name_mapping,
            common,
        } => {
            let common = &common;
            message_vault_io_core::run_cli(
                common,
                |c| parse_date_range(c.start_date.as_deref(), c.end_date.as_deref()),
                |date_range, output_format, compress| {
                    common.exporter_config(
                        date_range,
                        output_format,
                        compress,
                        input,
                        SourceConfig::SmsBackupPlus(SmsBackupPlusConfig {
                            owner_phones,
                            owner_emails,
                            name_mapping,
                            verbose: cli.verbose,
                            include_summary: !cli.no_summary,
                        }),
                    )
                },
                run,
            )
        }
    }
}
