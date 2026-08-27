//! Command-line flags for `whatsapp-exporter`.

use std::path::PathBuf;

use clap::{Command, Parser, ValueEnum};
use message_vault_io_core::{CommonCli, WhatsappPlatform};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliPlatform {
    Android,
    Ios,
}

impl From<CliPlatform> for WhatsappPlatform {
    fn from(value: CliPlatform) -> Self {
        match value {
            CliPlatform::Android => WhatsappPlatform::Android,
            CliPlatform::Ios => WhatsappPlatform::Ios,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "whatsapp-exporter")]
#[command(
    about = "Convert WhatsApp DB/backup (via wtsexporter) via common message to JSON/CSV/EML/MBOX/JSONL/XML"
)]
pub struct Cli {
    /// Directory (or msgstore.db file) used to resolve relative defaults such as
    /// `msgstore.db` / `wa.db` / `WhatsApp/`. Defaults to the process cwd.
    /// Extraction always runs in a temporary directory (not this path).
    /// The GUI omits this flag.
    #[arg(long)]
    pub input: Option<PathBuf>,

    /// Android or iOS (required unless --json)
    #[arg(long, value_enum)]
    pub platform: Option<CliPlatform>,

    /// Skip wtsexporter; convert an existing result.json
    #[arg(long)]
    pub json: Option<PathBuf>,

    /// Decryption key file path or crypt15 hex key (forwarded as -k)
    #[arg(long)]
    pub key: Option<String>,

    /// Encrypted backup / iOS backup path (forwarded as -b)
    #[arg(long)]
    pub backup: Option<PathBuf>,

    /// Contacts database wa.db / ContactsV2.sqlite (forwarded as -w)
    #[arg(long)]
    pub wa: Option<PathBuf>,

    /// WhatsApp media folder (forwarded as -m)
    #[arg(long)]
    pub media: Option<PathBuf>,

    /// Explicit msgstore.db path (forwarded as -d)
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// WhatsApp Business defaults
    #[arg(long)]
    pub business: bool,

    #[command(flatten)]
    pub common: CommonCli,
}

pub fn clap_command() -> Command {
    message_vault_io_core::clap_command::<Cli>()
}

message_vault_io_core::clap_command_uses_binary_name_test!("whatsapp-exporter");
