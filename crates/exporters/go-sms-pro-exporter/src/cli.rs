//! Command-line flags for `go-sms-pro-exporter`.

use std::path::PathBuf;

use clap::{Command, Parser};
use message_vault_io_core::CommonCli;

#[derive(Parser, Debug)]
#[command(name = "go-sms-pro-exporter")]
#[command(
    about = "Convert GO SMS Pro XML+PDU backups via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
pub struct Cli {
    /// Directory containing gosms_sys*.xml and I_*.pdu files
    #[arg(long)]
    pub input: PathBuf,

    /// Owner phone (E.164 or digits). Repeat for multiple owner numbers.
    /// Required — there is no demo default (wrong owner flips PDU direction).
    #[arg(long = "owner-phone", required = true)]
    pub owner_phones: Vec<String>,

    #[command(flatten)]
    pub common: CommonCli,
}

pub fn clap_command() -> Command {
    message_vault_io_core::clap_command::<Cli>()
}

message_vault_io_core::clap_command_uses_binary_name_test!("go-sms-pro-exporter");
