//! Command-line flags for `imazing-exporter`.

use std::path::PathBuf;

use clap::{Command, Parser};
use message_vault_io_core::CommonCli;

#[derive(Parser, Debug)]
#[command(name = "imazing-exporter")]
#[command(
    about = "Convert iMazing Messages / WhatsApp CSV exports via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
pub struct Cli {
    /// Messages/WhatsApp export directory (or a single CSV for CLI convenience)
    #[arg(long)]
    pub input: PathBuf,

    /// UTC offset for naive Message Date values (e.g. UTC-05:00). Default: host local.
    #[arg(long)]
    pub timezone: Option<String>,

    #[command(flatten)]
    pub common: CommonCli,
}

pub fn clap_command() -> Command {
    message_vault_io_core::clap_command::<Cli>()
}

message_vault_io_core::clap_command_uses_binary_name_test!("imazing-exporter");
