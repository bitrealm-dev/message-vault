use anyhow::Result;
use clap::Parser;
use imessage_ir_exporter::cli::Cli;
use imessage_ir_exporter::run;
use media::compress_options_from_cli;
use message_vault_io_core::{
    AppleConfig, ApplePlatform, ExporterConfig, MediaConfig, OutputFormat, SourceConfig,
    parse_date_range,
};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = build_config_from_cli(&cli)?;
    let result = run(&config)?;

    for line in &result.messages {
        println!("{line}");
    }
    Ok(())
}

/// Build the `ExporterConfig` from the parsed CLI (shared with tests).
pub(crate) fn build_config_from_cli(cli: &Cli) -> Result<ExporterConfig> {
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
    if let Some(path) = &cli.input {
        inputs.push(path.clone());
    }

    Ok(ExporterConfig {
        inputs,
        output: common.output.clone(),
        date_range,
        timezone: None,
        // `--contacts` carries the macOS AddressBook path; the shared
        // ContactsConfig (CSV/VCF) is derived from the same flag.
        contacts: common.contacts_config(),
        obfuscate: common.obfuscate_config(),
        media: MediaConfig {
            mode: common.media_mode,
            compress: compress_options_from_cli(
                common.media_max_resolution,
                common.media_max_fps,
                &common.media_min_size,
                common.media_skip_efficient,
            )?,
        },
        cancel: None,
        log: None,
        output_format,
        resume: false,
        source: SourceConfig::Apple(AppleConfig {
            platform,
            attachment_root: cli.attachment_root.clone(),
            copy_method: cli.copy_method.clone(),
            apple_contacts: common.contacts.clone(),
            backup_password: cli.backup_password.clone(),
            conversation_filter: cli.conversation.clone(),
            use_caller_id: cli.use_caller_id,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use media::{MaxResolution, MediaMode};

    #[test]
    fn media_flags_reach_the_config() {
        let cli = Cli::parse_from([
            "imessage-ir-exporter",
            "--output",
            "/tmp/imessage-ir-test-out",
            "--media-mode",
            "convert",
            "--media-max-resolution",
            "720p",
        ]);
        let config = build_config_from_cli(&cli).unwrap();
        assert_eq!(config.media.mode, MediaMode::Convert);
        assert_eq!(config.media.compress.max_resolution, MaxResolution::P720);
    }

    #[test]
    fn media_defaults_match_old_default_config() {
        let cli = Cli::parse_from([
            "imessage-ir-exporter",
            "--output",
            "/tmp/imessage-ir-test-out",
        ]);
        let config = build_config_from_cli(&cli).unwrap();
        let default_media = MediaConfig::default();
        assert_eq!(config.media.mode, default_media.mode);
        assert_eq!(config.media.compress, default_media.compress);
    }
}
