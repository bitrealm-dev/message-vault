use anyhow::Result;
use clap::Parser;
use media::compress_options_from_cli;
use message_vault_io_core::{
    ExporterConfig, MediaConfig, OutputFormat, SmsBackupPlusConfig, SourceConfig,
};
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
            let date_range =
                parse_date_range(common.start_date.as_deref(), common.end_date.as_deref())
                    .map_err(anyhow::Error::msg)?;
            let output_format = OutputFormat::parse(&common.format).map_err(anyhow::Error::msg)?;
            let compress = compress_options_from_cli(
                common.media_max_resolution,
                common.media_max_fps,
                &common.media_min_size,
                common.media_skip_efficient,
            )?;
            let result = run(&ExporterConfig {
                inputs: input,
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
                source: SourceConfig::SmsBackupPlus(SmsBackupPlusConfig {
                    owner_phones,
                    owner_emails,
                    name_mapping,
                    verbose: cli.verbose,
                    include_summary: !cli.no_summary,
                }),
            })?;

            message_vault_io_core::print_result(&result);
        }
    }
    Ok(())
}
