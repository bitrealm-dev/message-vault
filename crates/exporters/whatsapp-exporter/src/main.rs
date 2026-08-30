use anyhow::Result;
use clap::Parser;
use message_vault_io_core::{ExporterConfig, MediaConfig, SourceConfig, WhatsappConfig};
use whatsapp_exporter::cli::Cli;
use whatsapp_exporter::{parse_date_range, run};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let common = &cli.common;
    message_vault_io_core::run_cli(
        common,
        |c| parse_date_range(c.start_date.as_deref(), c.end_date.as_deref()),
        |date_range, output_format, compress| ExporterConfig {
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
            resume: false,
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
        },
        run,
    )
}
