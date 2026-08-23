//! Command-line flags for `vault-push`.

use std::path::PathBuf;

use clap::{Command, CommandFactory, Parser};

use crate::{DEFAULT_ASSET_MAX_BYTES, DEFAULT_ASSET_UPLOAD_WORKERS, DEFAULT_BATCH_SIZE};

#[derive(Debug, Parser)]
#[command(
    name = "vault-push",
    about = "Push a Message Vault JSONL folder into Message Vault",
    long_about = "Reads per-conversation .jsonl files (message-ir schema v3) plus \
attachments/, uploads media by SHA-256, then imports message batches.\n\n\
Prefer VAULT_KEY for the vault key. Prefer Message Vault → Vault for a GUI."
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

    /// Export directory containing .jsonl files and attachments/
    #[arg(long)]
    pub input: PathBuf,

    /// Import mode: append (resume-safe) or replace
    #[arg(long, default_value = "append")]
    pub mode: String,

    /// Continue after a failed conversation
    #[arg(long, default_value_t = true)]
    pub continue_on_error: bool,

    /// Ignore journal; re-upload assets and re-import messages
    #[arg(long)]
    pub force: bool,

    /// Import messages without uploading attachments
    #[arg(long)]
    pub skip_attachments: bool,

    /// Hash attachments and fail when on-disk sha256 differs from export digest_sha256
    #[arg(long, default_value_t = false)]
    pub verify_digests: bool,

    /// Trust export metadata: skip re-hashing attachments when size_bytes matches
    /// the file size on disk. Without this flag every attachment is re-hashed.
    #[arg(long, default_value_t = false)]
    pub trust_export: bool,

    /// Max retries for transient HTTP errors
    #[arg(long, default_value_t = 3)]
    pub max_retries: u32,

    /// Messages per import request
    #[arg(long, default_value_t = DEFAULT_BATCH_SIZE)]
    pub batch_size: usize,

    /// Simultaneous attachment uploads; message imports remain sequential
    #[arg(long, default_value_t = DEFAULT_ASSET_UPLOAD_WORKERS)]
    pub asset_upload_workers: usize,

    /// Max attachment size in bytes (must not exceed vault server.asset_max_bytes)
    #[arg(long, default_value_t = DEFAULT_ASSET_MAX_BYTES)]
    pub asset_max_bytes: u64,

    /// Authenticate only; do not import
    #[arg(long)]
    pub auth_only: bool,

    /// Report JSON path (default: <input>/vault-push-report.json)
    #[arg(long)]
    pub report: Option<PathBuf>,

    /// Log path (default: <input>/vault-push.log)
    #[arg(long)]
    pub log: Option<PathBuf>,

    /// Journal path (default: <input>/.vault-import-state.jsonl)
    #[arg(long)]
    pub journal: Option<PathBuf>,
}

pub fn clap_command() -> Command {
    Cli::command()
}

#[cfg(test)]
mod clap_command_tests {
    #[test]
    fn clap_command_is_named_vault_push() {
        let cmd = super::clap_command();
        assert_eq!(cmd.get_name(), "vault-push");
        assert!(cmd.get_arguments().any(|a| a.get_long() == Some("url")));
    }
}
