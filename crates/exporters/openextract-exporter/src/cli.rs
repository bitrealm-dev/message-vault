//! Command-line flags for `openextract-exporter`.

use std::path::PathBuf;

use clap::{Command, CommandFactory, Parser};
use message_vault_io_core::CommonCli;

#[derive(Parser, Debug)]
#[command(name = "openextract-exporter")]
#[command(
    about = "Convert OpenExtract conversation CSV (+ VCF) via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
pub struct Cli {
    /// OpenExtract CSV file or directory of conversation_*.csv / all_conversations.csv
    #[arg(long)]
    pub input: PathBuf,

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
        assert_eq!(cmd.get_name(), "openextract-exporter");
    }
}
