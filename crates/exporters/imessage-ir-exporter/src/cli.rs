//! Command-line flags for `imessage-ir-exporter`.

use std::path::PathBuf;

use clap::{Command, CommandFactory, Parser};
use message_vault_io_core::CommonCli;

#[derive(Parser, Debug)]
#[command(name = "imessage-ir-exporter")]
#[command(
    about = "Export Apple Messages (chat.db) via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
pub struct Cli {
    /// Path to chat.db (macOS) or iOS backup root (default: system Messages DB)
    #[arg(long)]
    pub input: Option<PathBuf>,

    /// Platform: `macOS`, `iOS`, or omit to auto-detect
    #[arg(long)]
    pub platform: Option<String>,

    /// Attachment mode: `clone` (default), `basic`, `full`, or `disabled`
    #[arg(long = "copy-method", default_value = "clone")]
    pub copy_method: String,

    /// Custom attachment root (macOS)
    #[arg(long = "attachment-root")]
    pub attachment_root: Option<String>,

    /// iOS backup password (cleartext; prompted elsewhere in GUI)
    #[arg(long = "backup-password")]
    pub backup_password: Option<String>,

    /// Limit export to one conversation (chat identifier / handle)
    #[arg(long = "conversation")]
    pub conversation: Option<String>,

    /// Use destination caller id for outgoing From display name (default on)
    #[arg(long = "use-caller-id", default_value_t = true, action = clap::ArgAction::Set)]
    pub use_caller_id: bool,

    #[command(flatten)]
    pub common: CommonCli,
}

pub fn clap_command() -> Command {
    Cli::command()
}

#[cfg(test)]
mod clap_command_tests {
    #[test]
    fn clap_command_uses_binary_name() {
        let cmd = super::clap_command();
        assert_eq!(cmd.get_name(), "imessage-ir-exporter");
    }
}
