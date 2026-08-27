//! Command-line flags for `vault-pull`.

use std::path::PathBuf;

use clap::{Command, CommandFactory, Parser};

use crate::DEFAULT_PAGE_LIMIT;

#[derive(Debug, Parser)]
#[command(
    name = "vault-pull",
    about = "Pull messages from Message Vault into a message-ir export folder",
    long_about = "Calls GET /v1/export/messages with Fastmail-style search, downloads \
attachments via GET /v1/assets/{sha256}, and writes per-conversation .jsonl files plus \
attachments/.\n\nPrefer VAULT_KEY for the vault key. Prefer Message Vault → Vault Export \
for a GUI."
)]
pub struct Cli {
    /// Vault base URL (e.g. http://127.0.0.1:8080)
    #[arg(long, env = "VAULT_URL")]
    pub url: String,

    /// Vault account username (optional; resolved from the vault key)
    #[arg(long, default_value = "")]
    pub username: String,

    /// App password / Vault key (Settings → Account). Prefer VAULT_KEY env.
    #[arg(long, env = "VAULT_KEY")]
    pub key: String,

    /// Output directory for message-ir JSONL + attachments/
    #[arg(long)]
    pub out: PathBuf,

    /// Fastmail-style search query (optional)
    #[arg(long, default_value = "")]
    pub query: String,

    /// Only messages on or after this date (YYYY-MM-DD); adds after:
    #[arg(long)]
    pub after: Option<String>,

    /// Only messages before this date (YYYY-MM-DD); adds before:
    #[arg(long)]
    pub before: Option<String>,

    /// Restrict to one vault source id
    #[arg(long)]
    pub source: Option<String>,

    /// Skip attachment downloads
    #[arg(long)]
    pub skip_attachments: bool,

    /// Page size for /v1/export/messages
    #[arg(long, default_value_t = DEFAULT_PAGE_LIMIT)]
    pub page_limit: usize,

    /// Authenticate only; do not export
    #[arg(long)]
    pub auth_only: bool,
}

/// The clap `Command` for embedding --help output into the docs pages and GUI.
pub fn clap_command() -> Command {
    Cli::command()
}

#[cfg(test)]
mod clap_command_tests {
    #[test]
    fn clap_command_is_named_vault_pull() {
        let cmd = super::clap_command();
        assert_eq!(cmd.get_name(), "vault-pull");
        assert!(cmd.get_arguments().any(|a| a.get_long() == Some("url")));
    }
}
