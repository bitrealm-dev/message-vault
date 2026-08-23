//! Command-line flags for `sms-backup-plus-exporter`.

use std::path::PathBuf;

use clap::{Command, CommandFactory, Parser, Subcommand};
use message_vault_io_core::CommonCli;

#[derive(Parser, Debug)]
#[command(name = "sms-backup-plus-exporter")]
#[command(
    about = "Convert SMS Backup+ EML exports via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
pub struct Cli {
    /// Log progress to stderr (inputs, scan/write progress, dedupe summary)
    #[arg(short = 'v', long, global = true)]
    pub verbose: bool,

    /// Skip the end-of-run summary on stdout
    #[arg(long, global = true)]
    pub no_summary: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Convert EML tree via common message to the chosen packaging format
    Convert {
        /// Path to a .eml file or directory tree of EMLs (Archive/, Sent/, …).
        /// Repeat for multiple roots; trees are merged and path-deduped.
        /// Default: source_dirs from config/owner.toml when set.
        #[arg(long = "input")]
        input: Vec<PathBuf>,

        /// Owner phone (E.164 or digits). Repeat for multiple owner numbers.
        /// Default: `phones` in config/owner.toml
        #[arg(long = "owner-phone")]
        owner_phones: Vec<String>,

        /// Owner email addresses used to detect sent messages when X-smssync-type is missing.
        /// Default: `emails` in config/owner.toml
        #[arg(long = "owner-email", value_name = "EMAIL")]
        owner_emails: Vec<String>,

        /// Name mapping CSV (`Phone,Incorrect Name`) for EML export aliases.
        /// Default: config/name-mapping.csv when that file exists.
        #[arg(long = "name-mapping")]
        name_mapping: Option<PathBuf>,

        #[command(flatten)]
        common: CommonCli,
    },
}

pub fn clap_command() -> Command {
    Cli::command()
}

#[cfg(test)]
mod clap_command_tests {
    #[test]
    fn clap_command_uses_binary_name() {
        let cmd = super::clap_command();
        assert_eq!(cmd.get_name(), "sms-backup-plus-exporter");
    }
}
