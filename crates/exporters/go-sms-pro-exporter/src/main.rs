use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use go_sms_pro_exporter::{parse_date_range, run};
use message_vault_io_core::{
    CommonCli, ExporterConfig, GoSmsProConfig, MediaConfig, OutputFormat, SourceConfig,
};
use media::compress_options_from_cli;

#[derive(Parser, Debug)]
#[command(name = "go-sms-pro-exporter")]
#[command(
    about = "Convert GO SMS Pro XML+PDU backups via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
struct Cli {
    /// Directory containing gosms_sys*.xml and I_*.pdu files
    #[arg(long)]
    input: PathBuf,

    /// Owner phone (E.164 or digits). Repeat for multiple owner numbers.
    /// Required — there is no demo default (wrong owner flips PDU direction).
    #[arg(long = "owner-phone", required = true)]
    owner_phones: Vec<String>,

    #[command(flatten)]
    common: CommonCli,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let common = &cli.common;
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
        inputs: vec![cli.input],
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
        source: SourceConfig::GoSmsPro(GoSmsProConfig {
            owner_phones: cli.owner_phones,
        }),
    })?;

    message_vault_io_core::print_result(&result);
    Ok(())
}
