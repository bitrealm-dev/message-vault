use anyhow::Result;
use clap::Parser;
use imessage_ir_exporter::cli::Cli;
use imessage_ir_exporter::run;
use message_vault_io_core::{
    AppleConfig, ApplePlatform, ExporterConfig, MediaConfig, OutputFormat, SourceConfig,
    parse_date_range,
};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let common = &cli.common;
    let output_format = OutputFormat::parse(&common.format).map_err(anyhow::Error::msg)?;
    let date_range = parse_date_range(common.start_date.as_deref(), common.end_date.as_deref())
        .map_err(anyhow::Error::msg)?;
    let platform = match cli.platform.as_deref() {
        None => None,
        Some(s) if s.eq_ignore_ascii_case("macos") => Some(ApplePlatform::MacOs),
        Some(s) if s.eq_ignore_ascii_case("ios") => Some(ApplePlatform::Ios),
        Some(s) if s.eq_ignore_ascii_case("auto") => Some(ApplePlatform::Auto),
        Some(other) => anyhow::bail!("invalid --platform {other}; use macOS, iOS, or auto"),
    };

    let mut inputs = Vec::new();
    if let Some(path) = cli.input {
        inputs.push(path);
    }

    let result = run(&ExporterConfig {
        inputs,
        output: common.output.clone(),
        date_range,
        timezone: None,
        // `--contacts` carries the macOS AddressBook path; the shared
        // ContactsConfig (CSV/VCF) is derived from the same flag.
        contacts: common.contacts_config(),
        obfuscate: common.obfuscate_config(),
        media: MediaConfig::default(),
        cancel: None,
        log: None,
        output_format,
        source: SourceConfig::Apple(AppleConfig {
            platform,
            attachment_root: cli.attachment_root,
            copy_method: cli.copy_method,
            apple_contacts: common.contacts.clone(),
            backup_password: cli.backup_password,
            conversation_filter: cli.conversation,
            use_caller_id: cli.use_caller_id,
            show_progress: false,
            ignore_disk_space: false,
        }),
    })?;

    for line in &result.messages {
        println!("{line}");
    }
    Ok(())
}
