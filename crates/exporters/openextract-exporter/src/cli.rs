//! Command-line flags for `openextract-exporter`.

use std::path::PathBuf;

use clap::{Command, Parser};
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
    message_vault_io_core::clap_command::<Cli>()
}

message_vault_io_core::clap_command_uses_binary_name_test!("openextract-exporter");
