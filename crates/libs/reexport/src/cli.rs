//! Command-line flags for `message-reexporter`.

use std::path::PathBuf;

use clap::{Command, CommandFactory, Parser};
use media::{MaxResolution, MediaMode};

#[derive(Parser, Debug)]
#[command(name = "message-reexporter")]
#[command(about = "Convert an existing Message Vault output to another format")]
/// Command-line flags for the `message-reexporter` binary; the about text
/// comes from the `#[command(about)]` attribute.
pub struct Cli {
    /// Directory containing a prior Message Vault output (auto-detected)
    #[arg(long)]
    pub input: PathBuf,

    /// Output directory for the converted export
    #[arg(long)]
    pub output: PathBuf,

    /// Output format: `json` (default), `jsonl`, `csv`, `eml`, `mbox`, or `xml`
    #[arg(long = "format", default_value = "json", value_name = "FORMAT")]
    pub format: String,

    /// Rewrite output with stable fake names/numbers/text and placeholder media
    #[arg(long)]
    pub obfuscate: bool,

    /// Optional 8-hex seed for reproducible obfuscation (implies --obfuscate)
    #[arg(long = "obfuscate-seed")]
    pub obfuscate_seed: Option<String>,

    /// Attachment media: disabled, clone (default), convert, or compress
    #[arg(long = "media-mode", default_value = "clone", value_name = "MODE")]
    pub media_mode: MediaMode,

    /// Compress only: max long edge (720p, 1080p, 4k)
    #[arg(
        long = "media-max-resolution",
        default_value = "1080p",
        value_name = "RES"
    )]
    pub media_max_resolution: MaxResolution,

    /// Compress only: max frame rate
    #[arg(long = "media-max-fps", default_value_t = 30.0)]
    pub media_max_fps: f32,

    /// Compress only: only re-encode videos at/above this size (e.g. 20M)
    #[arg(long = "media-min-size", default_value = "20M")]
    pub media_min_size: String,

    /// Compress only: skip already-efficient HEVC under max resolution (default on)
    #[arg(long = "media-skip-efficient", default_value_t = true, action = clap::ArgAction::Set)]
    pub media_skip_efficient: bool,
}

/// The clap `Command` for embedding `--help` output into GUI docs.
pub fn clap_command() -> Command {
    Cli::command()
}

#[cfg(test)]
mod clap_command_tests {
    #[test]
    fn clap_command_uses_binary_name() {
        let cmd = super::clap_command();
        assert_eq!(cmd.get_name(), "message-reexporter");
    }
}
